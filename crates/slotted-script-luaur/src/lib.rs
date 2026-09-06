//! Luau through `luaur`: the [`ScriptRuntime`] adapter that reaches the
//! browser (ADR 0004), and a native one behind the facade's `script-luaur`
//! feature.
//!
//! `luaur` is a pure-Rust, line-for-line port of Luau. This is the only script
//! runtime the workspace ships and it is the same one natively and on `wasm32`
//! (ADR 0004). One `Lua` state per loaded script, every
//! [`slotted_script::FORBIDDEN_GLOBALS`] entry removed *before*
//! `sandbox(true)` seals the state, an interrupt budget counted per `call`,
//! `set_memory_limit` for allocation, and values crossing through
//! `LuaSerdeExt`. Contract section 1.6.
//!
//! # The sandbox order is load-bearing
//!
//! Two things about `sandbox(true)` will silently defeat a sandbox that gets
//! them wrong, and both are tested here rather than trusted:
//!
//! * **Remove globals before sealing.** `sandbox(true)` installs a proxy
//!   global table whose `__index` falls through to the real environment.
//!   Setting a name to `nil` afterwards only makes it absent *from the proxy*,
//!   so the lookup falls through and the script still sees the original:
//!   `globals().set("string", Nil)` returns `Ok(())` and `string` is still a
//!   table. A sandbox that erased `io` after sealing would report success and
//!   leave `io` reachable. `build_state` therefore erases, then installs the
//!   prelude, then seals, in that order, and never removes anything
//!   afterwards. (Adding a name after sealing does work — it lands in the
//!   proxy and the script sees it — which is exactly what makes the failure
//!   quiet: writes appear to take effect, removals do not.)
//! * **`StdLib` flags do not select libraries.** luaur opens the Luau standard
//!   library as a unit, so any non-empty [`StdLib`] gives the whole safe
//!   stdlib and only [`StdLib::NONE`] gives an empty one. Nothing may be kept
//!   out by naming it; the [`FORBIDDEN_GLOBALS`] list is the whole defence.
//!
//! ```
//! use slotted_script::{Limits, ModId, ScriptEvent, ScriptRuntime, Stage};
//! use slotted_script_luaur::LuaurRuntime;
//!
//! let mut runtime = LuaurRuntime::new(Limits::default());
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
//!
//! # On `wasm32`, a Lua error aborts the module
//!
//! **Read this before shipping a page that loads untrusted mods.**
//!
//! luaur raises a Lua error by panicking (`luaD_throw` calls `panic_any`) and
//! catches it with `catch_unwind`. `wasm32-unknown-unknown` has no unwinding,
//! so in a browser every raise is a trap that takes the whole wasm module with
//! it. Concretely, on wasm:
//!
//! * `error("boom")` in a mod, a runtime type error such as `nil + 1`, an
//!   exhausted [`Limits::budget`] and a refused allocation all **abort**.
//!   They do not come back as [`ScriptError`].
//! * `pcall` inside a script **does not contain the error**. luaur implements
//!   `pcall` through the same panic, so wrapping a handler in `pcall` changes
//!   nothing.
//! * A *compile* error is the one recoverable case: that path never enters the
//!   VM, so a syntax error still returns [`ScriptError::Compile`].
//!
//! ADR 0004 accepts this in exchange for a fast, faithful Luau on both
//! targets. The consequence for a host is that **the host must be prepared to
//! restart**. Two pieces make that survivable and both are used by
//! `examples/web-playground`:
//!
//! * [`install_error_reporter`] hands the host the error text and the
//!   traceback *at the moment the error is raised*, which on wasm is before
//!   the trap. Luau runs the `xpcall` message handler before it throws, and
//!   that handler is where this adapter reports from, so the page still gets
//!   the message even though the module is about to die.
//! * The host catches the resulting `WebAssembly.RuntimeError` on the
//!   JavaScript side, re-instantiates the module and restores its state.
//!   `web/playground.js` and the playground's `snapshot_state` /
//!   `restore_state` exports are the working reference.
//!
//! Natively there is no such caveat. Unwinding works, errors come back as
//! `Result`, the state stays usable, and the error-recovery tests in this
//! crate run. Those tests are compiled out on `wasm32`, the way the Phase 0
//! spike did it: **the cfg is the finding, not a bug to fix.**

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};

use luaur_rt::{
    DeserializeOptions as DeOptions, Function, Lua, LuaOptions, LuaSerdeExt,
    SerializeOptions as SerOptions, StdLib, Value as LuaValue, VmState,
};
use slotted_script::{
    API_VERSION, FORBIDDEN_GLOBALS, Limits, ModId, PRELUDE, ScriptCommand, ScriptError,
    ScriptEvent, ScriptId, ScriptRuntime, Stage, TEST_PRELUDE,
};

/// Message prefix the interrupt raises with; the adapter maps it to
/// [`ScriptError::BudgetExceeded`].
pub const BUDGET_MARKER: &str = "slotted:budget";

/// What a script raised, handed to an [`install_error_reporter`] callback at
/// the moment the error is raised.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RaisedError {
    /// `<mod>/<file>` of the script that raised.
    pub script: String,
    /// The error value, as text.
    pub message: String,
    /// The Luau traceback, taken before the stack unwound. Possibly empty.
    pub traceback: String,
}

