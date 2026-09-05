//! The three core operations, run identically on native and in the browser.
//!
//! Native:  cargo test --test luaur_tests
//! Browser: wasm-pack test --headless --chrome

use script_runtimes_spike::luaur_backend as be;
use script_runtimes_spike::ScreenSpec;

#[cfg(target_arch = "wasm32")]
use wasm_bindgen_test::{wasm_bindgen_test as test_case, wasm_bindgen_test_configure};
#[cfg(target_arch = "wasm32")]
wasm_bindgen_test_configure!(run_in_browser);

#[cfg(not(target_arch = "wasm32"))]
use std::prelude::v1::test as test_case;

#[test_case]
fn loads_a_chunk() {
    let lua = be::prepare().expect("prepare");
    assert_eq!(be::chunk_ran(&lua).unwrap(), "chunk-ran");
}

#[test_case]
fn lua_calls_a_rust_function() {
    let lua = be::prepare().expect("prepare");
    assert_eq!(be::call_rust_from_lua(&lua, 20, 22).unwrap(), 42);
}

#[test_case]
fn round_trips_a_nested_table_through_serde() {
    let lua = be::prepare().expect("prepare");
    let input = ScreenSpec::sample();
    let out = be::round_trip(&lua, &input).expect("round trip");

    assert_eq!(out.id, input.id);
    assert_eq!(out.title, "Copper Chest (modded)");
    assert_eq!(out.size, [9, 3]);
    assert_eq!(out.meta.api_version, 2);
    assert_eq!(out.meta.tags, vec!["storage", "copper"]);
    assert_eq!(out.slots.len(), 3);
    assert_eq!(out.slots[0].role, "lua_storage_0");
    assert!(out.slots[2].locked);
}

#[test_case]
fn hot_path_call_works() {
    let lua = be::prepare().expect("prepare");
    let (f, _) = be::hot_fn(&lua).unwrap();
    assert_eq!(be::call_with_fresh_table(&lua, &f, 3, 4).unwrap(), 7);
}

#[test_case]
fn dangerous_globals_are_absent_under_the_sandbox() {
    let lua = be::prepare().expect("prepare");
    let absent = be::missing_globals(&lua).unwrap();
    // Report which of the forbidden names are gone; the assertion below is the
    // set we actually rely on.
    for name in ["io", "os", "package", "require", "loadstring", "debug"] {
        assert!(absent.contains(&name.to_string()), "{name} still reachable");
    }
}

// Native only: the assertion depends on an error being *raised*, and on wasm
// luaur raises by panicking, which aborts. See tests/wasm_errors.rs.
#[cfg(not(target_arch = "wasm32"))]
#[test_case]
fn sandbox_makes_globals_readonly() {
    let lua = be::prepare().expect("prepare");
    // Luau's sandbox freezes the global table after `sandbox(true)`; a script
    // may still create its own globals only if the host allows it. Record the
    // observed behaviour rather than asserting a guess.
    let res = lua.load("string.format = nil").exec();
    assert!(res.is_err(), "stdlib tables should be frozen by the sandbox");
}

// Limits are native-only in this file because the wasm test runner has no
// reliable way to abort a runaway loop if the interrupt does not fire.
#[cfg(not(target_arch = "wasm32"))]
#[test]
fn instruction_budget_aborts_a_runaway_loop() {
    let lua = be::sandboxed_lua().unwrap();
    let hits = be::set_instruction_budget(&lua, 200);
    let res = lua
        .load("local s = 0 for i = 1, 100000000 do s = s + i end return s")
        .exec();
    assert!(res.is_err(), "runaway loop should have been interrupted");
    assert!(hits.load(std::sync::atomic::Ordering::Relaxed) > 0);
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn memory_limit_aborts_a_runaway_allocation() {
    let lua = be::sandboxed_lua().unwrap();
    lua.set_memory_limit(1024 * 1024).unwrap();
    let res = lua
        .load("local t = {} for i = 1, 10000000 do t[i] = i end return #t")
        .exec();
    assert!(res.is_err(), "allocation past the cap should have failed");
}
