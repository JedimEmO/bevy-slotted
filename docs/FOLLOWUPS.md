# Follow-ups

Decisions deferred during phases. Each entry names the phase that should pick it up.

## From Phase 1 review

- **Ghost and Filter hints are stored as real count-1 stacks in the backing inventory.** `Inventories::count_of` therefore sees phantoms. Options: a parallel `ghosts` map on `Inventory`, or a `SlotBehaviour`-aware counting API. Decide in Phase 2 when the ECS layer needs to render ghost slots differently anyway. Pinned by `a_ghost_hint_is_visible_to_inventory_counting`.
- **Item conservation is asserted in debug builds only.** If `apply_click` runs as server-side validation, release builds need the check. Plan: make it a runtime `ValidationLevel` on the `Authority` adapter, not a compile-time assert. Decide when the networked authority adapter is designed.

## From Phase 2

- **`SemanticLabel` on a slot reads the item's namespaced id, not its display name.** A screen
  reader hears "minecraft:cobblestone x64" while the tooltip beside it says "Cobblestone".
  `slotted_ui::item::semantic_label` should prefer `ItemDef::display_name` and fall back to the id,
  once localisation exists to say which name is the player-facing one. **Phase 3** (localisation),
  which also owns `LocKey` resolution: the same snapshot shows `Text` nodes labelled `chest.title`
  and rail buttons labelled `sort`. Changing this rewrites the three accepted `ScreenTree`
  snapshots in `slotted-test`.
- **`UiNodeDef::{Tank, Bar, SideTab, Viewport, VirtualGrid}` spawn a layout-only placeholder.**
  `SpawnCtx::spawn_placeholder` gives them a `Node` and `SemanticRole::Custom(<name>)` so a screen
  using one still opens and still lays out. **Phase 6** implements them; the property-bound ones
  (`Tank`, `Bar`) need the `MenuProperty` binding that Phase 2 only mirrors.
- **`slotted_icons::LiveIcons` returns `IconRef::Missing`.** The `live` feature compiles the type
  but there is no offscreen item-model bake yet. **Phase 6**, together with `Viewport`.
- **Screen inheritance is not implemented.** `Screens::resolve` clones and warns. What
  "the ancestor's tree with this screen's anchors kept" means for a child screen that also has a
  root needs a decision. **Phase 3**, per `docs/design/phase2-notes-B.md` item 11.
- **`Favorite` is mirrored as a marker component but is not a theme role swap.** The glass theme
  defines `slot.favorite` and nothing selects it. **Phase 3**, with the rest of the slot state
  machine.
- **A refused submission cannot ask the authority for a resync.** `slotted_model::Authority` has no
  such method, so `submit` re-emits the menu's slots from local state instead. A networked adapter
  has to answer with `AuthorityEvent::Resync` rather than `Err`. Revisit when the networked
  authority adapter is designed, per `docs/design/phase2-notes-A.md` item 6.
- **`SLOT_SIZE` is a 44px constant in `slotted-ui`, not a theme token.** It should become one the
  first time a theme needs a different slot size. **Phase 3**.

## From the chest example

- **`SlottedPlugins::default()` is not enough on top of `DefaultPlugins`.** `slotted_ui`'s
  navigation systems need `TabNavigationPlugin` and `DirectionalNavigationPlugin`, and
  `DefaultPlugins` ships neither, so a windowed game panics on its first frame with
  `manual_directional_navigation ... Resource does not exist`. `examples/chest/src/main.rs` adds
  them by hand. The facade should either add them itself or ship a `SlottedPlugins::windowed()`
  companion to `headless()`. **Phase 3**, and it is a five-minute fix.
- **There is no `ScreenDef` asset loader.** `slotted-theme` registers a `ThemeLoader` for
  `*.theme.ron`, but a screen has to be read with `std::fs` and `ScreenDef::from_ron`, which costs
  the example hot reload and `AssetServer` path resolution. A `ScreenLoader` for
  `*.screen.ron`, plus an `AssetEvent` hook that respawns open screens, belongs with the rest of
  the reload story. **Phase 3**.
- **Nothing renders a rail button's or a text node's label in a human language.** `LocKey` is
  shown verbatim and rail buttons are labelled `quick_stack`, so the example carries a
  `fill_labels` system that substitutes strings by `test_id` and by the `action` tag. That is a
  reasonable thing for a game to own, but not for every game to have to write. **Phase 3**, with
  localisation; the same entry already appears above for `SemanticLabel`.
