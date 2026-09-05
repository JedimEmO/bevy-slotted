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
