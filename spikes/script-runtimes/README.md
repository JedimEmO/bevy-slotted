# Spike: Lua script runtimes, native and wasm

Phase 0 spike for the `slotted-script` port (PLAN.md 4.8, 4.9). Answers one
question: which Lua runtime backs the adapters, given that scripts must run both
natively and in a browser tab.

This is a standalone package with its own `Cargo.lock`, **not** a workspace
member, and it builds into `spikes/script-runtimes/target`.

## Headline result

`luaur` is a good native runtime and a **broken** browser runtime. Everything
works in Chrome until a script raises an error, at which point the whole wasm
module aborts and cannot be recovered. `piccolo` has no such problem but costs a
much thicker adapter. See "The wasm error problem" below.

## Environment note, read this first

The developer shell exports an Anaconda C/C++ toolchain (`CC`, `CXX`, `CFLAGS`
with `-march=nocona`, `CPPFLAGS` pointing into `~/anaconda3`). It breaks mlua's
vendored Luau build in two independent ways:

1. **Natively**, the build succeeds but the resulting binary dies with `SIGFPE`
   inside `Lua::new()`. Luau's `kSizeClassConfig` is a C++ global whose static
   constructor never runs, so `blockSize` is 0 and `newclasspage` divides by
   zero (`luau/VM/src/lmem.cpp:288`). Confirmed under gdb.
2. **For wasm**, conda's `CFLAGS` leak into the wasm32 clang invocation and it
   rejects `-march=nocona`, masking the real error.

`clean-env.sh` unsets the whole set. **Every mlua command below goes through
it.** luaur and piccolo are pure Rust and are unaffected.

## Commands actually run

```sh
# native, all three backends (26 tests, clippy-clean)
./clean-env.sh cargo test --features "mlua-native,bevy-native,piccolo"

# call cost and cold start
./clean-env.sh cargo run --release --features mlua-native --example bench

# luaur in the browser (headless Chrome)
PATH="$PWD/chromedriver-shim:$PATH" \
  wasm-pack test --headless --chrome --test luaur_tests --test wasm_errors

# piccolo in the browser (3 tests)
PATH="$PWD/chromedriver-shim:$PATH" RUSTFLAGS='--cfg getrandom_backend="wasm_js"' \
  wasm-pack test --headless --chrome --features piccolo --test piccolo_tests

# wasm sizes
cargo build --bin wasm_size --profile wasm-size --target wasm32-unknown-unknown
RUSTFLAGS='--cfg getrandom_backend="wasm_js"' cargo build \
  --bin wasm_size_piccolo --features piccolo --profile wasm-size \
  --target wasm32-unknown-unknown

# mlua on wasm (expected to fail)
./clean-env.sh cargo check --features mlua-native --target wasm32-unknown-unknown
```

`chromedriver-shim/chromedriver` execs `/snap/bin/chromium.chromedriver`,
because the snap does not put a binary named `chromedriver` on `PATH` and
wasm-pack looks for that exact name. Headless Chrome worked; the `--node`
fallback was not needed.

## Versions resolved (from Cargo.lock)

| crate | version |
| --- | --- |
| `luaur`, `luaur-rt`, `luaur-vm` | 0.1.8 |
| `mlua` | 0.11.6 |
| `mlua-sys` | 0.10.0 |
| `luau0-src` (vendored Luau) | 0.18.3+luau709 |
| `piccolo` | 0.3.3 |
| `gc-arena` | 0.5.3 |
| `bevy` | 0.19.1 |
| `wasm-bindgen-test` | 0.3.78 |
| rustc | 1.96.1 |

## What each backend does

All three run the same Lua source (`SCRIPT` in `src/lib.rs`) through the same
four operations: run a chunk, call a registered Rust function from Lua, call a
Lua function from Rust, and round-trip a nested struct (`ScreenSpec`, with a
vector of structs and a nested struct) through Lua and back.

| | luaur | mlua (luau) | piccolo |
| --- | --- | --- | --- |
| load a chunk | yes | yes | yes |
| Rust fn callable from Lua | yes | yes | yes |
| Lua fn callable from Rust | yes | yes | yes |
| nested table round trip | serde, `LuaSerdeExt` | serde, `LuaSerdeExt` | **manual**, no serde |
| native | yes | yes (with clean env) | yes |
| wasm32-unknown-unknown | builds and runs | **no** | builds and runs |
| Lua errors recoverable on wasm | **no** | n/a | yes |
| instruction budget | `set_interrupt` | `set_interrupt` | `Fuel` |
| memory cap | `set_memory_limit` | `set_memory_limit` | not checked |

