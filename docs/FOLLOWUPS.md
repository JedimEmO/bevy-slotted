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
