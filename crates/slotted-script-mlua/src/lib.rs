//! Luau through `mlua`: the native [`ScriptRuntime`] adapter (ADR 0001).
//!
//! One `Lua` state per loaded script. Each state is built with the `table`,
//! `string`, `math`, `bit32` and `utf8` libraries, has every
//! [`slotted_script::FORBIDDEN_GLOBALS`] entry removed, and is then sealed
//! with `sandbox(true)`; the spike found that a write to `globals()` after
//! sealing is silently ignored, so the order matters. Budgets are interrupt
//! ticks counted per `call`, memory is `set_memory_limit`, and values cross
//! through `LuaSerdeExt`. Contract section 1.6.
//!
//! ```
//! use slotted_script::{Limits, ModId, ScriptEvent, ScriptRuntime, Stage};
//! use slotted_script_mlua::MluaRuntime;
//!
//! let mut runtime = MluaRuntime::new(Limits::default());
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

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use mlua::{
    Function, Lua, LuaOptions, LuaSerdeExt, StdLib, Value as LuaValue, VmState,
    serde::de::Options as DeOptions, serde::ser::Options as SerOptions,
};
use slotted_script::{
    API_VERSION, FORBIDDEN_GLOBALS, Limits, ModId, PRELUDE, ScriptCommand, ScriptError,
    ScriptEvent, ScriptId, ScriptRuntime, Stage, TEST_PRELUDE,
};

/// Message prefix the interrupt raises with; the adapter maps it to
/// [`ScriptError::BudgetExceeded`].
pub const BUDGET_MARKER: &str = "slotted:budget";

/// Per-state counters the interrupt and the message handler share with the
/// runtime. Stored in the state's app data so [`MluaRuntime::sandboxed_state`]
/// can keep returning a bare [`Lua`].
#[derive(Clone)]
struct Hooks {
    /// Interrupt ticks consumed by the call in flight.
    ticks: Arc<AtomicU64>,
    /// Ticks the call in flight may consume.
    budget: Arc<AtomicU64>,
    /// Traceback captured by the `xpcall` message handler, if any.
    traceback: Arc<Mutex<String>>,
}

