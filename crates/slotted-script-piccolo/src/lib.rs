//! Lua through `piccolo`: the web [`ScriptRuntime`] adapter (ADR 0001).
//!
//! `slotted-script-mlua` is the reference; this crate mirrors its behaviour on
//! a runtime that has neither a serde bridge nor a `sandbox(true)` nor a
//! memory limit, and it exists for one reason: on `wasm32-unknown-unknown`
//! piccolo reports a Lua error as a `Result` and leaves the state usable,
//! where the alternatives abort the whole module.
//!
//! What is different from the mlua adapter, all of it invisible to a mod:
//!
//! - **Stdlib.** piccolo 0.3 ships a partial `base`, `string` and `table`.
//!   Everything in [`stdlib::POLYFILLED`] is implemented here as a Rust
//!   callback. Two documented gaps: `string.find` is plain-text only and
//!   `table.sort` takes no comparator.
//! - **Sandbox.** There is no `sandbox(true)`, so each library table is
//!   replaced by an empty read-only wrapper (`__index` reads through,
//!   `__newindex` raises), and so is `slotted` once the prelude has built it.
//! - **Budget.** [`Limits::budget`] is interrupt ticks on Luau and piccolo
//!   fuel here, at [`FUEL_PER_TICK`] fuel to the tick.
//! - **Memory.** `Limits::memory_bytes` is checked against
//!   `Lua::total_memory` between execution slices rather than enforced by an
//!   allocator hook, so a state overshoots by at most one slice.
//! - **Traceback.** piccolo exposes no Lua stack to the host, so
//!   [`ScriptError::Runtime::traceback`] names the chunk and says so.
//!
//! ```
//! use slotted_script::{Limits, ModId, ScriptEvent, ScriptRuntime, Stage};
//! use slotted_script_piccolo::PiccoloRuntime;
//!
//! let mut runtime = PiccoloRuntime::new(Limits::default());
//! let id = runtime
//!     .load(
//!         &ModId::new("demo").unwrap(),
//!         "data.lua",
//!         r#"slotted.register_item("apple", { max_stack_size = 16 })"#,
//!         Stage::Data,
//!     )
//!     .unwrap();
//! let commands = runtime
//!     .call(id, &ScriptEvent::DataStage { api_version: 1 })
//!     .unwrap();
//! assert_eq!(commands.len(), 1);
//! ```

pub mod bridge;
pub mod model_ser;
pub mod stdlib;

use std::collections::BTreeMap;

use piccolo::{
    Closure, Context, Executor, Fuel, Function, IntoValue, Lua, StashedExecutor, StashedFunction,
    StashedTable, StaticError, Table, Value as LuaValue,
};
use slotted_script::{
    API_VERSION, DISPATCH_FN, Limits, ModId, PRELUDE, ScriptCommand, ScriptError, ScriptEvent,
    ScriptId, ScriptRuntime, Stage, TEST_PRELUDE,
};

/// How much piccolo fuel one [`Limits::budget`] tick buys.
///
/// The two units are not the same thing and cannot be made the same thing.
/// Luau raises an interrupt about once per function call and once per loop
/// back-edge, so a tick is "one step of control flow"; piccolo charges fuel
/// per VM instruction, so the body of a loop costs several. Eight is the
/// ratio that makes the conformance suite behave the same on both adapters:
/// `budget.lua` (an empty infinite loop) still runs out, and `memory.lua`
/// (a few hundred 100 KB allocations) still reaches the memory cap first,
/// which is what it asserts. The default budget of 1,000,000 ticks is
/// therefore 8,000,000 fuel per `load` or `call`.
pub const FUEL_PER_TICK: u64 = 8;

/// Fuel spent between two memory checks. A state can overshoot
/// `Limits::memory_bytes` by whatever one slice allocates, so this trades
/// enforcement accuracy against the cost of leaving the GC arena.
const FUEL_SLICE: i32 = 4096;

/// One loaded script: its own `Lua`, with `__slotted_dispatch` resolved once.
struct Loaded {
    /// `<mod>/<file>`, for error messages.
    name: String,
    /// The state. Not `Send`, which is why the adapter is web-only.
    lua: Lua,
    /// The prelude's dispatch entry point.
    dispatch: StashedFunction,
}

