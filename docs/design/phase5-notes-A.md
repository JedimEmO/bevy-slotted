# Phase 5 notes A: `slotted-script-piccolo`

The web `ScriptRuntime` adapter (PLAN.md 4.9, ADR 0001). One state per loaded
script, the shared prelude unchanged, the same 30 conformance cases as the
native mlua adapter, and the same `ScriptError` classification. What follows is
what the spike did not know.

## What was built

| file | what |
| --- | --- |
| `src/lib.rs` | `PiccoloRuntime`, the fuel/memory loop, error classification, command decoding |
| `src/stdlib.rs` | the stdlib polyfills, the environment table, the read-only wrappers |
| `src/bridge.rs` | `slotted_model::Value` to and from piccolo values |
| `src/model_ser.rs` | a `serde::Serializer` that writes a `ScriptEvent` into a `Value` tree |
| `src/tests.rs` | 32 adapter tests |
| `tests/conformance.rs` | the shared suite, 30 cases |
| `tests/wasm.rs` | 8 `wasm-bindgen-test` cases, headless Chrome |
| `vendor/piccolo/` | piccolo 0.3.3 with the tombstone fix backported (`VENDOR.md`) |

The facade gains a `script-piccolo` feature and `PiccoloHostPlugin`, mirroring
`MluaHostPlugin`. Defaults are untouched: a native build is still mlua.
`.cargo/config.toml` gained one line, `rustflags` for
`wasm32-unknown-unknown`, so nobody has to remember `RUSTFLAGS`.

## 1. The stdlib piccolo does not have

piccolo 0.3.3's `Lua::core()` is `base` (without `tonumber`, `xpcall` and
`print`), `coroutine`, `math`, five `string` functions (`len`, `sub`, `upper`,
`lower`, `reverse`) and two `table` ones (`pack`, `unpack`). Everything below
is implemented in `stdlib.rs` as a Rust callback, and the list is pinned by
`stdlib::POLYFILLED` and the `every_polyfilled_function_exists` test.

| polyfill | needed by |
| --- | --- |
| `_G` | `sandbox_globals.lua`, and the prelude's `_G.slotted = slotted` |
| `tonumber` | `sandbox_globals.lua`'s kept-globals list |
| `xpcall` | same; a mod may use it |
| `getmetatable`, `setmetatable` | replaced, not added: piccolo's ignore `__metatable` |
| `string.format` | the prelude's `slotted.log`/`info`/`warn`/`error` |
| `string.rep` | `memory.lua` |
| `string.find` | the prelude's `namespaced()` |
| `string.byte`, `string.char` | completeness; a mod doing byte work |
| `table.concat` | the prelude's `print` |
| `table.insert`, `table.remove` | completeness; ordinary mod Lua |
| `table.sort` | the prelude's `subscriptions()` |
| `bit32.*` (band, bor, bxor, bnot, lshift, rshift, arshift) | `sandbox_globals.lua`'s kept list |
| `utf8.char`, `utf8.codepoint`, `utf8.len`, `utf8.charpattern` | same |

Two deliberate gaps, documented in the module and unreached by the suite:

- **`string.find` is plain-text only.** Lua patterns are not implemented; a
  pattern holding a magic character raises rather than silently matching the
  wrong thing. The prelude calls `string.find(id, ":", 1, true)`.
- **`table.sort` takes no comparator.** Calling a Lua function from inside a
  Rust callback needs a `Sequence` per comparison; the prelude sorts strings.

`string.format` covers `%%`, `%s`, `%q`, `%d`/`%i`/`%u`, `%c`, `%x`/`%X`,
`%o`, `%f`/`%e`/`%g`, each with `-`, `0`, a width and a `.precision`.

## 2. The sandbox is an environment table, not a deletion

This is the load-bearing discovery of the package, and it is a piccolo bug, not
a design choice.

