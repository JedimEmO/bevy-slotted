# Phase 3 contract: the item and recipe browser (`slotted-browser`)

Status: v1.0, 2026-09-05. Bevy 0.19.1. Companion to `docs/PLAN.md` 4.7 and Phase 3,
`docs/research/research-nei-overlays.md` (sections 2, 3, 5, takeaways) and
`docs/design/phase2-contract.md` v1.1, which this builds on and does not restate. Two agents
implement it in parallel without talking to each other: **Package A** (logic) and **Package B**
(UI). Anything not written here is a private decision of the package that owns the file. The
skeleton in `crates/slotted-browser` carries the real signatures; a body marked
`// PHASE3-IMPL: A` or `: B` is that package's to fill. Signatures may gain parameters only by
amending this document.

## 0. Ground rules

- Dependency direction: `model <- registry <- ecs <- theme, icons <- ui <- browser <- slotted <- test`.
  `slotted-browser` uses only `slotted-ui`'s public API (`def`, `screen`, `layers`, `semantic`,
  `tooltip`, `item`, `widgets::{SLOT_SIZE, spawn_button}`, `zbands`) and never imports
  `slotted-test`. `slotted-test` gains a dependency on `slotted-browser` for its `browser()` query
  object (section 7). The facade's `browser` feature (default on) adds `SlottedBrowserPlugin`.
- Package A files contain no `bevy_ui`, `bevy_text`, `bevy_picking` or `bevy_window` types.
  `bevy::ecs`, `bevy::math`, `bevy::tasks`, `bevy::time` and `bevy::input::keyboard::KeyCode`
  (for `KeyMappings` data) are allowed. Package B files are the only ones under `src/ui/`.
- Events between the packages are `Message`s (buffered, section 6), never entity events, because
  neither side owns an entity the other knows about. Everything else is a resource A writes and B
  reads (`BrowserRuntime`, `IndexState`) or B writes and A reads (`BrowserRuntime.has_keyboard_focus`).
- Frame order in `Update`: `BrowserSet::Input` (B: hotkeys, focus flag, search field diff) runs
  inside `SlottedUiSet::Input`; `BrowserSet::Apply` (A: drain messages, evaluate search, plan
  transfers, trigger `MenuAction`s) runs after `SlottedUiSet::Input` and before `SlottedEcsSet::Input`
  so a transfer's clicks are predicted the same frame; `BrowserSet::Index` (A: poll the build
  task) and `BrowserSet::Render` (B: rebind cards, cycle alternatives, button states) run inside
  `SlottedUiSet::Render`. `BrowserSet::Layout` (B: docking) runs in `PostUpdate` after
  `SlottedUiSet::Layout`. Configured once in `plugin.rs` (architect-owned).
- No wall clock. Ingredient cycling and everything else read `Time<Virtual>`.
- Item conservation stays intact: the browser mutates inventories only by triggering
  `slotted_ecs::MenuAction`. The one action that creates items, `ClickAction::Give`, is a model
  action gated on `Actor::can_cheat` (section 5).

## 1. Ingredients (A, `ingredient.rs`)

`IngredientTypeId(Namespaced)`; built-ins `types::{item, fluid, tag, info}()` are
`slotted:item`, `slotted:fluid`, `slotted:tag`, `slotted:info`. `SubtypeKey(String)` with
`SubtypeKey::none()` (`""`): the equals/hash-able output of a subtype interpreter, so two stacks
with the same key are one browser entry. `IngredientValue::{Item(ItemId), Fluid(Namespaced),
Tag(Namespaced), Info(Namespaced)}`. `Ingredient { ty: IngredientTypeId, value: IngredientValue,
subtype: SubtypeKey }` is `Eq + Hash + Ord + Serialize`; `Ingredient::item(id)` and
`Ingredient::item_with(id, key)` are the constructors everyone uses.

