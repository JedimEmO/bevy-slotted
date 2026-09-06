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
| `MluaHostPlugin`, `PiccoloHostPlugin` | Insert a `ScriptHost` when none exists. |
| `prelude` | Everything a game touches day to day. |

The member crates are re-exported as `slotted::model`, `::registry`, `::ecs`,
`::theme`, `::ui`, `::icons`, `::browser`, `::packs`, `::script` and
`::script_mlua`, so a game normally depends on this crate alone.

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
| `script-mlua` | yes | `slotted-script-mlua`, the native Luau runtime. |
| `script-piccolo` | no | `slotted-script-piccolo`, the pure-Rust runtime and the only one that builds for wasm. |
| `blur` | no | Backdrop blur for glass panels. |
| `dev` | no | Exclusion-zone highlighter, id tooltips, the HUD position editor, the input recorder. |
| `viewport` | no | Real cameras behind `viewport` nodes. |

Cargo has no per-target defaults, so a wasm build passes
`--no-default-features --features "ui,browser,packs,script-piccolo"`. Luau is C++
and cannot target wasm at all; see ADR 0001.

## Licence

MIT OR Apache-2.0, at your option.
