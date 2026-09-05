# bevy_slotted implementation plan

Status: v1.7, 2026-09-06, Phases 0 to 5 complete. Targets Bevy 0.19.1. Companion documents: `docs/moodboard.html` and `docs/research/*.md`.

## 1. Goal and non-goals

**Goal.** A workspace of Bevy plugins that gives a game Minecraft's inventory model and Minecraft's modding freedom, with a modern skin. Concretely:

- The seven-mode click model over slot lists, implemented once, server-authoritative with client prediction.
- A widget and theme layer on `bevy_ui` covering everything the mod catalogue needs: slot grids, virtual grids, tanks and bars, side tabs, tooltip composition, HUD layers, a 3D viewport widget, a carried-stack layer.
- An item and recipe browser layered over any screen through a screen-handler registry.
- A Lua modding surface with the same reach mods have in Minecraft: data-stage registration, screen definitions as data, injection into screens the mod does not own, tooltip and HUD hooks. Scripts must run natively and in the browser.
- A web playground where the Lua of a running mod can be edited live.

**Non-goals for v1.** Networking transport (we define the port, a replicon adapter is a later crate). Pan-zoom quest canvases and Markdown books (they become the first "mod-shaped" crates built on the extension API once it exists). Pixel-art fidelity.

## 2. Principles

1. **Domain never knows about IO or rendering.** The item, inventory, menu and click logic is a plain Rust crate with no Bevy dependency. It is testable with `cargo test` in milliseconds and can back a headless server.
2. **Ports for every boundary that has two implementations.** Authority (local vs networked), script runtime (native Luau vs pure-Rust Lua on wasm), asset source (plain dir vs layered packs), icon source (baked atlas vs live viewport).
3. **Screens are data.** A screen is a tree that the ui crate spawns into entities. Rust, RON and Lua all produce the same tree type. Injection is a patch onto that tree at a named anchor.
4. **Registries freeze.** Data stage fills registries, then they freeze, then the control stage runs. Browser lookups are pure functions over frozen data.
5. **Scripts declare and react, never mutate.** Events go out to scripts, commands come back, the authority validates. This is what makes the same script safe on a server and in a browser tab.
6. **One theme system, three skins.** Tokens map roles to materials. The glass direction ships first; paper and neon exist to prove the tokens are complete.
7. **Motion has a budget and an off switch** from day one.
8. **Consumers can automate tests against their UI.** Every screen built with these crates is drivable headless: no window, no GPU, deterministic time. A game's test suite can locate a slot by semantic role, click it through the real layout and picking path, advance frames, and assert on both the inventory model and the widget tree. We dogfood the same harness for our own tests and examples.

## 3. Workspace layout

```
bevy-slotted/
├── Cargo.toml                  workspace root, no [package]
├── justfile                    check, test, wasm, serve, bake-icons
├── rust-toolchain.toml
├── crates/
│   ├── slotted-model/              domain: ids, stacks, inventories, menus, click state machine
│   ├── slotted-registry/           namespaced registries, data-stage loading, freeze, tags, recipes
│   ├── slotted-ecs/                Bevy adapter of the model: components, events, systems, LocalAuthority
│   ├── slotted-theme/              tokens, materials, RON theme assets, hot reload, shipped themes
│   ├── slotted-ui/                 widgets, screen trees, anchors, injection, exclusion zones, HUD layers
│   ├── slotted-icons/              offscreen icon baking to an atlas, cache, IconSource port + adapters
│   ├── slotted-browser/            ingredient registry, categories, search index, browser panel, transfer
│   ├── slotted-script/             ScriptRuntime port, the slotted.* API surface, event/command types
│   ├── slotted-script-mlua/        adapter: Luau via mlua, native only
│   ├── slotted-script-piccolo/     adapter: pure-Rust Lua 5.4 (piccolo), wasm32 and native, recoverable errors
│   ├── slotted-script-luaur/       adapter: pure-Rust Luau (luaur), behind a feature until wasm error handling lands
│   ├── slotted-packs/              layered AssetReader for mods and resource packs, mod.toml loading, Fluent
│   ├── slotted/                    facade: SlottedPlugins, prelude, feature flags
│   ├── slotted-test/               public UI test harness for consumers: headless app, locators, synthetic input, snapshots
│   └── slotted-testutils/          internal fakes, builders, script conformance suite, dev-dependency only
├── examples/
│   ├── chest/                  native: chest + inventory, glass theme, browser, motion
│   ├── machine/                native: furnace-like machine with tanks, bars, side tabs, a Lua mod injecting a sort button
│   ├── modded/                 native: loads mods/ from disk, hot reloads scripts and RON
│   └── web-playground/         wasm: game canvas beside a live Lua editor
├── assets/                     shared demo assets: themes, items, recipes, icons, locale
├── mods/                       demo mods used by examples: copper_chest, sorter, appleskin-like
├── tools/
│   └── xtask/                      wasm build, playground bundling, icon bake CLI
└── docs/
    ├── PLAN.md  moodboard.html  research/  adr/
```

