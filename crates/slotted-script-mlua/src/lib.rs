//! Luau through `mlua`: the native [`ScriptRuntime`] adapter (ADR 0001).
//!
//! One `Lua` state per loaded script. Each state is built with the `table`,
//! `string`, `math`, `bit32` and `utf8` libraries, has every
//! [`slotted_script::FORBIDDEN_GLOBALS`] entry removed, and is then sealed
//! with `sandbox(true)`; the spike found that a write to `globals()` after
//! sealing is silently ignored, so the order matters. Budgets are interrupt
//! ticks counted per `call`, memory is `set_memory_limit`, and values cross
//! through `LuaSerdeExt`. Contract section 1.6.

use std::collections::BTreeMap;

use mlua::Lua;
use slotted_script::{
    Limits, ModId, ScriptCommand, ScriptError, ScriptEvent, ScriptId, ScriptRuntime, Stage,
};

/// Message prefix the interrupt raises with; the adapter maps it to
/// [`ScriptError::BudgetExceeded`].
pub const BUDGET_MARKER: &str = "slotted:budget";

/// One loaded script.
struct Loaded {
    /// `<mod>/<file>`, for error messages.
    name: String,
    /// The sealed state.
    #[allow(dead_code)]
    lua: Lua,
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
    /// # Errors
    ///
    /// [`ScriptError::Sandbox`] if a state cannot be built.
    pub fn sandboxed_state(
        _mod_id: &ModId,
        _stage: Stage,
        _limits: &Limits,
    ) -> Result<Lua, ScriptError> {
        // PHASE4-IMPL: A -- Lua::new_with(TABLE|STRING|MATH|BIT|UTF8), remove
        // FORBIDDEN_GLOBALS, set __slotted_mod_id/__slotted_stage/
        // __slotted_api_version, exec PRELUDE, set_memory_limit, sandbox(true).
        Err(ScriptError::Sandbox {
            name: String::new(),
            message: "not implemented".to_owned(),
        })
    }
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
        _source: &str,
        stage: Stage,
    ) -> Result<ScriptId, ScriptError> {
        let full = format!("{mod_id}/{name}");
        let lua = Self::sandboxed_state(mod_id, stage, &self.limits)?;
        // PHASE4-IMPL: A -- lua.load(source).set_name(&full).exec(), mapping
        // SyntaxError -> Compile and RuntimeError -> Runtime with traceback.
        let id = ScriptId(self.next);
        self.next += 1;
        self.scripts.insert(id, Loaded { name: full, lua });
        Ok(id)
    }

    fn unload(&mut self, id: ScriptId) {
        self.scripts.remove(&id);
    }

    fn call(
        &mut self,
        id: ScriptId,
        _event: &ScriptEvent,
    ) -> Result<Vec<ScriptCommand>, ScriptError> {
        let loaded = self
            .scripts
            .get(&id)
            .ok_or(ScriptError::UnknownScript(id))?;
        // PHASE4-IMPL: A -- reset the tick counter, set_interrupt with the
        // budget, to_value_with(event), call __slotted_dispatch, from_value_with
        // (encode_empty_tables_as_array) into Vec<ScriptCommand>; map errors.
        let _ = &loaded.name;
        Ok(Vec::new())
    }

    fn set_limits(&mut self, limits: Limits) {
        self.limits = limits;
        // PHASE4-IMPL: A -- apply set_memory_limit to every loaded state.
    }
}