type Reporter = Box<dyn Fn(&RaisedError) + Send + Sync + 'static>;

/// The process-wide reporter. A `OnceLock` rather than a settable slot: a
/// reporter that can be swapped while a state is mid-call would need a lock on
/// the hot path for no gain, and a host installs one reporter at start-up.
static REPORTER: OnceLock<Reporter> = OnceLock::new();

/// Installs a callback invoked the moment a script error reaches the dispatch
/// boundary, **before** anything unwinds. Returns `false` if one was already
/// installed, leaving the first in place.
///
/// This exists for `wasm32`, where an unwind is a trap that takes the module
/// down (see the crate docs). Luau calls the `xpcall` message handler before
/// it throws, so a reporter installed here still runs and the host can put the
/// error on screen before the tab loses the module.
///
/// Natively the same errors also come back as [`ScriptError`] from
/// [`ScriptRuntime::call`], so a native host that installs a reporter will see
/// each error twice. Errors a mod catches with its own `pcall` never reach
/// here.
pub fn install_error_reporter(report: impl Fn(&RaisedError) + Send + Sync + 'static) -> bool {
    REPORTER.set(Box::new(report)).is_ok()
}

/// Hands `raised` to the installed reporter, if there is one.
fn report(raised: &RaisedError) {
    if let Some(reporter) = REPORTER.get() {
        reporter(raised);
    }
}

/// Per-state counters the interrupt and the message handler share with the
/// runtime. Stored in the state's app data so [`LuaurRuntime::sandboxed_state`]
/// can keep returning a bare [`Lua`].
#[derive(Clone)]
struct Hooks {
    /// Interrupt ticks consumed by the call in flight.
    ticks: Arc<AtomicU64>,
    /// Ticks the call in flight may consume.
    budget: Arc<AtomicU64>,
    /// Traceback captured by the `xpcall` message handler, if any.
    traceback: Arc<Mutex<String>>,
    /// `<mod>/<file>`, filled in by `load` once the chunk has a name. The
    /// message handler is built before that, so it reads the name through
    /// this rather than capturing it.
    name: Arc<Mutex<String>>,
}

impl Hooks {
    fn new(budget: u64) -> Self {
        Self {
            ticks: Arc::new(AtomicU64::new(0)),
            budget: Arc::new(AtomicU64::new(budget)),
            traceback: Arc::new(Mutex::new(String::new())),
            name: Arc::new(Mutex::new(String::new())),
        }
    }

    /// Names the script every later report is attributed to.
    fn name_it(&self, name: &str) {
        if let Ok(mut slot) = self.name.lock() {
            name.clone_into(&mut slot);
        }
    }

    fn script_name(&self) -> String {
        self.name.lock().map(|n| n.clone()).unwrap_or_default()
    }

    /// Starts a fresh accounting window.
    fn arm(&self, budget: u64) {
        self.ticks.store(0, Ordering::Relaxed);
        self.budget.store(budget, Ordering::Relaxed);
        self.clear_traceback();
    }

    fn clear_traceback(&self) {
        if let Ok(mut tb) = self.traceback.lock() {
            tb.clear();
        }
    }

    fn take_traceback(&self) -> String {
        self.traceback
            .lock()
            .map(|mut tb| std::mem::take(&mut *tb))
            .unwrap_or_default()
    }
}

/// One loaded script.
struct Loaded {
    /// `<mod>/<file>`, for error messages.
    name: String,
    /// The sealed state.
    lua: Lua,
    /// Shared counters, cloned out of the state's app data once at load.
    hooks: Hooks,
    /// `__slotted_dispatch`, `xpcall` and the traceback handler, resolved
    /// once: three global lookups per click add up.
    entry: Entry,
}

/// The three functions `call` needs, held so a hot path does not re-read
/// globals.
struct Entry {
    dispatch: Function,
    xpcall: Function,
    handler: Function,
}

/// The adapter.
pub struct LuaurRuntime {
    limits: Limits,
    next: u32,
    /// The `Mutex` buys nothing but the `Sync` half of
    /// [`ScriptRuntime`]`: Send + Sync`. Under luaur's `send` feature a `Lua`
    /// and its handles are `Send` but deliberately never `Sync`, and
    /// `Mutex<T>: Sync` whenever `T: Send`. Every method here reaches the map
    /// through `&mut self` and `Mutex::get_mut`, which takes no lock, so the
    /// wrapper costs nothing on the hot path.
    scripts: Mutex<BTreeMap<ScriptId, Loaded>>,
}

impl LuaurRuntime {
    /// A runtime with `limits`.
    pub fn new(limits: Limits) -> Self {
        Self {
            limits,
            next: 0,
            scripts: Mutex::new(BTreeMap::new()),
        }
    }

    /// Current limits.
    pub const fn limits(&self) -> &Limits {
        &self.limits
    }

    /// Loaded script count.
    pub fn len(&self) -> usize {
        self.scripts
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .len()
    }

    /// Whether nothing is loaded.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The map, without taking the lock. See the field's own note.
    fn loaded(&mut self) -> &mut BTreeMap<ScriptId, Loaded> {
        self.scripts
            .get_mut()
            .unwrap_or_else(PoisonError::into_inner)
    }

