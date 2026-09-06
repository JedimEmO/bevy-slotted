//! The [`ScriptRuntime`] port and its error type.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::{ScriptCommand, ScriptEvent};

/// Handle to one loaded script inside a runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ScriptId(pub u32);

/// Which half of the Factorio-style split a script belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    /// Registries are open; the script may only register.
    Data,
    /// Registries are frozen; the script may only react.
    Control,
    /// A mod's `tests/*.lua` under `slotted-test` (Phase 6). The prelude and
    /// [`crate::TEST_PRELUDE`] are installed; registration raises.
    Test,
}

impl Stage {
    /// The string `slotted.stage` holds.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Data => "data",
            Self::Control => "control",
            Self::Test => "test",
        }
    }
}

/// A mod identifier: `[a-z0-9_.-]+`, the same rule as the registry's `ModId`.
/// Duplicated here so this crate does not depend on `slotted-registry`;
/// `slotted-packs` converts between the two.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ModId(String);

/// Why a string is not a [`ModId`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("`{0}` is not a mod id: use lowercase letters, digits, `_`, `.` and `-`")]
pub struct ModIdError(String);

impl ModId {
    /// Validates `raw`.
    ///
    /// # Errors
    ///
    /// [`ModIdError`] when a character is outside `[a-z0-9_.-]` or the string
    /// is empty.
    pub fn new(raw: impl Into<String>) -> Result<Self, ModIdError> {
        let raw = raw.into();
        let ok = !raw.is_empty()
            && raw.bytes().all(|b| {
                b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'_' | b'.' | b'-')
            });
        if ok {
            Ok(Self(raw))
        } else {
            Err(ModIdError(raw))
        }
    }

    /// The id as text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ModId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Resource limits for one script state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// Interrupt ticks one [`ScriptRuntime::call`] may consume. Luau raises an
    /// interrupt roughly once per function call or loop back-edge, so this is
    /// not an instruction count; the default is generous for UI hooks and far
    /// too small for an infinite loop.
    pub budget: u64,
    /// Bytes one state may allocate.
    pub memory_bytes: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            budget: 1_000_000,
            memory_bytes: 64 * 1024 * 1024,
        }
    }
}

/// What went wrong inside a runtime. Every variant names the script so a
/// console line can say which mod failed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ScriptError {
    /// The chunk did not parse.
    #[error("{name}: compile error: {message}")]
    Compile {
        /// `<mod>/<file>`.
        name: String,
        /// The parser's message.
        message: String,
    },
    /// A Lua error at run time, including `error()` from a handler.
    #[error("{name}: {message}")]
    Runtime {
        /// `<mod>/<file>`.
        name: String,
        /// The error value, as text.
        message: String,
        /// The Lua traceback, possibly empty.
        traceback: String,
    },
    /// The interrupt budget ran out during one call.
    #[error("{name}: budget exceeded")]
    BudgetExceeded {
        /// `<mod>/<file>`.
        name: String,
    },
    /// The state hit its memory limit.
    #[error("{name}: memory limit reached")]
    Memory {
        /// `<mod>/<file>`.
        name: String,
    },
    /// The script reached for something the sandbox forbids.
    #[error("{name}: sandbox violation: {message}")]
    Sandbox {
        /// `<mod>/<file>`.
        name: String,
        /// What it tried.
        message: String,
    },
    /// The dispatch function returned something that is not an array of
    /// command tables, or a table that does not deserialise.
    #[error("{name}: protocol error: {message}")]
    Protocol {
        /// `<mod>/<file>`.
        name: String,
        /// The serde error.
        message: String,
    },
    /// No script with that id is loaded.
    #[error("no script with id {0:?}")]
    UnknownScript(ScriptId),
}

/// The port every scripting backend implements. `docs/PLAN.md` 4.8.
///
/// One state per script. `load` installs the prelude, executes the chunk and
/// hands back a handle; `call` delivers one event through `__slotted_dispatch`
/// and returns whatever commands came back. The host, not the runtime, decides
/// what a command means.
pub trait ScriptRuntime: Send + Sync {
    /// Compiles and runs `source` as `name` for `mod_id` at `stage`.
    ///
    /// # Errors
    ///
    /// [`ScriptError::Compile`] or any error the chunk raised while running;
    /// a failed chunk is not registered.
    fn load(
        &mut self,
        mod_id: &ModId,
        name: &str,
        source: &str,
        stage: Stage,
    ) -> Result<ScriptId, ScriptError>;

    /// Drops the state. Unknown ids are ignored.
    fn unload(&mut self, id: ScriptId);

    /// Delivers `event` and collects the commands.
    ///
    /// # Errors
    ///
    /// Any [`ScriptError`]; a budget or memory error leaves the state loaded.
    fn call(
        &mut self,
        id: ScriptId,
        event: &ScriptEvent,
    ) -> Result<Vec<ScriptCommand>, ScriptError>;

    /// Applies `limits` to every state, now and for future loads.
    fn set_limits(&mut self, limits: Limits);
}
