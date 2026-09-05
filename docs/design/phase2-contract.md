# Phase 2 contract: ecs, theme, ui, icons, test harness

Status: v1.1, 2026-09-05, amended in integration. Bevy 0.19.1. Companion to `docs/PLAN.md` 4.3
to 4.6, 4.11, 4.12 and ADRs 0002, 0003. Three agents built against v1 without talking to each
other, so anything not written here is a private decision of the crate that owns it; where two of
them made incompatible calls, the section is marked **amended in integration** and says which way
it went. Their notes are `docs/design/phase2-notes-{A,B,C}.md`. Phase 2 is implemented: no
`// PHASE2-IMPL` stub bodies remain, and what Phase 2 deliberately leaves for later is listed in
`docs/FOLLOWUPS.md`.

## 0. Ground rules

- Dependency direction: `model <- registry <- ecs <- theme, icons <- ui <- slotted (facade) <- test`.
  `ecs` never imports `bevy_ui`. `ui` never imports `slotted-test`. `theme` knows nothing of `ecs`.
- Bevy 0.19 event split: `EntityEvent` (targets one entity, observed) for everything a widget or
  game reacts to; `Message` (buffered) for high-volume sync; no global `Event`s in Phase 2.
- Observers never mutate inventories. They enqueue; systems in named sets apply. This is what
  makes a frame deterministic and the harness's `settle()` meaningful.
- Ports are resources holding `Arc<dyn Port>`: `slotted_ecs::Authority`, `slotted_icons::Icons`,
  `slotted_ecs::Registries`. Insert your own before `add_plugins` to replace an adapter.
- Frame order in `Update`, configured by `SlottedUiPlugin` (the only crate that sees all three):
  `SlottedUiSet::Input -> SlottedEcsSet::{Input, Predict, Submit, Reconcile} -> SlottedUiSet::Render
  -> SlottedThemeSet::{Motion, Apply} -> SlottedUiSet::Semantics`. `SlottedUiSet::Layout` runs in
  `PostUpdate` after `UiSystems::Layout`.
- No wall clock anywhere. Everything reads `Time<Virtual>`.
- Workspace lint changes: `needless_pass_by_value` and `type_complexity` are allowed (Bevy systems
  and queries trip them by design). `[profile.dev.package."*"] opt-level = 3` as in the spikes.

## 1. slotted-ecs (agent A)

Bevy features: `std` only. Compiles on `wasm32-unknown-unknown` with `--no-default-features`.

Components (`menu.rs`):

| Component | On | Meaning |
|---|---|---|
| `Inventory(slotted_model::Inventory)` | any world entity | `Deref`/`DerefMut`. The source of truth. |
| `OpenMenu { def: Arc<MenuDef>, state: MenuState, inventories: Vec<Entity>, id: MenuId, actor: Actor }` | menu entity | `inventories[i]` backs `InventoryRef::new(i)`; lengths must match `def.inventory_sizes()` or every action is `MenuMismatch`. |
| `Carried(Option<ItemStack>)` | menu entity | Mirror of `state.carried`, kept for `Changed<Carried>` queries. |
| `MenuProperty { id: PropertyId, value: i32 }` | child of menu | One per `def.properties`. |
| `SlotEntities(HashMap<SlotIx, Entity>)` | menu entity | Reverse index of `SlotRef`s, filled by `register_slot_refs`, which also emits the slot's current contents. |
| `SlotRef { menu: Entity, slot: SlotIx }` | any UI entity | The one thing `ui` attaches to make a node a slot. |
| `Favorite` | `SlotRef` entity | Mirrored from `Inventory::is_favorite` in `Reconcile`. |

Resources: `Authority(Arc<dyn slotted_model::Authority>)` (defaulted to `LocalAuthority` at
`Startup` if absent), `PendingRoundTrips(u32)`, `Registries(Arc<FrozenRegistries>)` with
`.lookup() -> RegistryLookup` implementing `LookupCtx` (unknown item: max stack 1, no tags),
`ActionQueue(VecDeque<MenuAction>)`, `ClickInterpreter { double_click_window, last_click }`,
`MenuIdAllocator`.