Every crate is a workspace member and inherits edition, lints and shared dependencies. `slotted-model` and `slotted-registry` have no Bevy dependency. `slotted-script` depends on `serde` and `slotted-model` only. Adapters depend on their port crate plus their IO crate.

### 3.1 Dependency graph

```
slotted-model ◄── slotted-registry ◄── slotted-ecs ◄── slotted-ui ◄── slotted-browser
     ▲               ▲               ▲          ▲   ▲
     │               │               │          │   └── slotted-theme, slotted-icons
     └── slotted-script ┘               │          │
           ▲   ▲                     └──────────┴── slotted-packs
           │   ├── slotted-script-piccolo
           │   └── slotted-script-luaur (feature-gated)
           └────── slotted-script-mlua
                                    slotted  (facade, wires everything, feature flags)
                                    slotted-test  (depends on the facade; used by consumers' dev-dependencies)
```

Rule: an arrow never points from a lower crate to a higher one. `slotted-ui` does not know about scripting; `slotted-script` does not know about `bevy_ui`. The facade and the examples are the only places that see both. `slotted-test` is a normal published crate, not a dev-only one, because games depend on it from their own `[dev-dependencies]`.

### 3.2 Feature flags on the facade

| Feature | Default | Pulls in |
|---|---|---|
| `ui` | yes | slotted-ui, slotted-theme, slotted-icons |
| `browser` | yes | slotted-browser |
| `script-mlua` | yes on native | slotted-script-mlua (Luau, sandboxed) |
| `script-piccolo` | yes on wasm | slotted-script-piccolo |
| `script-luaur` | no | slotted-script-luaur, experimental until luaur errors are recoverable on wasm |
| `packs` | yes | slotted-packs |
| `blur` | no | scene blur pass for glass panels |
| `dev` | no | hot reload, exclusion-zone highlighter, id tooltips, script console |

## 4. Crate by crate

### 4.1 slotted-model (domain, no Bevy)

Types:

```rust
pub struct ItemId(Interned);                    // "copper_chest:chest", interned at freeze
pub struct ItemStack { id: ItemId, count: u32, patch: ComponentPatch }
pub struct ComponentPatch(BTreeMap<ComponentId, Value>);   // ordered so equality is cheap
pub struct Inventory { slots: Box<[Option<ItemStack>]>, changed: DirtyMask }

pub enum SlotBehaviour { Normal, Output, Ghost, Filter, Locked, Disabled }
pub struct SlotDef { source: InventoryRef, index: u16, behaviour: SlotBehaviour, max: Option<u32>, accepts: Option<Predicate> }
pub struct MenuDef { slots: Vec<SlotDef>, quick_move: RoutingTable, properties: Vec<PropertyDef> }
pub struct MenuState { carried: Option<ItemStack>, state_id: u32, drag: Option<DragState> }

pub enum ClickAction {
    Pickup { slot: SlotIx, button: Button },
    QuickMove { slot: SlotIx },
    Swap { slot: SlotIx, hotbar: u8 },
    Clone { slot: SlotIx },
    Throw { slot: SlotIx, all: bool },
    Drag { stage: DragStage, kind: DragKind, slot: Option<SlotIx> },
    PickupAll { slot: SlotIx, reverse: bool },
    Toolbar(ToolbarAction),                      // Sort, QuickStack, DepositAll, LootAll, Favorite
}

pub fn apply_click(def: &MenuDef, inv: &mut Inventories, state: &mut MenuState, action: ClickAction, actor: &Actor)
    -> Result<Delta, ClickError>;
```

`apply_click` is a pure function over the menu, the inventories it references and the carried stack. `Delta` lists changed slots and the new carried stack. Every path runs a conservation check in debug builds. The `Actor` carries permissions (creative, op) for `Clone` and cheat gives.

Ports defined here:

```rust
pub trait Authority: Send + Sync {
    fn submit(&self, menu: MenuId, action: ClickAction, predicted: Delta) -> Result<(), AuthorityError>;
    fn poll(&self) -> Vec<AuthorityEvent>;      // Ack{state_id} | Resync{menu, snapshot} | Property{id, value}
}
```

The local adapter applies immediately and always acks. A networked adapter serialises the action with the predicted delta, exactly like vanilla's click packet, and resyncs on mismatch.

Tests: unit tests per click mode, a property test that random action sequences conserve item counts, golden tests reproducing vanilla slot layouts (chest 0–26, 27–53, 54–62), quick-move routing tables for the standard windows.