/// A state with the prelude installed, and the environment table its chunks
/// run in. The environment is what the sandbox is built out of, so a caller
/// that wants to inspect the sandbox needs both halves.
pub struct SandboxedState {
    /// The VM.
    pub lua: Lua,
    /// The `_ENV` every chunk of this script sees. Not `ctx.globals()`: see
    /// [`stdlib::build_env`].
    pub env: StashedTable,
}

/// The adapter.
///
/// # Threading
///
/// [`ScriptRuntime`] is `Send + Sync` and piccolo's `Lua` is neither: the GC
/// arena is built on `Rc` and `Cell`, deliberately, because it is a
/// single-threaded VM for a single-threaded target. The two `unsafe impl`s
/// below are the whole cost of that mismatch, and they hold because of how
/// this type is shaped:
///
/// - every `Rc` and `Cell` piccolo allocates lives *inside* the arena a
///   `Loaded` owns. There is no handle outside it, nothing is cloned out, and
///   nothing crosses a `call` boundary (the `'gc` branding is what stops
///   that), so moving a `PiccoloRuntime` to another thread moves the whole
///   graph with it and leaves no aliasing reference behind. That is `Send`.
/// - every method that touches a state takes `&mut self`, and the type is
///   neither `Clone` nor `Copy`, so two threads cannot be inside one arena at
///   once through a shared reference. `&PiccoloRuntime` grants no access to a
///   `Lua` at all. That is `Sync`.
///
/// The intended deployment is a browser tab, where there is one thread and the
/// question does not arise; this exists so the adapter can sit behind the same
/// port as the native one, in a Bevy `Resource`, without a `NonSend` variant
/// of `ScriptHost`.
pub struct PiccoloRuntime {
    limits: Limits,
    next: u32,
    scripts: BTreeMap<ScriptId, Loaded>,
}

impl PiccoloRuntime {
    /// A runtime with `limits`.
    pub fn new(limits: Limits) -> Self {
        Self {
            limits,
            next: 0,
            scripts: BTreeMap::new(),
        }
    }

    /// Current limits.
    pub const fn limits(&self) -> &Limits {
        &self.limits
    }

    /// Loaded script count.
    pub fn len(&self) -> usize {
        self.scripts.len()
    }

    /// Whether nothing is loaded.
    pub fn is_empty(&self) -> bool {
        self.scripts.is_empty()
    }

    /// Builds a state with the prelude installed and everything frozen.
    /// Public so a test can assert the sandbox without loading a chunk.
    ///
    /// The order matters as much as it does on Luau, for the same reason in
    /// reverse: the wrappers reject writes, so every library function must be
    /// installed and the prelude must have run before they go on.
    ///
    /// # Errors
    ///
    /// [`ScriptError::Sandbox`] if the prelude does not compile or run, which
    /// is a bug in this crate rather than in a mod.
    pub fn sandboxed_state(
        mod_id: &ModId,
        stage: Stage,
        limits: &Limits,
    ) -> Result<SandboxedState, ScriptError> {
        let name = format!("{mod_id}/<prelude>");
        let sandbox = |message: String| ScriptError::Sandbox {
            name: name.clone(),
            message,
        };

        let mut lua = Lua::empty();
        // `load_core` is base, coroutine, math, string and table. `load_io`
        // is the only other one piccolo has and it is never called, so `io`
        // and `os` do not exist in the first place.
        lua.load_core();

        let (env, executor) = lua
            .try_enter(|ctx| {
                let env = stdlib::build_env(ctx);
                set_host_globals(ctx, env, mod_id, stage);
                let closure =
                    Closure::load_with_env(ctx, Some("@slotted/prelude"), PRELUDE.as_bytes(), env)?;
                Ok((
                    ctx.stash(env),
                    ctx.stash(Executor::start(ctx, closure.into(), ())),
                ))
            })
            .map_err(|err| sandbox(format!("the prelude did not compile: {err}")))?;

        run(&mut lua, &executor, limits).map_err(|outcome| {
            sandbox(format!("the prelude did not run: {}", outcome.describe()))
        })?;

        // The test stage gets `slotted.test` on top, in the same env and
        // before the freeze below: `slotted_test.lua` rawsets into the
        // prelude's table, which the readonly wrapper would refuse afterwards
        // (Phase 6 contract 3.1).
        if stage == Stage::Test {
            let executor = lua
                .try_enter(|ctx| {
                    let closure = Closure::load_with_env(
                        ctx,
                        Some("@slotted/test_prelude"),
                        TEST_PRELUDE.as_bytes(),
                        ctx.fetch(&env),
                    )?;
                    Ok(ctx.stash(Executor::start(ctx, closure.into(), ())))
                })
                .map_err(|err| sandbox(format!("the test prelude did not compile: {err}")))?;
            run(&mut lua, &executor, limits).map_err(|outcome| {
                sandbox(format!(
                    "the test prelude did not run: {}",
                    outcome.describe()
                ))
            })?;
        }

        lua.enter(|ctx| {
            let env: Table = ctx.fetch(&env);
            stdlib::freeze_stdlib(ctx, env);
            // The prelude's own `__newindex` guard cannot fire for a key the
            // `slotted` table already has, so the wrapper does the freezing.
            if let LuaValue::Table(inner) = env.get(ctx, ctx.intern(b"slotted")) {
                let frozen = stdlib::readonly(ctx, "slotted", inner);
                let _ = env.set(ctx, ctx.intern(b"slotted"), frozen);
            }
        });

        Ok(SandboxedState { lua, env })
    }

