# Phase 6 notes: package A (machine widgets, fluids, property writes)

What A changed outside its own files, and what B and C need to know. Contract:
`docs/design/phase6-contract.md`, sections 0, 1 and 4.

## Changes to shared files

### `slotted-ui/src/screen.rs` -- a def's tags are merged, not replaced

`SpawnCtx::spawn_child` used to overwrite a node's `Tags` with the def's after
the widget had spawned. An icon button writes `state=<id>` while spawning and a
slot grid writes `region=<n>`; a def that also carried `test_id` erased both.
The insert now merges: the widget's tags are kept, and the def's win on a key
they both name. Nothing else changed on that path, and `test_id` still becomes a
`TestId`.

**B and C:** a node can now carry tags a widget set *and* tags the screen author
wrote. A locator that matched on the def's tags still matches.

### `slotted-ui/src/tooltip.rs` -- `show_tooltip` calls `Widget::tooltip`

After the registered `TooltipParts` have composed, the hovered node's
`WidgetNode` kind is looked up in the `WidgetRegistry` and that widget's
`tooltip` hook appends to the same list (contract 1.2). A widget with no hook
appends nothing, so every existing tooltip is byte for byte what it was.

`tooltip_delay` needed no change: it already runs over every node with a
`Hovered`, so a `TooltipSource` node behaves like a slot as soon as the widget
attaches `Hovered` and the `on_slot_over` observer, which tank, bar, progress
and icon button all do.

### `slotted-ui/src/widgets.rs` -- icon helpers, additive

Two new public items, no existing one touched:

- `icon_image(world, &IconDef) -> ImageNode`, the spawn-path resolver;
- `IconImages`, the same thing as a `SystemParam`, for observers.

An `image` icon is an asset path; an `item` icon goes through the `Icons` port
with `placeholder_color` as the fallback. Side tabs and icon buttons use them,
and a Phase 7 widget with an `IconDef` should too.

### `slotted-ui/src/plugin.rs` -- two systems registered

`widgets::icon_button::icon_button_roles` joined the `Render` chain beside
`slot_state_roles` (it swaps `icon_button` for `icon_button.hover`), and
`viewport::orbit_viewport_cameras` joined the `viewport`-feature block. No
existing registration or ordering moved.

### `slotted-registry` -- one test file appended

`tests/data_stage.rs` gains `fluids_load_with_their_payload_intact_and_dense_ids`.
No source change: the `fluids` kind was already wired.

### `assets/data/machine/fluids/water.ron` -- new

The `machine:water` fluid the example's tank shows, in the opaque-def shape
(`(name: ..., payload: (...))`).

## Deviations from the skeleton's signatures

The contract fixes cross-package signatures. Three system parameter lists inside
A's own files grew, because a real body needs data the stub did not ask for.
None of them is called from another package; they are only named in
`plugin.rs`'s `add_systems`/`add_observer`.

| Item | Change |
|---|---|
| `tank::bind_properties` | catches `Added<TankFluidSource>` as well as `Added<PropertyBinding>`, and takes `Res<Fluids>` to resolve a static fluid name |
| `tank::on_property_changed` | second query for tanks whose `fluid_property` changed |
| `tank::render_fills` | fill origin, bar state, `AssetServer`, the `bar.text` child, `SemanticLabel` and `Commands` for the tiled `ImageNode` |
| `side_tab::on_side_tab_toggle` | `ComputedNode` to measure `open_width`, `Motion` and the theme's `Durations` for the tween |
| `icon_button::on_icon_button_cycle` / `on_icon_button_property` | the icon child, through `IconImages` |

## New public items in A's own files

- `tank::FillOrigin(Direction)` on every tank and bar root. A tank's
  `Orientation` is not a `Direction`, so the renderer needs one shape for both;
  `BarState::direction` still carries the authored value and the two are written
  together at spawn.
- `tank::fill_label`, `tank::fill_node` -- the label text and the fill child's
  `Node`, so the widgets and their tests agree on one definition.
- `bar::BarStyle::{roles, kind, size}`.
- `side_tab::ASSUMED_CONTENT_WIDTH`, `side_tab::on_header_activate`.
- `icon_button::IconButtonIcon`, `IconButtonState::next`, `icon_button_roles`.
- `virtual_grid::{SCROLLBAR_WIDTH, VirtualScrollThumb, UnknownGridSource,
  VirtualGridBuilt}` and `VirtualGridState::{total_rows, max_first_row,
  scroll_by, window}`.
- `viewport::{ViewportSize, ViewportCamera, ViewportPlaceholder, ORBIT_SPEED,
  orbit_viewport_cameras}` (the last four behind the `viewport` feature).
- `fluids::{MISSING_FLUID_COLOR, FluidDef::from_payload}`.

## Behaviour worth knowing

- **A fluid tint is re-asserted, not written once.** `apply_theme` repaints a
  node's role whenever the theme asset changes, which would undo the fluid's
  colour on the fill child. `render_fills` therefore compares and rewrites the
  tint every frame for a tank that holds a fluid. On the frame the theme
  reloads the fill shows the `tank.fill` role for one frame before the tint
  returns.
- **A malformed fluid keeps its registry slot.** `FluidId` is the registration
  index, so a payload that does not parse becomes a magenta placeholder rather
  than shifting every later id.
- **A negative fluid property means "no fluid"**, which is how an empty tank
  reports itself; `machine:water` is id 0, so `-1` and not `0` is empty.
- **Keyboard cycling always advances.** `KeyboardInput` carries no modifiers, so
  `Enter`/`Space` on a focused icon button cycles forward; shift-cycling is a
  pointer gesture (`IconButtonCycle { forward: false }` is still available to
  the harness and to scripts).
- **The virtual grid rebuilds against `VirtualGridBuilt`,** the window its
  spawned cells stand for, not against its own `VirtualGridState`: the scroll
  observer writes the state, so comparing the state with itself would never
  rebuild.
- **A viewport spawns no camera when `SlottedUiConfig.headless` is set,** even
  with the `viewport` feature compiled in. That is what the headless test
  asserts.

## Tests

- `slotted-ui/tests/machine_widgets.rs`, 19 tests through `slotted-test`.
- Unit tests in `fluids.rs`, `widgets/{tank,bar,side_tab,icon_button,virtual_grid,viewport}.rs`.
- `slotted-ecs/tests/prediction.rs`: two `SetProperty` cases.
- `slotted-registry/tests/data_stage.rs`: one `fluids` case.
