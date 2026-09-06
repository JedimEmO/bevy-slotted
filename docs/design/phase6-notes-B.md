# Phase 6 package B notes: HUD layers, editor, recording

What package B (HUD layers, the position editor, input recording and replay) added beyond the
Phase 6 contract, and every shared file it touched. Everything here is additive: no signature
another package depends on changed meaning, and nothing was removed.

## 1. Additions to B's own files

### `slotted-ui/src/hud.rs`

- **`HudAnchor::transform() -> UiTransform`** (new). `node(window)` places the wrapper's edge;
  centring the *content* on a midpoint needs a shift of half the wrapper's own size, which a
  `Node` cannot express for a content-sized node. The transform carries that shift together with
  the layer's `scale`, so the contract's "`UiTransform::from_scale(scale)`" is a `UiTransform`
  that also translates on a centred axis. A `UiTransform` translation is added after the scale
  (`Affine2::from_mat2_translation`), so the two do not interact.
- **`HudLayers::position(id)`** (new, public): the layer's index in `order()`. `insert_above`,
  `insert_below` and `replace` are written on top of it, and it is what a test asserts order with.
- **Re-placing an existing layer moves it.** `insert_above`/`insert_below` take an id already in
  the order out before inserting, so a mod that places the same layer twice never duplicates it.
- **`HudLayerDef::empty(id)`** (new): a zero-size `hud.panel`. The seven built-in placeholders are
  these, and `set_hud` on an unknown layer starts from one. It exists so `slotted-packs` can build
  a default layer without depending on `slotted-theme` for one `Role`.
- **`HudLayerPayload::from_value`** (new): the same two-step typing `ScreenDef::from_value` does,
  used by both `HudLayers::load_from_registry` and the packs `RegisterHudLayer` type check.
- **`reanchor_hud_layers`** (new system, `SlottedUiSet::Render`, after `sync_hud_layers`). It
  copies the wanted anchor (`HudLayout` override, else the def's) onto each wrapper's `HudAnchor`,
  then writes the wrapper's `Node` and `UiTransform` from it. Two things need this: a window
  resize, which changes where a centred or right-anchored layer sits, and an editor drag, which
  must move a layer *without* respawning it. `HudSpec` (the private component `sync_hud_layers`
  compares against) therefore holds the tree, the z position and the menu, but not the anchor and
  not `visible`.
- **`HudHotbar` is read at sync time, not at registration.** `register_builtin_layers` runs inside
  `SlottedUiPlugin::build`, before a game can say which menu slot its hotbar row starts at, so the
  built-in `hotbar` layer's `first` param is rewritten from the `HudHotbar` resource every sync.
  Changing the resource respawns the layer. Without this the resource would be unusable.
- `HUD_SCREEN_KIND` and the built-in `CROSSHAIR_SIZE` / `HOTBAR_OFFSET` constants are named rather
  than inline.

### `slotted-ui/src/hud_editor.rs`

- **`on_hud_drag`'s signature changed** from
  `(On<Pointer<Drag>>, Query<(&HudAnchored, &mut HudAnchor, &mut Node)>, ResMut<HudLayout>)` to
  `(On<Pointer<Drag>>, Query<(&HudAnchored, &mut HudAnchor, Option<&HudDragging>)>, ResMut<HudLayout>)`.
  Two reasons. Snapping the *accumulated* offset on every drag event quantises each delta on its
  own, so a slow drag of one pixel per frame never moves at all; the offset is computed instead
  from the drag's total `distance` and the offset the drag started at, which is what `HudDragging`
  records. And rewriting the wrapper's `Node` needs the window size, which an observer with that
  parameter list does not have, so `reanchor_hud_layers` does it.
- **`HudDragging { start }`** (new component): the offset a drag began at. Inserted by
  `on_hud_drag_start`, removed by `on_hud_drag_end`.
- **`HudEditable`** (new marker): the wrapper's drag observers are attached. Observers are attached
  once and stay; leaving edit mode removes the wrapper's `Pickable` and its outline, which is what
  stops the drags.
- **`cancel_hud_drag`** (new system, `SlottedUiSet::Input`): `Esc` during a drag restores the
  offset the drag started at. The contract asks for "Esc cancels" without saying where it lives.
- `save_hud_layout` writes only when `HudLayout` changed *and* is non-empty, so merely running the
  example never creates the file.

### `slotted-ui/src/recording.rs`

- **`InputRecorder::new(path, resolution, scale_factor, frame_delta)`** (new): fills the header a
  replay warns on. Without it every caller would have to build a `Recording` by hand.
- **`write_recording(path, recording)`** (new, public): the RON write, so a game can flush a
  recording on a key rather than only on `AppExit`.
- Key repeats are not recorded: replaying them would double every held key.

### `slotted-test`

- `replay_recording` treats recorded frame numbers as a floor, not a schedule. Every harness action
  steps a frame, so feeding two inputs recorded on one frame takes two frames; the recording's
  numbers say what may not happen before what, and `ReplayReport::frames` counts what was stepped.
- Gamepad inputs replay only when the app has a `Gamepad` entity. A headless harness has none, and
  inventing one would make a replay assert against a world the recording never saw.

## 2. Shared files, touched additively

| File | Change |
|---|---|
| `slotted-ui/src/plugin.rs` | registers `reanchor_hud_layers` (Render, in the Phase 6 chain) and `cancel_hud_drag` (Input, behind `dev`) |
| `slotted-packs/src/plugin.rs` | one `add_observer(route::on_property_changed)` |
| `slotted-packs/src/route.rs` | `SetHud`, `HudUpdate`, the private `hud_value` / `hud_def` helpers, `on_property_changed` |
| `slotted-packs/src/lifecycle.rs` | `RegisterFluid`, `RegisterHudLayer`; imports `FluidDef` and `HudLayerDef` from `slotted_registry::defs` |
| `slotted-test/src/tree.rs` | new `candidate_roots`: screens, then HUD roots bottom to top, then the carried layer. `roots` is left alone on purpose, because `screen_tree()` snapshots would otherwise all gain a HUD |
| `slotted-test/src/locator.rs` | `candidate_order` uses `candidate_roots`; `by::hud_layer(id)` is `role(HudLayer)` plus the id, which a layer root carries as its `SemanticLabel` |
| `examples/chest` | `--record <path>`, `F7` through `HudEditKey`, `HudLayoutStore`; the `slotted` dependency gains the `dev` feature |

## 3. Deviations from the contract

- `HudAnchor::node` alone does not centre a layer; see `transform()` above.
- `on_hud_drag` takes `Option<&HudDragging>` instead of `&mut Node`.
- The built-in `hotbar` layer's `first` follows the `HudHotbar` resource at runtime rather than
  being frozen when the plugin builds.
- HUD roots are in locator order but not in `screen_tree()`; `hud_tree()` shows them.