    /// Compiles and runs `source` in `state`, mapping every failure.
    fn exec_chunk(
        state: &mut SandboxedState,
        name: &str,
        source: &str,
        limits: &Limits,
    ) -> Result<(), ScriptError> {
        let chunk = format!("@{name}");
        let lua = &mut state.lua;
        let env = &state.env;
        let executor = lua
            .try_enter(|ctx| {
                let closure =
                    Closure::load_with_env(ctx, Some(&chunk), source.as_bytes(), ctx.fetch(env))?;
                Ok(ctx.stash(Executor::start(ctx, closure.into(), ())))
            })
            .map_err(|err| ScriptError::Compile {
                name: name.to_owned(),
                message: strip_prefix(&err.to_string()),
            })?;
        run(lua, &executor, limits).map_err(|outcome| outcome.classify(name))?;
        lua.try_enter(|ctx| ctx.fetch(&executor).take_result::<()>(ctx)?)
            .map_err(|err| Outcome::Failed(err).classify(name))
    }
}

// SAFETY: see the `Threading` section on `PiccoloRuntime`. Every `Rc` is
// owned by an arena this type owns, no handle escapes, and every entry point
// takes `&mut self`.
#[allow(unsafe_code)]
unsafe impl Send for PiccoloRuntime {}

// SAFETY: as above; `&PiccoloRuntime` exposes no way to reach a `Lua`.
#[allow(unsafe_code)]
unsafe impl Sync for PiccoloRuntime {}

impl Default for PiccoloRuntime {
    fn default() -> Self {
        Self::new(Limits::default())
    }
}

/// The three globals the prelude reads before it builds `slotted`.
fn set_host_globals<'gc>(ctx: Context<'gc>, env: Table<'gc>, mod_id: &ModId, stage: Stage) {
    let id = ctx.intern(mod_id.as_str().as_bytes());
    let stage_name = ctx.intern(stage.as_str().as_bytes());
    let _ = env.set(ctx, "__slotted_mod_id", id);
    let _ = env.set(ctx, "__slotted_stage", stage_name);
    let _ = env.set(ctx, "__slotted_api_version", i64::from(API_VERSION));
}

/// How an execution ended, for the classifier.
enum Outcome {
    /// Fuel ran out.
    Budget,
    /// `Lua::total_memory` passed the cap.
    Memory,
    /// Lua raised.
    Failed(StaticError),
}

impl Outcome {
    /// A one-line description, for the prelude's own failures.
    fn describe(&self) -> String {
        match self {
            Self::Budget => "the budget ran out".to_owned(),
            Self::Memory => "the memory limit was reached".to_owned(),
            Self::Failed(err) => err.to_string(),
        }
    }

    /// The [`ScriptError`] this outcome is, classified exactly as the mlua
    /// adapter classifies its own: a read-only violation is a sandbox error,
    /// everything else that raised is a runtime error.
    fn classify(self, name: &str) -> ScriptError {
        match self {
            Self::Budget => ScriptError::BudgetExceeded {
                name: name.to_owned(),
            },
            Self::Memory => ScriptError::Memory {
                name: name.to_owned(),
            },
            Self::Failed(err) => {
                let message = strip_prefix(&err.to_string());
                if is_readonly(&message) {
                    ScriptError::Sandbox {
                        name: name.to_owned(),
                        message,
                    }
                } else {
                    ScriptError::Runtime {
                        name: name.to_owned(),
                        message,
                        traceback: traceback(name),
                    }
                }
            }
        }
    }
}

