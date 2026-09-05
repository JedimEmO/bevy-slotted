# Phase 3, package B: what the UI package decided

Companion to `docs/design/phase3-contract.md` section 7. Everything here is either a shared file
package B touched additively, or a place where the implementation had to choose something the
contract leaves open. Package A's notes are `phase3-notes-A.md`.

## 1. `EditableText` works headless, so there is no `MinimalTextField`

The contract allowed a fallback if Bevy 0.19's `bevy_text::EditableText` could not be typed into
without a window. It can. `HeadlessBevyPlugins` carries `TextPlugin` (which adds `apply_text_edits`
and the clipboard) and `UiWidgetsPlugins` (which adds `EditableTextInputPlugin`), and the harness's
`type_text` reaches the field through `FocusedInput<KeyboardInput>` with no window and no GPU. A
spike confirmed a four-character query round-trips through `EditableText::value()` in three frames.
`search_field.rs` therefore wraps the real component and the fallback was never written.

## 2. The panel root is the `Panel` node, not its parent

The contract's `ScreenTree` shape is `Browser[browser, side] > TextField, Panel, Grid, Panel,
RecipeView`. The skeleton spawned `panel_def()` as a child of the browser root, which would have
put an extra `Panel` level between them. `spawn_panel` now applies the def's role, gap and padding
to the browser root itself and spawns the def's five children directly under it, so `panel_def()`
stays the data a later RON or Lua override edits while the tree matches the contract.

## 3. Docking recomputes every frame and writes only on change

The contract lists the four inputs that invalidate a dock (`ScreenLayout`, `WindowResized`,
`Changed<UiScale>`, the exclusion union's hash). `dock_panels` recomputes unconditionally in
`PostUpdate` and compares before writing `Node`, `Visibility`, the `side=` tag and `BrowserLayout`.
The plan is a handful of rectangle operations per open screen, the change detection the contract
wants is preserved exactly where it matters, and `UiHarness::settle()` still converges because a
settled frame writes nothing. Tracking an exclusion hash would have added a resource for no
observable difference.

`free_space` also clamps the occupied area to the window before cutting the strips. `Rect::new`
normalises its corners, so a screen wider than the window would otherwise have produced a wide
strip *outside* the window rather than no strip at all.

`Side` has only `Left` and `Right`, so "the panel does not fit" is the absence of `BrowserLayout`
plus `side=none` on the root, not a third variant.

## 4. The card pool is capped and resizes with the dock

`MAX_COLS = 8` and `MAX_ROWS = 6` cap the pool at 48 cards however tall the strip is. Without a cap
a 1600x900 window gives eleven columns of fifteen rows, and a screen-tree snapshot of the panel
becomes 165 unreadable lines. The pool resizes when the dock hands the grid a different `cols` or
`rows`, which the contract's "fixed pool" allows for and a resizable window needs.

Cards past the end of the page are `Visibility::Hidden` as the contract says, so they keep their
layout space and the panel does not reflow as the player types. An entry with no stack (a tag, an
info page) draws a centred `#` rather than an empty square, which otherwise read as an empty slot.

## 5. The arrow animates a `UiTransform` scale, written directly

The contract asks for "an arrow child with `Tween` progress". A repeating `slotted_theme::Tween`
would hold `ActiveMotions` above zero forever and `UiHarness::settle()` would never return, and
animating a `Node` width would move the layout every frame, which `settle`'s rect fingerprint also
watches. `animate_arrow` therefore writes `UiTransform::scale` from `Time<Virtual>` each frame:
scale is not part of a node's own rect or translation, so the panel stays settleable while the
arrow still sweeps once per `durations.slow`. `Motion::reduced` pins it full.

## 6. Roles added to `assets/themes/glass.theme.ron`

`browser.panel`, `browser.card`, `browser.card.hover`, `browser.badge`, `browser.chip`,
`browser.chip.active`, `browser.search`, `browser.search.focus`, `browser.tab`,
`browser.tab.active`, `browser.bookmark`, `browser.slot`, `browser.slot.missing`,
`browser.recipe.arrow`, `browser.recipe.arrow.progress`, `browser.button.disabled`. The panel is a
second sheet of the same glass, one step darker than a screen so it reads as beside rather than on
top of it. The theme's completeness and dangling-`$ref` tests still pass.

The contract's role list named `browser.chip.selected` and `browser.recipe.missing`; the skeleton
had already fixed the constants as `browser.chip.active` and `browser.slot.missing`, and section 7
of the contract itself says `Themed("browser.slot.missing")`, so the skeleton's names won.

## 7. Additive changes to shared files

- `ui/mod.rs`: `ShowsIngredient`, `ingredient_stack`, `ingredient_key`, six more `roles`
  constants, and the system and observer registration in `register`.
- `slotted-ui/src/semantic.rs`: **unchanged**. The skeleton had already added every role the
  contract lists (`Browser`, `TextField`, `Chip`, `Card`, `RecipeView`, `RecipeSlot`, `Tab`,
  `Bookmark`) with their AccessKit mappings.
