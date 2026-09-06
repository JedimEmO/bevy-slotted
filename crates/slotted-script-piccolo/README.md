# slotted-script-piccolo

Lua through `piccolo`: the web `ScriptRuntime` adapter for
[slotted](https://github.com/mathiasmyrland/bevy-slotted). See ADR 0001.

`slotted-script-mlua` is the reference. This crate mirrors its behaviour on a
runtime that has neither a serde bridge, nor a `sandbox(true)`, nor a memory
limit, and it exists for one reason: on `wasm32-unknown-unknown` piccolo reports
a Lua error as a `Result` and leaves the state usable, where the alternatives
abort the whole module.

What differs from the mlua adapter, all of it invisible to a mod:

- **Stdlib.** piccolo 0.3 ships a partial `base`, `string` and `table`.
  Everything in `stdlib::POLYFILLED` is a Rust callback here. Two documented
  gaps: `string.find` is plain-text only, and `table.sort` takes no comparator.
- **Sandbox.** There is no `sandbox(true)`, so each library table is replaced by
  an empty read-only wrapper, and so is `slotted` once the prelude has built it.
- **Budget.** `Limits::budget` is interrupt ticks on Luau and piccolo fuel here,
  at `FUEL_PER_TICK` fuel to the tick.
- **Memory.** `Limits::memory_bytes` is checked between execution slices rather
  than by an allocator hook, so a state overshoots by at most one slice.
- **Traceback.** piccolo exposes no Lua stack to the host, so the traceback
  names the chunk and says so.

## Main types

| Type | What it is |
|---|---|
| `PiccoloRuntime` | The `ScriptRuntime` implementation. |
| `stdlib::POLYFILLED` | The functions this crate implements in Rust. |
| `bridge`, `model_ser` | The manual value bridge, in place of a serde bridge. |

## Example

```rust
use slotted_script::{Limits, ModId, ScriptEvent, ScriptRuntime, Stage};
use slotted_script_piccolo::PiccoloRuntime;

let mut runtime = PiccoloRuntime::new(Limits::default());
let id = runtime
    .load(
        &ModId::new("demo").unwrap(),
        "data.lua",
        r#"slotted.register_item("apple", { max_stack_size = 16 })"#,
        Stage::Data,
    )
    .unwrap();
let commands = runtime.call(id, &ScriptEvent::DataStage { api_version: 1 }).unwrap();
assert_eq!(commands.len(), 1);
```

## Feature flags

None.

On `wasm32-unknown-unknown` this crate turns on the `getrandom` browser
backends that `ahash` needs transitively. The workspace `.cargo/config.toml`
also sets `--cfg getrandom_backend="wasm_js"` for that target.

## Publishing

Not yet publishable. The workspace patches `piccolo` to a vendored copy that
backports one upstream fix; see `vendor/piccolo/VENDOR.md`. This crate can be
published once piccolo releases past 0.3.3.

## Licence

MIT OR Apache-2.0, at your option.