### 4.2 slotted-registry (no Bevy)

- `Registry<T>` with namespaced string ids, `RegistryBuilder` open during the data stage, `freeze()` returns an immutable registry with interned numeric ids and a string map for scripts.
- Registries: items, tags, recipe types, recipes, screen kinds, widget kinds, tooltip components, HUD layers, ingredient types. Each has a RON schema.
- Data stage loader: reads `data/<ns>/items/*.ron`, `recipes/*.ron`, `tags/*.ron` from an `AssetSource` port (a trait over "list and read bytes", so it works with the filesystem, embedded assets and the browser).
- Patch lists in the style of Bedrock's `modifications`: a mod can insert, remove or replace entries of another mod in a later round (base → updates → final).
- Load order: manifest dependencies, topological sort, then stable alphabetical. Cycles are hard errors.

### 4.3 slotted-ecs (Bevy adapter of the model)

- Components: `Inventory`, `OpenMenu { def, state }`, `Carried`, `MenuProperty`, `Favorite`.
- Entity events: `SlotClicked`, `MenuOpened`, `MenuClosed`, `SlotChanged`, `PropertyChanged`. Messages for high-volume sync.
- Systems: gather input into `ClickAction`, run `apply_click` for prediction, submit to the `Authority` resource (`Arc<dyn Authority>`), reconcile acks and resyncs, mark dirty slots for the ui crate.
- `LocalAuthority` adapter lives here. `slotted-testutils` provides `RecordingAuthority` and `RejectingAuthority` for tests.
- No rendering, no `bevy_ui`. Runs under `MinimalPlugins` in tests.

### 4.4 slotted-theme

- `Theme` asset (RON): token table for spacing, radii, elevation, durations, blur, colours, rarity colours, and a `roles` map from semantic role (`panel`, `slot`, `slot.hover`, `slot.focus`, `tooltip.frame`, `button.primary`, `tab.side`, `tank.fill`) to a `Material`.
- `Material` is an enum: `Solid`, `Gradient`, `Sliced { image, slicer }`, `Shader { handle, params }`. Rounded corners and shadows come from Bevy's SDF `BorderRadius` and `BoxShadow`; glow and shimmer from a `UiMaterial`.
- `Themed(Role)` component; a system applies materials when the theme loads or changes. Hot reload through `AssetEvent::Modified`.
- Shipped themes: `glass` (v1), `paper`, `neon`. The latter two are gated behind the completeness of the token set, not behind extra code.
- Optional `blur` feature: scene camera renders to a texture, a quarter-resolution dual Kawase pass, sampled by the glass panel material.
- Motion: `Motion` resource with the duration scale and a reduced-motion switch; tweens through `bevy_tweening` lenses on `Node`, `UiTransform` and colours. Hover, press, drop-squash, fly-to-slot and stagger are named presets.

### 4.5 slotted-ui

The largest crate. Three layers inside it, kept as modules until a split is justified.

**Screen trees.**

```rust
pub enum UiNodeDef {
    Panel { role: Role, layout: Layout, children: Vec<UiNodeDef> },
    SlotGrid { inventory: InventoryRef, cols: u16, rows: u16, first: u16 },
    VirtualGrid { source: DataSourceId, cols: u16 },
    Slot { slot: SlotIx },
    Text { key: LocKey, style: TextRole },
    Button { widget: WidgetKind, tags: Tags },
    Tank { property: PropertyId, capacity: PropertyId, orientation: Orientation },
    Bar { property: PropertyId, max: PropertyId, direction: Direction },
    SideTab { icon: IconRef, side: Side, children: Vec<UiNodeDef> },
    Viewport { subject: ViewSubject },
    Anchor { id: AnchorId },
    Custom { kind: WidgetKind, params: Value, children: Vec<UiNodeDef> },
}
pub struct ScreenDef { kind: ScreenKind, inherits: Option<ScreenKind>, root: UiNodeDef, listring: Vec<InventoryRef> }
```

`spawn_screen(&mut Commands, &ScreenDef, &Injections) -> Entity` resolves `inherits`, applies injections at anchors, then spawns entities with `Node`, `Themed`, picking observers and the `bevy_ui_widgets` state components. This is the only place that knows how a `UiNodeDef` becomes entities. `bsn!` is used internally for the widget templates; `.bsn` files are adopted when Bevy ships the loader.

**Extension points.**

