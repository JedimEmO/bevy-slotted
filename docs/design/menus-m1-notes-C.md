# Menus M1 package C notes: text field, containers, scrolling

What package C built for the M1 contract (`menus-m1-contract.md` section 4), the decisions the
contract left open, what departs from it, the tests, and what is deferred. Nothing here changes a
signature the contract fixes; the additions are on top.

## 1. What was built

### `slotted-ui/src/widgets/text_field.rs` (4.1)

- **Three nodes.** The row is the control: `Focusable`, `TabIndex(0)`, `AutoDirectionalNavigation`,
  `SemanticRole::TextField`, `WidgetNode(text_field)`, `TextFieldState`, the optional label as a
  `LocText` child in `control.label`, height `control_height`. The frame is the themed box
  (`text_field`, `.focus`, `.disabled`) and holds the `EditableText` child and the placeholder.
  `TextFieldParts { frame, editable }` on the row and `TextFieldEditable { field }` on the child
  link them; `CommittedText` on the row is what `Back` reverts to.
- **Focus does not edit.** `Accept` (keyboard) or a click on the row hands `InputFocus` to the
  editable child. That is what sets `TextEntryFocused`: package B's `track_text_entry_focus` now
  reads `TextFieldState::editing` so a merely focused row does not swallow keyboard actions (asked
  for by C, done by B; see 2.1). `TextFieldState::editing` is derived every frame from
  `InputFocus == editable`, so a focus that leaves any other way (a click elsewhere, a stack
  restore) is seen too.
- **Leaving.** Enter commits: `CommittedText`, a `SetValue { Text }` when bound, focus back to the
  row. `Back` on the editable reverts to `CommittedText`, returns focus and claims, so
  `pop_on_back` leaves the screen alone. A gamepad `Accept` on the row writes
  `TextEntryRequested { entity }` and claims; on the editable (a pad pressed after a keyboard
  started editing) it commits. Focus leaving without Enter or `Back` commits what was typed.
- **Filter and length.** `editable_filter(TextFilter)` builds Bevy's `EditableTextFilter`
  (`numeric`: digits, `.`, `-`; `integer`: digits, `-`); `max_len` sets `max_characters`. A
  filtered field writes a `SetValue` on every change while editing, so a guard's clamp shows
  live; an `any` field writes on commit only.
- **The store seeds the field** (`sync_text_fields`, `Render`): a `Store` binding's value is
  copied into the editor and `CommittedText` whenever `ValueStore::version` moves and the field is
  not editing. Numbers and bools are formatted with `Display`.
- **Placeholder.** `TextPlaceholder` on the placeholder child; `sync_placeholders` hides it while
  the editable beside it (or above it, for the browser) has text.

### `slotted-ui/src/widgets/scroll.rs` (4.2)

- **Two nodes.** The outer node (`ScrollPanel { viewport }`, `SemanticRole::ScrollView`,
  `WidgetNode(scroll)`) carries the screen's `layout` (size, `place`, `grow`) as a flex row; the
  viewport (`ScrollViewport { panel }`) is `spawn_panel` with `overflow: scroll`, Bevy's
  `ScrollArea`, the children, and the layout's direction, gap and padding. The scrollbar is a
  sibling of the viewport, not a child: a child scrolls away with the content, which is also why
  Bevy's own example lays its scrollbar out beside the area.
- **Scrollbar.** `Scrollbar::new(viewport, Vertical, 16)` on a `scroll.bar` track,
  `ScrollbarThumb` in `scroll.thumb`. The thumb has no `Node` (Bevy positions it after layout),
  so its radius comes from `ScrollbarThumb::border_radius` (`radii.sm`) rather than the theme.
- **Focus follow.** `scroll_focus_into_view` (`Render`) triggers Bevy's `ScrollIntoView` for the
  focused entity when focus changed and the entity sits under a `ScrollArea`.
- **Paging.** `on_scroll_page`, a global `FocusedAction` observer: `PagePrev`/`PageNext` on a
  focused descendant scroll the nearest area by its visible height, clamped, and claim.
- **Direct children get `flex_shrink: 0`**, closing the M0 follow-up "Flex children of a scroll
  panel shrink". The viewport itself has no border (see 2.3).

