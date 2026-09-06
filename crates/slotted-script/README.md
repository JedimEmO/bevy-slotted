# slotted-script

The scripting port of [slotted](https://github.com/mathiasmyrland/bevy-slotted),
and the API surface every mod sees.

Deliberately small and dependency-light: `serde`, `thiserror` and
`slotted-model`. It defines the port, the wire types and the Lua prelude; it
contains no Lua runtime and no Bevy.

Scripts never touch state. They receive an event and return commands; the host
validates and applies them. That is what makes the same script safe on a server
and in a browser tab.

## Main types

| Type | What it is |
|---|---|
| `ScriptRuntime` | The port an adapter implements: `load`, `call`, `unload`. |
| `ScriptEvent` | What the host sends: `data_stage`, `control_start`, `slot_click`, `widget_activate`, `tooltip_build`, `recipe_lookup`, `screen_opened`, `screen_closed`, `hud_tick`, `search_changed`, `property_changed`, and the three test-stage events. |
| `ScriptCommand` | What a script answers with: the `register_*` family, `inject`, `add_tooltip_part`, `sort`, `quick_stack`, `move`, `toggle_favorite`, `click`, `set_hud`, `hud_update`, `log`. |
| `Limits`, `ModId`, `ScriptId`, `Stage`, `ScriptError` | Budgets, ids, the data/control/test stage, and the error type. |
| `PRELUDE`, `TEST_PRELUDE` | The shared Lua source every adapter installs, so both runtimes expose the same `slotted.*`. |
| `value::untagged` | The serde form a Lua table maps onto. |
| `TestOp`, `TestLocator` | The `slotted.test` protocol. |

## Example

```rust
use slotted_script::{ScriptCommand, ScriptEvent};

let event = ScriptEvent::DataStage { api_version: 1 };
assert_eq!(event.name(), "data_stage");

let command = ScriptCommand::Log {
    level: slotted_script::LogLevel::Info,
    message: "hello".into(),
};
assert_eq!(command.name(), "log");
```

Running a script needs an adapter. `slotted-script-luaur` is the one this
workspace ships, and it is the same runtime natively and in a browser
(ADR 0004).

## Feature flags

None.

## Licence

MIT OR Apache-2.0, at your option.