/// piccolo prefixes every error with `lua error: ` or `runtime error: `,
/// which says nothing a mod author needs; the class is already in the
/// [`ScriptError`] variant.
fn strip_prefix(text: &str) -> String {
    text.strip_prefix("lua error: ")
        .or_else(|| text.strip_prefix("runtime error: "))
        .unwrap_or(text)
        .to_owned()
}

/// Luau's wording for a write to a frozen table, plus this adapter's own.
fn is_readonly(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains(stdlib::READONLY_MARKER) || lower.contains("read-only")
}

/// piccolo keeps no Lua call stack the host can read, so this is the honest
/// stand-in for the traceback Luau's `xpcall` message handler produces.
fn traceback(name: &str) -> String {
    format!("stack traceback:\n\tin {name} (piccolo exposes no Lua traceback)")
}

/// Runs `executor` to completion inside the budget, checking memory between
/// slices. A failure leaves the executor stopped and the state usable.
fn run(lua: &mut Lua, executor: &StashedExecutor, limits: &Limits) -> Result<(), Outcome> {
    let total = limits.budget.saturating_mul(FUEL_PER_TICK);
    let mut spent: u64 = 0;

    loop {
        let slice = i32::try_from(total.saturating_sub(spent))
            .unwrap_or(i32::MAX)
            .min(FUEL_SLICE);
        let mut fuel = Fuel::with(slice);
        let finished = lua.enter(|ctx| ctx.fetch(executor).step(ctx, &mut fuel));
        spent = spent.saturating_add(u64::try_from(slice - fuel.remaining()).unwrap_or(0));

        if finished {
            return Ok(());
        }
        if lua.total_memory() > limits.memory_bytes {
            stop(lua, executor);
            return Err(Outcome::Memory);
        }
        if spent >= total {
            stop(lua, executor);
            return Err(Outcome::Budget);
        }
    }
}

/// Abandons a half-run executor. The state itself stays usable, which is the
/// whole reason this adapter exists.
fn stop(lua: &mut Lua, executor: &StashedExecutor) {
    lua.enter(|ctx| ctx.fetch(executor).stop(&ctx));
    // The abandoned frames are garbage now; collecting here keeps a budget
    // failure from being followed by a spurious memory failure.
    lua.gc_collect();
}

impl ScriptRuntime for PiccoloRuntime {
    fn load(
        &mut self,
        mod_id: &ModId,
        name: &str,
        source: &str,
        stage: Stage,
    ) -> Result<ScriptId, ScriptError> {
        let full = format!("{mod_id}/{name}");
        let mut sandbox = Self::sandboxed_state(mod_id, stage, &self.limits)?;
        Self::exec_chunk(&mut sandbox, &full, source, &self.limits)?;

        let SandboxedState { mut lua, env } = sandbox;
        let dispatch = lua
            .try_enter(|ctx| {
                let key = ctx.intern(DISPATCH_FN.as_bytes());
                match ctx.fetch(&env).get(ctx, key) {
                    LuaValue::Function(f) => Ok(ctx.stash(f)),
                    other => Err(piccolo::String::from_slice(
                        &ctx,
                        format!("{DISPATCH_FN} is a {}", other.type_name()).as_bytes(),
                    )
                    .into_value(ctx)
                    .into()),
                }
            })
            .map_err(|err| ScriptError::Sandbox {
                name: full.clone(),
                message: format!(
                    "the prelude did not install its dispatch entry point: {}",
                    strip_prefix(&err.to_string())
                ),
            })?;

        let id = ScriptId(self.next);
        self.next += 1;
        self.scripts.insert(
            id,
            Loaded {
                name: full,
                lua,
                dispatch,
            },
        );
        Ok(id)
    }

    fn unload(&mut self, id: ScriptId) {
        self.scripts.remove(&id);
    }