### `slotted-ui/src/widgets/list.rs` and `virtual_grid.rs` (4.3)

- **The grid grew a shape.** `GridShape { columns, row_height, gap, role }` and
  `spawn_shaped_virtual_grid`; `spawn_virtual_grid` is that with `GridShape::slots`. A list is
  one `fr(1)` column of `control_height_compact` rows over the unchanged `VirtualGridSource`.
- **Rows wrap cells.** `refresh_virtual_grids` asks `list::spawn_row(world, grid, index)` before
  each non-pooled cell; on a list it returns a themed `ListRow { list, index }` (`list.row`,
  `SemanticRole::ListItem`, `Focusable`, `TabIndex(0)`, `AutoDirectionalNavigation`, tag
  `row=<index>`) and the source's node is spawned under it, so a text cell keeps its own role.
  `VirtualCell`, the `cell` tag and `Focusable` go on the row. Any other grid gets `None` and the
  old behaviour.
- **Walking.** `on_list_row_action`: `Up`/`Down` on a row claim and focus the neighbour when it is
  spawned; when it is outside the window the grid scrolls one row and a `ListFocusRequest(index)`
  on the list is resolved by `sync_lists` after the grid respawned. At the first or last row the
  action is not claimed, so directional navigation can leave the list.
- **Selecting.** `Accept` (fresh) or a primary click: `ListState::selected`, a `SetValue { Int }`
  for a store binding or a `SetProperty` for a property one, then `Activate { entity: row }`,
  whose tags carry `row` and `cell`. `sync_lists` mirrors `VirtualGridState::total` into
  `ListState::len`, paints `selected` from the store, and `on_list_property` from
  `PropertyChanged`. `list_row_roles` paints `selected`, `focus`, `hover`.
- **Paging by action.** The grid's `on_virtual_grid_key` (`KeyCode::PageUp/PageDown` through
  `FocusedInput`) is replaced by `page_virtual_grids` (`Input`, after the dispatch): an
  unclaimed `PagePrev`/`PageNext` while focus is on a grid or one of its cells pages the window and
  claims. The Phase 6 test that focuses the grid and presses PageDown still passes through the
  action path, and `grep KeyCode:: widgets/` no longer finds the grid.

### `slotted-ui/src/widgets/tabs.rs` (4.4)

- **Root, bar, pages.** The root (`SemanticRole::Tabs`, `WidgetNode(tabs)`, `TabsState`) is a
  column: the `tabs.bar` with one `TabButton { tabs, index }` per tab (`tab`, `.active`, `.hover`,
  `.focus`; `SemanticRole::Tab`, `Focusable`, `TabIndex(0)`, tag `tab=<id>`, icon then a `LocText`
  label, height `control_height_compact`), then the pages in order, each `TabPage { tabs, index }`.
  The root itself is not focusable; the buttons are.
- **One visible page.** Every page but the active one is `Visibility::Hidden` and carries
  `FocusMask`. The focus ring skips a masked target (a small edit in `focus_ring.rs`), B's
  dispatch skips it, and `mask_focus` (`Navigate`, after `enforce_focus_scope`) moves a focus that
  landed under a mask to the active page's first focusable, else the active tab's button. That
  covers Tab navigation, which does not look at visibility; directional navigation already skips
  hidden nodes.
- **Switching.** `tab_actions` (`Input`, after the dispatch) reads `TabPrev`/`TabNext`: the
  innermost tabs around the focus, else the first tabs on the focused screen (or the top of the
  stack); wraps; claims. `Accept` on a button (`on_tab_button_action`) and a primary click
  (`on_tab_button_click`) switch too. A switch writes `SetValue { Text(id) }` (or `SetProperty`
  with the index), and moves a focus that sat on the old button to the new one, or one that sat in
  the old page to the new page's first focusable. `sync_tabs` paints from the store and paints
  the buttons.

### `slotted-ui/src/widgets/decor.rs` (4.5)

`separator` is a 1 px node across the cross axis in `separator`; `spacer` is `flex_grow: 1` for
`fill`, else its length as `flex_basis`, never shrinking; `image` is an `ImageNode` through
`icon_image` (`IconDef::Image`), so a world without an `AssetServer` still lays out. All three are
`SemanticRole::Decor`, `Decorative`, `Pickable::IGNORE`.

