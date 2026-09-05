# Follow-ups

Decisions deferred during phases. Each entry names the phase that should pick it up.

## From Phase 1 review

- **Ghost and Filter hints are stored as real count-1 stacks in the backing inventory.** `Inventories::count_of` therefore sees phantoms. Options: a parallel `ghosts` map on `Inventory`, or a `SlotBehaviour`-aware counting API. Decide in Phase 2 when the ECS layer needs to render ghost slots differently anyway. Pinned by `a_ghost_hint_is_visible_to_inventory_counting`.
- **Item conservation is asserted in debug builds only.** If `apply_click` runs as server-side validation, release builds need the check. Plan: make it a runtime `ValidationLevel` on the `Authority` adapter, not a compile-time assert. Decide when the networked authority adapter is designed.