- **The durability bar reads `slotted:damage` and `slotted:max_damage` off a stack's patch, but a
  component key only exists if some `ItemDef` declares it.** The demo's three tools each list both
  keys in `components` purely so the freeze interns them. A first-class "component types" registry,
  or interning the keys the renderer needs unconditionally, would remove the trap. **Phase 3**.
- **`slotted_test::fixtures` cannot be reused by a data-driven example.** `TestRegistries::basic()`
  builds its item table in Rust and freezes it into its own dense ids, so an example that loads
  items from RON has to duplicate the table. The item rows want to live in a shared `assets/data/`
  directory that both the fixtures and the examples read. **Phase 3**.
- **`examples/chest/assets` is a symlink to the workspace `assets/`.** Bevy's `AssetPlugin`
  resolves against `CARGO_MANIFEST_DIR`, so without it `cargo test -p chest` cannot find
  `themes/glass.theme.ron`. A `UiHarnessBuilder::asset_root(path)` would let a consumer point the
  harness at their own asset directory instead. **Phase 3**.

## From the Phase 2 adversarial review

- **Bevy gives every `ImageNode` its own `AccessibilityNode`.** The contract calls the icon child
  of a slot purely visual and gives it no `SemanticRole`, but AccessKit still announces 27
  anonymous images inside the chest grid. The semantic tree and the AccessKit tree therefore have
  different node counts, and a screen reader hears the difference. Suppressing the child's node
  (or labelling it) belongs with the rest of the a11y pass. Pinned by
  `slotted-ui/tests/review_adversarial.rs::every_interactive_node_is_semantic_and_reaches_accesskit`.
  **Phase 3**.
- **Two motion presets are still unused.** `MotionPreset::Hover`, `Press`, `DropSquash` and
  `FlyToSlot` are wired into slots by `slotted_ui::motion`, but `Stagger` (children appearing one
  after another when a screen opens) and `Fade` (tooltip and panel entry) have no caller, and
  buttons animate nothing at all. `SQUASH_SCALE` and friends are constants in `slotted-ui` rather
  than theme tokens, so a theme cannot say how far a slot pops. **Phase 3**, with the slot state
  machine and the `SLOT_SIZE` token above.
- **`directional_nav_keys` and `hotbar_swap_keys` read the keyboard unconditionally.** Neither
  consults `InputFocus` to see whether a text-entry node owns the keyboard, so the day a search
  field lands in the browser panel, typing "3" into it will also swap a hovered slot with hotbar
  slot 3 and the arrow keys will move focus out of the field. There is no text widget in Phase 2,
  so no test can prove it yet; `slotted-test/tests/review_adversarial.rs::type_text_is_inert_while_no_text_field_exists`
  documents the absence. **Phase 3**, with the browser's search box.
- **A screen and its tooltips live in different trees.** Tooltips are spawned under
  `TooltipLayer`, not under the screen root, so despawning a screen used to leave its tooltip on
  screen forever. `despawn_orphan_tooltips` now sweeps them. The same shape applies to anything
  else a screen parks in a shared layer; a "owned by screen" relationship would be sturdier than
  a sweep. **Phase 3**.

## From Phase 3

- **`BrowserRuntime::visibility_version` invalidates nothing.** `apply_search` recomputes only
  when the query changed, the index landed or `HiddenEntries` changed; a bump of
  `visibility_version` alone leaves `visible` stale even though the field is part of the
  `SearchCache` key. Nothing in Phase 3 writes it (cheat mode and dev items are Phase 6), so no
  test can fail today. Add it to the dirty check when the first writer arrives. **Phase 6**.
- **The recipe view's header changes the contract's tree order.** Section 7 pins
  `RecipeView > Tab*, RecipeSlot*, Button[browser.transfer], Button[browser.back],
  Button[browser.forward], Panel[browser.uses]`. The moodboard's header puts the back button and
  the focus's title above the tabs, so `browser.back` now comes first and `browser.forward` sits
  beside `+` under the recipe. Amend section 7 to match, or move the header behind a theme
  option. **Phase 4**, with the first contract revision.
