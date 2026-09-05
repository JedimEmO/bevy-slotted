//! Same shape as `wasm_size`, for piccolo, so the two .wasm numbers compare.

use script_runtimes_spike::piccolo_backend as p;

fn main() {
    let mut lua = p::prepare().expect("prepare");
    let marker = p::marker(&mut lua).expect("marker");
    let sum = p::call_rust_from_lua(&mut lua, 20, 22).expect("rust call");
    let rt = p::round_trip(&mut lua).expect("round trip");
    std::hint::black_box((marker, sum, rt));
}
