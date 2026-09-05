//! Cold start and per-call cost for both backends.
//!
//! Run: cargo run --release --features mlua-native --example bench
//!
//! Deliberately not criterion: the numbers wanted here are order-of-magnitude
//! ("is a Lua call per slot hover affordable?"), and a plain loop keeps the
//! spike free of a heavy dev-dependency.

use script_runtimes_spike::ScreenSpec;
use std::hint::black_box;
use std::time::Instant;

const CALLS: u32 = 200_000;
const ROUND_TRIPS: u32 = 20_000;
const STARTS: u32 = 200;

fn ns(d: std::time::Duration, n: u32) -> f64 {
    d.as_secs_f64() * 1e9 / f64::from(n)
}

fn main() {
    println!("{:<12} {:>16} {:>18} {:>20}", "backend", "cold start (us)", "call w/ table (ns)", "serde round trip (ns)");

    bench_luaur();
    #[cfg(feature = "mlua-native")]
    bench_mlua();
    #[cfg(not(feature = "mlua-native"))]
    println!("mlua         (build with --features mlua-native)");
}

fn bench_luaur() {
    use script_runtimes_spike::luaur_backend as be;

    let t = Instant::now();
    for _ in 0..STARTS {
        black_box(be::prepare().unwrap());
    }
    let cold = t.elapsed().as_secs_f64() * 1e6 / f64::from(STARTS);

    let lua = be::prepare().unwrap();
    let (f, _) = be::hot_fn(&lua).unwrap();
    // warm up
    for i in 0..1000 {
        black_box(be::call_with_fresh_table(&lua, &f, i, i).unwrap());
    }
    let t = Instant::now();
    for i in 0..CALLS {
        black_box(be::call_with_fresh_table(&lua, &f, i64::from(i), 1).unwrap());
    }
    let call = ns(t.elapsed(), CALLS);

    let spec = ScreenSpec::sample();
    let t = Instant::now();
    for _ in 0..ROUND_TRIPS {
        black_box(be::round_trip(&lua, &spec).unwrap());
    }
    let rt = ns(t.elapsed(), ROUND_TRIPS);

    println!("{:<12} {:>16.1} {:>18.0} {:>20.0}", "luaur", cold, call, rt);
}

#[cfg(feature = "mlua-native")]
fn bench_mlua() {
    use script_runtimes_spike::mlua_backend as be;

    let t = Instant::now();
    for _ in 0..STARTS {
        black_box(be::prepare().unwrap());
    }
    let cold = t.elapsed().as_secs_f64() * 1e6 / f64::from(STARTS);

    let lua = be::prepare().unwrap();
    let (f, _) = be::hot_fn(&lua).unwrap();
    for i in 0..1000 {
        black_box(be::call_with_fresh_table(&lua, &f, i, i).unwrap());
    }
    let t = Instant::now();
    for i in 0..CALLS {
        black_box(be::call_with_fresh_table(&lua, &f, i64::from(i), 1).unwrap());
    }
    let call = ns(t.elapsed(), CALLS);

    let spec = ScreenSpec::sample();
    let t = Instant::now();
    for _ in 0..ROUND_TRIPS {
        black_box(be::round_trip(&lua, &spec).unwrap());
    }
    let rt = ns(t.elapsed(), ROUND_TRIPS);

    println!("{:<12} {:>16.1} {:>18.0} {:>20.0}", "mlua", cold, call, rt);
}