- `WidgetKind` registry: `trait Widget { fn spawn(&self, ctx: &mut SpawnCtx, params: &Value) -> Entity; fn tooltip(&self, ..) }`. Built-in widgets register themselves; mods register `Custom` kinds by composing built-ins from data, or Rust crates register new ones.
- `Injection { target: ScreenKind, anchor: AnchorId, node: UiNodeDef, exclusion: bool }` registry, consulted by `spawn_screen`. This is the Fabric `ScreenEvents` and Factorio `gui.relative` equivalent.
- Screen lifecycle entity events on the screen root: `ScreenSpawned`, `ScreenLayout` (after taffy, with the panel rect), `ScreenClosed`, plus `Pointer<_>` bubbling as usual.
- `ExclusionZone` marker component; `exclusion_union(screen) -> Vec<Rect>` query used by the browser and HUD.
- `TooltipComponent` registry: `trait TooltipPart { fn build(&self, ctx: &TooltipCtx, out: &mut Vec<UiNodeDef>) }`. Tooltip composition is two-tier (compact, expanded) with a frame decorator hook.
- `HudLayer` registry: ordered, named built-in layers (`hotbar`, `health`, `crosshair`), insert above, below or replace by id, nine anchors plus offset, a dev-mode drag editor writing to settings.
- Carried-stack layer and tooltip layer under a dedicated root with high `GlobalZIndex`; `Pickable::IGNORE`.

**Widgets in v1.** Slot, slot grid, virtual grid with scrollbar, item renderer (icon, count, durability, cooldown, rarity ring and glyph), panel, text, button, icon button with state cycling, tabs, side tab, progress bar, tank, text field wrapping `EditableText`, tooltip, action rail. Focus and gamepad navigation come from `TabGroup` and `AutoDirectionalNavigation` with manual edges between grids.

### 4.6 slotted-icons

- `IconSource` port: `fn icon(&self, item: &ItemStack) -> IconRef`. Adapters: `AtlasIcons` (baked) and `LiveIcons` (a `ViewportNode` per request, for the hovered item and detail panes).
- Baking: an offscreen camera with a fixed three-point rig renders each registered item at 64 and 128 px into a `TextureAtlasBuilder` atlas, on first run or when the item's model hash changes. Cache to the asset processor output on native; in the browser bake lazily on first sight.

### 4.7 slotted-browser

- `Ingredient` = `(IngredientTypeId, Value, SubtypeKey)`. Types: item, fluid, tag, info. `IngredientType` trait supplies display name, mod, tags, renderer, subtype interpreter.
- Registration phases as ordered schedule sets: `Subtypes → IngredientTypes → Categories → Recipes → Transfer → ScreenHandlers → Runtime`. Cross-references validated at each boundary.
- `RecipeCategory` trait: id, title, icon, size, `layout(&Recipe, &mut LayoutBuilder)` emitting slots with roles (input, output, catalyst, render-only).
- `ScreenHandler` registry keyed by `ScreenKind`: bounds, exclusions (from the ui crate), clickable areas, stack under cursor, ghost drop acceptor, transfer handler with `dry_run`.
- Search: tokenizer with quotes, `-`, `|`, prefixes `@ # $ &` each with enabled / require-prefix / disabled modes. Index built off the main thread: substring index for names and tooltips, string-to-bitset maps for mod, tag and category, results cached until filter, hidden set or visibility changes.
- Panel: card grid, category chips, recipe view with history, "used in", bookmarks (typed, serialisable), recipe tree (EMI-style) as a stretch goal.
- `BrowserRuntime` resource exposing filter text, visible entries, bookmarks and key mappings for sibling plugins.

### 4.8 slotted-script (port and API surface)

```rust
pub trait ScriptRuntime: Send + Sync {
    fn load(&mut self, mod_id: &ModId, name: &str, source: &str, stage: Stage) -> Result<ScriptId, ScriptError>;
    fn unload(&mut self, id: ScriptId);
    fn call(&mut self, id: ScriptId, event: &ScriptEvent) -> Result<Vec<ScriptCommand>, ScriptError>;
    fn set_limits(&mut self, limits: Limits);   // instruction budget, memory cap, wall time
}
```

- `ScriptEvent` and `ScriptCommand` are `serde` enums shared by every adapter and by the Bevy side. Events: `DataStage`, `ControlStart`, `SlotClick`, `WidgetActivate`, `TooltipBuild`, `RecipeLookup`, `ScreenOpened`, `HudTick`. Commands: `RegisterItem`, `RegisterRecipe`, `RegisterScreen`, `Inject`, `Sort`, `Move`, `AddTooltipPart`, `SetHud`, `Log`.
- The `slotted.*` Lua API is a single Lua prelude file shipped by this crate, so both adapters expose identical functions. The prelude turns table-based calls into `ScriptCommand` values and dispatches events to registered handlers. Adapters only need `to_value` / `from_value` bridges and a `call` entry point.
- A conformance suite in `slotted-testutils`: a directory of `.lua` scripts with expected command output, run against every adapter in CI.
- Sandbox: no `io`, `os`, `package`, `load`; per-script globals; instruction budget and memory cap through the adapter; scripts that exceed limits are unloaded and reported.
- API versioning: `api_version` in `mod.toml`; the prelude exposes `slotted.api_version`; deprecated calls warn once.

