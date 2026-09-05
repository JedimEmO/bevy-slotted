//! piccolo 0.3, the fallback for the web path.
//!
//! Native:  cargo test --features piccolo --test piccolo_tests
//! Browser: RUSTFLAGS='--cfg getrandom_backend="wasm_js"' \
//!            wasm-pack test --headless --chrome --features piccolo --test piccolo_tests
#![cfg(feature = "piccolo")]

use script_runtimes_spike::piccolo_backend as p;

#[cfg(target_arch = "wasm32")]
use wasm_bindgen_test::{wasm_bindgen_test as test_case, wasm_bindgen_test_configure};
#[cfg(target_arch = "wasm32")]
wasm_bindgen_test_configure!(run_in_browser);
#[cfg(not(target_arch = "wasm32"))]
use std::prelude::v1::test as test_case;

#[test_case]
fn three_operations() {
    let mut lua = p::prepare().expect("prepare");
    assert_eq!(p::marker(&mut lua).unwrap(), "chunk-ran");
    assert_eq!(p::call_rust_from_lua(&mut lua, 20, 22).unwrap(), 42);
    let (title, api, roles) = p::round_trip(&mut lua).expect("round trip");
    assert_eq!(title, "Copper Chest (modded)");
    assert_eq!(api, 2);
    assert_eq!(roles, vec!["lua_storage_0", "lua_storage_1", "lua_storage_2"]);
}

/// The reason piccolo is here at all: unlike luaur, this must hold in the
/// browser as well as natively.
#[test_case]
fn runtime_errors_are_results_not_panics() {
    assert!(p::runtime_error_is_a_result());
}

#[test_case]
fn fuel_stops_a_runaway_loop() {
    assert!(p::fuel_stops_a_runaway_loop());
}