piccolo tombstones a removed table key (`Key::Dead`). The map-growth path then
hashes every key and panics on a dead one: *"all keys must be live when table
is grown"*, `table/raw.rs:235`. The array-growth path next to it drops dead
keys with a `retain`; the map path does not. So `collectgarbage = nil` on the
globals table, followed by the prelude adding `slotted` and `print`, aborts the
process. On `wasm32-unknown-unknown` a panic is an abort of the whole module,
which is exactly the failure that disqualified luaur in ADR 0001.

So nothing is ever deleted. `stdlib::build_env` copies the globals a mod may
have into a **fresh table**, skips every `FORBIDDEN_GLOBALS` entry, installs
the polyfills there, and every chunk of that script is compiled with
`Closure::load_with_env` against it. `collectgarbage` is the only forbidden
name piccolo defines at all; the rest were never there. As a bonus this is
closer to what Luau's sandbox does than a global deletion would have been.

**A mod could still hit it, so piccolo is vendored.** `t[k] = nil` followed by
enough insertions to grow the table is ordinary Lua and aborted the module.
`vendor/piccolo` is 0.3.3 with the fix backported: one `retain` that drops dead
keys before the rehash, which is what upstream's own `RawTable::reserve_map`
does on `main` (checked at `ce709eb1dae5c543cbc78e7e12bb80249d88c55f`).
`[patch.crates-io]` in the root manifest wires it in, so
`slotted-script-piccolo` still declares `piccolo = "0.3.3"`.

Depending on piccolo `main` directly was the first thing tried and it does fix
the bug, verified with a scratch crate. It was rejected for one reason:
`main` takes `gc-arena` from git, and `deny.toml` sets `unknown-git = "deny"`
with no allowed git sources. A path patch keeps `cargo deny check sources`
happy and keeps the workspace on released crates.

`vendor/piccolo/VENDOR.md` describes all three patches (the tombstone fix, one
raw-pointer autoref that current rustc rejects in uncapped code, and two style
lints allowed) and how to delete the directory once piccolo releases past
0.3.3. `vendor/piccolo/tests/tombstone.rs` covers the fix at the VM level; the
adapter covers it at its own level with `deleting_a_table_key_then_growing_it_survives`,
`deleting_a_global_then_adding_more_survives`,
`ten_thousand_key_insert_delete_churn_survives` and
`emptying_and_refilling_a_table_survives`, each with a browser twin in
`tests/wasm.rs`.

A second piccolo rule cost an afternoon before that one: **table keys are held
weakly**, so a string key built with `String::from_slice` can be collected
while its table still holds it, and the next growth panics with the same
message. Every key this crate writes goes through `ctx.intern`.

## 3. Freezing without `sandbox(true)`

Luau marks a table read-only. piccolo has nothing, and a metatable alone is not
enough: `__newindex` only fires for a key the table does not already have, so
`string.format = f` would succeed silently on the real table.

Each library table is therefore replaced by an **empty wrapper** whose
`__index` reads through and whose `__newindex` raises with the word `readonly`,
which is what the classifier turns into `ScriptError::Sandbox`. The same
wrapper goes over `slotted` after the prelude has built it, because the
prelude's own `__newindex` guard cannot fire for the keys it just wrote.

The wrapper's metatable carries `__metatable = false`, and `getmetatable` and
`setmetatable` are replaced with versions that honour it. Without that
replacement a mod reads `getmetatable(string).__index` and writes straight to
the real table; `the_frozen_wrapper_cannot_be_unwrapped` pins both halves.

## 4. Budget and memory

`Limits::budget` is interrupt ticks on Luau and piccolo fuel here. The two
cannot be made the same unit: Luau interrupts about once per call and per loop
back-edge, piccolo charges per VM instruction, so a loop body costs several
fuel per tick.

**The ratio is 8 fuel to the tick** (`FUEL_PER_TICK`). The default budget of
1,000,000 ticks is 8,000,000 fuel per `load` or `call`. Eight is what makes the
suite behave the same on both adapters: `budget.lua` still runs out, and
`memory.lua` still reaches the memory cap before the budget, which is what it
asserts.