    fn call(
        &mut self,
        id: ScriptId,
        event: &ScriptEvent,
    ) -> Result<Vec<ScriptCommand>, ScriptError> {
        let limits = self.limits;
        let loaded = self
            .scripts
            .get_mut(&id)
            .ok_or(ScriptError::UnknownScript(id))?;
        let name = loaded.name.clone();

        // serde writes the event into a `Value` tree, the bridge turns the
        // tree into tables. mlua does both hops in `to_value_with`.
        let payload = model_ser::to_model(event).map_err(|err| ScriptError::Protocol {
            name: name.clone(),
            message: format!("the event did not serialise: {err}"),
        })?;

        let lua = &mut loaded.lua;
        let executor = lua.enter(|ctx| {
            let arg = bridge::to_lua(ctx, &payload);
            let dispatch: Function = ctx.fetch(&loaded.dispatch);
            ctx.stash(Executor::start(ctx, dispatch, (arg,)))
        });

        run(lua, &executor, &limits).map_err(|outcome| outcome.classify(&name))?;

        lua.try_enter(|ctx| {
            let reply = ctx.fetch(&executor).take_result::<LuaValue>(ctx)??;
            Ok(decode_commands(ctx, &name, reply))
        })
        .map_err(|err| Outcome::Failed(err).classify(&name))?
    }

    fn set_limits(&mut self, limits: Limits) {
        // Every limit is read per call, so there is nothing to push into a
        // loaded state the way `set_memory_limit` needs on Luau.
        self.limits = limits;
    }
}

/// Turns the dispatch reply into commands, naming the offending entry.
///
/// Decoding the array in one go would report `invalid value at index 2` with
/// no clue which command that was, so each entry is decoded on its own and the
/// message carries its position, its `type` and its fields. The mlua adapter
/// says the same things; the conformance suite does not check the wording, but
/// a modder reads it.
fn decode_commands<'gc>(
    ctx: Context<'gc>,
    name: &str,
    reply: LuaValue<'gc>,
) -> Result<Vec<ScriptCommand>, ScriptError> {
    let protocol = |message: String| ScriptError::Protocol {
        name: name.to_owned(),
        message,
    };
    let LuaValue::Table(table) = reply else {
        return Err(protocol(format!(
            "{DISPATCH_FN} returned {}, expected an array of command tables",
            reply.type_name()
        )));
    };

    let len = table.length();
    let mut out = Vec::with_capacity(usize::try_from(len).unwrap_or(0));
    for index in 1..=len {
        let entry = table.get_value(LuaValue::Integer(index));
        let kind = command_kind(ctx, entry);
        let fields = command_fields(entry);
        let value = bridge::from_lua(entry)
            .map_err(|err| protocol(format!("command #{index}{kind}: {err}{fields}")))?;
        let decoded = slotted_model::value::from_value::<ScriptCommand>(value)
            .map_err(|err| protocol(format!("command #{index}{kind}: {err}{fields}")))?;
        out.push(decoded);
    }

    // A reply with keys outside `1..n` is a prelude bug rather than a mod's,
    // but dropping half the commands silently would be worse than saying so.
    let extra = table
        .iter()
        .filter(|(k, _)| !matches!(k, LuaValue::Integer(i) if (1..=len).contains(i)))
        .count();
    if extra > 0 {
        return Err(protocol(format!(
            "{DISPATCH_FN} returned a table with {extra} non-array key(s); expected a 1..n array"
        )));
    }

    Ok(out)
}

/// `` (type = "sort")`` for an error message, when the entry has one.
fn command_kind<'gc>(ctx: Context<'gc>, entry: LuaValue<'gc>) -> String {
    let LuaValue::Table(table) = entry else {
        return format!(" (a {}, expected a table)", entry.type_name());
    };
    match table.get(ctx, ctx.intern(b"type")) {
        LuaValue::String(s) => format!(" (type = {:?})", s.to_str_lossy()),
        _ => " (no `type` field)".to_owned(),
    }
}

/// The entry's fields and their Lua types, so a message that says only
/// `expected u32` still tells the modder which key is wrong.
fn command_fields(entry: LuaValue<'_>) -> String {
    let LuaValue::Table(table) = entry else {
        return String::new();
    };
    let mut parts: Vec<String> = table
        .iter()
        .filter(|(k, _)| !matches!(k, LuaValue::String(s) if s.as_bytes() == b"type"))
        .map(|(k, v)| {
            let key = match k {
                LuaValue::String(s) => s.to_str_lossy().into_owned(),
                other => other.to_string(),
            };
            format!("{key} = {}", v.type_name())
        })
        .collect();
    if parts.is_empty() {
        return String::new();
    }
    parts.sort();
    format!(" (fields: {})", parts.join(", "))
}

#[cfg(test)]
mod tests;
