# slotted-script-luaur

Luau through `luaur`: the `ScriptRuntime` adapter that reaches the browser for
[slotted](https://github.com/JedimEmO/bevy-slotted), and a native one
behind the facade's `script-luaur` feature. See ADR 0004.

`luaur` is a pure-Rust, line-for-line port of Luau with an mlua-shaped API, so
this adapter is `slotted-script-mlua` with different imports. One `Lua` state per
loaded script, every `slotted_script::FORBIDDEN_GLOBALS` entry removed *before*
`sandbox(true)` seals the state, an interrupt budget counted per `call`,
`set_memory_limit` for allocation, and values crossing through `LuaSerdeExt`.
Errors are classified into the same `ScriptError` variants the mlua adapter
produces, and both pass the same 32-case conformance suite.

## On `wasm32`, a Lua error aborts the module

Read this before shipping a page that loads untrusted mods.

luaur raises a Lua error by panicking and catches it with `catch_unwind`.
`wasm32-unknown-unknown` has no unwinding, so in a browser every raise is a trap
that takes the whole module with it. `error(...)`, a runtime type error, an
exhausted budget and a refused allocation all abort; `pcall` inside a script does
not contain them, because luaur implements `pcall` the same way. A *compile*
error is the one recoverable class, because that path never enters the VM.

The host has to be prepared to restart. `install_error_reporter` hands you the
error text and the traceback from inside the `xpcall` message handler, which Luau
runs before it throws, so the page can say what crashed even though the module is
about to die. Catch the `WebAssembly.RuntimeError` on the JavaScript side,
re-instantiate the module, and restore your state.
`examples/web-playground` in the repository is a working reference: it snapshots
the open menu's inventories by item name, restarts, and puts the chest back.

Natively none of this applies. Errors come back as `Result` and the state stays
usable.

## Main types

| Type | What it is |
|---|---|
| `LuaurRuntime` | The `ScriptRuntime` implementation. |
| `RaisedError` | What a script raised, handed to a reporter before the trap. |
| `install_error_reporter` | Installs that reporter, once, process-wide. |

## Example

```rust
use slotted_script::{Limits, ModId, ScriptEvent, ScriptRuntime, Stage};
use slotted_script_luaur::LuaurRuntime;

let mut runtime = LuaurRuntime::new(Limits::default());
let id = runtime
    .load(
        &ModId::new("demo").unwrap(),
        "data.lua",
        r#"slotted.register_item("apple", { max_stack_size = 16 })"#,
        Stage::Data,
    )
    .unwrap();
let commands = runtime
    .call(id, &ScriptEvent::DataStage { api_version: 1 })
    .unwrap();
assert_eq!(commands.len(), 1);
```

## Feature flags

None.

## Licence

MIT OR Apache-2.0, at your option.
