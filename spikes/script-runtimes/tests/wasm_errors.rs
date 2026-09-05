//! Does a Lua runtime error come back as `Err`, or does it take the whole
//! module down?
//!
//! RESULT: luaur raises Lua errors through a Rust panic (`luaD_throw` ->
//! `panic_any`) and catches them with `catch_unwind`. `wasm32-unknown-unknown`
//! has no unwinding, so on the web every one of these aborts the module. Even
//! an in-VM `pcall` aborts, because luaur implements `pcall` the same way.
//! Only a *compile* error is recoverable in the browser, because that path
//! never enters the VM.
//!
//! The tests below are therefore native-only except `compile_error...`. Do not
//! "fix" them by deleting the cfg: the cfg IS the finding. See README.md.

use luaur::Lua;
#[cfg(not(target_arch = "wasm32"))]
use luaur::Value;

#[cfg(target_arch = "wasm32")]
use wasm_bindgen_test::{wasm_bindgen_test as test_case, wasm_bindgen_test_configure};
#[cfg(target_arch = "wasm32")]
wasm_bindgen_test_configure!(run_in_browser);
#[cfg(not(target_arch = "wasm32"))]
use std::prelude::v1::test as test_case;

#[cfg(not(target_arch = "wasm32"))]
#[test_case]
fn explicit_error_is_recoverable() {
    let lua = Lua::new();
    let res = lua.load("error('boom')").exec();
    assert!(res.is_err(), "error() should surface as Err");
}

#[cfg(not(target_arch = "wasm32"))]
#[test_case]
fn arithmetic_on_nil_is_recoverable() {
    let lua = Lua::new();
    let res: Result<Value, _> = lua.load("local x = nil; return x + 1").eval();
    assert!(res.is_err(), "runtime type error should surface as Err");
}

#[test_case]
fn compile_error_is_recoverable() {
    let lua = Lua::new();
    let res = lua.load("this is not lua ((").exec();
    assert!(res.is_err(), "syntax error should surface as Err");
}

#[cfg(not(target_arch = "wasm32"))]
#[test_case]
fn state_is_usable_after_a_caught_error() {
    let lua = Lua::new();
    let _ = lua.load("error('boom')").exec();
    let v: f64 = lua.load("return 1 + 1").eval().unwrap();
    assert_eq!(v, 2.0);
}

/// The candidate workaround: the shared Lua prelude wraps every handler in
/// `pcall`, so a script error is caught inside the VM and never reaches the
/// Rust boundary where luaur raises a panic.
#[cfg(not(target_arch = "wasm32"))]
#[test_case]
fn pcall_contains_the_error_inside_the_vm() {
    let lua = Lua::new();
    let (ok, msg): (bool, String) = lua
        .load("local ok, err = pcall(function() error('boom') end) return ok, tostring(err)")
        .eval()
        .expect("pcall itself must not fail");
    assert!(!ok);
    assert!(msg.contains("boom"), "got {msg}");
}

#[cfg(not(target_arch = "wasm32"))]
#[test_case]
fn pcall_contains_a_runtime_type_error() {
    let lua = Lua::new();
    let ok: bool = lua
        .load("local ok = pcall(function() local x = nil return x + 1 end) return ok")
        .eval()
        .expect("pcall itself must not fail");
    assert!(!ok);
}

#[cfg(not(target_arch = "wasm32"))]
#[test_case]
fn state_survives_a_pcall_caught_error() {
    let lua = Lua::new();
    let _: bool = lua
        .load("local ok = pcall(function() error('boom') end) return ok")
        .eval()
        .unwrap();
    let v: f64 = lua.load("return 1 + 1").eval().unwrap();
    assert_eq!(v, 2.0);
}
