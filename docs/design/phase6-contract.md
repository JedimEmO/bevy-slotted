# Phase 6 contract: machine widgets, HUD layers, recording, mod tests

Status: v1.0, 2026-09-06. Bevy 0.19.1. Companion to `docs/PLAN.md` 4.5, 4.12, 5.2 and Phase 6,
`docs/research/research-mod-ui-catalogue.md` sections 1, 3 and the checklist, and the Phase 2 (v1.1)
and Phase 4 (v1.1) contracts, which this builds on and does not restate. Three packages implement it
in parallel without talking to each other: **A** (machine widgets, theme roles, property binding),
**B** (HUD layers, HUD editor, input recording and replay), **C** (Lua `slotted.test`, `xtask
test-mods`, playground Tests tab, `examples/machine`). The skeletons compile and carry the real
signatures; a body marked `// PHASE6-IMPL: A|B|C` is that package's to fill. Signatures change only
by amending this file. FOLLOWUPS entries tagged Phase 6 belong to the package whose area they touch.

## 0. Ground rules

- Dependency direction unchanged: `model <- registry <- ecs <- theme, icons <- ui <- browser <-
  packs <- slotted <- test`. `slotted-ui` still has no `bevy_ui_render`; every new widget lays out
  and picks headless and is inspectable through plain components (section 1.7).
- **Fill state is a component, not a render detail.** Every property-driven widget reads
  `FillValue { value: f32, max: f32 }` on its own root. A `PropertyBinding` copies `MenuProperty`
  values into it when the node sits in a menu screen; a HUD layer, a menu-less screen, a
  `HudUpdate` or a game system writes `FillValue` directly. The harness asserts on `FillValue`.
- **Host writes go through two `slotted-ecs` entity events** targeting the menu entity (skeleton:
  `events.rs`, observers `apply_set_property` (A) and `apply_set_slot` (C) in `systems.rs`):
  `SetProperty { entity, id, value }` updates `OpenMenu.state`, the `MenuProperty` child and triggers
  `PropertyChanged`, as `AuthorityEvent::Property` does; `SetSlot { entity, slot, stack }` writes the
  backing `Inventory`, emits `SlotSync` and triggers `SlotChanged` on the registered slot entity.
  They bypass prediction on purpose: the machine simulation is the authority.
- Bevy runs global observers before entity observers, so a widget whose state must change *before*
  `slotted-packs` reads its tags triggers `Activate` itself, from its own event (1.4).
- No wall clock. `MachineSim`, side-tab motion and the HUD editor read `Time<Virtual>`.

## 1. Package A: machine widgets (`slotted-ui`, `slotted-theme`, `slotted-ecs`, `slotted-registry`)

### 1.1 Data (`def.rs`, done in the skeleton)

| `type` | Fields besides `tags` | Notes |
|---|---|---|
| `tank` | `property, capacity: PropertyId, orientation: Orientation, fluid: Option<Namespaced>, fluid_property: Option<PropertyId>, unit = "mB"` | `fluid_property`'s value is a frozen `FluidId`; it wins over `fluid` |
| `bar` | `property, max, direction: Direction, text = false` | `text` adds a `"{value} / {max}"` child |
| `progress` | `property, max, direction` | a `Bar` with roles `progress`, `progress.fill`; a `Gradient` material there is the optional gradient |
| `side_tab` | `icon: IconDef, side: Side, label: Option<LocKey>, open = false, children` | |
| `icon_button` | `states: Vec<IconButtonState { id, icon: IconDef, label: LocKey }>, property: Option<PropertyId>` | at least one state |
| `virtual_grid` | `source: DataSourceId, cols, rows = 3` | `rows` is the visible window |
| `viewport` | `subject: ViewSubject, size = 96.0` | `ViewSubject` gains `Block(Namespaced)` |

