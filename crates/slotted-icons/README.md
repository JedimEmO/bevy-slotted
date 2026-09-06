# slotted-icons

Item icons for [slotted](https://github.com/JedimEmO/bevy-slotted).

`IconSource` is the port: hand it a stack, get back an `IconRef` the item
renderer can draw. `AtlasIcons` is the shipped adapter, one texture atlas with
one index per item.

An item says what it looks like in its `icon` field: a texture path, a lit
primitive (`cube`, `slab`, `ingot`, `gem`, `rod`, `sphere`) with a colour and
material parameters, or a glTF model. An item that says nothing gets a cube in
a colour hashed from its id, so no screen ever shows a grid of missing
textures.

## Two bakes, one description

What a cell draws is decided once, as a `shape::CellDraw`, and drawn twice.

The **CPU bake** rasterises flat polygons. It needs no camera and no
`bevy_render`, so it works in the headless harness and on wasm; it is
deterministic, so snapshot tests see the same atlas every run; and the result
is an ordinary `Image` plus `TextureAtlasLayout`.

The **GPU bake** (`gpu` feature) renders the same cells as lit meshes under a
fixed three-point rig, into a grid render target that *is* the atlas. There is
no readback at any point, which is what makes it work on WebGL2. The target
starts life holding the CPU bake, so a screen drawn before the rig's pipelines
have compiled shows flat-shaded icons rather than nothing.

The rig switches its camera off as soon as it has drawn: a render-world system
reports how many of the bake view's pipelines are compiled, and the main world
stops once every one of them is ready and no glTF is still loading. A timeout
warns and gives up rather than leaving a camera running.

## glTF models

With the `gltf` feature, a `model` icon loads a glTF scene, normalises it to a
unit cube and lights it under the same rig. Write the shape fields beside
`model` and they become the model's stand-in: what the CPU bake draws, and the
angle and size the model itself is rendered at.

```ron
icon: Some((model: "models/pickaxe.gltf", shape: "rod", color: "#6b4c33"))
```

Without the feature, or with no renderer at all, the stand-in is what a slot
shows. A model that declared none gets a box in a colour hashed from its path.

## Main types

| Type | What it is |
|---|---|
| `IconSource`, `IconRef`, `Icons` | The port, its result, and the resource holding the active source. |
| `AtlasIcons`, `BakedAtlas` | The baked-atlas adapter and the bake's output. |
| `shape::CellDraw` | What one cell draws. The one description both bakes read. |
| `bake_icon_atlas`, `bake_placeholder_atlas` | The CPU bake, with and without `icon` defs. |
| `gpu::IconBakeRig`, `gpu::IconBakeProgress` | The GPU rig and the render world's readiness signal. |
| `SlottedIconsPlugin` | The plugin. |

## Example

```rust
use bevy::prelude::*;
use slotted_icons::prelude::*;

App::new()
    .add_plugins(MinimalPlugins)
    .add_plugins(SlottedIconsPlugin::default())
    .run();
```

## Feature flags

| Feature | Default | Effect |
|---|---|---|
| `gpu` | no | The offscreen three-point rig. Falls back to the CPU bake at runtime when the app has no renderer, so a headless app pays only compile time. |
| `gltf` | no | `IconDef::Model`: the rig loads and lights a glTF scene. Implies `gpu`; adds `bevy_gltf`. |
| `live` | no | `LiveIcons`: a `ViewportNode` camera per requested item, for the hovered item and detail panes only. ADR 0003 measured about 0.8 ms per camera. |
| `hidpi` | no | Bake at 128 px a cell instead of 64. |

Through the facade these are `slotted/gpu-icons`, `slotted/gltf-icons` and
`slotted/live-icons`; `gpu-icons` is on by default there.

## Licence

MIT OR Apache-2.0, at your option.