### `slotted-browser/src/ui/search_field.rs`

Kept, not replaced: the browser's field is focus-equals-edit by design (Ctrl+F, typing at once),
writes `SearchChanged` rather than a value, and its tests set `InputFocus` on the `test_id` entity
and type. Making it the Accept-gated control would break that contract. What it now shares is the
placeholder: the child carries `TextPlaceholder` and `slotted_ui::sync_placeholders` hides it, so
`SearchPlaceholder` and half of `search_field_state_role` are gone. See 2.4.

### Shared files

`plugin.rs` registers the four `build` functions; `lib.rs` exports `GridShape`,
`spawn_shaped_virtual_grid`, `ListRow`, `ListFocusRequest`, `ScrollViewport`, `TabButton`,
`TabPage`, `TextFieldParts`, `TextFieldEditable`, `CommittedText`, `TextPlaceholder`;
`focus_ring.rs` gained the `FocusMask` filter. The theme files were not touched: the skeleton's
first-pass materials for `text_field.*`, `scroll.*`, `list.row.*`, `tabs.bar`, `tab.*` and
`separator` read well in all three themes and the role-completeness test stays green.

## 2. Decisions the contract left open

### 2.1 A focused row is not text entry

`track_text_entry_focus` counted any focused `SemanticRole::TextField` as owning the keyboard,
which would have suppressed the keyboard `Accept` that starts editing. The row keeps the role
(the tree and AccessKit want it there); B changed the predicate to read
`TextFieldState::editing`, and the editable child carries `EditableText`, which is the clause that
fires while editing. The row is `SemanticRole::TextField` and the editable child has no semantic
role, so the tree shows one `TextField` node per field.

### 2.2 Enter is a key

While a field edits, the emitter lets only `Back` through, so Enter cannot arrive as an action.
`on_text_field_enter` observes `FocusedInput<KeyboardInput>` on the editable child for the
logical `Key::Enter`, which Bevy's text input deliberately leaves unhandled "to allow for submit
actions". No `KeyCode` is read; the contract's grep stays clean.

### 2.3 The viewport has no border

Bevy's `ScrollIntoView` measures the target from the scroll area's edge, not its content box, so
a 1 px border leaves the first child one pixel scrolled after a focus-follow back to the top. The
viewport is an invisible panel, so it drops the border; the layout's padding still offsets the
first child by that much, which is Bevy's behaviour and is left alone.

### 2.4 The scrollbar is a sibling and `ScrollPanel` grew a field

`ScrollPanel` was a unit marker in the skeleton; it now names the `viewport`, because the entity
the screen's tags land on is the outer row and the one whose `ScrollPosition` moves is inside it.
Nothing outside `scroll.rs` used the marker.

### 2.5 Where the list's row lives

The alternative was to decorate the source's cell in place (insert `Themed(list.row)` on it),
which would repaint a text cell as a solid box and lose its text material. Wrapping costs one
node per row and keeps the source's node exactly as the source wrote it.

### 2.6 Tabs pick their target by focus, then by screen

`TabPrev`/`TabNext` "from anywhere in the screen" is read as: the innermost `tabs` above the
focus wins, then the first `tabs` under the focused screen root, then the first under the top of
the stack when nothing is focused. Two sibling `tabs` on one screen with focus outside both
switch the first one.

### 2.7 Controls seed themselves from the store

The contract puts the store-to-state copy in B's `sync_value_bindings`, which cannot know
`ListState`, `TabsState` or `TextFieldState` without a cross-package edit. Each of the three
carries a `StoreSeen(version)` and copies its own value in its `Render` system when the version
moves. A property binding is mirrored for the list (`SetProperty`/`PropertyChanged`) and written
for tabs (index); the text field is store-only, since a property is an `i32`.

## 3. Deviations from the contract

- `ScrollPanel` carries `viewport: Entity` (2.4); `ScrollViewport` is new.
- The scroll node is two entities; the contract's "`spawn_panel` with `overflow: scroll`" is the
  inner one.