    /// Builds a sealed state with the prelude installed and the three
    /// `__slotted_*` globals set. Public so the conformance suite can assert
    /// the sandbox directly.
    ///
    /// The order is load-bearing: the stdlib opens, then the forbidden globals
    /// are erased, then the prelude runs (it writes `_G.slotted` and `print`),
    /// then `sandbox(true)` freezes everything. After sealing, a write to
    /// `globals()` is silently dropped, so nothing may be installed later.
    ///
    /// luaur opens the Luau standard library as a unit and ignores [`StdLib`]
    /// flags, so `coroutine` is present at every stage and not only at
    /// [`Stage::Test`]. That is invisible to a mod: the contract never
    /// promised `coroutine` would be absent, only the [`FORBIDDEN_GLOBALS`].
    ///
    /// # Errors
    ///
    /// [`ScriptError::Sandbox`] if a state cannot be built or the prelude
    /// does not run.
    pub fn sandboxed_state(
        mod_id: &ModId,
        stage: Stage,
        limits: &Limits,
    ) -> Result<Lua, ScriptError> {
        let name = format!("{mod_id}/<prelude>");
        Self::build_state(mod_id, stage, limits).map_err(|err| ScriptError::Sandbox {
            name,
            message: err.to_string(),
        })
    }

    fn build_state(mod_id: &ModId, stage: Stage, limits: &Limits) -> luaur_rt::Result<Lua> {
        // Any non-empty `StdLib` opens the whole safe stdlib here; the flags
        // name the intent, and luaur has no `UTF8` flag of its own because the
        // library is not separable from the rest.
        let libs = StdLib::BASE | StdLib::TABLE | StdLib::STRING | StdLib::MATH | StdLib::BIT32;
        let lua = Lua::new_with(libs, LuaOptions::new())?;

        {
            let globals = lua.globals();
            for name in FORBIDDEN_GLOBALS {
                globals.set(*name, LuaValue::Nil)?;
            }
            globals.set("__slotted_mod_id", mod_id.as_str())?;
            globals.set("__slotted_stage", stage.as_str())?;
            globals.set("__slotted_api_version", API_VERSION)?;
        }

        let hooks = Hooks::new(limits.budget);

        // The `xpcall` message handler runs before the stack unwinds, so this
        // is the only place a Luau traceback can be taken. It stashes the
        // traceback and returns the error value unchanged, leaving luaur's own
        // error mapping intact.
        {
            let hooks = hooks.clone();
            let handler = lua.create_function(move |lua, err: LuaValue| {
                let traceback = lua
                    .traceback(None, 1)
                    .map(|s| s.to_string_lossy())
                    .unwrap_or_default();
                if let Ok(mut slot) = hooks.traceback.lock() {
                    slot.clone_from(&traceback);
                }
                // On wasm the raise this handler precedes is a trap, so this
                // is the last moment the host can be told anything at all.
                report(&RaisedError {
                    script: hooks.script_name(),
                    message: lua_error_text(&err),
                    traceback,
                });
                Ok(err)
            })?;
            lua.globals().set("__slotted_traceback", handler)?;
        }

        lua.load(PRELUDE).set_name("@slotted/prelude").exec()?;

        // The test stage gets `slotted.test` on top, before the sandbox seals
        // the state: `slotted_test.lua` rawsets into the prelude's table and a
        // frozen state would refuse it (Phase 6 contract 3.1).
        if stage == Stage::Test {
            lua.load(TEST_PRELUDE)
                .set_name("@slotted/test_prelude")
                .exec()?;
        }

        {
            let ticks = hooks.ticks.clone();
            let budget = hooks.budget.clone();
            lua.set_interrupt(move |_| {
                let used = ticks.fetch_add(1, Ordering::Relaxed) + 1;
                if used > budget.load(Ordering::Relaxed) {
                    Err(luaur_rt::Error::runtime(BUDGET_MARKER))
                } else {
                    Ok(VmState::Continue)
                }
            });
        }

        lua.set_app_data(hooks);
        lua.sandbox(true)?;
        lua.set_memory_limit(limits.memory_bytes)?;
        Ok(lua)
    }

    /// Maps whatever a chunk or a dispatch raised into the [`ScriptError`] the
    /// contract names for it.
    fn classify(name: &str, hooks: &Hooks, err: &luaur_rt::Error) -> ScriptError {
        let text = err.to_string();
        if is_budget(err) {
            return ScriptError::BudgetExceeded {
                name: name.to_owned(),
            };
        }
        if is_memory(err) {
            return ScriptError::Memory {
                name: name.to_owned(),
            };
        }
        if let luaur_rt::Error::SyntaxError { message, .. } = err {
            return ScriptError::Compile {
                name: name.to_owned(),
                message: message.clone(),
            };
        }
        if is_readonly(&text) {
            return ScriptError::Sandbox {
                name: name.to_owned(),
                message: text,
            };
        }
        let mut traceback = hooks.take_traceback();
        if traceback.is_empty() {
            traceback = luaur_traceback(err);
        }
        ScriptError::Runtime {
            name: name.to_owned(),
            message: text,
            traceback,
        }
    }
}

/// Whether `err`, anywhere in its chain, is the interrupt's budget signal.
fn is_budget(err: &luaur_rt::Error) -> bool {
    err.to_string().contains(BUDGET_MARKER)
}

