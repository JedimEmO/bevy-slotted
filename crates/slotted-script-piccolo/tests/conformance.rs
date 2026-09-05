//! The ADR 0001 gate: every `conformance/*.lua` case passes on this adapter
//! too, which is what makes the web runtime interchangeable with the native
//! one.

#![cfg(not(target_arch = "wasm32"))]

use slotted_script::Limits;
use slotted_script_piccolo::PiccoloRuntime;

#[test]
fn piccolo_passes_the_conformance_suite() {
    let mut runtime = PiccoloRuntime::new(Limits::default());
    slotted_testutils::conformance::assert_conformance(&mut runtime);
}
