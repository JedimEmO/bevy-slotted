//! The port's contract: every `conformance/*.lua` case passes on the runtime
//! the workspace ships. The suite is generic over `&mut dyn ScriptRuntime`, so
//! this is the case the workspace runs and a game with its own runtime runs
//! the same one.
//!
//! Native only. The suite reads its cases off the filesystem, which
//! `wasm32-unknown-unknown` does not have, and several cases expect an error
//! that on wasm aborts the module instead of returning. `tests/wasm.rs`
//! inlines the browser half.

#![cfg(not(target_arch = "wasm32"))]

use slotted_script::Limits;
use slotted_script_luaur::LuaurRuntime;

#[test]
fn luaur_passes_the_conformance_suite() {
    let mut runtime = LuaurRuntime::new(Limits::default());
    slotted_testutils::conformance::assert_conformance(&mut runtime);
}
