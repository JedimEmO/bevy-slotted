# glass-ui spike

Phase 0 spike for the "Obsidian Glass" direction (`docs/PLAN.md` 4.4/4.5/4.6, `docs/research/research-modern-ui.md` 4 and 5A) on Bevy 0.19.1.

Standalone package, not a workspace member. Throwaway: the findings live in `docs/adr/0003-glass-rendering.md`.

## Running

```
cd spikes/glass-ui
cargo run --release                                    # interactive, orbiting scene, move the mouse over slots
cargo run --release -- --shot                          # 2s warm-up, screenshot to shots/, exit
cargo run --release -- --shot --hover 11               # same, with a synthetic pointer parked on slot 11
cargo run --release -- --bench --secs 14               # frame-time stats, exit
cargo run --release -- --bench --no-blur --no-viewport # baseline
```

Flags: `--no-blur`, `--no-viewport`, `--no-shadows`, `--backdrop-div N`, `--hover N`, `--shot`, `--bench`, `--secs F`.

The window is fixed at 1920x1080 with `scale_factor_override(1.0)` and `PresentMode::AutoNoVsync`.
Assets and screenshot paths resolve against `CARGO_MANIFEST_DIR`, so the release binary runs from anywhere.

## What is in the picture

`shots/glass.png` (and `glass-hover.png`, `glass-no-blur.png`, `glass-no-viewport.png`) show, over an
orbiting 3D scene of six lit cubes on a ground plane:

- A glass panel: 16px radius, 1px translucent border, `BoxShadow`, tinted translucent fill, backdrop blur.
- A 9x3 grid of 44px slots, 8px radius, hairline border, 1px inner top highlight, hover and press states.
- Slots 4 and 22 with the legendary `SlotGlowMaterial`: shimmering SDF ring plus soft outer glow.
- Slot 13 with a `ViewportNode` showing a live rotating cube from its own camera.

## Measurements

RTX 4090, X11 `:1`, 1920x1080, no vsync, ~12s of samples after a 2s warm-up, `FrameTimeDiagnosticsPlugin` mean.

| config | mean ms | p50 ms | p99 ms |
| --- | --- | --- | --- |
| baseline (no blur, no viewport) | 2.31 | 2.24 | 4.42 |
| + viewport camera | 3.07 | 2.99 | 4.06 |
| + blur camera | 3.25 | 3.14 | 4.72 |
| + both | 4.31 | 4.21 | 7.10 |

Same four, with directional shadow maps off:

| config | mean ms |
| --- | --- |
| baseline | 2.08 |
| + blur camera | 2.99 |
| + blur + viewport | 3.78 |

Backdrop resolution sweep, blur camera on, no viewport:

| divisor | backdrop size | mean ms |
| --- | --- | --- |
| 1 | 1920x1080 | 3.23 |
| 2 | 960x540 | 3.17 |
| 4 | 480x270 | 3.27 |
| 8 | 240x135 | 3.09 |

Read: the extra `Camera3d` costs about 0.9 ms and that cost does not move with target resolution
or with shadows. It is fixed per-view overhead (extract, view uniforms, queue, render graph), not
fill. The blur shader and the downsample are free on this GPU. The quarter-res target is cheap
insurance for weaker hardware, not what saves the frame here.

The `ViewportNode` camera costs about the same 0.75-0.9 ms even at 128x128 on its own render
layer with one cube. One live viewport for a hovered item is fine; one per slot is not.

## Shader inputs, and how the panel finds itself on screen

The interesting result: **the glass material needs no node rect at all.**

`UiVertexOutput` (`bevy_ui::ui_vertex_output`) carries `@builtin(position)`, which in the fragment
stage is the framebuffer pixel coordinate, plus `size` and `border_radius` in pixels. Bind group 0
of the UI material pipeline is always the view uniform and the globals uniform. So:

```wgsl
let screen_uv = (in.position.xy - view.viewport.xy) / view.viewport.zw;
```

gives the backdrop UV directly, and `in.size` / `in.border_radius` drive the rounded-rect SDF.
Nothing has to be pushed from a layout system into the material every frame. The whole material
uniform is styling: tint, edge colour, blur radius in backdrop texels, and an enable flag.