- The virtual grid's key paging became action paging (`page_virtual_grids`), and the grid
  observer `on_virtual_grid_key` is gone.
- The tabs root is not `Focusable`; its buttons are. A container with no action of its own would
  only be a dead tab stop.
- `sync_placeholders` is a `slotted-ui` system the browser relies on, so the browser's placeholder
  no longer needs its own visibility code.

## 4. Tests added

`crates/slotted-ui/tests/text_field.rs` (9) and `containers.rs` (9), both on the headless
harness with a collector plugin that records every `SetValue`, `TextEntryRequested` and
`Activate`, so nothing depends on the store committing. Actions are triggered as `FocusedAction`s
on the entity, as the dispatch does; tabs are driven through `UiActionEvent`.

- Text field: Accept enters editing and sets `TextEntryFocused` (a focused row does not); typing
  changes the state and Enter commits with one `SetValue` and returns focus; `Back` reverts, is
  claimed, and Escape while editing does not pop; the numeric filter drops letters and writes
  live; `max_len` caps; a gamepad Accept writes `TextEntryRequested` and nothing else; the
  placeholder hides once there is text, the label is a `control.label` `LocText`, the row is
  `control_height`; a click edits and a click elsewhere commits; the store seeds and reseeds, a
  disabled field is inert and painted `.disabled`.
- Scroll: the wheel moves `ScrollPosition` both ways and the scrollbar carries the roles; rows
  keep their height and the tail is clipped below the viewport, no scrollbar when asked; focus on
  the ninth row scrolls it into the viewport and back to zero on the first; `PageNext` moves one
  visible height, claims, `PagePrev` back, clamped at the end.
- List: ten rows in a four-row window, `Down` claims and walks, the fifth `Down` scrolls the
  window and focus lands on the row that scrolled in, `Up` walks back and scrolls, `Up` on the
  first row is not claimed; `Accept` claims, selects, writes `Int`, activates the row with its
  `row` tag, paints `.selected`; a click selects; the store paints the selection.
- Tabs: `TabNext` from a button in the first page switches, claims, hides and masks the old page,
  moves focus into the new one; cycles and wraps, `TabPrev` backwards; the bound ids arrive in
  order; the bar paints `.active`; the store paints; focus set on a hidden page's button lands on
  the active page's first focusable and the ring never sat on it; a click and an `Accept` on a
  tab button switch; focus on a tab button follows the switch.
- Decor: the separator is 1 px across the panel in `separator`; the image is its given size with
  an `ImageNode`; a fixed spacer is its length; a fill spacer pushes the last row to the bottom;
  all four are `Decor` and never focusable.

Gates run: `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings` (clean),
`cargo test -p slotted-ui -p slotted-browser -p slotted-test` (green),
`cargo check --target wasm32-unknown-unknown -p slotted-ui`, `RUSTDOCFLAGS="-D warnings" cargo
doc -p slotted-ui -p slotted-browser --all-features`. `cargo test --workspace` had one failure
outside this package: `examples/chest/tests/ui.rs::the_screen_tree_is_theme_independent`, where
the browser docks on the other side under paper; A's typography change (`panel.title` from 15 px
to `$title`) widens the chest panel, which is that test's business, not a container's.

## 5. Not done, deferred

- **A numeric filter is per character.** "One point, a leading minus" cannot be expressed through
  `EditableTextFilter`; `1-2.3.4` types. The store's rule is where the parse belongs. Pinned by
  nothing.
- **Horizontal scroll panels.** The viewport is `scroll_y` only (M0's `Overflow::Scroll`); paging
  handles `x` when a node ever sets it, but nothing spawns one.
- **`ScrollIntoView` and padding.** A padded viewport scrolls the first child to the padding edge
  (2.3).
- **A list inside a scroll panel** pages the panel, not the list: the panel's observer claims
  first. Which of the two should win is a design call for the settings template (M2).
- **The text field ring while editing** sits on the editable child, since it is `Focusable` so the
  dispatch can deliver `Back` to it. A ring on the frame would read better; that needs the ring to
  know about `TextFieldParts`.
- **Tab icons** are sized from the compact control height, since the tokens have no icon size.
