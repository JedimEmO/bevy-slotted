//! The scripting port of slotted, and the API surface every mod sees.
//!
//! This crate is deliberately small and dependency-light: `serde`,
//! `thiserror` and `slotted-model`. It defines
//!
//! - the [`ScriptRuntime`] port an adapter (`slotted-script-luaur`, on every
//!   target) implements,
//! - the [`ScriptEvent`]s the host sends and the [`ScriptCommand`]s a script
//!   answers with, both plain serde enums,
//! - the untagged serde form of [`slotted_model::Value`] Lua tables map onto
//!   ([`value::untagged`]),
//! - the shared Lua prelude ([`PRELUDE`]), so every adapter exposes the same
//!   `slotted.*` functions and the same dispatch protocol.
//!
//! Scripts never touch state. They receive an event and return commands; the
//! host validates and applies them. See `docs/design/phase4-contract.md`
//! section 1.

pub mod api;
pub mod commands;
pub mod events;
pub mod testing;
pub mod value;

pub use api::{Limits, ModId, ScriptError, ScriptId, ScriptRuntime, Stage};
pub use commands::{LogLevel, ScriptCommand, TierFilter, TooltipFilter};
pub use events::{Button, LookupMode, Modifiers, ScriptEvent, StackInfo, Tier};
pub use testing::{TestLocator, TestOp};

/// The `slotted.test` module, installed after [`PRELUDE`] for
/// [`Stage::Test`] scripts only. Phase 6 contract section 3.1 describes the
/// coroutine-driven `test_run` / `test_step` / `test_resume` protocol.
pub const TEST_PRELUDE: &str = include_str!("../prelude/slotted_test.lua");

/// The `slotted.*` Lua prelude, installed into every script state before the
/// mod's chunk runs. Contract section 1.4 describes what it exposes and the
/// `__slotted_dispatch` protocol between it and the host.
pub const PRELUDE: &str = include_str!("../prelude/slotted.lua");

/// The version of the `slotted.*` API this prelude speaks. A mod declares the
/// version it was written against in `mod.toml`.
pub const API_VERSION: u32 = 1;

/// The global the host calls to deliver an event.
pub const DISPATCH_FN: &str = "__slotted_dispatch";

/// Globals a sandboxed state must not have. Removed by every adapter before
/// the sandbox is sealed and asserted absent by the conformance suite.
pub const FORBIDDEN_GLOBALS: &[&str] = &[
    "io",
    "os",
    "package",
    "require",
    "dofile",
    "loadfile",
    "load",
    "loadstring",
    "debug",
    "collectgarbage",
    "getfenv",
    "setfenv",
    "newproxy",
];