/// Whether `err` is an out-of-memory condition. Luau raises `not enough
/// memory` for an allocation the limit refused; luaur reports the same as
/// [`luaur_rt::Error::MemoryError`] when it escapes uncaught.
fn is_memory(err: &luaur_rt::Error) -> bool {
    fn walk(err: &luaur_rt::Error) -> bool {
        match err {
            luaur_rt::Error::MemoryError(_) => true,
            luaur_rt::Error::CallbackError { cause, .. } => walk(cause),
            other => other.to_string().contains("not enough memory"),
        }
    }
    walk(err)
}

/// Luau's wording for a write to a frozen table, plus the prelude's own.
fn is_readonly(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("readonly") || lower.contains("read-only")
}

/// The traceback luaur itself carries, if the error crossed a Rust callback.
fn luaur_traceback(err: &luaur_rt::Error) -> String {
    match err {
        luaur_rt::Error::CallbackError { traceback, .. } => traceback.clone(),
        _ => String::new(),
    }
}

/// `Option::None` must vanish rather than become `null`: the prelude reads
/// `event.stack == nil`, and the contract's value bridge has no null.
fn ser_options() -> SerOptions {
    SerOptions::new()
        .serialize_none_to_null(false)
        .serialize_unit_to_null(false)
        .set_array_metatable(false)
}

/// An empty Lua table is an empty list, not an empty map (contract 1.3).
fn de_options() -> DeOptions {
    DeOptions::new()
        .deny_unsupported_types(false)
        .deny_recursive_tables(true)
        .encode_empty_tables_as_array(true)
}

impl Default for LuaurRuntime {
    fn default() -> Self {
        Self::new(Limits::default())
    }
}

impl ScriptRuntime for LuaurRuntime {
    fn load(
        &mut self,
        mod_id: &ModId,
        name: &str,
        source: &str,
        stage: Stage,
    ) -> Result<ScriptId, ScriptError> {
        let full = format!("{mod_id}/{name}");
        let lua = Self::sandboxed_state(mod_id, stage, &self.limits)?;
        let hooks = lua
            .app_data_ref::<Hooks>()
            .map(|h| h.clone())
            .ok_or_else(|| ScriptError::Sandbox {
                name: full.clone(),
                message: "state lost its hooks".to_owned(),
            })?;

        hooks.name_it(&full);
        hooks.arm(self.limits.budget);
        let chunk_name = format!("@{full}");
        if let Err(err) = lua.load(source).set_name(&chunk_name).exec() {
            return Err(Self::classify(&full, &hooks, &err));
        }

        let entry = (|| -> luaur_rt::Result<Entry> {
            let globals = lua.globals();
            Ok(Entry {
                dispatch: globals.get(slotted_script::DISPATCH_FN)?,
                xpcall: globals.get("xpcall")?,
                handler: globals.get("__slotted_traceback")?,
            })
        })()
        .map_err(|err| ScriptError::Sandbox {
            name: full.clone(),
            message: format!("the prelude did not install its dispatch entry point: {err}"),
        })?;

        let id = ScriptId(self.next);
        self.next += 1;
        self.loaded().insert(
            id,
            Loaded {
                name: full,
                lua,
                hooks,
                entry,
            },
        );
        Ok(id)
    }

    fn unload(&mut self, id: ScriptId) {
        self.loaded().remove(&id);
    }

    fn call(
        &mut self,
        id: ScriptId,
        event: &ScriptEvent,
    ) -> Result<Vec<ScriptCommand>, ScriptError> {
        let budget = self.limits.budget;
        let loaded = self
            .loaded()
            .get(&id)
            .ok_or(ScriptError::UnknownScript(id))?;
        let name = loaded.name.clone();
        let lua = &loaded.lua;
        let hooks = &loaded.hooks;

        hooks.arm(budget);

        let entry = &loaded.entry;
        let outcome = (|| -> luaur_rt::Result<(bool, LuaValue)> {
            let arg = lua.to_value_with(event, ser_options())?;
            entry
                .xpcall
                .call::<(bool, LuaValue)>((&entry.dispatch, &entry.handler, arg))
        })();

        let (ok, value) = match outcome {
            Ok(pair) => pair,
            Err(err) => return Err(LuaurRuntime::classify(&name, hooks, &err)),
        };
        if !ok {
            let err = luaur_rt::Error::runtime(lua_error_text(&value));
            return Err(LuaurRuntime::classify(&name, hooks, &err));
        }

        decode_commands(lua, &name, &value)
    }

    fn set_limits(&mut self, limits: Limits) {
        self.limits = limits;
        for loaded in self.loaded().values() {
            loaded.hooks.budget.store(limits.budget, Ordering::Relaxed);
            // A limit below what the state already holds is refused; that is
            // the caller raising the ceiling later, not an error here.
            let _ = loaded.lua.set_memory_limit(limits.memory_bytes);
        }
    }
}

/// The text of a Lua error value caught by `xpcall`.
fn lua_error_text(value: &LuaValue) -> String {
    match value {
        LuaValue::String(s) => s.to_string_lossy(),
        other => other
            .to_string()
            .unwrap_or_else(|_| format!("a {} error value", other.type_name())),
    }
}

