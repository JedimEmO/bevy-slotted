# slotted-script-mlua

Luau through `mlua`: the native `ScriptRuntime` adapter for
[slotted](https://github.com/mathiasmyrland/bevy-slotted). See ADR 0001.

One `Lua` state per loaded script. Each state is built with the `table`,
`string`, `math`, `bit32` and `utf8` libraries, has every
`slotted_script::FORBIDDEN_GLOBALS` entry removed, and is then sealed with
`sandbox(true)`. The order matters: a write to `globals()` after sealing is
silently ignored.

Budgets are interrupt ticks counted per `call`, memory is `set_memory_limit`,
and values cross the boundary through `LuaSerdeExt`.

This crate builds Luau from C++ source. It cannot target wasm; use
`slotted-script-piccolo` there.

## Main types

| Type | What it is |
|---|---|
| `MluaRuntime` | The `ScriptRuntime` implementation. |

## Example

```rust
use slotted_script::{Limits, ModId, ScriptEvent, ScriptRuntime, Stage};
use slotted_script_mlua::MluaRuntime;

let mut runtime = MluaRuntime::new(Limits::default());
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

None. `mlua` is pinned to `luau`, `vendored`, `serialize` and `send` at the
workspace root.

## Licence

MIT OR Apache-2.0, at your option.