Entity events (`events.rs`): `SlotClicked { entity: slot, button: Button, modifiers: Modifiers }`,
`MenuAction { entity: menu, action: ClickAction }`, `MenuOpened { entity, id }`, `MenuClosed { entity, id }`
(triggered before despawn), `SlotChanged { entity: slot, menu, slot, stack }` (only for slots with a
`SlotRef`), `PropertyChanged { entity: property child, menu, id, value }`. Message: `SlotSync { menu,
slot, stack }` for every changed slot, always.

Flow (**amended in integration**). `SlotClicked` -> observer `interpret_slot_click` (resolves
`SlotRef`, vanilla mapping: left/right `Pickup`, shift+left `QuickMove`, middle `Clone`, second
left click on the same slot within `double_click_window` of `Time<Virtual>` `PickupAll`) ->
`commands.trigger(MenuAction)` -> observer `enqueue_menu_action` pushes to `ActionQueue`.
`Predict` drains the queue in order: clone the menu's inventories into a `slotted_model::Inventories`,
`apply_click(def, inv, state, action, actor, &registries.lookup())`, write changed inventories back,
update `Carried`, write `SlotSync`, trigger `SlotChanged` for registered slots, stash the delta.
`Submit`: `authority.submit(id, action, &delta)`, `PendingRoundTrips += 1`; `Err(Rejected)` rolls
back by requesting a resync path. `Reconcile`: `authority.poll()`; `Ack` decrements the counter;
`Resync` overwrites `state` and the inventory components and re-emits every slot; `Property`
updates the child and triggers `PropertyChanged`; then mirror `Favorite`.

Drag painting does **not** go through `SlotClicked`. That event carries one completed click, so an
observer of it can never see press, move and release; `ui` triggers it on `Pointer<Release>`.
`ClickInterpreter` instead exposes `begin_drag`, `paint` and `end_drag`, which return the three
`ClickAction::Drag` stages, and whoever owns the pointer stream triggers `MenuAction` with them
directly. In Phase 2 that is `slotted-ui`'s `on_slot_drag_start` / `drag_enter` / `drag_end`, with a
`DragPaint` resource suppressing the `Release` picking sends after a drag so a paint never also
reads as a click. Number keys are the same story: `ClickInterpreter::swap(slot, hotbar)` builds the
action and the crate that has the hover information triggers it. This is the reconciliation of
`docs/design/phase2-notes-A.md` item 7 and `-B.md` item 8, which agreed.

`register_slot_refs` (**amended in integration**) runs in `SlottedEcsSet::Input` and does two
things for every `Added<SlotRef>`: it inserts the entity into its menu's `SlotEntities`, and it
writes one `SlotSync` and triggers one `SlotChanged` carrying the slot's current contents. Slot
entities are spawned after the menu is opened, so without this seeding they would never have seen a
`SlotChanged` and a freshly spawned screen would render empty until the player's first click.

Functions: `open_menu(&mut Commands, &mut MenuIdAllocator, Arc<MenuDef>, Vec<Entity>, Actor) -> Entity`
(spawns `OpenMenu + Carried + SlotEntities`, property children, triggers `MenuOpened`);
`close_menu(&mut Commands, menu, id)`. Toolbar buttons, number keys and the harness all trigger
`MenuAction` directly; `SlotClicked` is only for pointer gestures on slots.

Tests to write: `MinimalPlugins` app, `RecordingAuthority` and `RejectingAuthority` in
`slotted-testutils`, prediction then forced `Resync` converges, `PendingRoundTrips` returns to zero.

## 2. slotted-theme (agent B)

Bevy features: `std, bevy_asset, bevy_ui, bevy_log`. Feature `blur` adds `bevy_render,
bevy_core_pipeline, bevy_ui_render, bevy_shader` and the `blur` module.

`Role(Cow<'static, str>)`, dotted; `Role::parent()` strips one segment. Well-known constants in
`roles::*`: `PANEL, PANEL_TITLE, SLOT, SLOT_HOVER, SLOT_FOCUS, SLOT_CARRIED, TOOLTIP, TOOLTIP_FRAME,
BUTTON, BUTTON_PRIMARY, BUTTON_HOVER, TEXT, TEXT_MUTED, COUNT, RAIL`; `roles::ALL` for completeness.