## Measurements

Native, `opt-level = 3`, lto, on a 13th-gen i9-13900KF. Plain `Instant` loops
(`examples/bench.rs`), not criterion, because the question is order of magnitude.
Three consecutive runs agreed to within a few percent; the middle run is shown.

| backend | cold start | call with a small table | serde round trip of `ScreenSpec` |
| --- | --- | --- | --- |
| luaur | 39.8 us | 265 ns | 4956 ns |
| mlua | 40.7 us | 308 ns | 7034 ns |

luaur being the faster of the two was not expected. It holds across runs and
across both the size-optimised and speed-optimised profiles. Treat it as "the
pure-Rust VM is not a performance problem", not as "luaur beats C Luau in
general": this is one microbenchmark on one machine.

At 265 ns a call, a Lua hook on every slot hover or click is free. A hook on
every slot of a 9x3 grid every frame is about 7 us, still fine.

piccolo call cost was **not measured**. Its executor model does not have a
comparable `Function::call`, and building a fair harness was outside the time
box.

### wasm size

`--profile wasm-size` (`opt-level = "z"`, lto, `panic = "abort"`, stripped),
`cargo build --target wasm32-unknown-unknown`, no `wasm-opt` pass, no
wasm-bindgen glue. Each binary runs all four operations so nothing is
dead-code-eliminated that a real adapter would keep.

| backend | .wasm |
| --- | --- |
| luaur | 760,399 bytes (743 KiB) |
| piccolo | 1,010,574 bytes (987 KiB) |

Depending on `luaur-rt` directly instead of the `luaur` umbrella (which
unconditionally re-exports the type checker and the require resolver) saves only
1,646 bytes, so the linker already strips them. It still avoids compiling
`luaur-analysis` and `luaur-require`, which is worth doing for build time.

## The wasm error problem

This is the finding that changes the decision. `tests/wasm_errors.rs`.

luaur raises Lua errors by **panicking** (`luaur_vm::functions::lua_d_throw_ldo`
calls `std::panic::panic_any`) and catches them with `catch_unwind`.
`wasm32-unknown-unknown` has no unwinding, so on the web every raise aborts the
module. Observed in headless Chrome:

- `error('boom')` from Rust: aborts.
- A runtime type error (`nil + 1`): aborts.
- An in-VM `pcall` around either: **also aborts**, because luaur implements
  `pcall` the same way. There is no Lua-level workaround.
- A *compile* error: fine, returns `Err`. That path never enters the VM.

Escape hatches checked and rejected:

- `RUSTFLAGS="-C target-feature=+exception-handling -C panic=unwind"` fails with
  "the crate `panic_unwind` does not have the panic strategy `unwind`". It needs
  `-Z build-std` on nightly, which is not acceptable for a shipped crate.
- Not checked: running each script in its own worker or wasm instance and
  rebuilding after an abort. Plausible for a playground, unacceptable as the
  normal path for mod scripts.

Consequence: with luaur on the web, one buggy mod takes down the page. For a
modding surface, where scripts are by definition untrusted and buggy, that is
disqualifying on its own.

piccolo returns `StaticError` from the same cases and the state stays usable,
verified in the same browser (`tests/piccolo_tests.rs`).

The native-only tests in `wasm_errors.rs` and one test in `luaur_tests.rs` are
`#[cfg(not(target_arch = "wasm32"))]`. **The cfg is the finding, not a bug to
fix.**

## API delta: mlua vs luaur

How thick is the adapter layer, and can one shared prelude serve both?

Everything `slotted-script` needs is **name-for-name identical**. The two
backend modules in this spike differ only in their `use` lines, one `?` on
`create_table`, and the sandbox setup. That is the whole delta.

| what we need | mlua 0.11 | luaur 0.1.8 | same? |
| --- | --- | --- | --- |
| state | `Lua::new_with(StdLib, LuaOptions)` | same | yes |
| load a chunk | `lua.load(src).set_name(n).exec()` / `.eval::<T>()` | same | yes |
| Rust fn | `lua.create_function(\|_, args\| ...)` | same | yes |
| table create | `lua.create_table()?` | `lua.create_table()` | returns `Table`, not `Result` |
| table get/set | `t.get::<V>(k)?`, `t.set(k, v)?` | same | yes |
| table iterate | `pairs`, `sequence_values` | same, plus `pairs_vec` | superset |
| call Lua from Rust | `f.call::<R>(args)?` | same | yes |
| globals | `lua.globals()` | same, plus `set_globals(t)` | superset |
| chunk env | `set_environment` | same | yes |
| serde | `LuaSerdeExt::{to_value, from_value}` | same trait, same methods | yes |
| UserData | `impl UserData`, `UserDataMethods`, `AnyUserData` | same, plus `#[derive(UserData)]` on `luaur-rt` | yes |
| conversions | `FromLua`, `IntoLua`, `FromLuaMulti`, `IntoLuaMulti` | same | yes |
| errors | `mlua::Error`, `Result` | `luaur::Error`, `Result` | shape matches |
| sandbox | `lua.sandbox(true)?` | same | yes |
| interrupt | `set_interrupt(\|_\| Ok(VmState::Continue))` | same signature, same `VmState` | yes |
| memory cap | `set_memory_limit(n)?` | same | yes |
| prelude module | `mlua::prelude::*` | `luaur::prelude::*`, mlua-style `Lua*` aliases | yes |