```rust
pub trait SubtypeInterpreter: Send + Sync {
    fn key(&self, stack: &ItemStack, registries: &FrozenRegistries) -> SubtypeKey;
    fn variants(&self, item: ItemId, registries: &FrozenRegistries) -> Vec<SubtypeKey>; // entries to expand
}
pub trait IngredientType: Send + Sync {
    fn id(&self) -> IngredientTypeId;
    fn display_name(&self, ing: &Ingredient, ctx: &IngredientCtx<'_>) -> String;
    fn mod_namespace(&self, ing: &Ingredient, ctx: &IngredientCtx<'_>) -> String; // `@` field
    fn tags(&self, ing: &Ingredient, ctx: &IngredientCtx<'_>) -> Vec<String>;   // `#` field
    fn tooltip_text(&self, ing: &Ingredient, ctx: &IngredientCtx<'_>) -> Vec<String>; // `$` field
    fn icon(&self, ing: &Ingredient, icons: &dyn IconSource) -> IconRef;
    fn entries(&self, ctx: &IngredientCtx<'_>) -> Vec<Ingredient>;  // everything this type lists
    fn matches(&self, ing: &Ingredient, stack: &ItemStack, ctx: &IngredientCtx<'_>) -> bool;
    fn as_stack(&self, ing: &Ingredient, count: u32) -> Option<ItemStack>; // for Give and ghosts
}
```

`IngredientCtx { registries: &FrozenRegistries, subtypes: &Subtypes }`. `Subtypes` resource:
`register(item: Namespaced, Arc<dyn SubtypeInterpreter>)`, default interpreter returns `none()`
and one variant. `IngredientTypes` resource: `register(Arc<dyn IngredientType>)`, `get(&id)`,
`iter()`. Built-ins: `ItemType` (entries = every `FrozenRegistries.items` entry times its subtype
variants; tooltip text = `TooltipParts` output is *not* available here, so it is display name,
rarity and the `slotted:*` component keys the def declares), `TagType` (one entry per tag name,
matches any member), `InfoType` (entries registered through `InfoPages` resource: id, title,
body lines; matches nothing), `FluidType` placeholder (no entries, all methods trivial).

## 2. Registration phases and validation (A, `plugin.rs` sets, `validate.rs`)

`BrowserPhase::{Subtypes, IngredientTypes, Categories, Recipes, Transfer, ScreenHandlers,
Runtime}` are chained `SystemSet`s in `Startup`, after `slotted_ui`'s `load_registry_screens`.
A consumer registers with `app.add_systems(Startup, my_system.in_set(BrowserPhase::Categories))`
and writes to the phase's resource (`Subtypes`, `IngredientTypes`, `Categories`, `RecipeStore`,
`TransferHandlers`, `ScreenHandlers`). The last system of every set is `validate_<phase>`, which
checks cross-references against earlier phases (a category naming an unknown recipe type, a
transfer handler naming an unknown category, a handler for an unregistered `ScreenKind`), logs
each `ValidationError`, appends it to the `BrowserValidation` resource and removes the offending
entry. `BrowserConfig::strict` (default: `cfg!(debug_assertions)`) panics instead. `Runtime`
builds `RecipeStore` from `FrozenRegistries.recipes`, inserts `BrowserRuntime`, starts the index
build and writes `IndexReady` when it lands. Hot reload (Phase 4) re-runs the same systems by
writing `RebuildBrowser`.

## 3. Recipe categories (A, `category.rs`, `recipes.rs`)

`CategoryId(Namespaced)`. `RecipeRef(RecipeId)`: Phase 3 recipes are registry recipes; synthetic
recipes (mob drops, info) are Phase 6. `SlotRole::{Input, Output, Catalyst, RenderOnly}`.
`RecipeSlotIx(u16)` indexes `RecipeLayout.slots`.

```rust
pub struct RecipeSlot { pub role: SlotRole, pub pos: Vec2 /* px inside the category size */,
                        pub alternatives: Vec<Ingredient> /* concrete, tags expanded */, pub count: u32 }
pub struct RecipeLayout { pub slots: Vec<RecipeSlot>, pub extras: Vec<UiNodeDef>, pub arrow: Option<Rect> }
pub struct LayoutBuilder { .. } // slot(role, pos, alternatives, count) -> RecipeSlotIx; extra(UiNodeDef); arrow(Rect); finish() -> RecipeLayout
pub struct RecipeView<'a> { pub id: RecipeRef, pub def: &'a RecipeDef, pub registries: &'a FrozenRegistries,
                            pub types: &'a IngredientTypes, pub focus: Option<&'a Ingredient> }
pub trait RecipeCategory: Send + Sync {
    fn id(&self) -> CategoryId;
    fn title_key(&self) -> LocKey;
    fn icon(&self) -> IconDef;
    fn size(&self) -> Vec2;                       // px, panel-independent
    fn recipe_types(&self) -> Vec<Namespaced>;    // registry types it lays out
    fn layout(&self, recipe: &RecipeView<'_>, out: &mut LayoutBuilder);
    fn catalysts(&self) -> Vec<Ingredient> { vec![] } // workstations shown beside the tabs
}
```

`Categories` resource: `register(Arc<dyn RecipeCategory>)`, `for_recipe_type(&Namespaced)`,
`get(&CategoryId)`, `iter()` in registration order (tab order). `validate_categories` calls
`Categories::bind_defaults`, which binds every `RecipeTypeDef` that no category claims to a default: `CraftingCategory::new(id, type, size)`
when `RecipeTypeDef::size != (1,1)`, else `ProcessingCategory::new(id, type)`; the default
category id is the recipe type's id. `CraftingCategory` lays shaped recipes by `shape`/`key` on a
`cols x rows` grid of `SLOT_SIZE` cells, shapeless row-major, output to the right of an arrow;
`ProcessingCategory` is one input, arrow, one output, optional `RenderOnly` fuel slot from
`RecipeDef::extra.fuel`. Tag ingredients expand through `FrozenRegistries.tag_index` in
`RecipeView` order; `Ingredient::AnyOf` flattens. `RecipeStore` (built in `Runtime`): per
category the ordered `Vec<RecipeRef>`; `recipes_for(output: &Ingredient)` and `uses(&Ingredient)`
over `FrozenRegistries.recipe_index` widened to subtype `none()`; `layout(RecipeRef, focus) ->
RecipeLayout` memoised per (recipe, focus is Some).

## 4. Search index (A, `index/`, `search.rs`)

`EntryId(u32)` is the dense position in `BrowserIndex.entries: Vec<Entry>`; `Entry {
ingredient, display: String, mod_ns: String, rarity: Rarity, tags: Vec<String>, categories:
Vec<String> /* categories with a recipe producing it */ }`. `BrowserIndex::build(&FrozenRegistries,
&IngredientTypes, &Subtypes, &RecipeStore) -> BrowserIndex` is pure and runs on
`AsyncComputeTaskPool` (`index/build.rs`, `IndexState::{Empty, Building(Task<BrowserIndex>),
Ready(Arc<BrowserIndex>)}` resource, polled in `BrowserSet::Index`). Fields:

| Field | Prefix | Storage | Complexity |
|---|---|---|---|
| display name | none | `SubstringIndex`: sorted suffix array over the lowercased concatenation with entry boundaries; query = binary search for the prefix range, then dedupe entry ids | build `O(n log n)` with `n` total chars, query `O(m log n + k)` |
| tooltip text | `$` | second `SubstringIndex` | same |
| mod namespace | `@` | `StringBitsetMap`: `BTreeMap<String, Bitset>` with prefix range scan | query `O(log d + matches)` |
| tag | `#` | `StringBitsetMap` | same |
| category | `%` | `StringBitsetMap` | same |
| id (`ns:path`) | `&` | `SubstringIndex` | as name |

`Bitset` is a `Vec<u64>` with and/or/and_not/iter. `SearchConfig { modes: BTreeMap<char,
PrefixMode> }` with `PrefixMode::{Enabled, RequirePrefix, Disabled}`; defaults per JEI: `$`
Enabled (plain words also hit tooltips), `@ # %` RequirePrefix, `&` Disabled. Grammar
(`tokenize(&str, &SearchConfig) -> Vec<Token>`): whitespace splits terms (AND); `|` between
terms is OR and binds tighter than AND (`a b|c` = `a AND (b OR c)`); `-` at term start negates;
`"quoted phrase"` keeps spaces; `\` escapes the next char; a leading prefix char from the table
selects the field, a Disabled prefix is treated as literal text, an Enabled prefix's field also
receives unprefixed terms. `Token { field: Field, text: String, negate: bool, or_group: u32 }`.
`evaluate(&BrowserIndex, &[Token], &SearchConfig, hidden: &Bitset) -> Bitset` intersects
positive groups, subtracts negated ones, then `and_not hidden`; an empty query is every visible
entry. `sort(&BrowserIndex, Bitset, &[SortStage]) -> Vec<EntryId>` with `SortStage::{Registration,
Alphabetical, IngredientType, ModName, Rarity}`; default `[IngredientType, Alphabetical]`.
`SearchCache` keeps the last `(query, hidden_version, visibility_version) -> Arc<Vec<EntryId>>`.
`HiddenEntries { set: Bitset, version: u32 }` resource is the edit-mode blacklist. Acceptance:
50k synthetic entries build under one second in release (`benches/index.rs`, criterion), the
grammar tests in `search.rs` cover every rule above.

## 5. Bookmarks, transfer, screen handlers, runtime (A)

`Bookmark::{Item(Ingredient), Recipe(RecipeRef)}` (`Serialize`, `#[serde(tag = "type")]`; actions
are Phase 6). `Bookmarks { entries: Vec<Bookmark> }` resource: `toggle`, `contains`, `move_to`.

**Transfer.** `TransferHandlers` resource keyed by `(ScreenKind, CategoryId)`; a `None` category
key is the handler's fallback for every category of that screen.

```rust
pub struct TransferCtx<'a> { pub def: &'a MenuDef, pub inventories: &'a Inventories, pub state: &'a MenuState,
                             pub actor: Actor, pub lookup: &'a dyn LookupCtx, pub types: &'a IngredientTypes, pub ctx: IngredientCtx<'a> }
pub struct TransferError { pub kind: TransferErrorKind, pub missing: Vec<RecipeSlotIx>, pub message: String }
pub enum TransferErrorKind { NoHandler, CursorNotEmpty, MissingIngredients, GridNotEmpty }
pub struct TransferPlan { pub actions: Vec<ClickAction> }
pub trait TransferHandler: Send + Sync {
    fn plan(&self, ctx: &TransferCtx<'_>, layout: &RecipeLayout, max: bool) -> Result<TransferPlan, TransferError>;
    fn dry_run(&self, ctx: &TransferCtx<'_>, layout: &RecipeLayout) -> Result<(), TransferError> { self.plan(ctx, layout, false).map(|_| ()) }
}
```

A plan is a sequence of `ClickAction`s the `execute_transfers` system triggers as `MenuAction`s
on the menu entity in order, in one frame; prediction applies them sequentially, the authority
sees ordinary clicks, and conservation holds by construction. `SimpleTransfer { grid: InventoryRef,
grid_cols: u16, sources: Vec<InventoryRef> }` plans: refuse if the carried stack is `Some`
(`CursorNotEmpty`) or a grid cell is occupied by a non-matching stack (`GridNotEmpty`); for each
`Input` slot find a source slot whose stack matches one alternative (first alternative with any
match wins, ties by lowest slot); if any slot has none, `MissingIngredients` with every such
`RecipeSlotIx`; otherwise emit `Pickup{src, Left}`, one `Pickup{cell, Right}` per grid cell fed
from that source, `Pickup{src, Left}` to return the remainder (or `Pickup{first empty source,
Left}` when `src` was emptied), grouped by source. `max` repeats the set while every input still
has a match, up to the cell cap. Recipe slot `i` maps to grid cell `pos / SLOT_SIZE` on the
handler's grid, so the category's pixel layout and the menu's `SlotIx` never meet outside this
function.

**Give.** `slotted_model::ClickAction::Give { item: ItemId, count: u32, target: GiveTarget }`,
`GiveTarget::{Cursor, Inventory(InventoryRef)}`, added in this phase (skeleton implements it).
`apply_click` returns `ClickError::Permission` unless `actor.can_cheat`; `Cursor` sets or merges
the carried stack (`NotAllowed` on a different kind), `Inventory` runs `move_into` over that
inventory's slots (`NothingToDo` if nothing fits); the debug conservation assertion treats it
like `Clone`. The authority path is unchanged: `LocalAuthority` trusts prediction, so the
permission check in `apply_click` is the whole gate, exactly as it is for `Clone`; a networked
authority re-runs `apply_click` with its own `Actor`. Subtype patches are not carried by `Give`
in Phase 3 (`ClickAction` stays `Copy`); `IngredientType::as_stack` is where a later phase adds
them. `UiHarness::assert_conserved` baselines break after a `Give`; tests call `rebaseline()`
(Package B adds it, section 7).

**Screen handlers.** `ScreenHandlers` resource keyed by `ScreenKind`, `AttachPolicy::{Registered,
AllMenus}` in `BrowserConfig` (default `Registered`, so no Phase 2 snapshot changes).

```rust
pub trait ScreenHandler: Send + Sync {
    fn bounds(&self, screen: &ScreenGeometry) -> Rect { screen.root_rect }        // default: the def root's rect
    fn extra_exclusions(&self, screen: &ScreenGeometry) -> Vec<Rect> { vec![] }  // added to the ExclusionZone union
    fn clickable_areas(&self) -> Vec<ClickableArea> { vec![] }                   // rect (screen-relative) -> CategoryId
    fn stack_under_cursor(&self, screen: &ScreenGeometry, world: &World, cursor: Vec2) -> Option<Ingredient> { None } // non-slot widgets; slots are found by SlotRef
    fn ghost_drop(&self, screen: &ScreenGeometry, world: &World, target: Entity, ing: &Ingredient) -> GhostDrop { GhostDrop::Refuse }
}
```

`ScreenGeometry { screen: Entity, kind: ScreenKind, menu: Option<Entity>, root_rect: Rect,
window: Rect, exclusions: Vec<Rect> }`. `GhostDrop::{Accept(Vec<ClickAction>), Refuse}`: the default
handler accepts a drop on a `SlotRef` whose `SlotBehaviour::is_ghost()` with `[Give { .., Cursor },
Pickup { slot, Left }]` when `actor.can_cheat`, else `Refuse`; a non-cheating ghost drop will set
the hint through `GhostHint` (a `Message` A defines and the ecs crate does not yet consume; Phase 6
wires it). `DefaultScreenHandler` implements the
defaults and is what `AttachPolicy::AllMenus` uses.

**Runtime.** `BrowserRuntime { filter_text: String, visible: Arc<Vec<EntryId>>, bookmarks_version:
u32, keys: KeyMappings, has_keyboard_focus: bool, history: Vec<RecipePage>, forward: Vec<RecipePage>,
open: Option<RecipePage>, visibility_version: u32 }`
where `RecipePage { category: CategoryId, focus: Ingredient, mode: PageMode::{Recipes, Uses},
recipe: usize }`. `KeyMappings { recipes: KeyCode::KeyR, uses: KeyU, bookmark: KeyA, focus_search:
(ctrl, KeyF), back: Backspace, close: Escape, prev_page: PageUp, next_page: PageDown }`. A owns
every field except `has_keyboard_focus`, which B's `track_keyboard_focus` writes in
`BrowserSet::Input`: true while `InputFocus` is on any entity with `EditableText`.

## 6. Messages between A and B (`events.rs`, architect-owned)

| Message | Writer | Reader | Meaning |
|---|---|---|---|
| `SearchChanged { text }` | B | A | The search field's value differs from `filter_text`. A re-evaluates and writes `visible`. |
| `IndexReady { entries: usize }` | A | B | `IndexState` became `Ready`; B rebinds cards. |
| `OpenRecipes(Ingredient)` / `OpenUses(Ingredient)` | B | A | A pushes a `RecipePage` (history) and sets `open`. |
| `RecipeNav::{Back, Forward, Close, Page(i32), Category(CategoryId)}` | B | A | History and paging. |
| `TransferRequested { recipe: RecipeRef, max: bool }` | B | A | A plans through the handler and triggers `MenuAction`s or writes `TransferFailed`. |
| `TransferFailed { recipe, error: TransferError }` | A | B | Red highlights on `error.missing`. |
| `BookmarkToggled(Bookmark)` | B | A | A toggles and bumps `bookmarks_version`. |
| `GiveRequested { ingredient, count, target }` | B | A | A resolves `as_stack` and triggers `MenuAction { Give }` if `actor.can_cheat`, else logs. |
| `RebuildBrowser` | anyone | A | Re-run phases and rebuild the index. |

B never reads `IndexState` directly for card content; it reads `BrowserRuntime.visible` and
resolves `EntryId` through `IndexState::ready() -> Option<&Arc<BrowserIndex>>`.

## 7. The panel (B, `src/ui/`)

**Attachment.** `attach_panels` observes `ScreenLayout` and, if `ScreenHandlers` has the kind (or
`AllMenus`), spawns one panel root per screen: a top-level node with `GlobalZIndex(zbands::BROWSER)`,
`ScreenRoot { kind: ScreenKind("slotted:browser"), menu }`, `SemanticRole::Browser`, `TestId("browser")`,
`BrowserPanel { screen }`, `TabGroup::new(1)`. `ScreenClosed` on the screen despawns it. The
subtree is spawned from `panel_def()` (a `UiNodeDef::Panel` with `Custom` children of kinds
`slotted:search_field`, `slotted:chip_row`, `slotted:card_grid`, `slotted:bookmarks_strip`,
`slotted:recipe_view`) through `SpawnCtx`, so RON and Lua can restyle it later. Tags on the root:
`side=left|right`.

**Docking** (`dock.rs`, `BrowserSet::Layout`): `free = window minus (handler.bounds ∪
exclusions.union(screen) ∪ handler.extra_exclusions)`, projected to the left and right strips;
dock to the wider strip when it fits `2 * (SLOT_SIZE + gap) + padding`, else hide the panel
(`Visibility::Hidden`, tag `side=none`). `cols`, `rows` derive from the strip; recompute on
`ScreenLayout`, `WindowResized`, `Changed<UiScale>` and when the exclusion union's hash changes.
The panel writes `BrowserLayout { side: Side, rect: Rect, cols: u16, rows: u16 }`.

**Widgets** (each a `Widget` registered in `WidgetRegistry` under `slotted:*`):

| Kind | Root | Children and behaviour |
|---|---|---|
| `search_field` | `SemanticRole::TextField`, `TestId("browser.search")`, wraps `bevy_text::EditableText` (`EditableTextInputPlugin` is in `UiWidgetsPlugins`, `TextPlugin` adds the clipboard, so it runs headless; if `type_text` in the harness does not reach it within the first day, replace the inner component with `MinimalTextField` reading `KeyboardInput`, keep the wrapper) | `SearchChanged` on value change; Ctrl+F focuses it; Esc blurs |
| `chip_row` | `SemanticRole::Panel`, `TestId("browser.chips")` | one `Chip` per category, tag `category=<id>`, toggles a `%category` term |
| `card_grid` | `SemanticRole::Grid`, `TestId("browser.cards")` | a fixed pool of `cols*rows` `Card` entities (`SemanticRole::Card`, `ItemView`, tag `entry=<ingredient id>`, `SemanticLabel(display)`, rarity strip child, mod badge child); `CardGrid { page }` rebinds on `Changed<BrowserRuntime>`; wheel and PgUp/PgDn page; hidden cards are `Visibility::Hidden` |
| `bookmarks_strip` | `SemanticRole::Panel`, `TestId("browser.bookmarks")` | one `Bookmark`-role node per entry |
| `recipe_view` | `SemanticRole::RecipeView`, `TestId("browser.recipes")`, hidden until `open` is `Some` | `Tab` per category (`tag category=`), `RecipeSlot` nodes (`ItemView`, tag `role=input|output|catalyst|render_only`, `tag ix=`), `Button` `TestId("browser.transfer")` enabled from `dry_run` each frame the page changes, `Themed("browser.slot.missing")` on `TransferFailed.missing`, back/forward buttons, arrow child with `Tween` progress, "used in" list of `Card`s |

Alternatives cycle every `durations.slow` ms of `Time<Virtual>`, paused while Shift is held.
Hotkeys (`hotkeys.rs`, `BrowserSet::Input`) read `KeyMappings` and fire only when
`!has_keyboard_focus`: R/U over a `Card`, `RecipeSlot` or `SlotRef` write `OpenRecipes`/`OpenUses`
for the hovered ingredient (`handler.stack_under_cursor` as the fallback); A writes
`BookmarkToggled`; Backspace/Esc write `RecipeNav`. Card click writes `OpenRecipes`; right click
`OpenUses`; Ctrl+click writes `GiveRequested` when the menu's `Actor::can_cheat`. Ghost drag:
`Pointer<DragStart>` on a card spawns a `CarriedLayer` ghost, `Pointer<DragDrop>` on a `SlotRef`
asks `handler.ghost_drop` and triggers the returned action. Cards and slots request tooltips
through `TooltipRequest`, so the shared tooltip layer draws them above the panel.

**SemanticRole additions** (made in `slotted-ui` by the skeleton): `Browser` (Pane), `TextField`
(TextInput), `Chip` (CheckBox), `Card` (ListItem), `RecipeView` (Group), `RecipeSlot` (Cell),
`Tab` (Tab), `Bookmark` (ListItem).

**ScreenTree shape** (what snapshots pin; order is spawn order):
`Browser[browser, side] > TextField[browser.search], Panel[browser.chips] > Chip*, Grid[browser.cards] > Card*(item), Panel[browser.bookmarks] > Bookmark*, RecipeView[browser.recipes] > Tab*, RecipeSlot*, Button[browser.transfer], Button[browser.back], Button[browser.forward], Panel[browser.uses] > Card*`.

**Harness additions Package B may make in `slotted-test`** (new `browser.rs`, plus
`UiHarness::rebaseline()`): `h.browser() -> Browser<'_>` with `visible_entries() -> Vec<Ingredient>`,
`search(text)` (focus the field, `type_text`, settle), `open_recipes(entity)`, `open_uses(entity)`,
`open_page() -> Option<RecipePage>`, `transfer()` (clicks `browser.transfer`), `bookmark(entity)`,
`is_attached(screen) -> bool`, `layout(screen) -> Option<BrowserLayout>`, `wait_for_index()`
(steps until `IndexState::Ready`, capped by `max_settle_frames`). Locators: `by::role(Role::Card)`,
`by::test_id("browser.search")`, `.tag("entry", "minecraft:coal")` already work.

## 8. Fixtures, example, features

`TestRegistries::basic()` gains recipe types `demo:crafting` (3x3) and `demo:smelting` (1x1) and
three recipes: `demo:iron_pickaxe` (shaped), `demo:diamond_sword` (shapeless, `#minecraft:planks`),
`demo:coal` (smelting, `#minecraft:planks -> coal`). `assets/data/demo` gains the matching
`recipe_types/smelting.ron` and `recipes/coal.ron`. The chest example registers
`DefaultScreenHandler` for `demo:chest` and no transfer handler (a chest has no grid), so its
browser shows recipes with a disabled `+`; Package B re-accepts its snapshots. Features on
`slotted-browser`: `dev` (exclusion highlighter, id tooltips, copy recipe id); the facade's
`browser` feature (default) adds the plugin to `SlottedPlugins` and re-exports the crate as
`slotted::browser`. The crate checks on `wasm32-unknown-unknown`; the index build runs on the main
thread there. `crates/slotted-test/tests/browser_skeleton.rs` pins what the skeleton already does
(index lands, three recipes filed, a search narrows `visible`, attach policy); Package B may fold
it into `tests/browser.rs`.

## 9. Ownership

| Package | Files |
|---|---|
| A | `ingredient.rs`, `category.rs`, `recipes.rs`, `index/*`, `search.rs`, `bookmarks.rs`, `transfer.rs`, `handlers.rs`, `runtime.rs`, `validate.rs`, `benches/index.rs`, `tests/{search,transfer,index}.rs`; `slotted-model` `Give` follow-ups; fixtures recipes if missing |
| B | `src/ui/*`, `slotted-test/src/browser.rs`, `UiHarness::rebaseline`, chest example browser wiring and snapshots, `tests/panel.rs` |
| shared (amend here first) | `lib.rs`, `plugin.rs`, `events.rs`, `Cargo.toml`, `slotted-ui/src/semantic.rs` |

## 10. Deviations from PLAN.md 4.7

- `Ingredient` is a struct with an `IngredientValue` enum, not a raw tuple, so it can be `Ord`
  and serde-tagged for bookmarks.
- Transfer handlers are keyed by `(ScreenKind, Option<CategoryId>)` instead of living on the screen
  handler, matching JEI's separation and keeping `ScreenHandler` object-safe without a category.
- Cheat-give is a model `ClickAction::Give`, not a browser-only path, so it rides the authority.
- The panel attaches only to registered screen kinds by default (`AttachPolicy::Registered`).
- The `%` prefix means "recipe category that produces the entry"; there are no creative tabs.
- The recipe tree (EMI-style) stays a stretch goal and has no skeleton surface.