- `crates/slotted-browser/Cargo.toml`: `slotted-test`, `slotted-testutils` and `insta` as
  dev-dependencies. This is a dev-dependency cycle, since `slotted-test` depends on this crate;
  Cargo allows it because dev-dependencies never enter the library's own build graph, and it is
  what lets `tests/panel.rs` drive the panel through the public harness rather than through
  private systems.
- `plugin.rs`, `events.rs`, `lib.rs`: unchanged.

## 8. Harness surface

`slotted-test/src/browser.rs` adds `UiHarness::browser() -> Browser<'_>` with `wait_for_index`,
`visible_entries`, `visible_cards`, `search`, `search_has_focus`, `focus_search`, `blur_search`,
`open_recipes`, `open_uses`, `bookmark`, `back`, `close`, `open_page`, `transfer`,
`transfer_enabled`, `is_attached`, `layout`, `dock_side` and `dock_tag`.

Two additions beyond the contract's list. `visible_cards` returns the `entry=` tags of the cards
actually bound right now, which is the page a player sees, as against `visible_entries`, which is
the whole result set. `blur_search` exists because `search` leaves the field focused, exactly as
typing does on screen, and the hotkeys are gated on keyboard focus: a test that means to press R
has to stop typing first, and making `search` blur on its own would have hidden the gate the
contract asks for.

`UiHarness::rebaseline()` retakes the conservation census, for the tests that cheat-give.

## 9. Chest example

`examples/chest/src/lib.rs` registers `DefaultScreenHandler` for `demo:chest` in
`BrowserPhase::ScreenHandlers`, which is the whole of "give this screen an overlay". No
`TransferHandler` goes with it, because a chest has no crafting grid, so the `+` button stays
disabled, which is what the contract expects the example to show.

`--cheat` opens the chest as `Actor::CREATIVE` through a `CheatMode` resource, so the browser's
Ctrl+click give is permitted; survival stays the default so `assert_conserved` keeps its teeth.
`--recipe <ns:item>` opens a recipe page before `--shot` fires, which is how
`shots/chest-recipe.png` is captured. `open_chest` gained a `cheat: bool` parameter.

`the_screen_tree_matches_the_ron_file` now drops the `slotted:browser` root before snapshotting.
The browser panel is a screen root of its own with its own snapshot in
`slotted-browser/tests/panel.rs`, and that test is about what the RON file spawns.

## 10. Left for later

- `ScreenHandler::clickable_areas` is not wired to anything: no Phase 3 handler returns one, and
  the furnace arrow it exists for is a Phase 6 screen.
- The dev feature's exclusion highlighter, id tooltips and copy-recipe-id are not implemented.
- Cards request a `Compact` tooltip on hover; the `Expanded` tier on shift is Phase 6.
- The "used in" list shows one card per recipe that consumes the focus, keyed on that recipe's
  output. A recipe with no output slot contributes nothing.

## 11. What the integration review changed

The Phase 3 review (adversarial pass plus a visual pass against `docs/moodboard.html`) changed
these decisions. Everything else in this file still holds.

- **The panel is 352 px wide, not a full strip.** `dock::choose` now hugs the outer window edge
  with a 12 px margin, takes `PANEL_WIDTH` where the strip allows it, insets the rectangle
  vertically so the panel can never run off the top or bottom of the window, and writes
  `Node::max_height` rather than `Node::height` so the panel wraps its content. `CARD_GAP` is 8,
  `PANEL_PADDING` 14 and `PANEL_GAP` 10, straight off the moodboard.
- **A card is a 75x94 tile, not a 44 px square.** Icon box, display name (11 px, centred, two
  lines) and the full `@mod` namespace (9 px), with a 2 px rarity strip under the top edge
  themed `browser.rarity.<rarity>`. The shared `RarityRing` child is despawned on cards, since a
  ring around a name-bearing tile reads as a border rather than as rarity.
- **A sixth widget, `slotted:status_line`.** The moodboard's `.bfoot`: the result count on the
  left and the hotkey hint on the right. It is also the loading affordance, because
  `rebind_cards` has nothing to bind while `IndexState::Building` and an empty grid otherwise
  reads as "no items". Neither node carries a `SemanticRole`, so the contract's tree is unchanged.
- **The card grid and the recipe view take turns.** `toggle_browse_regions` gives the grid and the
  chip row `Display::None` while a page is open, and the recipe view gets it while none is.
  `Visibility::Hidden` alone still occupies the flex column, which pushed the footer off the
  bottom of a short window. Tests that browse to a second item now close the first page first,
  because that is what a player has to do.
- **The recipe view grew a header.** A back pill and the focus's name above the tabs, the body
  centred with `align_self`, `+` and forward as pills below it, and a "Used in:" line. This moves
  `browser.back` to the front of the view's tree; see `docs/FOLLOWUPS.md`.
- **Arrow glyphs are ASCII.** `<` and `>`, not `←` and `→`: the bundled font has no arrows and
  drew a missing-glyph box. The same goes for the footer's separators.
- **Centred text needs `LineBreak::WordBoundary`.** `Justify::Center` with `LineBreak::NoWrap`
  leaves the text block content-sized and anchored left, so the mod badge and the tag glyph both
  hugged the card's left edge until the line break mode changed.