/// Turns the dispatch reply into commands, naming the offending entry.
///
/// Deserialising the array in one go would report `invalid value at index 2`
/// with no clue which command that was, so each entry is decoded on its own
/// and the message carries its position and its `type`.
fn decode_commands(
    lua: &Lua,
    name: &str,
    value: &LuaValue,
) -> Result<Vec<ScriptCommand>, ScriptError> {
    let protocol = |message: String| ScriptError::Protocol {
        name: name.to_owned(),
        message,
    };
    let LuaValue::Table(table) = value else {
        return Err(protocol(format!(
            "{} returned {}, expected an array of command tables",
            slotted_script::DISPATCH_FN,
            value.type_name()
        )));
    };

    let len = table.raw_len();
    let mut out = Vec::with_capacity(len);
    for index in 1..=len {
        let entry: LuaValue = table
            .raw_get(index)
            .map_err(|err| protocol(format!("command #{index}: {err}")))?;
        let kind = command_kind(&entry);
        let fields = command_fields(&entry);
        let decoded = lua
            .from_value_with::<ScriptCommand>(entry, de_options())
            .map_err(|err| protocol(format!("command #{index}{kind}: {err}{fields}")))?;
        out.push(decoded);
    }

    // A dispatch reply with non-array keys is a prelude bug, not a mod's, but
    // silently dropping half the commands would be worse than saying so.
    let extra = table
        .pairs::<LuaValue, LuaValue>()
        .filter_map(Result::ok)
        .filter(|(k, _)| {
            !matches!(k, LuaValue::Integer(i)
                if usize::try_from(*i).is_ok_and(|i| (1..=len).contains(&i)))
        })
        .count();
    if extra > 0 {
        return Err(protocol(format!(
            "{} returned a table with {extra} non-array key(s); expected a 1..n array",
            slotted_script::DISPATCH_FN
        )));
    }

    Ok(out)
}

