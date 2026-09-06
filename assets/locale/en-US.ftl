# Base pack strings. A mod's own locale/<lang>.ftl layers over this one.
#
# The machine example's screen (examples/machine/screens/furnace.screen.ron)
# is host content rather than a mod's, so its keys live here. Fluent ids use
# `-`, and `normalise_id` is what lets a screen file write `machine.title`.

machine-title = Alloy Furnace
machine-tab-redstone = Redstone control
machine-tab-sides = Side configuration
machine-redstone-ignore = Ignore redstone
machine-redstone-low = Run without a signal
machine-redstone-high = Run with a signal
machine-face-none = Face: closed
machine-face-input = Face: input
machine-face-output = Face: output

# The item browser's own chrome. `slotted-browser` resolves each of these
# through `slotted_ui::Localization` and falls back to the same English when
# no catalogue is loaded, so a game with no locale files is unchanged.
# Category chips and recipe tabs are not here: they resolve the category's own
# `RecipeTypeDef::title_key`, or `category-<ns>-<path>` when it declares none.

browser-search-placeholder = Search items
browser-search-label = Search
browser-status-indexing = indexing…
browser-status-item = item
browser-status-items = items
browser-status-hints = R recipes / U uses / A bookmark
browser-uses-title = Used in:
browser-uses-empty = Used in: nothing yet
browser-button-transfer = +
browser-button-back = <
browser-button-forward = >

# Action rail buttons (the widget falls back to these English labels).
slotted-rail-sort = Sort
slotted-rail-quick_stack = Quick stack
slotted-rail-deposit_all = Deposit all
slotted-rail-loot_all = Loot all
