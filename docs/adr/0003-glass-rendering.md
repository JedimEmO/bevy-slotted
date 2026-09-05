# ADR 0003: Glass rendering, backdrop blur, and `bsn!`

Status: accepted (Phase 0 spike)
Date: 2026-09-05
Spike: `spikes/glass-ui/` (screenshots in `spikes/glass-ui/shots/`)

## Context

`docs/PLAN.md` 4.4 puts the "Obsidian Glass" skin first and lists backdrop blur as an optional
`blur` feature on `slotted-theme`. Section 4.5 says `bsn!` is used internally for widget templates.
Phase 0 asks whether both are buildable on `bevy_ui` 0.19.1 and what they cost at 1080p.

The spike builds a glass panel over a live 3D scene: 16px `BorderRadius`, 1px translucent
`BorderColor`, `BoxShadow`, tinted fill, a 9x3 grid of 44px slots with hover and press states from
`bevy_ui_widgets::Button`, a shimmering rarity `UiMaterial` on two slots, and a `ViewportNode`
showing a live cube in a third.

## What we found

**Backdrop blur works, and needs no plumbing.** `UiVertexOutput` carries `@builtin(position)` (the
framebuffer pixel coordinate) plus node size and border radius in pixels, and bind group 0 of the UI
material pipeline is always the view and globals uniforms. So `(in.position.xy - view.viewport.xy) /
view.viewport.zw` is the backdrop UV, and the SDF rounded rect comes from `in.size` and
`in.border_radius`. No system has to push a node's screen rect into the material. The material
uniform holds only styling. This removes the main design worry: blur does not couple the theme layer
to the layout layer.

Implementation is a second `Camera3d` rendering the same scene into a quarter-resolution `Image`,
transform-synced to the main camera each frame, sampled by a 13-tap tent in the panel's `UiMaterial`.
It is not a real dual-Kawase chain; a multi-pass chain needs render-graph nodes, which the spike did
not build.

**The cost is one camera, not the blur.** RTX 4090, 1920x1080, no vsync, mean frame time:

| config | mean ms |
| --- | --- |
| baseline | 2.31 |
| + `ViewportNode` camera | 3.07 |
| + blur camera | 3.25 |
| + both | 4.31 |

Sweeping the backdrop target from 1920x1080 down to 240x135 moved the mean between 3.09 and 3.27 ms,
i.e. not at all. Turning directional shadow maps off moved the blur delta from 0.94 to 0.91 ms. The
~0.9 ms is fixed per-view overhead (extract, view uniforms, queue, render graph), not fill or shadow
re-rendering. The blur shader itself is free at this resolution on this GPU.

That is a large fraction of a 60 Hz budget spent on overhead we do not control, and it is a fixed
cost that a lower-end GPU will not shrink. It is also a cost the game pays whenever a glass screen is
open, which for an inventory UI is intermittent.

**`bsn!` is good enough to build widgets in.** Slot, slot grid and the panel are scene functions.
Limitations hit: no loop syntax (repeated children are a `Vec<impl Scene>` built in Rust and spliced
with `{...}`); no `..` rest syntax; named-field components like `BorderColor` and a fully specified
`Node` are best injected with `template_value(...)` from an ordinary Rust constructor; handles need
`MaterialNode<M>({handle})` with an explicit type parameter, and `template_value(MaterialNode(h))`
does not compile because `MaterialNode` derives `FromTemplate` and so is not itself a `Template`;
`queue_spawn_scene` spawns in the unexported `SpawnScene` schedule, so post-spawn decoration must run
in `Update` on `Added<T>`.

**`ViewportNode` is viable for one live preview, not for slots.** Its camera costs the same ~0.8 ms
as any other camera even at 128x128 with one cube. `Pickable::IGNORE` on the viewport child lets
pointer events reach the slot beneath, so picking into the viewport scene is not required.

## Decision

1. **Backdrop blur ships as a `blur` feature on `slotted-theme`, not a companion crate.** The
   surface is one `UiMaterial`, one WGSL file, one camera-spawn helper and one transform-sync
   system. It has no dependency the theme crate does not already have, and no coupling to
   `slotted-ui` layout. A separate crate would buy nothing and cost a version to keep in step.
   The feature is off by default. `Material::Shader` in the theme token table already covers the
   panel role, so a theme that does not enable `blur` simply maps `panel` to `Solid` or `Gradient`.
2. **`bsn!` is used internally for widget templates**, as `docs/PLAN.md` 4.5 assumed. Its limits are
   ergonomic, not structural, and every one has a plain-Rust escape hatch. `.bsn` asset files are
   still deferred until Bevy ships a loader.
3. **`slotted-icons` uses baked atlas icons as the default**, with `LiveIcons` (`ViewportNode`)
   restricted to the hovered item and detail panes, as planned. The per-camera cost rules out a
   viewport per slot; this ADR turns that from an assumption into a measurement.
4. **The theme's motion/quality budget gains a "backdrop" tier.** Blur is one of the first things a
   low-spec preset turns off, because it is a whole extra view.

## Consequences

- `slotted-theme` gains a `blur` feature, a `GlassPanelMaterial`, `assets/shaders/glass_panel.wgsl`,
  and a plugin that spawns and syncs the backdrop camera. Consumers who enable it accept roughly one
  extra millisecond per frame while a glass screen is open.
- The backdrop camera must be transform-synced to whatever camera the game considers primary. That
  is a public seam: the theme cannot guess which camera, so the plugin takes it as configuration.
- Because the material derives its screen position from `@builtin(position)`, panels can move, resize
  and animate with no material updates. Nothing invalidates on layout change.
- The 13-tap tent is what ships first. Upgrading to a real dual-Kawase chain is a later, contained
  change: same material, same inputs, different production of the backdrop image.
- `RenderTarget` being a component and `BorderRadius` being a `Node` field in 0.19 are recorded in
  the spike README so the ui crate does not rediscover them.

## Not verified

- NVIDIA / Vulkan / X11 only. No wasm, Metal, integrated GPU or software rasteriser run.
- No multi-pass Kawase, so the research note's "few hundred microseconds at 1080p" for the real
  algorithm is neither confirmed nor refuted.
- Measured against a six-cube scene, not a game scene, so the ~0.9 ms per camera is a floor. A real
  world will add per-camera culling and draw submission on top.
- Only one glass panel on screen at a time; overlapping translucent panels were not measured.