Mismatches worth knowing:

- `create_table()` is infallible on luaur and fallible on mlua. One `?`.
- `StdLib` flags are honoured by mlua and ignored by luaur (above).
- luaur has no `LightUserData`, so its serde `null` sentinel is a per-state
  table rather than a null pointer. Behaviourally equivalent; only matters if
  something compares raw values.
- The `#[derive(UserData)]` / `#[derive(FromLua)]` macros must be reached as
  `luaur_rt::...`, not through the `luaur` umbrella, because they emit absolute
  `::luaur_rt::` paths.
- luaur's numbers are all `f64` at the VM level, as in Luau generally, and
  `Value::Integer` is reconstructed from whether an `f64` is an exact whole
  number. mlua's Luau build behaves the same way.

**One shared prelude works.** A single `slotted.*` Lua file, one shared
`ScriptEvent`/`ScriptCommand` serde bridge, and roughly 20 lines of per-backend
glue. Swapping mlua for luaur, or supporting both, is cheap.

piccolo is the opposite: no serde, no `Function::call`, GC-arena lifetimes that
prevent values crossing an `enter` boundary, untyped `Table::get_value`, and
string keys that must be interned first. `src/piccolo_backend.rs` is roughly
three times the length of the other two for the same four operations, and the
prelude would need rewriting against a different value model.

## Sandboxing

Both mlua and luaur share one footgun: after `Lua::sandbox(true)`, a write to
`globals()` returns `Ok(())` and **silently does nothing**. luaur's
`Table::is_readonly()` on the globals still reports `false`. Anything to be
removed must be removed *before* the sandbox call. Both backends here do that.

Beyond that they differ:

- **luaur ignores `StdLib` flags.** Anything other than `StdLib::NONE` opens the
  whole safe stdlib. `StdLib::BASE | TABLE | STRING | MATH` produced exactly the
  same globals as `Lua::new()`. Only `new_empty()` / `NONE` gives an empty
  environment. Removal has to be by name.
- **mlua honours `StdLib` flags**, but injects `require` and `loadstring` of its
  own, which still need removing by hand.
- Luau itself has no `io`, `package`, `dofile`, `loadfile`, `load` or
  `loadstring`, so the dangerous surface is smaller than Lua 5.x to begin with.
  `os` exists but holds only `clock`, `date`, `difftime`, `time`. luaur's `debug`
  holds only `info` and `traceback`.
- luaur also offers `set_globals(curated_table)` and per-chunk
  `Chunk::set_environment`, both verified working. Per-chunk environments are
  the better fit for per-mod script isolation than a shared frozen global table.

Instruction budgets and memory caps work on **both**, contrary to PLAN.md 4.9's
assumption that luaur has no debug hooks:

- `set_interrupt` returning `Err` aborts a runaway `for` loop on both. Verified.
- `set_memory_limit` makes a runaway table allocation fail on both. Verified.
- Neither was verified **on wasm**, because a failing budget raises, and on wasm
  a raise aborts. This is the same problem as above.

## Bevy

`src/bevy_app.rs`, `tests/bevy_tests.rs`. A `MinimalPlugins` app holding
`luaur::Lua` in a `Resource` and driving all three operations from an `Update`
system, over 100 frames. Works.

`luaur::Lua` is `Send` only with the crate's `send` feature and is never `Sync`,
so the resource wraps it in a `Mutex`, which is what a real adapter would do.
mlua is the same shape. This is not a difference between them.

## Not verified

- piccolo call cost and cold start.
- piccolo memory limits (only `Fuel` was exercised).
- Whether a wasm worker-per-script scheme would make luaur's aborts survivable.
- Any of the three under a real mod workload; `ScreenSpec` is a stand-in.
- `wasm-opt`; the sizes above are pre-optimisation.