Each is also a `Custom` kind (`slotted:tank`, `slotted:bar`, `slotted:progress`, `slotted:side_tab`,
`slotted:icon_button`, `slotted:virtual_grid`, `slotted:viewport`) with the same fields as params
(`widgets/*.rs` `*Params`); `kinds::all()` is 15. `SpawnCtx::spawn_child` dispatches every variant
to its `spawn_*`; `spawn_placeholder` is gone.

`FluidDef { name, color: "#RRGGBB[AA]", texture: Option<String>, unit = "mB" }` (`fluids.rs`) is the
typed form of the new **`fluids` registry kind** (`RegistryKind::Fluids`, dir `fluids`, opaque
payload; skeleton done). `Fluids(Vec<FluidDef>)` resource, indexed by frozen id (`FluidId`), filled by
`Fluids::load_from_registry` at `Startup` and at packs' Control step (call sites exist).
`ScriptCommand::RegisterFluid { id, def }` (Data) and `slotted.register_fluid(id, def)` exist; B
applies it in `lifecycle.rs` like `RegisterScreen`.

### 1.2 Tank and bar (`widgets/tank.rs`, `widgets/bar.rs`)

Root: `Node` (tank `SLOT_SIZE x 3*SLOT_SIZE`, transposed when horizontal; bar `120 x 12` along its
direction; progress `24 x 16`), `Themed(tank | bar | progress)`, `SemanticRole::Tank | Bar` (progress
is `Bar` with `WidgetNode(slotted:progress)`), `SemanticLabel("{value} / {max} {unit}")`, `FillValue`,
`PropertyBinding { menu, value, max }` when `ctx.menu` is `Some`, `TankFluid(Option<FluidId>)` +
`TankFluidSource` (tank), `BarState { direction, style, text }` (bar), `Hovered`, `Pickable`,
`TooltipSource`, the `on_slot_over` observer. Children without a role: `FillNode` with `Themed(*.fill)`
whose `Node::width`/`height` is `percent(100 * fraction)`, anchored to the fill origin (`Left` =
right-aligned, `Up` and vertical tanks = bottom-aligned); `BarText` for `text: true`.

Tank paint: fluid with `texture` -> `ImageNode { image, image_mode: Tiled { tile_x, tile_y, stretch
1.0 }, color: fluid.color() }`; otherwise `BackgroundColor(fluid.color())`; no fluid -> the
`tank.fill` material. No `UiMaterial` this phase (this crate has no `bevy_ui_render`).

Systems, `Render` set after `clear_gesture_target`, chained: `bind_properties` (`Added<PropertyBinding>`
catch-up), `render_fills` (`Changed<FillValue>` or `Changed<TankFluid>`: fill size, tint, label, text).
Observer `on_property_changed` (`slotted_ecs::PropertyChanged`) updates every bound `FillValue` and
every tank whose `fluid_property` changed. **`Widget::tooltip` is now called**: `show_tooltip`
appends `WidgetRegistry[WidgetNode(kind)].tooltip(entity, world, out)` after the parts; tank and bar
push one `Text` line `"<value> / <max> <unit>"` (a literal; `LocText` leaves unknown keys verbatim).
`tooltip_delay` treats a `TooltipSource` node like a slot.

### 1.3 Side tab (`widgets/side_tab.rs`)

The tab positions nothing: it is a flex child that changes width. Authors put tabs in a column
`Panel { role: "tab.rail" }` beside the main panel. Root: `Node` (row for `Side::Right`, row-reverse
for `Left`, clipped), `Themed(tab.side | tab.side.open)`, `SemanticRole::SideTab`, `SideTabState {
open, side, closed_width = SLOT_SIZE, open_width }`. Children: **header** (`SideTabHeader`,
`SemanticRole::Button`, `Themed(tab.side.header)`, `bevy_ui_widgets::Button`, `TabIndex(0)`, `Hovered`,
tag `side_tab=header`, `SemanticLabel(label or icon path)`, icon child, `Activate` observer ->
`SideTabToggle { entity: root }`) and **content** (`SideTabContent`, `SemanticRole::Panel`,
`Themed(tab.side.content)`, `Visibility::Hidden` while closed, the def's children inside).

`on_side_tab_toggle`: flips `open`, swaps the root role, inserts `Tween { target: TweenTarget::Size {
from, to } }` on the tab's `SideTabPanel` (never on the root, whose size the rail lays out against;
`advance_tweens` writes `Node` size), sets content
visibility, inserts `ExclusionZone` on the **content** while open and removes it when closed.
`open_width = SLOT_SIZE + content width` measured from the content's `ComputedNode` after first
layout (`4 * SLOT_SIZE` before). `Enter`/`Space` on the focused header work through `bevy_ui_widgets`;
`Motion::REDUCED` snaps. Harness: `toggle_side_tab(e)` triggers the event directly.

