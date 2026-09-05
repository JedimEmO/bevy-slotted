//! The ADR 0001 gate: every `conformance/*.lua` case passes on this adapter.

use slotted_script::Limits;
use slotted_script_mlua::MluaRuntime;

#[test]
fn mlua_passes_the_conformance_suite() {
    let mut runtime = MluaRuntime::new(Limits::default());
    slotted_testutils::conformance::assert_conformance(&mut runtime);
}