- **The panel does not react to `UiScale`.** `PANEL_WIDTH`, `CARD_WIDTH` and `CHROME_HEIGHT` are
  logical pixels at scale 1, and `dock::choose` never reads `UiScale`, so a scaled UI gets a panel
  that is the right number of pixels but the wrong number of cards. The contract already lists
  `Changed<UiScale>` as a dock input. **Phase 4**.
- **`CHROME_HEIGHT` is a measured constant, not a measurement.** The card grid's row count is
  derived from a fixed 190 px allowance for the search field, chips, bookmarks and footer. A theme
  that changes those font sizes gets a row too many or too few; the grid scrolls either way, so
  nothing breaks, but the last row can end up half visible. Deriving rows from the grid's own
  `ComputedNode` after the first layout would be exact. **Phase 4**.
- **The arrow sweeps from its centre, not its left edge.** `animate_arrow` writes only
  `UiTransform::scale`, because `UiHarness::settle` watches translation and a left-anchored sweep
  never settles. A `Node::width` animation excluded from the settle fingerprint, or a settle that
  ignores nodes marked as decorative, would let the arrow fill the way the moodboard's does.
  **Phase 4**, with the motion pass.
- **The dev feature is still empty.** `slotted-browser`'s `dev` feature is declared but the
  exclusion highlighter, the id tooltips and copy-recipe-id are not implemented (notes B, section
  10). **Phase 6**.
- **`ScreenHandler::clickable_areas` has no caller.** No Phase 3 handler returns one and nothing
  reads them; the furnace arrow they exist for is a Phase 6 screen. **Phase 6**.

## From Phase 4

- **`a_slot_click_round_trip_is_cheap` fails intermittently under a full-workspace test run.** Seen
  twice on 2026-09-05 during `cargo test --workspace`, never in 30-odd standalone runs of the same
  binary, never with `--test-threads=1`, and not reproducible under synthetic CPU load. The two
  failures raised *different* Lua errors from the same 21 000-call loop, which is what a corrupted
  value looks like rather than a script bug: `test/control.lua:4: attempt to index number with
  'slot'` and `slotted/prelude:239: attempt to index string with number` inside `append_result`.
  Each `MluaRuntime` owns its own `Lua`, the counters are per-state `Arc`s and nothing in the
  adapter is shared, so the suspicion is mlua 0.11 or the vendored Luau across parallel states, and
  the memory limit's allocator path is the first thing to rule out. The test is deliberately left
  running rather than `#[ignore]`d, so the next occurrence is visible. **Phase 5**, and worth a
  minimal reproduction to take upstream.
- **The browser's own chrome is not localised.** The item names on cards, in the search index and
  in a recipe page title now resolve through `slotted_ui::Localization`, but "Search items",
  "R recipes / U uses / A bookmark", "indexing…" and a category chip's label are English literals
  in `slotted-browser`. A chip is the harder one: it draws the category's path because
  `RecipeCategory::title_key` invents `category.<ns>.<path>` and nothing carries the
  `RecipeTypeDef::title_key` a mod actually declared into the category. **Phase 5**, with the
  localisation pass.
- **A stack's `ComponentPatch` is not remapped across a reload.** `remap_inventories` remaps
  `ItemStack::id` by name across every `Inventory` and `Carried`, but a patch is keyed by
  `ComponentId`, which the same freeze re-interns; those keys are left as they are. No Phase 4
  example writes a component patch, so nothing fails today. **Phase 5** (packs notes B, item 13).
- **The per-frame script budget is a constant, not configuration.** `route::MAX_SCRIPT_CALLS_PER_FRAME`
  is 512 calls. `PacksConfig` has no field for it and its shape is shared, so making it
  configurable means amending the contract. **Phase 5** (packs notes B, item 10).
- **An untyped payload loses the width of its numbers.** Every def is now read through
  `slotted_model::Value`, whose only numeric variants are `i64` and `f64`, so a widget `params`
  payload that RON parsed as `U8(54)` comes back as `I64(54)`. Nothing reads a payload's numeric
  width, and both sides of a payload comparison go through the same conversion, so this shows only
  if something starts comparing a payload against a freshly parsed `ron::Value`. **Phase 6**.
- **A resource pack cannot override a script that lives at a mod's root.** `read_script` tries
  `scripts/<mod id>/<entry>` through the layering first and falls back to `<mod root>/<entry>`;
  `ModWatch` registers a handle only for the first form, so a root-level script reloads through an
  explicit `ReloadMod` and not the file watcher. **Phase 5** (packs notes B, item 4).