/// The entry's fields and their Lua types, so a message that says only
/// `expected u32` still tells the modder which key is wrong.
fn command_fields(entry: &LuaValue) -> String {
    let LuaValue::Table(table) = entry else {
        return String::new();
    };
    let mut parts: Vec<String> = table
        .pairs::<LuaValue, LuaValue>()
        .filter_map(Result::ok)
        .filter(|(k, _)| !matches!(k, LuaValue::String(s) if s.to_string_lossy() == "type"))
        .map(|(k, v)| {
            let key = match &k {
                LuaValue::String(s) => s.to_string_lossy(),
                other => format!("{other:?}"),
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

/// `` (type = "sort")`` for an error message, when the entry has one.
fn command_kind(entry: &LuaValue) -> String {
    let LuaValue::Table(table) = entry else {
        return format!(" (a {}, expected a table)", entry.type_name());
    };
    match table.get::<Option<String>>("type") {
        Ok(Some(kind)) => format!(" (type = {kind:?})"),
        _ => " (no `type` field)".to_owned(),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn mod_id() -> ModId {
        ModId::new("test").unwrap()
    }

    fn load(source: &str, stage: Stage) -> (LuaurRuntime, Result<ScriptId, ScriptError>) {
        let mut rt = LuaurRuntime::new(Limits::default());
        let id = rt.load(&mod_id(), "case.lua", source, stage);
        (rt, id)
    }

    #[test]
    fn forbidden_globals_are_absent() {
        let lua =
            LuaurRuntime::sandboxed_state(&mod_id(), Stage::Data, &Limits::default()).unwrap();
        for name in FORBIDDEN_GLOBALS {
            let value: LuaValue = lua.globals().get(*name).unwrap();
            assert!(value.is_nil(), "{name} is still reachable");
        }
    }

    #[test]
    fn compile_errors_are_compile_errors() {
        let (_rt, id) = load("this is not lua", Stage::Data);
        assert!(matches!(id, Err(ScriptError::Compile { .. })), "{id:?}");
    }

    // Everything below needs an error to be *raised* and caught. On wasm
    // luaur raises by panicking and wasm32-unknown-unknown cannot unwind, so
    // each of these would abort the module rather than fail. The cfg is the
    // finding, not a bug to fix: see the crate docs and ADR 0004.
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn a_runtime_error_carries_a_traceback() {
        let mut rt = LuaurRuntime::new(Limits::default());
        let id = rt
            .load(
                &mod_id(),
                "control.lua",
                r#"slotted.on("slot_click", function() error("boom") end)"#,
                Stage::Control,
            )
            .unwrap();
        let err = rt
            .call(
                id,
                &ScriptEvent::SlotClick {
                    menu: slotted_model::MenuId(1),
                    screen: "test:chest".into(),
                    slot: 0,
                    button: slotted_script::Button::Left,
                    modifiers: slotted_script::Modifiers::default(),
                    stack: None,
                },
            )
            .unwrap_err();
        let ScriptError::Runtime {
            message, traceback, ..
        } = &err
        else {
            panic!("expected a runtime error, got {err:?}");
        };
        assert!(message.contains("boom"), "{message}");
        assert!(!traceback.is_empty(), "traceback was empty");
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn the_state_survives_a_handler_error() {
        let mut rt = LuaurRuntime::new(Limits::default());
        let id = rt
            .load(
                &mod_id(),
                "control.lua",
                r#"
                slotted.on("search_changed", function(ev)
                    if ev.text == "boom" then error("no") end
                    return slotted.cmd.log("info", ev.text)
                end)
                "#,
                Stage::Control,
            )
            .unwrap();
        let boom = ScriptEvent::SearchChanged {
            text: "boom".into(),
        };
        let fine = ScriptEvent::SearchChanged { text: "ok".into() };
        assert!(rt.call(id, &boom).is_err());
        let out = rt.call(id, &fine).unwrap();
        assert_eq!(
            out,
            vec![ScriptCommand::Log {
                level: slotted_script::LogLevel::Info,
                message: "ok".into(),
            }]
        );
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn the_state_survives_budget_exhaustion() {
        let mut rt = LuaurRuntime::new(Limits {
            budget: 50_000,
            ..Limits::default()
        });
        let id = rt
            .load(
                &mod_id(),
                "control.lua",
                r#"
                slotted.on("search_changed", function(ev)
                    if ev.text == "spin" then while true do end end
                    return slotted.cmd.log("info", ev.text)
                end)
                "#,
                Stage::Control,
            )
            .unwrap();
        let err = rt
            .call(
                id,
                &ScriptEvent::SearchChanged {
                    text: "spin".into(),
                },
            )
            .unwrap_err();
        assert!(matches!(err, ScriptError::BudgetExceeded { .. }), "{err:?}");
        // Same state, next event: the budget is re-armed per call.
        let out = rt
            .call(id, &ScriptEvent::SearchChanged { text: "ok".into() })
            .unwrap();
        assert_eq!(out.len(), 1);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn a_memory_bomb_hits_the_limit() {
        let mut rt = LuaurRuntime::new(Limits {
            budget: u64::MAX,
            memory_bytes: 4 * 1024 * 1024,
        });
        let err = rt
            .load(
                &mod_id(),
                "data.lua",
                "local t = {} for i = 1, 100000000 do t[i] = string.rep('x', 64) end",
                Stage::Data,
            )
            .unwrap_err();
        assert!(matches!(err, ScriptError::Memory { .. }), "{err:?}");
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn two_states_do_not_share_globals() {
        let mut rt = LuaurRuntime::new(Limits::default());
        let a = rt
            .load(&mod_id(), "a.lua", "shared_marker = 1", Stage::Data)
            .unwrap();
        let b = rt
            .load(
                &ModId::new("other").unwrap(),
                "b.lua",
                r#"
                if shared_marker ~= nil then error("leaked") end
                slotted.info("mod is %s", slotted.mod_id)
                "#,
                Stage::Data,
            )
            .unwrap();
        assert!(
            rt.call(a, &ScriptEvent::DataStage { api_version: 1 })
                .is_ok()
        );
        let out = rt
            .call(b, &ScriptEvent::DataStage { api_version: 1 })
            .unwrap();
        assert_eq!(
            out,
            vec![ScriptCommand::Log {
                level: slotted_script::LogLevel::Info,
                message: "mod is other".into(),
            }]
        );
    }

    #[test]
    fn unknown_ids_and_unload() {
        let mut rt = LuaurRuntime::new(Limits::default());
        let id = rt.load(&mod_id(), "a.lua", "", Stage::Data).unwrap();
        assert_eq!(rt.len(), 1);
        rt.unload(id);
        assert!(rt.is_empty());
        let err = rt
            .call(id, &ScriptEvent::DataStage { api_version: 1 })
            .unwrap_err();
        assert_eq!(err, ScriptError::UnknownScript(id));
        rt.unload(id);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn a_malformed_command_names_its_position_and_type() {
        let mut rt = LuaurRuntime::new(Limits::default());
        let id = rt
            .load(
                &mod_id(),
                "control.lua",
                r#"
                slotted.on("search_changed", function()
                    return { { type = "sort", menu = "not a number", inventory = 0 } }
                end)
                "#,
                Stage::Control,
            )
            .unwrap();
        let err = rt
            .call(id, &ScriptEvent::SearchChanged { text: "x".into() })
            .unwrap_err();
        let ScriptError::Protocol { message, .. } = &err else {
            panic!("expected a protocol error, got {err:?}");
        };
        assert!(message.contains("command #1"), "{message}");
        assert!(message.contains("sort"), "{message}");
        assert!(message.contains("menu"), "{message}");
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn a_wrong_argument_names_the_function_that_rejected_it() {
        let (_rt, id) = load(
            r#"slotted.register_item("apple", "not a table")"#,
            Stage::Data,
        );
        let err = id.unwrap_err();
        let ScriptError::Runtime { message, .. } = &err else {
            panic!("expected a runtime error, got {err:?}");
        };
        assert!(message.contains("def must be a table"), "{message}");

        let (_rt, id) = load(r#"slotted.inject("slotted:chest", {})"#, Stage::Data);
        let err = id.unwrap_err();
        let ScriptError::Runtime { message, .. } = &err else {
            panic!("expected a runtime error, got {err:?}");
        };
        assert!(message.contains("slotted.inject"), "{message}");
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn a_slot_click_round_trip_is_cheap() {
        let mut rt = LuaurRuntime::new(Limits::default());
        let id = rt
            .load(
                &mod_id(),
                "control.lua",
                r#"
                slotted.on("slot_click", function(ev)
                    if ev.modifiers.alt then
                        return slotted.cmd.toggle_favorite(ev.menu, ev.slot)
                    end
                end)
                "#,
                Stage::Control,
            )
            .unwrap();
        let event = ScriptEvent::SlotClick {
            menu: slotted_model::MenuId(1),
            screen: "test:chest".into(),
            slot: 3,
            button: slotted_script::Button::Left,
            modifiers: slotted_script::Modifiers {
                alt: true,
                ..slotted_script::Modifiers::default()
            },
            stack: Some(slotted_script::StackInfo {
                item: "demo:apple".into(),
                count: 3,
                components: slotted_model::Value::Map(std::collections::BTreeMap::new()),
            }),
        };
        // Warm the state, then time a run long enough to be stable.
        for _ in 0..1_000 {
            rt.call(id, &event).unwrap();
        }
        let runs = 20_000;
        let start = std::time::Instant::now();
        for _ in 0..runs {
            rt.call(id, &event).unwrap();
        }
        let per_call = start.elapsed() / runs;
        println!("slot_click round trip: {per_call:?} per call");
        // Generous: a debug build on a loaded machine, not a benchmark. The
        // point is that a per-click hook is not milliseconds.
        assert!(
            per_call < std::time::Duration::from_micros(200),
            "a slot_click round trip took {per_call:?}"
        );
    }

    // -----------------------------------------------------------------
    // Native error handling.
    //
    // luaur raises a Lua error by panicking and catches it with
    // `catch_unwind`, so on a target that can unwind, every one of these
    // comes back as a value and leaves the state usable. That is the whole
    // native contract and it is the half that `wasm32` does not have
    // (see the crate docs), so it is pinned here case by case rather than
    // assumed.
    // -----------------------------------------------------------------

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn an_explicit_error_comes_back_as_a_value() {
        let mut rt = LuaurRuntime::new(Limits::default());
        let err = rt
            .load(&mod_id(), "data.lua", r#"error("boom")"#, Stage::Data)
            .unwrap_err();
        let ScriptError::Runtime { message, .. } = &err else {
            panic!("expected a runtime error, got {err:?}");
        };
        assert!(message.contains("boom"), "{message}");
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn a_runtime_type_error_comes_back_as_a_value() {
        let mut rt = LuaurRuntime::new(Limits::default());
        let err = rt
            .load(
                &mod_id(),
                "data.lua",
                "local x = nil\nlocal y = x + 1\n",
                Stage::Data,
            )
            .unwrap_err();
        assert!(matches!(err, ScriptError::Runtime { .. }), "{err:?}");
    }

    /// `pcall` inside a script contains the error and the script carries on.
    /// This is the one behaviour a mod author can rely on natively and cannot
    /// rely on in a browser, so the modding guide says so and this pins it.
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn pcall_contains_an_error_inside_the_vm() {
        let mut rt = LuaurRuntime::new(Limits::default());
        let id = rt
            .load(
                &mod_id(),
                "control.lua",
                r#"
                slotted.on("search_changed", function()
                    local ok, why = pcall(function() error("inner") end)
                    local typed = pcall(function() local t = nil return t.missing end)
                    return slotted.cmd.log(
                        "info",
                        tostring(ok) .. ":" .. tostring(typed) .. ":" .. tostring(why)
                    )
                end)
                "#,
                Stage::Control,
            )
            .unwrap();
        let out = rt
            .call(id, &ScriptEvent::SearchChanged { text: "x".into() })
            .unwrap();
        let [ScriptCommand::Log { message, .. }] = out.as_slice() else {
            panic!("expected one log, got {out:?}");
        };
        assert!(message.starts_with("false:false:"), "{message}");
        assert!(message.contains("inner"), "{message}");
    }

    /// Every error class in turn, against one state, each followed by an event
    /// the state must still answer. A runtime that leaked a poisoned VM would
    /// fail on the call after the first.
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn the_state_is_usable_after_every_class_of_error() {
        let mut rt = LuaurRuntime::new(Limits {
            budget: 50_000,
            memory_bytes: 8 * 1024 * 1024,
        });
        let id = rt
            .load(
                &mod_id(),
                "control.lua",
                r#"
                slotted.on("search_changed", function(ev)
                    if ev.text == "raise" then error("boom") end
                    if ev.text == "typeerror" then local t = nil return t.missing end
                    if ev.text == "spin" then while true do end end
                    if ev.text == "bomb" then
                        local held = {}
                        for i = 1, 100000 do held[i] = string.rep(tostring(i) .. "y", 100000) end
                    end
                    return slotted.cmd.log("info", ev.text)
                end)
                "#,
                Stage::Control,
            )
            .unwrap();

        let ok = |rt: &mut LuaurRuntime| {
            let out = rt
                .call(id, &ScriptEvent::SearchChanged { text: "ok".into() })
                .expect("the state answers after an error");
            assert_eq!(
                out,
                vec![ScriptCommand::Log {
                    level: slotted_script::LogLevel::Info,
                    message: "ok".into(),
                }]
            );
        };

        ok(&mut rt);
        for (text, expected) in [
            ("raise", "runtime"),
            ("typeerror", "runtime"),
            ("spin", "budget"),
            ("bomb", "memory"),
        ] {
            let err = rt
                .call(
                    id,
                    &ScriptEvent::SearchChanged {
                        text: (*text).into(),
                    },
                )
                .unwrap_err();
            let actual = match &err {
                ScriptError::Runtime { .. } => "runtime",
                ScriptError::BudgetExceeded { .. } => "budget",
                ScriptError::Memory { .. } => "memory",
                other => panic!("{text}: unexpected {other:?}"),
            };
            assert_eq!(actual, expected, "{text} was classified wrong: {err:?}");
            ok(&mut rt);
        }
    }

    // -----------------------------------------------------------------
    // The sandbox order, which is silent when it is wrong.
    // -----------------------------------------------------------------

    /// A name cannot be *removed* after sealing, and the attempt reports
    /// success. `sandbox(true)` installs a proxy global table whose `__index`
    /// falls through to the real environment, so setting a name to `nil` makes
    /// it absent from the proxy and the lookup finds the original anyway. An
    /// adapter that erased the forbidden globals after sealing would see
    /// `Ok(())` from every call and ship a sandbox with `io` in it, which is
    /// why `build_state` erases first.
    ///
    /// Adding a name after sealing does take effect. That asymmetry is what
    /// makes the failure quiet, so both halves are pinned here: if luaur ever
    /// starts refusing the write, or starts honouring the removal, this fails
    /// and the ordering note above needs rewriting.
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn a_global_cannot_be_removed_after_sealing_and_the_attempt_reports_success() {
        let lua =
            LuaurRuntime::sandboxed_state(&mod_id(), Stage::Data, &Limits::default()).unwrap();

        let removed = lua.globals().set("string", LuaValue::Nil);
        assert!(
            removed.is_ok(),
            "the removal reported an error, so the ordering note is stale: {removed:?}"
        );
        let still_there: LuaValue = lua.load("return string").eval().unwrap();
        assert!(
            still_there.is_table(),
            "the post-seal removal took effect, so erasing after sealing would now be safe"
        );

        // The other half: an addition does land, in the proxy, and the script
        // sees it. This is why a removal returning `Ok(())` looks convincing.
        lua.globals().set("late_marker", "host wrote me").unwrap();
        let seen: String = lua.load("return late_marker").eval().unwrap();
        assert_eq!(seen, "host wrote me");
    }

    /// `StdLib` flags do not select libraries: any non-empty set opens the
    /// whole safe stdlib. Nothing may be kept out by leaving its flag off, so
    /// `FORBIDDEN_GLOBALS` is the only defence and the conformance suite's
    /// `sandbox_globals` case is what enforces it.
    #[test]
    fn stdlib_flags_do_not_narrow_the_library_set() {
        let lua = Lua::new_with(StdLib::MATH, LuaOptions::new()).unwrap();
        for present in ["string", "table", "math", "coroutine", "os"] {
            let value: LuaValue = lua.globals().get(present).unwrap();
            assert!(
                !value.is_nil(),
                "{present} was absent, so `StdLib` flags now select libraries"
            );
        }
        let empty = Lua::new_with(StdLib::NONE, LuaOptions::new()).unwrap();
        let value: LuaValue = empty.globals().get("string").unwrap();
        assert!(value.is_nil(), "StdLib::NONE opened the stdlib anyway");
    }

    /// The sealed state freezes the library tables, so a mod cannot hand the
    /// next one a different `string.format`.
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn the_stdlib_is_frozen_in_a_sealed_state() {
        let (_rt, id) = load(
            "string.format = function() return 'hijacked' end",
            Stage::Data,
        );
        let err = id.unwrap_err();
        assert!(matches!(err, ScriptError::Sandbox { .. }), "{err:?}");
    }

    /// The reporter is the wasm survival kit: on the web this callback is the
    /// only thing that runs between a mod raising and the module trapping.
    /// `REPORTER` is process-wide and set once, so this is the one test that
    /// installs one, and it only asserts that its own error turned up.
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn the_error_reporter_sees_a_raise_with_its_script_and_traceback() {
        static SEEN: Mutex<Vec<RaisedError>> = Mutex::new(Vec::new());
        install_error_reporter(|raised| {
            if let Ok(mut seen) = SEEN.lock() {
                seen.push(raised.clone());
            }
        });

        let mut rt = LuaurRuntime::new(Limits::default());
        let id = rt
            .load(
                &mod_id(),
                "reporter.lua",
                r#"slotted.on("search_changed", function() error("reported boom") end)"#,
                Stage::Control,
            )
            .unwrap();
        assert!(
            rt.call(id, &ScriptEvent::SearchChanged { text: "x".into() })
                .is_err()
        );

        let seen = SEEN.lock().unwrap();
        let mine = seen
            .iter()
            .find(|r| r.message.contains("reported boom"))
            .expect("the reporter never saw the raise");
        assert_eq!(mine.script, "test/reporter.lua");
        assert!(!mine.traceback.is_empty(), "the traceback was empty");
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn set_limits_reaches_a_loaded_state() {
        let mut rt = LuaurRuntime::new(Limits::default());
        let id = rt
            .load(
                &mod_id(),
                "control.lua",
                r#"slotted.on("search_changed", function() while true do end end)"#,
                Stage::Control,
            )
            .unwrap();
        rt.set_limits(Limits {
            budget: 1_000,
            memory_bytes: 64 * 1024 * 1024,
        });
        let err = rt
            .call(id, &ScriptEvent::SearchChanged { text: "x".into() })
            .unwrap_err();
        assert!(matches!(err, ScriptError::BudgetExceeded { .. }), "{err:?}");
    }
}
