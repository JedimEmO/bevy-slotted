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
| `SlottedPlugins` | The plugin group. `default()` for a game with a window, `headless()` for a test, `server()` for a dedicated server. |
| `HeadlessBevyPlugins`, `HeadlessRenderAssets` | The Bevy plugins ADR 0002 proved sufficient for layout, picking, focus and keyboard with no renderer. Behind `ui`. |
| `ServerBevyPlugins`, `ServerStack` | A task pool, a clock, states, assets and a runner. No window, no input, no layout, no renderer. |
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
| `ui` | yes | `slotted-ui`, `slotted-theme`, `slotted-icons`, `slotted-packs/ui`, **and every Bevy UI feature**: `bevy_ui`, `bevy_text`, `bevy_picking`, `bevy_window`, `bevy_scene`, `bevy_camera`, `bevy_input_focus`, `default_font`, `ui_picking`. |
| `browser` | yes | `slotted-browser`. Implies `ui`. |
| `packs` | yes | `slotted-packs` and `slotted-script`: mod discovery, `pack://`, the data/control lifecycle. Does **not** imply `ui`. |
| `script-luaur` | yes | `slotted-script-luaur`, the Luau runtime. The same one natively and on wasm (ADR 0004). |
| `net` | no | `slotted-net`: a `RemoteAuthority` over a `ClientTransport` you insert. |
| `server` | no | `packs`, `script-luaur` and `net`, and nothing from `ui`. See below. |
| `blur` | no | Backdrop blur for glass panels. Implies `ui`. |
| `dev` | no | Exclusion-zone highlighter, id tooltips, the HUD position editor, the input recorder. Implies `ui`. |
| `viewport` | no | Real cameras behind `viewport` nodes. Implies `ui` and `gpu-icons`. |
| `gpu-icons` | yes | The offscreen rig that bakes item icons into the atlas. Implies `ui`. |
| `gltf-icons` | no | `IconDef::Model`: icons baked from a glTF scene. Implies `gpu-icons`. |
| `live-icons` | no | A turning 3D item behind a tooltip. Implies `viewport`. |

`ui` is where the Bevy UI features live, not the `bevy` dependency itself, so
turning it off really removes them:

```toml
slotted = { version = "0.1", default-features = false, features = ["server"] }
```

That graph has no `bevy_ui`, `bevy_text`, `bevy_picking`, `bevy_winit`,
`bevy_window`, `bevy_scene` or `bevy_render` in it: 160 crates against the 300
a default build compiles. Mods still load, the data stage still runs and control
scripts still answer events, because `slotted-packs` has its own `ui` feature
and the half of the lifecycle that publishes screens, widget templates, tooltip
parts and HUD layers is what that feature gates. A script that calls `set_hud`
on a server gets a named error rather than a silent success.

`just server-check` asserts all of this and CI runs it as a job of its own: it
greps `cargo tree` on native and on `wasm32-unknown-unknown`, builds both, and
then runs `crates/slotted/tests/server.rs`, which compiles only in this profile
and drives one click from a `RemoteAuthority` client to an ack through a
`MenuServer` pumped by the server app's own schedule.

Pair the feature with the plugin group:

```rust
App::new().add_plugins(SlottedPlugins::server()).run();
```

The defaults build for wasm as they stand: there is one script runtime and it
is pure Rust, so a browser build takes the same feature set a native one does.
On wasm an uncaught Lua error aborts the module and the host has to restart it;
see ADR 0004 and `slotted-script-luaur`'s crate docs.

## Licence

MIT OR Apache-2.0, at your option.