The blur is a 13-tap tent (centre + two rings, weights 4/2/1) on the quarter-res image, not a real
dual-Kawase chain. A multi-pass Kawase needs render-graph nodes or a chain of 2D cameras, which
this spike did not build. Visually the tent is adequate at this radius; it will band on high-contrast
edges at larger radii.

The backdrop camera must be transform-synced to the main camera every frame or the blurred image
slides against the world behind the panel. Its `RenderTarget` is a separate component in 0.19, not
a `Camera` field.

## bsn! ergonomics

`slot()`, `slot_grid()` and `glass_screen()` in `src/ui.rs` are `bsn!` scene functions, spawned with
`commands.queue_spawn_scene(...)`. It works and reads well. What we hit:

- **No loop syntax.** A 27-slot grid is `(0..27).map(slot).collect::<Vec<_>>()` outside the macro,
  spliced in as `Children [{slots}]`. `Vec<S: Scene>` implements `SceneList`, so this is clean, but
  it means every repeated structure needs a Rust helper. Fine for us; screens come from data anyway.
- **No `..` rest syntax.** `bsn!` patches only the fields you name, so `..default()` is not just
  unnecessary, it is a parse error. Easy to trip over coming from struct-literal habits.
- **Named-field components are verbose.** `BorderColor` has four `Color` fields, so
  `BorderColor::all(c)` becomes `template_value(BorderColor::all(c))`. Same for a `Node` with many
  fields: `template_value(slot_node())` where `slot_node()` is an ordinary Rust function returning
  `Node`. This is the escape hatch and it is a good one, but a theme layer will use it constantly.
- **Handles.** `MaterialNode<M>({handle})` works because `HandleTemplate<T>: From<Handle<T>>` and the
  tuple-field position applies an implicit `.into()`. `template_value(MaterialNode(h))` does *not*
  compile: `MaterialNode` derives `FromTemplate`, so it is not itself a `Template`. The turbofish is
  required, since the macro cannot infer `M`.
- **Generic components need explicit parameters** in the macro, as above.
- **Enum and unit values** need braces: `{PositionType::Absolute}`, `{FontSize::Px(15.0)}`.
- **Spawn timing.** `queue_spawn_scene` spawns during the `SpawnScene` schedule, between `Update`
  and `PostUpdate`, and `SpawnScene` is not publicly exported, so you cannot order a `Startup`
  system after it. Post-spawn decoration runs in `Update` filtered on `Added<Slot>`.
- Inline `on(...)` observers were not exercised here; hover and press use `bevy_ui_widgets::Button`
  plus `bevy_picking::hover::Hovered` and `bevy_ui::Pressed`, read by a plain system.

## 0.19 API notes that cost time

- `BorderRadius` is a **field of `Node`**, not a component (it was a component in 0.17).
- `RenderTarget` is a **component**, not a `Camera` field.
- `DirectionalLight::shadows_enabled` is now `shadow_maps_enabled`.
- `TextFont::font_size` is a `FontSize` enum, not `f32`.
- `Hovered` lives in `bevy_picking::hover`, not in `bevy_ui_widgets`.
- Events are messages: `MessageWriter<AppExit>`, `MessageWriter<PointerInput>`.

## Picking through the viewport

The `ViewportNode` child carries `Pickable::IGNORE`, so pointer events pass through to the slot
underneath and the slot's hover state still works. Picking *into* the viewport's own scene was not
needed and was not enabled. `ViewportNode` does require the `PointerId::Custom(...)` component by
default, which is why the crate depends on `uuid` transitively; that machinery is dormant when the
node ignores picking.

## Not verified

- Only NVIDIA / Vulkan on X11. No wasm, no Metal, no integrated GPU, no llvmpipe run.
- No real dual-Kawase chain, so the "few hundred microseconds at 1080p" figure from the research
  notes is neither confirmed nor refuted for the multi-pass version.
- No measurement with many panels on screen at once, or with a full game scene rather than six cubes.
- Hot reload of the theme, `UiTransform` motion, and `BackgroundGradient` were not touched.
