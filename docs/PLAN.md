# bevy_slotted implementation plan

Status: v2.0, 2026-09-06, Phases 0 to 7 complete. Targets Bevy 0.19.1. Companion documents: `docs/moodboard.html` and `docs/research/*.md`.

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
2. **Ports for every boundary that has two implementations.** Authority (local vs networked), script runtime (the shipped Luau adapter vs a game's own), asset source (plain dir vs layered packs), icon source (baked atlas vs live viewport).
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
│   ├── slotted-script-luaur/       adapter: Luau via luaur (pure Rust), the one runtime, native and wasm32 (ADR 0004)
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
     ▲                                                   ▲   ▲
     │                                                   │   └── slotted-theme, slotted-icons
     └── slotted-script ◄── slotted-script-luaur         │
              ▲                                          │
              └───────────── slotted-packs ──────────────┘

                                    slotted  (facade, wires everything, feature flags)
                                    slotted-test  (depends on the facade; used by consumers' dev-dependencies)
```

Rule: an arrow never points from a lower crate to a higher one. `slotted-ui` does not know about scripting; `slotted-script` does not know about `bevy_ui`. The facade and the examples are the only places that see both. `slotted-test` is a normal published crate, not a dev-only one, because games depend on it from their own `[dev-dependencies]`.

### 3.2 Feature flags on the facade

| Feature | Default | Pulls in |
|---|---|---|
| `ui` | yes | slotted-ui, slotted-theme, slotted-icons |
| `browser` | yes | slotted-browser |
| `script-luaur` | yes | slotted-script-luaur (Luau, sandboxed). The same runtime on every target, so there is nothing to swap for a wasm build |
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
pub struct MenuState { carried: Option<ItemStack>, state_id: u32, drag: Option<DragState>,
                       properties: Vec<i32>, hints: BTreeMap<SlotIx, ItemStack> }

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

`apply_click` is a pure function over the menu, the inventories it references and the carried stack. `Delta` lists changed slots and the new carried stack. The `Actor` carries permissions (creative, op) for `Clone` and cheat gives.

A `Ghost` or `Filter` slot shows an item it does not hold. Those hints live in `MenuState::hints`, keyed by menu slot, not in the backing inventory, so `count_of`, the conservation tally and inventory sync never see a phantom stack. `slot_view(def, inv, state, ix)` is the accessor that answers "what does this slot show" for both kinds of slot.

Item conservation is a runtime choice rather than a compile-time one, because the same function runs as client prediction and as server validation:

```rust
pub enum ValidationLevel { Off, Debug, Always }
pub fn apply_click_validated(.., validation: ValidationLevel) -> Result<Delta, ClickError>;
```

`Debug` is the default and is what `apply_click` uses: check in debug builds, panic on a violation, cost nothing in release. `Always` checks in every build and returns `ClickError::Conservation`, which is what an authoritative server wants. The check runs after the action, so a caller at `Always` applies to a scratch copy and commits only on `Ok`.

Ports defined here:

```rust
pub trait Authority: Send + Sync {
    fn submit(&self, menu: MenuId, action: ClickAction, predicted: &Delta) -> Result<(), AuthorityError>;
    fn poll(&self) -> Vec<AuthorityEvent>;
    // Ack{state_id} | Resync{menu, snapshot} | Slot{menu, slot, stack} | Property{id, value}
    fn request_resync(&self, menu: MenuId) -> Result<ResyncRequest, AuthorityError>;  // defaulted
    fn validation(&self) -> ValidationLevel;                                          // defaulted
}
```

The local adapter applies immediately and always acks. A networked adapter serialises the action with the predicted delta, exactly like vanilla's click packet, and resyncs on mismatch.

A client that can no longer trust its copy asks for a snapshot rather than guessing: `request_resync` answers `Pending` when an `AuthorityEvent::Resync` is on its way and `Unsupported` when the authority *is* the client's state, which is the local adapter's case. `AuthorityEvent::Slot` is the cheap half of a resync, for an authority that knows exactly which slot changed. See 4.13 and `docs/design/gaps-notes-A.md`.

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
- `LocalAuthority` adapter lives here; `LocalAuthority::with_validation` picks the `ValidationLevel` `predict` runs at, which every authority reports through `Authority::validation`. `slotted-testutils` provides `RecordingAuthority` and `RejectingAuthority` for tests.
- A submission the authority refuses asks it for a snapshot (`request_resync`) and only falls back to re-emitting the menu from local state when the answer is `Unsupported`. `PendingRoundTrips` counts the request and the `Resync` that settles it one for one.
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

### 4.9 The script adapter: slotted-script-luaur

Decided in Phase 0 (ADR 0001), revised twice in Phase 5 and settled by ADR 0004: **one runtime, luaur, on every target.** Measurements in `spikes/script-runtimes/`:

- **`luaur` 0.1.8**, a pure-Rust line-for-line port of Luau, with `Lua::sandbox(true)`, `set_interrupt` for instruction budgets, `set_memory_limit` and a serde bridge. It builds for `wasm32-unknown-unknown` and for native from the same source, so there is one adapter, one prelude, one value model and one conformance run. Faster than the mlua adapter it replaced (4.6 µs against 7.2 µs for a `SlotClick` round trip).
- **Nothing compiles C or C++ any more.** mlua's vendored Luau was the only such build, and with it goes the Anaconda-toolchain miscompile ADR 0001 documents; `.cargo/config.toml`'s `CC`/`CXX` pin is deleted and the file records what would bring it back.
- **The sandbox order is load-bearing and tested.** `sandbox(true)` installs a proxy global table whose `__index` falls through to the real environment, so a name set to `nil` afterwards is still reachable from a script while the call reports `Ok(())`. Removals happen before sealing; the adapter pins both halves of the behaviour in a test.
- **The price is that an uncaught Lua error aborts the wasm module.** luaur raises by panicking and `wasm32-unknown-unknown` has no unwinding, so `error()`, a runtime type error, an exhausted budget and a refused allocation all trap in a browser, and `pcall` in a script does not contain them. Only compile errors are recoverable there. ADR 0004 accepts this: the host reports the error through `install_error_reporter` before the trap, then re-instantiates the module and restores its state. `examples/web-playground` is the reference. piccolo, which did report errors as `Result`, was removed: its adapter was three times the code, its stdlib was not Luau's, and the vendored fix it needed made it unpublishable.

The conformance suite in `slotted-testutils` is therefore load-bearing: it runs the same `.lua` cases against all enabled adapters and is the gate for any swap.

`bevy_mod_scripting` 0.21 (Bevy 0.19, native only) is not used for the curated API. It remains an option for a "power mod" tier with reflection access, behind a feature, later.

### 4.10 slotted-packs

- `LayeredReader`: a Bevy `AssetReader` that resolves `pack://` paths through resource packs, then mods in reverse load order, then base assets. Also implements the registry crate's `AssetSource` port so the data stage reads through the same layering.
- `mod.toml`: id, version, api_version, dependencies with `optional`, `incompatible` and ordering hints, entry points `data.lua`, `control.lua`, asset root.
- Fluent localisation through `bevy_fluent`, keys namespaced per mod, layered by load order.

### 4.13 slotted-net (transport-agnostic networked authority)

The other side of the `Authority` port, and the one crate in the workspace that speaks a protocol.

- Messages mirror vanilla's container packets: `ClickContainer { menu, state_id, seq, action, predicted }` and `RequestResync` client to server; `Ack`, `SetSlot`, `SetContent`, `SetProperty` server to client. The predicted `Delta` rides along so an agreeing server answers with an ack rather than a container, and `seq` identifies a click, which `state_id` cannot do because the drag stages leave it unchanged.
- `RemoteAuthority<T>` is the client: it implements `slotted_model::Authority`, retransmits an unanswered click after a set number of polls, and turns server messages into `AuthorityEvent`s. A snapshot names the sequence number it answers, so it retires exactly that submission and everything older rather than whichever is oldest.
- `MenuServer` is the server. It holds a `ContainerStore` of inventories, each shared or private to one player, and one *session* per open screen. A session belongs to one peer, owns that player's cursor, drag, hints and properties, and binds one `InventoryId` per inventory its definition addresses; two players at one chest are two sessions binding one container. It applies every click to a scratch copy with `apply_click_validated` at `ValidationLevel::Always`, commits on `Ok`, acks a matching prediction, sends the container otherwise, and fans a container change out off the dirty masks to every other session bound to it, translated into that session's own slot numbering.
- Authorisation: every message that names a session is checked before anything is read. The server enforces that the peer owns the session and that a private inventory is never bound into another player's, and the `Access` port (`may_open`, `may_act`) is asked on top of that; `OwnerOnly` is the default. A peer naming another peer's session gets `Refused` and no state.
- Corrections: the server records the answer it gave per `(session, seq)` for a bounded window and replays it by kind, so a lost correction repeats as a correction and cannot be turned into an ack by the retry.
- `Transport` is the port: `send`, `poll`, `peers`. `Loopback` is the in-process adapter with a tick clock, latency, reordering jitter and a deterministic drop rate, which is what the tests run against.
- A `bevy_replicon` adapter is designed but not built; the shape is in `docs/design/gaps-notes-A.md` section 6.

### 4.11 slotted facade

`SlottedPlugins` plugin group wiring the defaults: `LocalAuthority`, `AtlasIcons`, the glass theme, the luaur runtime, `LayeredReader`. Everything is a resource holding an `Arc<dyn Port>` so an app can replace any adapter before `add_plugins`.

The `net` feature adds `SlottedNetPlugin`, which swaps `LocalAuthority` for a `slotted_net::RemoteAuthority` when the app has inserted a `ClientTransport` resource, and leaves the default alone when it has not. The same binary therefore plays single-player by simply not connecting.

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

Runtime: `slotted-script-luaur`. Built with `wasm-bindgen` through the `xtask`, served by `just serve`. Icons bake lazily in the browser. The page catches a module abort and restarts it (ADR 0004).

Stretch: a share button that encodes the two scripts into the URL hash.

## 6. Phases

Each phase ends with something runnable and a short verification list. Estimates assume one developer part-time.

### Phase 0. Spikes (1 to 2 weeks)

Throwaway code in `spikes/`, deleted after decisions are recorded as ADRs in `docs/adr/`.

- luaur, piccolo and mlua natively and on wasm32: load a script, call a function, round-trip a table, recover from a runtime error. Measure binary size and call cost.
- The Luau sandbox with an interrupt budget in a Bevy system.
- Glass panel: `BorderRadius` + `BoxShadow` + a `UiMaterial` sampling a blurred scene texture. Measure cost at 1080p.
- `bsn!` templates for a slot and a slot grid, and whether `ViewportNode` icons are viable for a hovered item.
- Headless UI: `MinimalPlugins` plus `bevy_ui` layout with a spawned `Window` entity and a non-rendering UI camera; drive `bevy_picking`'s UI backend with a custom pointer and confirm `Pointer<Click>` lands on the right node. This is the foundation of `slotted-test`.
- Decisions (all taken, see `docs/adr/0001` to `0004`): one runtime, luaur, on every target (ADR 0004 superseded ADR 0001 entirely); blur is a `blur` feature on `slotted-theme`; `bsn!` is used internally; the real picking backend works headless with three workarounds (fill `Camera::computed.target_info`, register the render asset types, use `PointerId::Mouse` for the primary pointer because `Hovered` is hard-wired to it).

### Phase 1. Model and registry (2 weeks)

`slotted-model`, `slotted-registry`, `slotted-testutils`. All seven click modes plus toolbar actions, routing tables, property defs, conservation property test, RON loading with patch rounds and freeze. No Bevy. Verification: `cargo test -p slotted-model -p slotted-registry`, golden vanilla layouts.

### Phase 2. ECS, ui core, theme, test harness, chest example (5 weeks)

`slotted-ecs` with `LocalAuthority`; `slotted-theme` with the glass theme and motion presets; `slotted-ui` with screen trees, slot grid, item renderer, tooltip, carried layer, action rail, semantic roles; `slotted-icons` with atlas baking; **`slotted-test` with the headless harness, locators, semantic and pointer actions, virtual time and `screen_tree()` snapshots**, built alongside the widgets so the widgets are designed to be driven; `examples/chest` without the browser, with a `tests/` directory written against `slotted-test`. Verification: the chest example matches the moodboard demo's behaviours; every click mode has a harness test that goes through real layout and picking; a screen-tree snapshot test; `settle()` terminates with motion on and off.

### Phase 3. Browser (3 weeks)

`slotted-browser`: ingredient registry, phases, categories, search index off-thread, panel, recipe view, transfer with dry-run, screen handlers reading exclusion zones. Chest example gains the browser. Verification: search grammar tests, 50k synthetic entries indexed under a second, transfer dry-run highlights.

### Phase 4. Scripting, packs, modded example (4 weeks)

`slotted-script` with the Lua prelude and the event and command enums; `slotted-script-luaur`; `slotted-packs` with `mod.toml`, layered reader and Fluent; `examples/modded` with the `copper_chest` mod and hot reload. Verification: conformance suite passes; a mod adds an item, recipe and screen without Rust; hot reload keeps inventory state.

### Phase 5. Web playground (3 weeks)

`slotted-script-luaur`; conformance suite passes on all enabled adapters; `xtask wasm`; `examples/web-playground` with editor, reload channel, console; CI builds and publishes the site. Verification: edit `control.lua` in the browser and see the behaviour change without a page reload; a script with an infinite loop is stopped and reported.

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
| luaur aborts the wasm module on an uncaught Lua error | one bad mod kills the page until it is rebuilt | the adapter reports the error before the trap and the host restarts and restores state; `examples/web-playground` proves the path in headless Chromium (ADR 0004) |
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
3. ~~Confirm the two-adapter scripting approach~~ Decided by the Phase 0 spike: mlua (Luau) native, piccolo on wasm, luaur feature-gated (ADR 0001). Settled in Phase 5 the other way: one adapter, luaur, on every target, with piccolo and mlua both removed (ADR 0004).
4. Confirm that networking stays a port with a local adapter in v1.
5. Confirm `slotted-test` as a published, public crate with locators over a semantic tree (which also gives us accessibility), rather than a test-only helper.

## 11. Outcome

Phases 0 to 7 are done, so is the gap-closing round that followed them
(2026-09-06, `docs/design/gaps-notes-{A,B,C}.md`), and so is the external
review round after that (2026-09-06, `docs/design/review-notes-{A,B,C}.md`,
eight findings, all closed -- see `docs/FOLLOWUPS.md` for the test that proves
each). What shipped, crate by crate:

| Crate | What landed |
|---|---|
| `slotted-model`, `slotted-registry` | The domain and the registries. No Bevy, no IO. Ghost and filter hints live on `MenuState`, and conservation is a runtime `ValidationLevel` rather than a debug assertion. |
| `slotted-ecs` | The model as components and events, prediction, the `Authority` port, and the resync a refused submission can ask for. |
| `slotted-net` | The networked adapter the plan's section 4.1 described: a predicting `RemoteAuthority`, an authoritative `MenuServer` running at `ValidationLevel::Always`, and a `Transport` port whose in-process `Loopback` can add latency, reorder and drop. The server holds a `ContainerStore` and one *session* per open screen, each belonging to one peer and authorised on every message through the `Access` port; answers are recorded per sequence number so a retransmission replays the kind of answer it first got. |
| `slotted-theme` | Ten materials, 36 roles, size and type tokens, per-preset motion, and three shipped skins with their OFL font files: glass, paper, neon. |
| `slotted-ui` | Screens as data and as `.screen.ron` assets, with inheritance, 14 node types, tooltips, anchors and injection, HUD layers, localisation, recording and replay. |
| `slotted-icons` | Lit-shape item icons: a deterministic CPU bake, an offscreen GPU rig that renders into the atlas, glTF item models behind the `gltf` feature, and live viewport icons. |
| `slotted-browser` | Item and recipe browser over any screen, with search, categories, bookmarks and transfer, laid out from theme tokens and re-docked on a scale change. |
| `slotted-script` and its adapter | One `slotted.*` surface on one runtime, Luau through luaur, on every target (ADR 0004). |
| `slotted-packs` | Mod discovery, layered assets, the two-stage lifecycle, hot reload, Fluent. |
| `slotted-test`, `slotted-testutils` | The public headless harness and the internal fakes. |
| `slotted` | The facade: `SlottedPlugins`, the prelude, the feature flags. `SlottedPlugins::server()` with `--no-default-features --features server` is a real dedicated-server graph: 160 crates against a default build's 300, with no Bevy UI stack on either target. |

**951 tests pass** with every feature on, plus two more that compile only in
the dedicated-server profile and run from `just server-check`, and three behind
a GPU gate that cannot currently be lifted (see the limitations below). The three native
examples and the web playground all run, each covered by its own harness tests
and its mods' `tests/*.lua`.

Known limitations, in the order they would block someone:

- On `wasm32` a Lua error a mod raises aborts the module and the host has to
  restart; `pcall` in a script does not contain it (ADR 0004).
- There is no `bevy_replicon` adapter. `slotted-net` is transport-agnostic and
  the adapter is designed in gaps notes A section 6, but it is not written, so
  a game shipping multiplayer today writes its own `Transport`.
- The CPU and GPU icon bakes still author their geometry twice, so a seventh
  `ShapeKind` has to be added to both.
- A `Model` icon's stand-in cannot be derived from the glTF: it is declared in
  the data file or it is a box in a colour hashed from the path.
- The GPU icon bake is confirmed on a WebGL2 context, but only a software one
  (ANGLE over SwiftShader). Its cost on hardware is unmeasured.
- The GPU render tests cannot run even on a machine with a GPU, because libtest
  gives each test its own thread and winit will not build an event loop off the
  main one. The evidence for that path is `just shot-chest` plus the pixel
  check in `just shot-check`.
- `cargo deny check` passes with one dated, single-id ignore for
  RUSTSEC-2026-0192 (`ttf-parser`, reached through winit's Wayland
  decorations), to be reviewed 2027-03-06.

Recommended next steps: write the `bevy_replicon` adapter against the
`Transport` port; project the bake meshes so the two bakes share their
geometry as well as their description; build a headless render harness so the
icon rig can be tested without a screenshot; and measure the GPU bake on a
hardware WebGL2 context. `docs/FOLLOWUPS.md` carries the rest.