### 1.4 Icon button (`widgets/icon_button.rs`)

Root: `Node` (`SLOT_SIZE` square), `Themed(icon_button | icon_button.hover)`, `SemanticRole::Button`,
`SemanticLabel(current label)`, `IconButtonState { states, current, property, menu }`, `Tags` with
`state=<id>` kept current, `TabIndex(0)`, `Hovered`, `Pickable`, `TooltipSource`. **No**
`bevy_ui_widgets::Button`. Child: icon `ImageNode`. Observers on the root: `Pointer<Click>` (primary
-> `IconButtonCycle { entity, forward: !shift }`), `FocusedInput<KeyboardInput>` (`Enter`/`Space`
pressed: same), and `on_icon_button_cycle` (`On<IconButtonCycle>`): advances `current` wrapping,
rewrites `Tags["state"]`, the label and the icon, **then triggers `bevy_ui_widgets::Activate {
entity }`** so packs' `WidgetActivate` carries the new state, and triggers `SetProperty { menu, id,
value: current }` when bound. `on_icon_button_property` sets `current` from a `PropertyChanged`
for the bound property without re-triggering. `Widget::tooltip` pushes the current label. Harness:
`cycle(e, forward)` triggers `IconButtonCycle`; `activate(e)` does not cycle (semantic path).

### 1.5 Virtual grid (`widgets/virtual_grid.rs`)

`trait VirtualGridSource { fn len(&self) -> usize; fn version(&self) -> u64; fn cell(&self, index) ->
UiNodeDef }` and `VirtualGridSources(HashMap<DataSourceId, Arc<dyn VirtualGridSource>>)` resource.
Root: `Display::Grid` `cols` wide plus an 8 px scrollbar column, `Themed(virtual_grid)`,
`SemanticRole::Grid`, `VirtualGridState { source, cols, rows, first_row, total, version }`, `Pickable`
(`Pointer<Scroll>`, one row per notch; `PageUp/PageDown` on a focused cell, one page). Children: the
visible cells spawned from `cell(i)` through `SpawnCtx::spawn_child`, each with `VirtualCell(i)` and
tag `cell=<i>`; a `VirtualScrollbar` track `Themed(virtual_grid.scrollbar)` with a draggable
`Themed(virtual_grid.thumb)` child, `SemanticRole::Custom("scrollbar")`. `refresh_virtual_grids`
(exclusive, `Render`) despawns and respawns cells when `first_row`, `len()` or `version()` changed; no
pooling. Unknown source: empty grid, one warning. The browser's pooled card grid stays separate; a
Phase 7 follow-up unifies it.

### 1.6 Viewport (`widgets/viewport.rs`)

Root: `Node` (`size x size`), `bevy::ui::widget::ViewportNode { camera: None }`,
`SemanticRole::Viewport`, `SemanticLabel(subject.label())`, `ViewportSubject::{Player, Item, Block,
Entity(Entity)}` (runtime enum; `Entity` is Rust-only), `Themed(viewport)`. That is all, headless.

Feature **`viewport`** on `slotted-ui` (`bevy_core_pipeline`, `bevy_pbr`, `bevy_render`; facade
feature `viewport` forwards; `examples/machine` enables it): `spawn_viewport_cameras` (exclusive,
`Render`, skipped when `SlottedUiConfig.headless`) gives each `Added<ViewportSubject>` a `size x size`
RGBA8 `Image` target, a `Camera3d` on `RenderLayers::layer(VIEWPORT_LAYER_BASE + n)` (`n` from
`ViewportLayers`, freed by `despawn_viewport_cameras`), a light and the subject: `Item`/`Block` a
placeholder cuboid tinted with `slotted_icons::placeholder_color(name)`, `Player` a capsule,
`Entity(e)` adds the layer to `e`'s `RenderLayers` (it stays visible in the world too). The camera
orbits on virtual time. `LiveIcons` stays `Missing`: no item models exist (deviation, section 5).

### 1.7 Theme, semantics, and the render-free contract

Roles added to `roles::*` and `ALL`, defined in glass (done): `TANK, TANK_FILL, BAR, BAR_FILL,
BAR_TEXT, PROGRESS, PROGRESS_FILL, TAB_SIDE, TAB_SIDE_OPEN, TAB_SIDE_HEADER, TAB_SIDE_CONTENT, TAB_RAIL,
ICON_BUTTON, ICON_BUTTON_HOVER, VIRTUAL_GRID, VIRTUAL_GRID_SCROLLBAR, VIRTUAL_GRID_THUMB, VIEWPORT,
HUD_PANEL, HUD_CROSSHAIR, HUD_EDIT_FRAME`. `SemanticRole::{Tank, Bar, SideTab, Viewport, HudLayer}`
(AccessKit `ProgressIndicator`, `ProgressIndicator`, `Tab`, `Image`, `Pane`). `TweenTarget::Size`.

**What the harness asserts without a GPU:** `fill_of(e) -> Option<FillValue>`, `tank_fill(e) ->
f32`, `property_of(e) -> Option<(PropertyId, i32)>` (A implements; from `PropertyBinding` and the
menu's `MenuProperty`), `TankFluid`, the `FillNode` child's `Node` size and `BackgroundColor` /
`ImageNode::color` (a tint is a component), `side_tab_open(e)` and the content's `Visibility`,
`IconButtonState` and `Tags["state"]`, `VirtualGridState` plus `cell=` tags, `viewport_subject(e)` and
`ViewportNode.camera == None` under the headless group. `screen_tree()` shows the new roles; the
machine snapshot pins them. A's tests: `slotted-ui/tests/machine_widgets.rs` through `slotted-test`.

## 2. Package B: HUD layers, editor, recording (`slotted-ui`, `slotted-packs`, `slotted-test`)

### 2.1 Registry (`hud.rs`, types done)

`HudLayerId(Cow<str>)`; `builtin::{CROSSHAIR, HOTBAR, HEALTH, HUNGER, AIR, EXPERIENCE, BOSS_BAR, CHAT,
DEBUG, ORDER}`; `NineAnchor` (nine lowercase variants); `HudAnchor { anchor, offset: Vec2, scale }`
(serde, also a component on the anchor wrapper); `HudLayerDef { id, anchor, tree: UiNodeDef, visible,
hide_with_screen }`; `HudPlacement::{Top, Above(id), Below(id), Replace(id)}`; `HudLayerPayload`
(registry/script shape: `anchor, above | below | replace, visible, hide_with_screen, tree`, split by
`into_def`). `HudLayers` resource: `register` (append or replace in place), `insert_above`,
`insert_below`, `replace`, `remove`, `place`, `set_visible`, `order`, `get`, `root`, `set_root`,
`load_from_registry` (the `hud_layers` payloads). `HudMenu(Option<Entity>)` is the `OpenMenu` HUD
slot widgets bind to; `HudHotbar { first }` the hotbar's first slot. `SlottedUiConfig.hud: HudConfig
{ spawn, builtins }` (done; facade uses `..Default::default()`). With `builtins`,
`register_builtin_layers` registers `ORDER`: `crosshair` (12 px `Panel { role: hud.crosshair }`,
`hide_with_screen`), `hotbar` (`Custom slotted:hotbar (first: HudHotbar.first)`, `Bottom`, offset
`(0, -24)`) and seven zero-size placeholder panels so `insert_above("health", ..)` has a target.
`ScriptCommand::RegisterHudLayer { id, def: HudLayerPayload }` (Data),
`slotted.register_hud_layer(id, def)` exist; B applies in `lifecycle.rs`.

`sync_hud_layers` (exclusive, `Render`; runs when `HudLayers` or `HudMenu` changed): despawns roots
whose def changed or vanished, spawns missing ones. A root: full-window absolute node,
`Pickable::IGNORE`, `GlobalZIndex(zbands::HUD + position)`, `HudLayerRoot { id }`,
`SemanticRole::HudLayer`, `SemanticLabel(id)`; its one child is the **anchor wrapper** (`HudAnchored {
id }`, `HudAnchor`, absolute node from `HudAnchor::node(window)`, `UiTransform::from_scale(scale)`);
the tree spawns under it with `SpawnCtx { screen: root, kind: ScreenKind::new("slotted:hud"), menu:
HudMenu.0, parent: wrapper }`. `hotbar` is skipped while `HudMenu` is `None`. `HudLayout.anchors[id]`
overrides the def's anchor. `hud_screen_visibility` hides `hide_with_screen` layers while any
`ScreenRoot` exists. `HudUpdate { layer, path, value: HudValue::{Text, Fill { value, max }, Visible} }`
`Message`; `apply_hud_updates` resolves `path` as a `TestId` under the layer root and writes `Text`,
`FillValue` or `Visibility` (a miss is one `warn!`).

Packs (`route.rs`, stubs exist): `ScriptCommand::HudUpdate { layer, path, value }` (Control) maps
`Str -> Text`, `Bool -> Visible`, `Int/Float -> Fill { value, max: 1.0 }`, `Map { value, max } -> Fill`.
`SetHud { layer, value }` is real: `value` is a map with optional `tree`, `anchor`, `offset`, `scale`,
`visible`; an unknown layer is created on top. `ScriptEvent::PropertyChanged { menu, screen, property,
value }` is routed from `slotted_ecs::PropertyChanged` (new observer in `route.rs`). Prelude:
`slotted.cmd.hud_update(layer, path, value)` exists.

### 2.2 Position editor (`hud_editor.rs`, feature `dev`; stubs exist)

`HudEditMode(bool)`, `HudEditKey(KeyCode = F7)`, `toggle_hud_edit` (`Input`, done). While on,
`apply_hud_edit_mode` makes every wrapper `Pickable::default()`, adds a `HudEditFrame` child
`Themed(hud.edit.frame)` and `Pointer<DragStart/Drag/DragEnd>` observers; `on_hud_drag` adds the delta
to `offset` snapped to `SNAP` (4 px), writing the wrapper's `HudAnchor` and `HudLayout.anchors[id]`.
`HudLayout` is serde; `HudLayoutStore { path }` names a RON file; `load_hud_layout` at `Startup`,
`save_hud_layout` in `Last` (native `std::fs`, no-op on wasm; write only when changed). Bevy 0.19 has
no settings plugin, so "SettingsPlugin if practical" resolves to the file. The chest example binds
`F7` and stores `examples/chest/hud_layout.ron` (git-ignored).

### 2.3 Recording and replay (`slotted-ui/src/recording.rs`, `slotted-test/src/replay.rs`)

`Recording { version: 1, resolution: Vec2, scale_factor, frame_delta_us, frames: Vec<RecordedFrame {
frame, inputs }> }`, `RecordedInput::{PointerMove { pos }, PointerPress(RecordedButton),
PointerRelease(RecordedButton), Scroll { delta }, Key { key_code, logical, pressed }, GamepadButton {
button, pressed }, GamepadAxis { axis, value }}` (`RecordedButton` mirrors `PointerButton`, which has
no serde; the input enums get theirs from `bevy/serialize`, now on). Types are always compiled;
feature `dev` adds `InputRecorder { path, recording, frame }` and `record_inputs` (`First`: mirrors
`PointerInput` for `PointerId::Mouse`, `KeyboardInput`, `GamepadEvent`) and `flush_recording` (`Last`,
on `AppExit`). The chest example gains `--record <path>`. `UiHarness::replay(path) -> Result<ReplayReport
{ frames, inputs }, ReplayError>` and `replay_recording(&Recording)` (skeleton parses and checks the
version): warn on a resolution or scale mismatch, then per frame step to `frame.frame` and feed each
input through the action paths (`pointer_move_to`, `pointer_press`, `key_with`...).
`slotted-test/tests/replay.rs` builds a `Recording` of a click in code and replays it green.

### 2.4 Harness (B)

`hud_layers()` (real, done), `hud_layer(id)` (done), `hud_tree()` (stub), `set_hud_menu(menu)` (done),
`by::hud_layer(id)` (stub: match `HudLayerRoot { id }`). Locators resolve HUD roots after screens and
before the carried layer.

## 3. Package C: mod tests and the machine example

### 3.1 Protocol (`slotted-script`, done in the skeleton)

`Stage::Test` (`"test"`). Adapters install `PRELUDE` then `TEST_PRELUDE`
(`prelude/slotted_test.lua`) for that stage: C adds the three lines in mlua and piccolo and a
conformance case per adapter. A test body is a coroutine; each harness action is a yielded op:

| `ScriptEvent` | Reply |
|---|---|
| `TestList` | `TestList { names }` in registration order |
| `TestRun { name }` | first `TestStep { op: TestOp }` the body yields, or `TestDone { name, passed, message }` |
| `TestResume { value, error: Option<String> }` | next `TestStep` or `TestDone` |

`TestOp` (`testing.rs`, `#[serde(tag = "op")]`): `OpenScreen { kind, fixture }`, `Click | ShiftClick |
RightClick | Hover { loc }`, `Key { key }`, `TypeText { text }`, `Settle`, `Step { frames }`, `StackAt |
TextOf | PropertyOf | TankFill | IsVisible { loc }`, `LogContains { text }`, `Cycle { loc, forward }`.
`TestLocator { role, tag, test_id, text, widget, item, index }`, all optional; `role` is the
`SemanticRole` name in snake case. `slotted_test.lua` (shape written, C finishes and covers it)
defines `slotted.test`: `test(name, fn)`, the actions and queries above (`open_screen` returns `{ menu,
screen }`, `stack_at` returns `{ item, count }` or nil, `property_of` `{ id, value }`), and the pure
`expect`, `expect_eq`, `expect_stack(loc, item, count)`. `slotted.lua`'s dispatcher already routes
the three test events to `slotted.test.__list/__run/__resume`.

Fixtures: `fixture` is `"empty"` (default), a host alias (`h.register_fixture(alias, fixture)`;
`test-mods` registers `"chest"` = 27+27+9 empty), or a table `{ slots = {3, 27, 9}, fill = { ["1:0"]
= { item, count } } }` typed by `LuaFixture::from_value` against the harness's `Registries`.

### 3.2 Running (`slotted-test/src/{lua_tests,live}.rs`, `src/bin/test_mods.rs`, xtask)

`LuaTestResult { name, passed, message, steps }`, `LuaTestReport { mod_id, file, results }`.
`UiHarness::run_lua_tests(mod_id, file, source)`, `run_mod_tests(mod_id)` (`tests/*.lua` sorted under
`ModsUnderTest`), `register_fixture`. `trait TestDriver { fn perform(&mut self, world, op) ->
StepOutcome::{Done(Result<Value, String>), Pending} }`, implemented by `UiHarness` (op through the
harness: `OpenScreen` -> `open_mod_screen`, `Settle` -> `settle`) and by `live::LiveDriver`, which
performs one op per frame against a running app: `Settle` is `Pending` until `ActiveMotions == 0 &&
PendingRoundTrips == 0` two frames running, `Step` counts frames, `OpenScreen` **uses the open
screen** and fails with `"expected screen <kind>, found <other>"`. `lua_tests::ops` holds the
`World`-level pieces both share: `to_locator`, `resolve_one` (done), `query`. The test script is
loaded through `ScriptHost` with `Stage::Test` and unloaded afterwards.

`slotted-test` now builds for wasm32 (done, in `just wasm-check`): its `slotted` dependency is a path
dep with `default-features = false, features = ["ui", "browser"]` (native consumers unify
`script-mlua` back in; the crate's own tests add it as a dev-dependency), `insta` is behind default
feature `snapshots`, `uuid` gets `js` on wasm. `src/bin/test_mods.rs` (`required-features =
["script"]`): `test-mods <dir> [--filter <mod>]` builds a headless harness with `.mods_dir(dir)`, loads,
runs every mod's tests, prints `ok <mod>/<file>: <name>` or `FAIL ...: <message>` plus a summary, exits
1 on failure. `cargo xtask test-mods <dir>` shells out to it (done); `just test-mods` defaults to
`examples/machine/mods`.

Playground: a **Tests** tab beside the file tabs listing the selected mod's bundled `tests/*.lua`, a
Run button, `Request::RunTests { mod_id }` and `bridge::run_tests(mod_id)` (both exist), a
`LiveTestRunner` resource driving `LiveDriver` one op per frame and logging `ok`/`FAIL` through
`Bus::log("test", ..)`. **Chosen: run against the live app.** A second headless harness inside the
page would double the world, need its own window and camera fakes beside the real ones, and could not
show the player what the test does; against the live app `open_screen` means "the screen you have
open", which is what a modder iterating in the editor wants. Stated cost: playground tests are not
isolated, and a fixture the open screen does not match fails with that message.

### 3.3 `examples/machine` (skeleton crate exists and opens headless)

`machine:furnace` in `screens/furnace.screen.ron` (written): a row of the main `panel` and a
`tab.rail` column. Main: title + `Anchor title_end`; a row `Tank(6, 7, fluid_property 8, test_id tank)`,
`Slot 0 input`, `Progress(2, 3, right, test_id cook)`, `Slot 2 output`, `Bar(4, 5, up, text, test_id
energy)`; a row `Slot 1 fuel`, `Progress(0, 1, up, test_id burn)`; player grid `first 3`, hotbar `first
30`. Rail: `SideTab redstone` holding `IconButton ignore/low/high, property 9, test_id redstone_mode`;
`SideTab sides` holding `Custom machine:face_config (first_property: 10)`. `menu_def()` (done):
`MenuDef::generic(3)` with slot 2 `Output`, properties `0 burn, 1 burn_max, 2 cook, 3 cook_max, 4
energy, 5 energy_max, 6 tank, 7 tank_cap, 8 tank_fluid, 9 redstone_mode, 10..16 faces` (`props::*`).
Fluid `machine:water` in `assets/data/machine/fluids/water.ron` (`#3B7DD8B0`); `load_registries`
runs `demo` and `machine`. `machine_sim` (`Update`, virtual time; stub): burns fuel (one fuel item ->
`burn = burn_max` via `SetSlot`), cooks while `burn > 0` and the redstone gate allows (`0 ignore, 1 needs
signal off, 2 needs on` against `Redstone(bool)`, `R` toggles), moves one input to output through
`SetSlot` at `cook >= cook_max`, drains energy and tank through `SetProperty`. `machine:face_config`
(`FaceConfigWidget`, machine-local `Custom`): a 3x3 panel of six `IconButton`s cycling
`none/input/output` bound to `first_property + face`; local because a generic face widget needs
block orientation data the ui crate lacks. `main.rs` (stub): as `modded`'s, plus `--shot`, `--redstone`.

`mods/sorter/` is a **copy** of the modded sorter (written) injecting at **`slotted:any`**, the new
wildcard target (`ScreenKind::any()`; `Injections::at` and the unmatched-anchor report honour it; done),
with no dependency and `tests/sort.lua` (written: fixture with two unsorted stacks, click
`sorter_sort`, `settle`, `log_contains("sorting")`, `expect_stack` on player slot 0). Copied rather
than shared because the modded screenshots pin that example's three mods. `tests/ui.rs` (one test
passes today, one `#[ignore]`d): tree snapshot; cooking advances `cook` and moves an item; tank fill
follows the property; the redstone button cycles by click and shift-click and the property follows;
a side tab opens on click and publishes an exclusion zone the browser respects;
`run_mod_tests("sorter")` all pass; `assert_conserved` after the sim ran.

## 4. Ownership

| Package | Files |
|---|---|
| A | `slotted-ui/src/widgets/{tank,bar,side_tab,icon_button,virtual_grid,viewport}.rs` bodies, `fluids.rs` bodies, `Widget::tooltip` impls in `widgets.rs` and the `show_tooltip` hook, `tooltip_delay` for `TooltipSource`, `slotted-ui/tests/machine_widgets.rs`; `slotted-ecs::apply_set_property`; `slotted-registry` fluids tests; harness `property_of`; `assets/data/machine/fluids/water.ron` |
| B | `slotted-ui/src/{hud,hud_editor,recording}.rs` bodies; packs `RegisterHudLayer`, `RegisterFluid`, `HudUpdate`, `SetHud`, `PropertyChanged` routing; `slotted-test/src/replay.rs` body, `hud_tree`, `by::hud_layer`, `tests/replay.rs`; chest `--record`, `F7`, `hud_layout.ron` ignore |
| C | adapters' `TEST_PRELUDE` install and conformance cases, `slotted_test.lua` completion; `slotted-test/src/{lua_tests,live}.rs` bodies, `src/bin/test_mods.rs`, `xtask test-mods` polish; playground Tests tab, bundle tests, `LiveTestRunner`; `examples/machine/**` bodies and tests; `slotted-ecs::apply_set_slot` |
| shared (amend here first) | `def.rs`, `semantic.rs`, `roles`, `ScriptEvent`/`ScriptCommand`/`Stage`/`TestOp`, `hud.rs` types, `recording.rs` types, `Injections::at`, `RegistryKind::Fluids`, `SlottedUiConfig`, every `Cargo.toml` and `justfile` |

## 5. Deviations from PLAN.md

- Tanks tint a tiled `ImageNode` or a solid fill; no `UiMaterial` in `slotted-ui`. `Material::Shader`
  stays unimplemented until Phase 7.
- Side tabs are flow children in a `tab.rail` panel, not edge-absolute overlays; the exclusion zone,
  not geometry, keeps the browser clear. **Amended after play testing**: the tab *root* is a flow
  child and is always one header wide and one header tall, but the content opens into a sibling node
  with `PositionType::Absolute` anchored outside that root. Growing the root itself widened the rail
  and pushed the machine panel sideways every time a tab opened.
- `icon_button` is its own widget without `bevy_ui_widgets::Button`, so the state change precedes
  `Activate` deterministically.
- The virtual grid respawns cells; the browser card grid keeps its pool this phase.
- `viewport` shows placeholder geometry behind a `viewport` feature; `LiveIcons` stays `Missing`
  (no item models exist; the FOLLOWUPS entry moves to Phase 7).
- `HudLayout` persists to a RON file, not a settings plugin (Bevy 0.19 has none).
- Mod tests are coroutine-driven through the existing dispatch protocol, not a new runtime method;
  the playground runs them against the live app.
- `Injections` gains one wildcard target (`slotted:any`) instead of a per-screen-type filter.
- `slotted-test` compiles for wasm32 (facade features narrowed, `insta` optional) so the playground
  shares the op implementations.
