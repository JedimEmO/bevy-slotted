# slotted-icons

Item icons for [slotted](https://github.com/mathiasmyrland/bevy-slotted).

`IconSource` is the port: hand it a stack, get back an `IconRef` the item
renderer can draw. `AtlasIcons` is the shipped adapter, one texture atlas with
one index per item.

The atlas is baked on the CPU: a coloured rounded square per item, hue from the
item id hash. That needs no camera and no `bevy_render`, so it works in the
headless harness and on wasm, and it is deterministic, so snapshot tests see the
same atlas every run. The result is an ordinary `Image` plus `TextureAtlasLayout`
pair, which is exactly what an offscreen bake of real item models will feed
later.

## Main types

| Type | What it is |
|---|---|
| `IconSource`, `IconRef`, `Icons` | The port, its result, and the resource holding the active source. |
| `AtlasIcons`, `PlaceholderAtlas` | The baked-atlas adapter. |
| `bake_placeholder_atlas`, `placeholder_color` | The CPU bake and its hue function. |
| `SlottedIconsPlugin` | The plugin. |

## Example

```rust
use bevy::prelude::*;
use slotted_icons::prelude::*;

App::new()
    .add_plugins(MinimalPlugins)
    .add_plugins(SlottedIconsPlugin)
    .run();
```

## Feature flags

| Feature | Default | Effect |
|---|---|---|
| `live` | no | `LiveIcons`: a `ViewportNode` camera per requested item, for the hovered item and detail panes only. ADR 0003 measured about 0.8 ms per camera. |

## Licence

MIT OR Apache-2.0, at your option.