Memory is not enforced by an allocator hook, because piccolo has none. The
executor is stepped in 4096-fuel slices and `Lua::total_memory()` (gc-arena's
`total_allocation`) is checked between them, so a state can overshoot
`memory_bytes` by whatever one slice allocates. To keep that bound tight,
`string.rep` and `table.concat` charge fuel proportional to the bytes they
produce, which is also the honest accounting: they are the two callbacks that
can allocate without executing an instruction.

A budget or memory failure calls `Executor::stop` and then `gc_collect`, so the
abandoned frames are gone before the next call and a budget failure is not
followed by a spurious memory failure.

## 5. The value bridge

piccolo has no serde. An event takes one hop more than on mlua: `model_ser`
serialises it into a `slotted_model::Value` tree, then `bridge::to_lua` builds
the tables. A reply comes back through `bridge::from_lua` and then
`slotted_model::from_value`, which is what gives the same per-field error text
the mlua adapter produces. Each command is decoded on its own so the message
can name its position, its `type` and its fields.

The rules match `ser_options`/`de_options` exactly: `Option::None` is an absent
key rather than a `nil` value; a table whose keys are `1..n` is a `List`; an
empty table is an empty `List`; an integral Lua number is an `Int`. Cycles stop
at 64 levels of nesting rather than recursing off the stack.

## 6. Threading

`ScriptRuntime` is `Send + Sync`; piccolo's `Lua` is neither, deliberately.
`PiccoloRuntime` carries two `unsafe impl`s with the argument written out on
the type: every `Rc` lives inside an arena the runtime owns, no handle escapes
(the `'gc` branding enforces that), and every entry point takes `&mut self`, so
`&PiccoloRuntime` grants no access to a state at all. The intended deployment
is a browser tab with one thread. This is the only `unsafe` in the crate, and
the alternative was a `NonSend` variant of `ScriptHost` through the whole pack
loader.

## 7. Numbers

| measurement | value |
| --- | --- |
| conformance | 30 of 30 cases pass |
| adapter tests | 32 pass |
| vendored piccolo tests | 24 pass, 4 of them new |
| wasm tests, headless Chrome | 8 pass |
| `slot_click` round trip, debug build | 10.2 µs per call |

ADR 0001 left piccolo's call cost unmeasured. 10.2 µs is a **debug** build,
against mlua's sub-microsecond debug figure; that is fine for a click or a
hover and too slow for a per-slot-per-frame hook on a 9x3 grid. Measure a
release build before the playground promises anything about HUD ticks.

## 8. Commands

```sh
cargo test -p slotted-script-piccolo
cargo clippy -p slotted-script-piccolo --all-targets -- -D warnings
cargo check -p slotted-script-piccolo --target wasm32-unknown-unknown
(cd vendor/piccolo && cargo test)
PATH="$PWD/spikes/script-runtimes/chromedriver-shim:$PATH" \
  wasm-pack test --headless --chrome crates/slotted-script-piccolo --test wasm
```

The wasm ones need no `RUSTFLAGS`: `.cargo/config.toml` sets
`--cfg getrandom_backend="wasm_js"` for the target. Two `getrandom` lines sit
in the crate's wasm dependencies (0.3 with `wasm_js`, 0.2 with `js`) because
`ahash` reaches both and Cargo only lets a direct dependency turn a feature on.

## 9. What is left

- Release-build call cost, unmeasured.
- Wasm size of this adapter in a real bundle, unmeasured here; the spike's
  987 KiB figure was a bare piccolo binary before `wasm-opt`.
- `string.find` patterns and a `table.sort` comparator, if a real mod wants
  them.
- `just wasm-check` does not list `slotted-script-piccolo` or
  `slotted --features script-piccolo`; both pass when run by hand. Whoever owns
  the justfile should add the two lines.
- Delete `vendor/piccolo` when piccolo releases past 0.3.3.