impl Hooks {
    fn new(budget: u64) -> Self {
        Self {
            ticks: Arc::new(AtomicU64::new(0)),
            budget: Arc::new(AtomicU64::new(budget)),
            traceback: Arc::new(Mutex::new(String::new())),
        }
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
pub struct MluaRuntime {
    limits: Limits,
    next: u32,
    scripts: BTreeMap<ScriptId, Loaded>,
}

impl MluaRuntime {
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

    /// Builds a sealed state with the prelude installed and the three
    /// `__slotted_*` globals set. Public so the conformance suite can assert
    /// the sandbox directly.
    ///
    /// The order is load-bearing: libraries, then the forbidden globals are
    /// erased, then the prelude runs (it writes `_G.slotted` and `print`),
    /// then `sandbox(true)` freezes everything. After sealing, a write to
    /// `globals()` is silently dropped, so nothing may be installed later.
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

    fn build_state(mod_id: &ModId, stage: Stage, limits: &Limits) -> mlua::Result<Lua> {
        let mut libs = StdLib::TABLE | StdLib::STRING | StdLib::MATH | StdLib::BIT | StdLib::UTF8;
        // A test body is a coroutine (Phase 6 contract 3.1), so the test
        // stage is the one stage that needs `coroutine`. Data and control
        // scripts keep the smaller surface they have always had; piccolo
        // installs coroutine unconditionally, which is the one place the two
        // sandboxes differ and it is invisible to a mod.
        if stage == Stage::Test {
            libs |= StdLib::COROUTINE;
        }
        let lua = Lua::new_with(libs, LuaOptions::new())?;

        {
            let globals = lua.globals();
            // mlua injects `require` and `loadstring` of its own even when the
            // matching `StdLib` flag is off, so the list is walked by hand.
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
        // traceback and returns the error value unchanged, leaving mlua's own
        // error mapping intact.
        {
            let sink = hooks.traceback.clone();
            let handler = lua.create_function(move |lua, err: LuaValue| {
                if let Ok(mut slot) = sink.lock() {
                    *slot = lua
                        .traceback(None, 1)
                        .map(|s| s.to_string_lossy())
                        .unwrap_or_default();
                }
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
                    Err(mlua::Error::runtime(BUDGET_MARKER))
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

    /// Runs `f` with the budget armed, mapping whatever it raises.
    fn classify(name: &str, hooks: &Hooks, err: &mlua::Error) -> ScriptError {
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
        if let mlua::Error::SyntaxError { message, .. } = err {
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
            traceback = mlua_traceback(err);
        }
        ScriptError::Runtime {
            name: name.to_owned(),
            message: text,
            traceback,
        }
    }
}

/// Whether `err`, anywhere in its chain, is the interrupt's budget signal.
fn is_budget(err: &mlua::Error) -> bool {
    err.to_string().contains(BUDGET_MARKER)
}

/// Whether `err` is an out-of-memory condition. Luau raises `not enough
/// memory` for an allocation the limit refused; mlua reports the same as
/// [`mlua::Error::MemoryError`] when it escapes uncaught.
fn is_memory(err: &mlua::Error) -> bool {
    fn walk(err: &mlua::Error) -> bool {
        match err {
            mlua::Error::MemoryError(_) => true,
            mlua::Error::CallbackError { cause, .. } | mlua::Error::WithContext { cause, .. } => {
                walk(cause)
            }
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

/// The traceback mlua itself carries, if the error crossed a Rust callback.
fn mlua_traceback(err: &mlua::Error) -> String {
    match err {
        mlua::Error::CallbackError { traceback, .. } => traceback.clone(),
        mlua::Error::WithContext { cause, .. } => mlua_traceback(cause),
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

impl Default for MluaRuntime {
    fn default() -> Self {
        Self::new(Limits::default())
    }
}

impl ScriptRuntime for MluaRuntime {
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

        hooks.arm(self.limits.budget);
        let chunk_name = format!("@{full}");
        if let Err(err) = lua.load(source).set_name(&chunk_name).exec() {
            return Err(MluaRuntime::classify(&full, &hooks, &err));
        }

        let entry = (|| -> mlua::Result<Entry> {
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
        self.scripts.insert(
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
        self.scripts.remove(&id);
    }

    fn call(
        &mut self,
        id: ScriptId,
        event: &ScriptEvent,
    ) -> Result<Vec<ScriptCommand>, ScriptError> {
        let loaded = self
            .scripts
            .get(&id)
            .ok_or(ScriptError::UnknownScript(id))?;
        let name = loaded.name.clone();
        let lua = &loaded.lua;
        let hooks = &loaded.hooks;

        hooks.arm(self.limits.budget);

        let entry = &loaded.entry;
        let outcome = (|| -> mlua::Result<(bool, LuaValue)> {
            let arg = lua.to_value_with(event, ser_options())?;
            entry
                .xpcall
                .call::<(bool, LuaValue)>((&entry.dispatch, &entry.handler, arg))
        })();

        let (ok, value) = match outcome {
            Ok(pair) => pair,
            Err(err) => return Err(MluaRuntime::classify(&name, hooks, &err)),
        };
        if !ok {
            let err = mlua::Error::runtime(lua_error_text(lua, &value));
            return Err(MluaRuntime::classify(&name, hooks, &err));
        }

        decode_commands(lua, &name, &value)
    }

    fn set_limits(&mut self, limits: Limits) {
        self.limits = limits;
        for loaded in self.scripts.values() {
            loaded.hooks.budget.store(limits.budget, Ordering::Relaxed);
            // A limit below what the state already holds is refused by mlua;
            // that is the caller raising the ceiling later, not an error here.
            let _ = loaded.lua.set_memory_limit(limits.memory_bytes);
        }
    }
}

/// The text of a Lua error value caught by `xpcall`.
fn lua_error_text(lua: &Lua, value: &LuaValue) -> String {
    match value {
        LuaValue::String(s) => s.to_string_lossy(),
        other => lua
            .coerce_string(other.clone())
            .ok()
            .flatten()
            .map_or_else(|| format!("{other:?}"), |s| s.to_string_lossy()),
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
    if let LuaValue::Table(t) = value {
        let extra = t
            .clone()
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
        .clone()
        .pairs::<LuaValue, LuaValue>()
        .filter_map(Result::ok)
        .filter(|(k, _)| !matches!(k, LuaValue::String(s) if s == "type"))
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

    fn load(source: &str, stage: Stage) -> (MluaRuntime, Result<ScriptId, ScriptError>) {
        let mut rt = MluaRuntime::new(Limits::default());
        let id = rt.load(&mod_id(), "case.lua", source, stage);
        (rt, id)
    }

    #[test]
    fn forbidden_globals_are_absent() {
        let lua = MluaRuntime::sandboxed_state(&mod_id(), Stage::Data, &Limits::default()).unwrap();
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

    #[test]
    fn a_runtime_error_carries_a_traceback() {
        let mut rt = MluaRuntime::new(Limits::default());
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

    #[test]
    fn the_state_survives_a_handler_error() {
        let mut rt = MluaRuntime::new(Limits::default());
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

    #[test]
    fn the_state_survives_budget_exhaustion() {
        let mut rt = MluaRuntime::new(Limits {
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

    #[test]
    fn a_memory_bomb_hits_the_limit() {
        let mut rt = MluaRuntime::new(Limits {
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

    #[test]
    fn two_states_do_not_share_globals() {
        let mut rt = MluaRuntime::new(Limits::default());
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
        let mut rt = MluaRuntime::new(Limits::default());
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

    #[test]
    fn a_malformed_command_names_its_position_and_type() {
        let mut rt = MluaRuntime::new(Limits::default());
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

    #[test]
    fn a_slot_click_round_trip_is_cheap() {
        let mut rt = MluaRuntime::new(Limits::default());
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
                components: slotted_model::Value::Map(BTreeMap::new()),
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

    #[test]
    fn set_limits_reaches_a_loaded_state() {
        let mut rt = MluaRuntime::new(Limits::default());
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
