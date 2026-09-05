//! The same operations against mlua's vendored C Luau. Native only.
//!
//! Run: cargo test --features mlua-native --test mlua_tests
#![cfg(feature = "mlua-native")]

use script_runtimes_spike::mlua_backend as be;
use script_runtimes_spike::ScreenSpec;

#[test]
fn loads_a_chunk() {
    let lua = be::prepare().expect("prepare");
    assert_eq!(be::chunk_ran(&lua).unwrap(), "chunk-ran");
}

#[test]
fn lua_calls_a_rust_function() {
    let lua = be::prepare().expect("prepare");
    assert_eq!(be::call_rust_from_lua(&lua, 20, 22).unwrap(), 42);
}

#[test]
fn round_trips_a_nested_table_through_serde() {
    let lua = be::prepare().expect("prepare");
    let input = ScreenSpec::sample();
    let out = be::round_trip(&lua, &input).expect("round trip");
    assert_eq!(out.title, "Copper Chest (modded)");
    assert_eq!(out.size, [9, 3]);
    assert_eq!(out.meta.api_version, 2);
    assert_eq!(out.slots[0].role, "lua_storage_0");
    assert!(out.slots[2].locked);
}

#[test]
fn dangerous_globals_are_absent_under_the_sandbox() {
    let lua = be::prepare().expect("prepare");
    let absent = be::missing_globals(&lua).unwrap();
    for name in be::FORBIDDEN_GLOBALS {
        assert!(absent.contains(&name.to_string()), "{name} still reachable");
    }
}

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

#[test]
fn memory_limit_aborts_a_runaway_allocation() {
    let lua = be::sandboxed_lua().unwrap();
    lua.set_memory_limit(1024 * 1024).unwrap();
    let res = lua
        .load("local t = {} for i = 1, 10000000 do t[i] = i end return #t")
        .exec();
    assert!(res.is_err(), "allocation past the cap should have failed");
}