`Theme { name, tokens: Tokens, roles: HashMap<Role, Material> }` is an `Asset` loaded from
`*.theme.ron` by `ThemeLoader`. `Tokens { spacing{xs,sm,md,lg,xl}, radii{sm,md,lg},
elevation: BTreeMap<String, Elevation{y,blur,spread,color}>, durations{fast,normal,slow} (ms),
blur{radius, backdrop_divisor}, palette: BTreeMap<String, ThemeColor>, rarity: BTreeMap<String,
ThemeColor> }`. `ThemeColor(String)` is `"#RRGGBB[AA]"` or `"$palette_name"`; `Theme::color()`
resolves (magenta on failure). `Theme::material(role)` falls back through parents.
`assets/themes/glass.theme.ron` is the reference file; `theme.rs` tests pin that it parses, defines
every well-known role, has no dangling `$refs`, and round-trips. Files use `#![enable(implicit_some)]`.

`Material` and the components `apply_theme` writes (and nothing else; layout stays `ui`'s):

| Variant | Writes |
|---|---|
| `Solid { fill, border?, radius?, elevation? }` | `BackgroundColor`, `BorderColor`, `Node::border_radius`, `BoxShadow` |
| `Gradient { angle, stops, border?, radius?, elevation? }` | `BackgroundGradient`, border, radius, shadow |
| `Sliced { image, border, scale, tint? }` | `ImageNode` with `NodeImageMode::Sliced` |
| `Glass { tint, blur?, border?, radius?, elevation?, fallback_alpha_boost }` | with `blur`: `MaterialNode<GlassPanelMaterial>`; without: as `Solid` with the alpha boost |
| `Shader { shader, params }` | logged and skipped in Phase 2 |
| `Text { color, size }` | `TextColor`, `TextFont::font_size` |

`Themed(Role)` component; `ActiveTheme(Handle<Theme>)` resource. `apply_theme`
(`SlottedThemeSet::Apply`) repaints `Changed<Themed>` every frame and every `Themed` node when
`ActiveTheme` changes or `AssetEvent<Theme>::{Added, Modified, LoadedWithDependencies}` arrives for
the active handle. State is a role change: `ui` swaps `Themed(SLOT)` for `Themed(SLOT_HOVER)`.

Motion: `Motion { scale: f32, reduced: bool }` resource (`Motion::REDUCED`),
`MotionPreset::{Hover, Press, DropSquash, FlyToSlot, Stagger, Fade}`,
`Motion::duration(preset, &tokens.durations)` (zero when reduced). `Tween { target: TweenTarget,
duration, elapsed }` component with `TweenTarget::{Scale, Translate, Alpha}`; `advance_tweens`
(`SlottedThemeSet::Motion`) advances by `Time<Virtual>`, applies ease-out cubic to
`UiTransform`/`BackgroundColor`, removes finished tweens and sets `ActiveMotions(u32)`. The harness
waits on `ActiveMotions`. No `bevy_tweening` in Phase 2.

`blur` (ADR 0003): `GlassPanelMaterial` (uniform: tint, edge colour, blur radius, enabled flag;
texture: backdrop), `assets/shaders/glass_panel.wgsl` (copied from the spike), `BackdropSource`
marker for the game's camera, `BackdropCamera`, `BackdropImage`, `BackdropPlugin` that spawns the
quarter-res camera and transform-syncs it. Screen UV comes from `@builtin(position)`; no layout
data reaches the material.

## 3. slotted-icons (agent A)

Bevy features: `std, bevy_asset, bevy_image, bevy_ui, bevy_log`; direct dep `wgpu-types` for
`Extent3d`/`TextureFormat`. Feature `live` adds `bevy_camera` and the `LiveIcons` stub.

`trait IconSource: Send + Sync { fn icon(&self, stack: &ItemStack) -> IconRef }`.
`IconRef::{Atlas { image: Handle<Image>, layout: Handle<TextureAtlasLayout>, index }, Solid(Color),
Missing}`. `Icons(Arc<dyn IconSource>)` resource. `AtlasIcons { image, layout, index: HashMap<ItemId,
usize> }` is the adapter. `bake_placeholder_atlas(items, cell) -> PlaceholderAtlas` draws a rounded
square per item, hue from an FNV hash of `namespace:path`, into an RGBA8 image and a
`TextureAtlasLayout`; `SlottedIconsPlugin { cell: 64 }` bakes from `Registries.items` at `Startup`
unless `Icons` already exists. Implemented; tests pin grid layout and colour stability.

CPU rather than an offscreen camera for Phase 2 because: no camera and no `bevy_render` means it
runs in the headless harness and on wasm; the result is deterministic so snapshots are stable; and
the output is an ordinary `Image` plus layout, so the item renderer path is identical to what the
real camera bake will feed later. There are no item models to render yet anyway.

## 4. slotted-ui (agent B)

Bevy features: `std, bevy_asset, bevy_camera, bevy_input_focus, bevy_log, bevy_picking, bevy_scene,
bevy_text, bevy_ui, bevy_ui_widgets, bevy_window, ui_picking`; direct dep `accesskit 0.24`. No
`bevy_ui_render`: this crate lays out and picks, it does not draw. `bevy_scene` is for `bsn!`
(spawn timing caveats in ADR 0003; post-spawn decoration runs in `Update` on `Added<T>`).

### 4.1 Data (`def.rs`)

`UiNodeDef` has the PLAN 4.5 variants plus `tags: Tags` on every entity-producing variant. Two
deliberate deviations from the plan's sketch:

1. **Internally tagged serde** (`#[serde(tag = "type", rename_all = "snake_case")]`) and unit enums
   as lowercase strings (`TextRole`, `LayoutDirection`, `Orientation`, `Direction`, `Side`).
   `slotted_registry` keeps screen payloads as `ron::Value`, which forgets variant names
   (`defs.rs`, "the Value-safe rule"), so `SlotGrid(..)` would not survive the patch stage. RON now
   reads `(type: "slot_grid", inventory: 0, cols: 9, rows: 3, first: 0)`, which is also exactly the
   Lua table shape. Pinned by `def::tests::chest_screen_parses_and_round_trips`.
2. `SideTab.icon` is `IconDef::{Item(Namespaced), Image(path)}`, not the runtime `IconRef`
   (handles are not data). `Custom.params` is `slotted_registry::Value` (`Value::Unit` when absent).

`ScreenDef { kind: ScreenKind, inherits: Option<ScreenKind>, root: UiNodeDef, listring: Vec<InventoryRef> }`.
`ScreenKind(Namespaced)`, `WidgetKind(Namespaced)`, `AnchorId(String)`, `LocKey(String)`,
`DataSourceId(Namespaced)`, `Tags(BTreeMap<String,String>)` (also a `Component`; key `test_id` is
reserved and becomes `TestId`). `Layout { direction, gap, padding, width?, height?, center }` where
gap and padding are multiples of `tokens.spacing.sm`.

### 4.2 Node to entities mapping (what the harness may rely on)

Every spawned node: `Node`, its `Tags` (if any), `TestId` from the `test_id` tag, `SemanticRole`.
Purely visual children (highlight strips, icon and count children) have **no** `SemanticRole`, so
the semantic tree skips them.

| Def | Root components | Children |
|---|---|---|
| `Panel` | `Themed(role)`, `SemanticRole::Panel`, `Node` from `Layout` + spacing tokens | the def's children |
| `Text` | `Text(key)`, `Themed(style.role())`, `SemanticRole::Text`, `SemanticLabel(text)` | none |
| `Slot` / each grid cell | `SlotRef { menu, slot }`, `ItemView`, `Themed(SLOT)`, `SemanticRole::Slot`, `SemanticLabel("<item> x<count>")`, `bevy_ui_widgets::Button`, `Hovered`, `TabIndex`, `AutoDirectionalNavigation`, `Pickable` default; observers on `Pointer<Release>` that trigger `SlotClicked` with `Modifiers` from `ButtonInput<KeyCode>`, and on `Pointer<Over>` that trigger `TooltipRequest` | `ItemIcon` (`ImageNode`, `Pickable::IGNORE`), `ItemCount` (`Text`, `Themed(COUNT)`, `Pickable::IGNORE`) |
| `SlotGrid` | `Display::Grid` node, `SemanticRole::Grid`, `WidgetNode(slotted:slot_grid)`, tag `region` defaults to the inventory index if unset | `cols*rows` slots for `SlotIx(first + i)`, row-major, each also carrying the grid's tags |
| `Button` | `bevy_ui_widgets::Button`, `Themed(BUTTON)`, `SemanticRole::Button`, `WidgetNode(widget)`, `TabIndex`, `Activate` observer | label text |
| `Anchor` | `AnchorNode(id)`, `SemanticRole::Anchor`, zero-size node | injected nodes, in registration order |
| `Custom` | whatever the `Widget` spawns; must include `WidgetNode(kind)` and a `SemanticRole` | widget-defined |
| `slotted:action_rail` | `Themed(RAIL)`, `SemanticRole::Rail`; params `(actions: ["sort","quick_stack","deposit_all","loot_all"])` | one `Button` per action with `RailAction(ToolbarAction)`, tag `action=<name>`; `Activate` -> `MenuAction { Toolbar(..) }` |
| `slotted:hotbar` | a `SlotGrid` 9x1, `SemanticRole::Hotbar`; params `(first: u16)` | slots tagged `region=hotbar` |
| `slotted:tooltip` | `Themed(TOOLTIP)`, `SemanticRole::Tooltip`, `TooltipHost(target)`, `Pickable::IGNORE` | a `Themed(TOOLTIP_FRAME)` child, then the composed parts |
| screen root | `ScreenRoot { kind, menu }`, `SemanticRole::Screen`, full-window centring node, `GlobalZIndex(zbands::SCREEN)`, `Pickable::IGNORE` | the def root |

Slot state roles (`slot_state_roles`, `Render` set): `Hovered` -> `SLOT_HOVER`; `InputFocus` on it
-> `SLOT_FOCUS`; hovered while the menu's `Carried` is `Some` -> `SLOT_CARRIED`; else `SLOT`.
Focus wins over hover; carried wins over both.

### 4.3 Spawning, injection, lifecycle (`screen.rs`)

`Screens(HashMap<ScreenKind, Arc<ScreenDef>>)` resource: `register`, `get`, `load_from_registry`
(deserialises every `FrozenRegistries.screens` payload at `Startup`), `resolve` (flattens
`inherits`; Phase 2 screens do not inherit). `Injections(Vec<Injection { target, anchor, node,
exclusion }>)`; `spawn_screen` splices `Injections::at(kind, anchor)` under the anchor node and adds
`ExclusionZone` when asked. `WidgetRegistry(HashMap<WidgetKind, Arc<dyn Widget>>)`; built-ins are
registered by the plugin (`widgets::register_builtins`, kinds in `widgets::kinds`).

`trait Widget: Send + Sync { fn spawn(&self, ctx: &mut SpawnCtx, params: &Value, children: &[UiNodeDef]) -> Entity;
fn tooltip(&self, entity, &World, &mut Vec<UiNodeDef>) {} }`. `SpawnCtx { world: &mut World, screen,
kind, menu: Option<Entity>, parent }` with `spawn_child(&UiNodeDef) -> Entity`, the single dispatch
point (`Custom` goes through the registry, everything else through the built-in widget of that
variant). The spawn runs as a `Command` (`SpawnScreen`) so it has `&mut World`.

`spawn_screen(&mut Commands, Arc<ScreenDef>, menu: Option<Entity>) -> Entity` returns the
pre-reserved root; the tree exists after commands apply. (Plan sketch took `&Injections`; it now
reads the resource, so injection registration order is the only coupling.) `close_screen(&mut
Commands, root)` triggers `ScreenClosed` then despawns. The facade or game observes
`slotted_ecs::MenuOpened` to call `spawn_screen`; the harness does both in `open_screen`.

Lifecycle entity events on the root: `ScreenSpawned { entity }` (tree exists, no layout yet),
`ScreenLayout { entity, rect }` (once, first frame the panel has a size, from `SlottedUiSet::Layout`),
`ScreenClosed { entity }`.

### 4.4 Layers, exclusions, tooltips, items, navigation

`zbands::{HUD = 0, SCREEN = 100, BROWSER = 200, TOOLTIP = 1000, CARRIED = 2000, DEV = 9000}` are the
only `GlobalZIndex` values this crate writes. The plugin spawns one `TooltipLayer` and one
`CarriedLayer` root (full-window, `Pickable::IGNORE`, `SemanticRole::Carried` on the latter). The
carried layer's single child is an `ItemView` mirroring the active menu's `Carried`, positioned at
the primary pointer each frame in `Render`.

`ExclusionZone` marker; `Exclusions` `SystemParam` with `union(screen) -> Vec<Rect>` (logical px,
descendants only, not merged).

Tooltips (**amended in integration**). The `Pointer<Over>` observer on a slot does not request a
tooltip; it inserts `HoverStart(Time<Virtual>::elapsed)`, and `tooltip_delay` (`Render`) triggers
the request once `durations.hover_delay_ms` has passed. `tooltip_delay` raises the request exactly
twice: once when a hovered slot past the delay has no `TooltipContent`, and again on the frame a
shift key is pressed or released while one is shown. It must not re-assert the shift-derived tier
every frame, because that would undo a `TooltipRequest` raised by anything else (a widget, a
script, or the harness's `request_tooltip`, which bypasses the delay by design) one frame later.
The harness's `request_tooltip` therefore survives `settle()`.

Teardown is the pointer *leaving*, not the absence of hover (**amended in review**). `HoverStart`
is on a node exactly while the pointer is over it, so `tooltip_delay` drops a `TooltipContent`
only on a node that still carries one. A node that was never hovered keeps whatever tooltip
something else asked for: an explicit `TooltipRequest` stands until the pointer leaves the host or
`slotted_ui::clear_tooltip` (the harness's `clear_tooltip`) clears it. Without this the delay
system tore down every requested tooltip on the following frame, since a never-hovered node and a
just-left one look the same to a `Hovered` query.

`TooltipRequest { entity, tier: TooltipTier::{Compact, Expanded} }` entity event ->
`show_tooltip` observer composes `TooltipParts` (ordered `Arc<dyn TooltipPart>`,
`fn build(&self, &TooltipCtx { stack, registries, tier }, &mut Vec<UiNodeDef>)`) into
`TooltipContent { tier, parts }` on the hovered entity and spawns a `slotted:tooltip` under the layer
with `TooltipHost(entity)`. Phase 2 built-in parts: item name, count, rarity; `dev`: item id.
The harness reads `TooltipContent`, never the spawned nodes.

Item renderer: `ItemView { stack }` on slots and the carried node; `render_items` (`Render`) on
`Changed<ItemView>` resolves `Icons.icon(stack)`, writes the `ItemIcon` child's `ImageNode`
(`IconRef::Atlas` -> `image` + `texture_atlas: Some(TextureAtlas { layout, index })`; `Solid` ->
`BackgroundColor`; `Missing` -> theme glyph), the `ItemCount` text (hidden for count <= 1), and the
slot's `SemanticLabel`. The slot widget sets `ItemView` from `SlotChanged`.

Navigation: `NavKeys` resource (arrows by default); `directional_nav_keys` (`Input` set) drives
`AutoDirectionalNavigator`. Implemented; the spike proved it. Gamepad d-pad joins in Phase 6.

Semantics: `SemanticRole::{Screen, Panel, Grid, Slot, Button, Text, Tooltip, Rail, Hotbar, Anchor,
Carried, Custom(String)}` with `accesskit_role()`; `SemanticLabel(String)`; `TestId(String)`;
`ScreenRoot`, `WidgetNode(WidgetKind)`, `AnchorNode(AnchorId)`. `sync_accessibility` (`Semantics`)
writes `AccessibilityNode` from role and label. Implemented. This is the whole contract between
`ui` and the harness: the harness reads these components plus `ComputedNode`, `UiGlobalTransform`,
`InheritedVisibility`, `InputFocus`, `Children`, `SlotRef`, `ItemView`, `TooltipContent`.

## 5. slotted (facade)

Features: `ui` (default; theme + ui + icons), `blur`, `dev`. `SlottedPlugins { headless: bool }` is a
`PluginGroup` adding `SlottedEcsPlugin`, `SlottedThemePlugin`, `SlottedIconsPlugin`,
`SlottedUiPlugin { config: SlottedUiConfig { headless, spawn_layers: true } }`; it assumes Bevy's own
plugins are present. `SlottedPlugins::headless()` returns `HeadlessStack` = `HeadlessBevyPlugins`
(the ADR 0002 list: TaskPool, FrameCount, Time, ScheduleRunner::run_once, Transform, Asset,
Window{DontExit}, Input, Accessibility, Camera, Text, Scene, Ui, Picking, Interaction, InputFocus,
InputDispatch, TabNavigation, DirectionalNavigation, UiWidgetsPlugins, `HeadlessRenderAssets`) then
`SlottedPlugins { headless: true }`. `HeadlessRenderAssets` registers `Image`, `TextureAtlasLayout`,
`Mesh`, `SkinnedMeshInverseBindposes` (workaround 2). The facade does not load a theme; a game sets
`ActiveTheme`, the harness's `.theme("glass")` does it for tests.

## 6. slotted-test (agent C)

Bevy features: the headless list plus `default_font` and `debug`. Deps: `insta`, `uuid`.

`UiHarness::builder()`: `.plugins(impl Plugins)` (repeatable), `.resolution(w, h)`,
`.scale_factor(f)`, `.theme(name)`, `.frame_delta(d)` (default 1/60 s), `.max_settle_frames(n)`
(default 600), `.motion(Motion)`, `.registries(Arc<FrozenRegistries>)`, `.build()`. Build applies
the three ADR 0002 workarounds (camera `target_info`, render assets via the facade, a
`PointerId::Mouse` pointer), sets `TimeUpdateStrategy::ManualDuration`, resizes the primary window,
and runs one frame. Pinned by `tests/harness.rs`: exact layout, primary pointer hover and click
through real picking, `custom_pointer_clicks_but_does_not_update_hovered` (fails if upstream
generalises `update_is_hovered`), Tab and arrow navigation, settle with motion on and off, settle
timeout, `open_screen`.

Time: `step(n)`, `advance(Duration)`, `settle() -> usize` (panics), `try_settle() -> Result<usize,
SettleTimeout>`. Settled means `PendingRoundTrips == 0 && ActiveMotions == 0` and the hash of every
`ComputedNode` rect is unchanged from the previous frame.

Fixtures: `trait MenuFixture { def(); inventories(); actor() }` implemented for `MenuDef` (empty
inventories) and `(Arc<MenuDef>, Vec<Inventory>)`. `open_screen(kind, fixture) -> Opened { menu,
screen, inventories }` spawns `Inventory` entities, `open_menu`, looks `kind` up in `Screens`,
`spawn_screen`, one frame.

Locators (`by::role`, `by::tag`, `by::test_id`, `by::text`, `by::anchor`, `by::screen`,
`by::widget_kind`; `.tag()`, `.index(n)`, `.nth_visible(n)`, `.within(entity)`, `.with_item("ns:id")`)
resolve depth-first from each `ScreenRoot`, then the carried layer, then any matching entity
outside a screen by index. `find` panics on zero or several; `try_find`, `find_all`.

Actions. Semantic: `activate(e)` (`bevy_ui_widgets::Activate`), `click_slot(e, Button, Modifiers)`
(`SlotClicked`), `menu_action(menu, ClickAction)`, `request_tooltip(e, tier)`. Pointer (one frame per
step, `PointerInput` messages on the `Mouse` pointer): `click`, `right_click`, `middle_click`,
`shift_click`, `double_click`, `hover`, `drag(from, to)`, `drag_paint(&[..])`, `scroll(e, delta)`,
`click_at(pos, button)`, `pointer_move_to`, `pointer_press/release`, `spawn_custom_pointer`.
Keyboard: `key(KeyCode)`, `key_with(code, Key)`, `hold`, `release`, `type_text`, `focus_next`,
`focus_prev`, `focus_dir(CompassOctant)`, `set_focus`.

Queries: `stack_at(slot)` (from the model through `SlotRef`), `displayed_stack(e)` (from `ItemView`),
`carried(menu)`, `is_visible`, `is_focused`, `focused()`, `text_of`, `tooltip() -> Option<TooltipContent>`,
`rect_of`, `center_of`, `exclusion_zones(screen)`, `screen_tree() -> ScreenTree`, `world()`, `world_mut()`.

Conservation (**added in integration**): `assert_conserved()` sums per-kind counts across every
`Inventory` component, every menu's `Carried` stack and the `Dropped` resource, and compares that
to a baseline `open_screen` captures. Every test in `tests/chest_screen.rs` ends with it. A
gesture may move items anywhere; it may not create or destroy one.

Pointer and semantic actions take `Entity`, not `Locator`: `h.click(h.find(&loc))` is the idiom.
A pointer press also moves `InputFocus`, which the semantic path does not, since it skips picking
by design; a test comparing the two paths' `ScreenTree`s has to account for that.

`ScreenTree { roots: Vec<TreeNode> }`, `TreeNode { role, label?, test_id?, tags, widget?, anchor?,
screen?, visible, focused, item?: ItemSummary { id, count }, children }` (Serialize, `None`/empty
skipped). Nodes without a `SemanticRole` are elided and their semantic descendants lifted.
`assert_tree_snapshot!(tree)` wraps `insta::assert_ron_snapshot!`.

Not in Phase 2: `load_mod`, recording and replay, the `render` feature, gamepad.

## 7. Deviations from PLAN.md, summarised

- `UiNodeDef` is internally tagged with string unit enums (Value-safe rule, Lua parity).
- `tags` on every entity-producing `UiNodeDef` variant; `test_id` is a reserved tag.
- `SideTab.icon: IconDef`, not `IconRef`.
- `spawn_screen` reads `Injections` and `WidgetRegistry` from the world and takes the menu entity.
- Motion uses a small in-crate `Tween`, not `bevy_tweening`.
- The primary virtual pointer is `PointerId::Mouse` (ADR 0002), not `Custom`.
- `LookupCtx` is implemented in `slotted-ecs` (`RegistryLookup`), since neither `model` nor
  `registry` can implement it for the other's type without a dependency the plan forbids.
- Icons are baked on the CPU in Phase 2 (section 3).
- `slotted-registry` is pulled with `default-features = false` at the workspace level so the ecs
  crate builds on wasm; the facade re-enables `std-fs`.

Amended in integration (see the marked sections above, and `docs/design/phase2-notes-{A,B,C}.md`):

- Drag painting and number keys trigger `MenuAction` directly; `SlotClicked` carries only a
  completed pointer click on a slot (section 1).
- `register_slot_refs` seeds a newly registered slot with its current contents (section 1).
- `Pointer<Over>` starts a hover timer; `tooltip_delay` requests the tooltip, and only re-requests
  it when the shift key changes (section 4.4).
- Widget params carry no `Option`: an untyped `ron::Value` forgets `Some`, so every field is a
  plain type with a `#[serde(default)]` (section 4.3, notes B item 5).
- Built-in `UiNodeDef` variants are dispatched by `SpawnCtx::spawn_child` straight to their spawn
  functions rather than through `Widget::spawn`, which only sees `params` (notes B item 6). The
  `WidgetRegistry` still holds a `Widget` per built-in kind and `Custom` still goes through it, so
  every spawned node still carries `WidgetNode`.
- Harness actions take `Entity`, not `Locator` (section 6, notes C).
- `MenuClosed` keeps its `{ entity, id }` shape; a dropped stack lands in the `Dropped` resource
  (notes A item 1).
- `UiHarness::assert_conserved()` is part of the harness surface (section 6).
