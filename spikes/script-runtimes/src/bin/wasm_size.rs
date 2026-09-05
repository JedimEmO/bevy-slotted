//! A minimal binary that exercises all three operations, so the resulting
//! `.wasm` is a fair "what does the runtime cost in bytes" number: nothing is
//! dead-code-eliminated that a real adapter would keep.
//!
//! cargo build --bin wasm_size --profile wasm-size --target wasm32-unknown-unknown

use script_runtimes_spike::luaur_backend as be;
use script_runtimes_spike::ScreenSpec;

fn main() {
    let lua = be::prepare().expect("prepare");
    let marker = be::chunk_ran(&lua).expect("marker");
    let sum = be::call_rust_from_lua(&lua, 20, 22).expect("rust call");
    let spec = be::round_trip(&lua, &ScreenSpec::sample()).expect("round trip");
    // Keep the results observable so nothing above is optimised away.
    std::hint::black_box((marker, sum, spec));
}
