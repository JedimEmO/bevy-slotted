# slotted

The facade for [slotted](https://github.com/mathiasmyrland/bevy-slotted): one
plugin group, one prelude, one set of feature flags.

A game adds `SlottedPlugins` and gets the model, the ECS layer, the widgets, the
theme system, the icon atlas, the item browser, mod loading and a Lua runtime.
Each of those is a crate of its own; this one wires them together and decides
which are compiled in.

## Main types

| Type | What it is |
|---|---|
| `SlottedPlugins` | The plugin group. `default()` for a game with a window, `headless()` for a dedicated server or a test. |
| `HeadlessBevyPlugins`, `HeadlessRenderAssets` | The Bevy plugins ADR 0002 proved sufficient for layout, picking, focus and keyboard with no renderer. |
| `LuaurHostPlugin` | Inserts a `ScriptHost` when none exists. |
| `prelude` | Everything a game touches day to day. |

The member crates are re-exported as `slotted::model`, `::registry`, `::ecs`,
`::theme`, `::ui`, `::icons`, `::browser`, `::packs`, `::script` and
`::script_luaur`, so a game normally depends on this crate alone.

## Example

```rust
use bevy::prelude::*;
use slotted::prelude::*;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(SlottedPlugins::default())
        .run();
}
```

## Feature flags

| Feature | Default | Pulls in |
|---|---|---|
| `ui` | yes | `slotted-ui`, `slotted-theme`, `slotted-icons`. Off for a dedicated server. |
| `browser` | yes | `slotted-browser`. |
| `packs` | yes | `slotted-packs` and `slotted-script`: mod discovery, `pack://`, the data/control lifecycle. |
| `script-luaur` | yes | `slotted-script-luaur`, the Luau runtime. The same one natively and on wasm (ADR 0004). |
| `blur` | no | Backdrop blur for glass panels. |
| `dev` | no | Exclusion-zone highlighter, id tooltips, the HUD position editor, the input recorder. |
| `viewport` | no | Real cameras behind `viewport` nodes. |

The defaults build for wasm as they stand: there is one script runtime and it
is pure Rust, so a browser build takes the same feature set a native one does.
On wasm an uncaught Lua error aborts the module and the host has to restart it;
see ADR 0004 and `slotted-script-luaur`'s crate docs.

## Licence

MIT OR Apache-2.0, at your option.