### 4.9 Script adapters: slotted-script-mlua, slotted-script-piccolo, slotted-script-luaur

Decided in Phase 0 (ADR 0001, measurements in `spikes/script-runtimes/`):

- **Native: `mlua` with the `luau` feature**, vendored, `Lua::sandbox(true)`, `set_interrupt` for instruction budgets, `set_memory_limit`. Cold start about 41 µs, a call with a small table about 310 ns. Does not build for wasm32 because Luau is C++ and the target has no C++ runtime. Note: sandboxed globals silently ignore writes, so stdlib removals must happen before `sandbox(true)`; and a stray C toolchain in the environment (the user's shell exports an Anaconda `CC`/`CXX`) miscompiles Luau, so the workspace pins `CC`/`CXX` through `.cargo/config.toml` and CI pins its toolchain.
- **Web: `piccolo` 0.3**, pure-Rust Lua 5.4 with a fuel budget and arena-tracked memory. Runtime errors come back as `Result` and the VM stays usable, verified in headless Chrome. Wasm size about 990 KiB before `wasm-opt`. Cost: no serde bridge and no `Function::call` convenience, so its adapter is roughly three times the code of the mlua one, and its stdlib is incomplete (`string`, `table`, `utf8` partially), which the shared prelude must paper over or avoid.
- **luaur 0.1.8 stays in the tree behind a feature, not as the default web runtime.** It is the fastest of the three (265 ns per call, 743 KiB wasm) and its API mirrors mlua so closely that the two adapters share nearly all glue, but it implements Lua errors with Rust panics and `catch_unwind`, and wasm32-unknown-unknown has no unwinding: any `error()`, runtime type error or even an in-VM `pcall` aborts the whole module in the browser. Only compile errors are recoverable there. One buggy mod would take down the page. When luaur gains non-unwinding error propagation, the conformance suite tells us it is ready and the web default flips to it.

The conformance suite in `slotted-testutils` is therefore load-bearing: it runs the same `.lua` cases against all enabled adapters and is the gate for any swap.

`bevy_mod_scripting` 0.21 (Bevy 0.19, native only) is not used for the curated API. It remains an option for a "power mod" tier with reflection access, behind a feature, later.

### 4.10 slotted-packs

- `LayeredReader`: a Bevy `AssetReader` that resolves `pack://` paths through resource packs, then mods in reverse load order, then base assets. Also implements the registry crate's `AssetSource` port so the data stage reads through the same layering.
- `mod.toml`: id, version, api_version, dependencies with `optional`, `incompatible` and ordering hints, entry points `data.lua`, `control.lua`, asset root.
- Fluent localisation through `bevy_fluent`, keys namespaced per mod, layered by load order.

### 4.11 slotted facade

`SlottedPlugins` plugin group wiring the defaults: `LocalAuthority`, `AtlasIcons`, the glass theme, the mlua or piccolo runtime depending on target, `LayeredReader`. Everything is a resource holding an `Arc<dyn Port>` so an app can replace any adapter before `add_plugins`.

`SlottedPlugins::headless()` is the same group without rendering, icon baking, blur and motion side effects. It is what `slotted-test` builds on, and it is also the right group for a dedicated server.

### 4.12 slotted-test (consumer-facing UI automation)

The promise: a game that builds its screens with these crates can write end-to-end UI tests that run in `cargo test`, on CI, with no window and no GPU, in milliseconds per step.

**How it works headless.** `bevy_ui` layout runs in `PostUpdate` against `Window` components, not against the renderer, so a harness can spawn a `Window` entity with a fixed resolution and scale factor, add a `Camera2d` marked as the UI camera, and taffy produces real `ComputedNode` rectangles. `bevy_picking` hit-tests UI against those rectangles through its UI backend, driven by `PointerInput` messages on a pointer entity. The harness owns a virtual pointer (`PointerId::Custom`) and a virtual keyboard and gamepad, and feeds them the same way winit would. Nothing in `slotted-ui` is test-specific; the harness only replaces the input source and the window source. This is verified in Phase 0 because the UI picking backend's camera requirements are the one uncertain piece.

**API sketch.**

```rust
use slotted_test::prelude::*;

#[test]
fn shift_click_moves_cobblestone_to_player_inventory() {
    let mut h = UiHarness::builder()
        .plugins(my_game::plugins_for_test())        // includes SlottedPlugins::headless()
        .resolution(1280, 720)
        .theme("glass")
        .build();

    h.open_screen(ScreenKind::new("copper_chest:chest"), ChestFixture::filled());
    h.settle();                                         // run frames until layout and motion are stable

    let src = h.find(by::role(Role::Slot).tag("region", "chest").index(0));
    h.shift_click(&src);
    h.settle();

    assert_eq!(h.stack_at(&src), None);
    let dst = h.find(by::role(Role::Slot).tag("region", "player").with_item("minecraft:cobblestone"));
    assert_eq!(h.stack_at(&dst).unwrap().count, 64);
    assert_tree_snapshot!(h.screen_tree());              // insta snapshot of roles, tags, anchors, visibility
}
```

**Locators.** `by::role`, `by::tag(key, value)`, `by::test_id` (a `TestId("...")` component consumers attach in their screen trees; Lua exposes it as `test_id = "..."`), `by::text`, `by::anchor`, `by::screen`, `by::widget_kind`, combinable with `.index(n)`, `.nth_visible`, `.within(other)`. Locators resolve against a semantic tree that `slotted-ui` maintains anyway for accessibility (see below), so they are stable across theme changes and layout tweaks.

**Actions, two levels.**

- *Semantic* actions dispatch the widget's own entity events (`Activate`, `SlotClicked`, `ValueChange`) directly. Fast and layout-independent; right for model-heavy tests.
- *Pointer* actions move the virtual pointer to the locator's computed rectangle and emit press, release, move and scroll through `bevy_picking`. They test hit-testing, z-order, exclusion zones and drag. `click`, `right_click`, `shift_click`, `double_click`, `drag(from, to)`, `drag_paint([slots])`, `hover`, `scroll`, `key(Key)`, `type_text`, `gamepad(Button)`, `focus_next`, `focus_dir(Dir)`.

**Time.** The harness drives `Time<Virtual>` with fixed steps. `step(n)` advances n frames, `advance(Duration)` advances virtual time, `settle()` runs until no tween is active and no layout is dirty, with a frame cap that fails the test if the UI never settles. Motion presets read the same clock, so fly-to-slot animations complete deterministically or are skipped with `Motion::reduced()`.

**Queries.** `stack_at`, `is_visible`, `is_focused`, `text_of`, `tooltip()` (compact or expanded content as a tree), `rect_of`, `exclusion_zones(screen)`, `screen_tree()`, `hud_layers()`, `browser().visible_entries()`, `carried()`. The `screen_tree()` output is a stable, theme-independent structure meant for `insta` snapshots.

**Scripts under test.** `h.load_mod(path)` runs a mod through the same registry stages as the game, so a game can assert that a mod's injection landed at the right anchor. Mod authors get the same power from Lua: a `slotted.test` module wraps the harness so `tests/*.lua` can `open_screen`, `click`, `expect`, and the `xtask test-mods` command runs them natively and, in the playground, from a Tests tab.

**Recording and replay.** In `dev` builds the real input source can record `PointerInput`, key and gamepad streams with frame numbers to a RON file. The harness replays them. A bug seen in the playground becomes a failing test by saving its recording.

**Optional pixels.** Behind a `render` feature the harness can run the real renderer with a software adapter where CI has one, capture the UI camera to an image, and compare against a golden with a tolerance. Off by default; the semantic tree is the primary contract.

**Accessibility as a by-product.** `slotted-ui` gives every widget a semantic role, a label and state through `bevy_a11y` (AccessKit): slots become list items with the item name and count, tabs and buttons get their roles, the carried stack is announced. The locator tree is that same data. Good tests and screen-reader support come from the same work.

## 5. Examples

### 5.1 `chest`

The moodboard demo in Bevy. A world with a camera and a few blocks, a copper chest screen with the action rail, the glass theme, tooltips, motion presets, the browser on the right. Verifies: model, ecs, ui, theme, icons, browser. Runs native. Also compiled to wasm in CI as a smoke test.

### 5.2 `machine`

A furnace-like machine screen: input, fuel and output slots, a tank, an energy bar, two progress bars driven by `MenuProperty`, side tabs for redstone mode and side config. The `sorter` demo mod injects a sort button at the `title_end` anchor of every container screen and publishes an exclusion zone; the browser moves aside. Verifies: machine widgets, injection, exclusion zones, properties.

### 5.3 `modded`

Loads `mods/` from disk with `mod.toml` manifests. `copper_chest` registers an item, a recipe and a screen in `data.lua` and reacts to slot clicks in `control.lua`. `appleskin_like` adds a tooltip part for foods. Scripts and RON hot reload through the file watcher. A dev console shows script logs and errors. Verifies: registry stages, packs, script API, native adapter.

### 5.4 `web-playground`

The showcase. A static site with the Bevy canvas on the left and a code editor on the right (CodeMirror 6 from a CDN, or a plain textarea as fallback), tabs for `data.lua` and `control.lua` of a demo mod, and a Run button plus run-on-idle.

Flow: the editor calls a `wasm-bindgen` export `reload_mod(mod_id, data_src, control_src)`. The Bevy app receives it through a channel resource, tears down that mod's registrations, re-runs its data stage (re-freezing the affected registries), re-spawns open screens from the new `ScreenDef`, and reloads the control script. Errors surface in a console pane through the `Log` command and `ScriptError`. Because scripts only emit commands, a broken script cannot corrupt the inventory state; the worst case is a rejected reload with the previous version kept running.

Runtime: `slotted-script-piccolo`. Built with `wasm-bindgen` through the `xtask`, served by `just serve`. Icons bake lazily in the browser.

Stretch: a share button that encodes the two scripts into the URL hash.

## 6. Phases

Each phase ends with something runnable and a short verification list. Estimates assume one developer part-time.

### Phase 0. Spikes (1 to 2 weeks)

Throwaway code in `spikes/`, deleted after decisions are recorded as ADRs in `docs/adr/`.

- luaur, piccolo and mlua natively and on wasm32: load a script, call a function, round-trip a table, recover from a runtime error. Measure binary size and call cost.
- mlua Luau sandbox with interrupt budget in a Bevy system.
- Glass panel: `BorderRadius` + `BoxShadow` + a `UiMaterial` sampling a blurred scene texture. Measure cost at 1080p.
- `bsn!` templates for a slot and a slot grid, and whether `ViewportNode` icons are viable for a hovered item.
- Headless UI: `MinimalPlugins` plus `bevy_ui` layout with a spawned `Window` entity and a non-rendering UI camera; drive `bevy_picking`'s UI backend with a custom pointer and confirm `Pointer<Click>` lands on the right node. This is the foundation of `slotted-test`.
- Decisions (all taken, see `docs/adr/0001` to `0003`): mlua native + piccolo web with luaur feature-gated; blur is a `blur` feature on `slotted-theme`; `bsn!` is used internally; the real picking backend works headless with three workarounds (fill `Camera::computed.target_info`, register the render asset types, use `PointerId::Mouse` for the primary pointer because `Hovered` is hard-wired to it).

### Phase 1. Model and registry (2 weeks)

`slotted-model`, `slotted-registry`, `slotted-testutils`. All seven click modes plus toolbar actions, routing tables, property defs, conservation property test, RON loading with patch rounds and freeze. No Bevy. Verification: `cargo test -p slotted-model -p slotted-registry`, golden vanilla layouts.

### Phase 2. ECS, ui core, theme, test harness, chest example (5 weeks)

`slotted-ecs` with `LocalAuthority`; `slotted-theme` with the glass theme and motion presets; `slotted-ui` with screen trees, slot grid, item renderer, tooltip, carried layer, action rail, semantic roles; `slotted-icons` with atlas baking; **`slotted-test` with the headless harness, locators, semantic and pointer actions, virtual time and `screen_tree()` snapshots**, built alongside the widgets so the widgets are designed to be driven; `examples/chest` without the browser, with a `tests/` directory written against `slotted-test`. Verification: the chest example matches the moodboard demo's behaviours; every click mode has a harness test that goes through real layout and picking; a screen-tree snapshot test; `settle()` terminates with motion on and off.

### Phase 3. Browser (3 weeks)

`slotted-browser`: ingredient registry, phases, categories, search index off-thread, panel, recipe view, transfer with dry-run, screen handlers reading exclusion zones. Chest example gains the browser. Verification: search grammar tests, 50k synthetic entries indexed under a second, transfer dry-run highlights.

### Phase 4. Scripting, packs, modded example (4 weeks)

`slotted-script` with the Lua prelude and the event and command enums; `slotted-script-mlua`; `slotted-packs` with `mod.toml`, layered reader and Fluent; `examples/modded` with the `copper_chest` mod and hot reload. Verification: conformance suite passes on mlua; a mod adds an item, recipe and screen without Rust; hot reload keeps inventory state.

### Phase 5. Web playground (3 weeks)

`slotted-script-piccolo` (and the feature-gated luaur adapter); conformance suite passes on all enabled adapters; `xtask wasm`; `examples/web-playground` with editor, reload channel, console; CI builds and publishes the site. Verification: edit `control.lua` in the browser and see the behaviour change without a page reload; a script with an infinite loop is stopped and reported.

### Phase 6. Machine widgets, injection, HUD (3 weeks)

Tanks, bars, side tabs, icon buttons with state cycling, `Injection` registry, `TooltipPart` registry, `HudLayer` registry with drag editor in dev mode; `examples/machine` with the `sorter` mod; `slotted-test` gains `load_mod`, the Lua `slotted.test` module, `xtask test-mods`, and input recording and replay. Verification: the injected button appears on every container screen and the browser panel respects its exclusion zone; a Lua tooltip part renders on foods; the `sorter` mod ships a passing `tests/sort.lua`; a recorded playground session replays green.

### Phase 7. Second and third theme, docs, release prep (2 weeks)

Paper and neon themes prove the token set; missing roles are added rather than special-cased. `cargo doc` with examples per crate, a modding guide generated from the prelude's doc comments, Luau type stubs (`.d.luau`) for editor support, crates.io dry run.

## 7. Testing strategy

Two audiences. Our own crates are tested with the usual unit and integration tests below. Games and mods are tested with `slotted-test`, and every example in this repo uses `slotted-test` for its own tests so the consumer path is exercised continuously.

- **Model:** unit tests per click mode, proptest for conservation and idempotent resync, golden layouts.
- **Registry:** fixture data packs with patch rounds, cycle detection, freeze immutability.
- **ECS:** integration tests under `MinimalPlugins` with `RecordingAuthority`; prediction followed by a forced `Resync` converges.
- **UI:** through `slotted-test`: every widget has harness tests for hover, focus, activate and, where relevant, drag, going through real layout and picking; `screen_tree()` snapshots for every shipped `ScreenDef`; theme swaps must not change the semantic tree. Pixels only behind the `render` feature, one smoke render per example where CI has a software adapter.
- **Browser:** grammar tests, index benchmarks with `criterion`, transfer dry-run cases.
- **Script:** the conformance suite runs on every adapter; a budget test with an infinite loop; a sandbox test attempting `io` and `os`.
- **Web:** `wasm-pack test --headless --chrome` for the luaur adapter and the reload channel; a Playwright smoke test that edits a script and asserts a DOM change is a stretch.

## 8. Tooling and CI

- Workspace lints per the project conventions: clippy pedantic with the usual allowances, `unsafe_code = "deny"` except in the icon baking crate if a graphics API needs it.
- `justfile`: `check`, `test`, `test-wasm`, `wasm-build <example>`, `serve`, `bake-icons`, `fmt`, `doc`.
- GitHub Actions matrix: native (linux, macos, windows) test; wasm build of `chest` and `web-playground`; `cargo deny`; docs build; playground deploy to Pages on `main`.
- Bevy version pinned at the workspace root; upgrade is one line plus the migration guide. Plan for 0.20 within the project's life: `bsn` file loader and any `bevy_ui_widgets` API churn land in `slotted-ui` only.

## 9. Risks and mitigations

| Risk | Impact | Mitigation |
|---|---|---|
| piccolo's incomplete stdlib and heavier adapter | prelude has to avoid some string/table functions; web adapter is more code | shared prelude tested by the conformance suite; polyfill the few missing functions in Lua inside the prelude; luaur ready to swap in when its wasm error handling lands |
| Two script adapters drift | mods behave differently on web | single Lua prelude, shared event and command enums, conformance suite in CI on both |
| Glass blur cost or complexity | v1 theme looks flat | `blur` is a feature; tinted translucent panels without blur are the default and still on-brand |
| `bevy_ui_widgets` API churn | breakage on upgrade | wrap widget state components behind `slotted-ui` types; only that crate imports them |
| No `.bsn` loader | no hot-reloadable structure | screens are `UiNodeDef` RON already; `bsn!` is internal; adopt the loader when it ships |
| Scope creep from the mod catalogue | v1 never ships | canvases, books, in-world UI are explicitly post-v1 crates on top of the extension API |
| Icon baking on wasm | slow first paint | lazy per-item bake with a placeholder glyph; ship a prebaked atlas for demo items |
| Headless picking depends on Bevy internals (UI backend, camera requirements) | harness breaks on upgrade | Phase 0 spike; if the backend is awkward, the harness hit-tests `ComputedNode` rectangles itself and injects `Pointer<_>` events, keeping the same public API |
| Harness tests flake on timing | consumers distrust it | virtual time only, `settle()` with a hard frame cap, no wall-clock anywhere in `slotted-ui` |

## 10. Decisions to take now

1. ~~Confirm the workspace crate names~~ Decided 2026-09-05: the project is **slotted**. Facade crate `bevy_slotted`, member crates `slotted-*` (published as `slotted_*`), Lua namespace `slotted.*`, built-in id namespace `slotted:`. Both `slotted` and `bevy_slotted` were free on crates.io at decision time.
2. Confirm glass as the first theme.
3. ~~Confirm the two-adapter scripting approach~~ Decided by the Phase 0 spike: mlua (Luau) native, piccolo on wasm, luaur feature-gated. See ADR 0001.
4. Confirm that networking stays a port with a local adapter in v1.
5. Confirm `slotted-test` as a published, public crate with locators over a semantic tree (which also gives us accessibility), rather than a test-only helper.
