# Gaps pass, package B: UI, theme and browser

What the UI half of the gaps pass changed, and the decisions inside it worth
disagreeing with later. The entries it closes are struck through in
[FOLLOWUPS.md](../FOLLOWUPS.md).

## 1. Sizes are theme tokens, and a UI unit is not a pixel

`Tokens::sizes` carries `slot_size`, `slot_gap`, `panel_width`, `card_width`,
`card_height` and `chrome_height`. Every field is optional and defaults to the
constant it replaced, so the three shipped themes render exactly as they did
and a theme file that names no `sizes` still parses. The constants stay where
they were, documented as those defaults: code that has no theme in reach (the
carried item spawned before any asset loads, a recipe layout) still needs a
number.

`slot_gap` is `Option<f32>` rather than a plain float, read through
`Tokens::slot_gap()`. The gap used to be `spacing.sm`, and a theme that moves
its spacing scale should keep moving the gap with it unless it says otherwise.
A serde default cannot see a sibling field, so the option is the fallback.

The part worth stating plainly, because getting it backwards is a real bug and
the follow-up entry asked for the backwards version: **a widget must not
multiply a token by `UiScale`.** Bevy already multiplies every `Node` pixel by
the render target's scale factor and by `UiScale`. A `Node` is written in UI
units, and `UiScale` is how many of the window's logical pixels one unit
covers. Multiplying in the widget scales twice.

What does need converting is everything Bevy measured in another space: a
window's size, a pointer's position, the free strip the browser docks into.
`slotted_ui::UiUnits` is that conversion, and three places now use it:

| Place | Was | Is |
|---|---|---|
| `dock::window_rect` | window logical px, compared against tokens | divided by `UiScale` |
| `layers::update_carried_layer` | pointer px written straight into `Node` | divided, and sized from `sizes.slot_size` |
| `tooltip::place_tooltips` | window size and pointer in logical px, host rect in UI units | all three in UI units |

The tooltip is the one that was already half right: `host_rect` divided by
`ComputedNode::inverse_scale_factor`, which is UI units, and then clamped that
against a window measured in logical pixels. It only showed at `UiScale != 1`,
which nothing tested.

`dock::choose` takes a `DockMetrics` rather than reading the constants, which
is also what lets a test ask for a dock decision without a world.

Two numbers stayed constants on purpose. `slotted-browser`'s recipe layouts
(`category.rs`) place their slots from `SLOT_SIZE` and their own `SLOT_GAP`,
because `RecipeCategory::size` and `RecipeCategory::layout` are a public trait
a mod implements: threading sizes through them changes that contract for every
implementor. The recipe view spawns its slots at the same constant, so the two
cannot drift; a theme with a 64-unit slot gets 64-unit slots on a screen and
44-unit slots inside a recipe page. That is visible, and it is the next thing
to fix here.

## 2. The arrow, and a node the harness does not watch

`animate_arrow` scaled about the fill's centre because a left-anchored sweep
moves the node, and `UiHarness::settle` waits for every node's rect to hold
still. The follow-up offered two ways out; this took the second.
`slotted_ui::Decorative` marks a node whose rect is animation rather than
layout, and `settle` skips it *and its descendants* when it fingerprints the
tree. The arrow's fill is the only node that carries it today.

The risk is obvious and worth writing down: a `Decorative` node is invisible to
every test that waits for layout. Put it on something a test measures and the
test will read a rect that was still moving. It belongs on decoration nothing
asserts against.

## 3. Keyboard shortcuts and the focused field

`TextEntryFocused` is a resource, written each frame by
`track_text_entry_focus` from `InputFocus`, and `directional_nav_keys` and
`hotbar_swap_keys` return early while it is set. A node counts as text entry if
it is a `SemanticRole::TextField` or carries Bevy's `EditableText`, so a game's
own field is covered either way.

It is a resource and not a `SystemParam` that reads `InputFocus` directly
because `AutoDirectionalNavigator` already takes `ResMut<InputFocus>`, and Bevy
refuses a system holding both (B0002). The cost is that the two systems act on
last frame's focus; the tracking system is ordered before them, so the window
is one frame at the start of a focus change, which is not long enough to type
into.

## 4. One virtual grid

`VirtualGridSource` gained `pooled`, `spawn_cell`, `rebind` and `len_in`, all
defaulted, so a source that wants the old respawn-per-window behaviour writes
nothing new. A pooled source keeps `cols * rows` cells with stable entities,
rebinds them as the window moves, and hides the ones past the end. That is
exactly what the browser's card grid did by hand, so the card grid now
registers a `CardSource` and `slotted-ui` owns the pool sizing, the cell
lifetime, the window arithmetic and the rebind loop.

Stable entities are not a nicety here: the screen-tree snapshots list every
pooled card, hidden ones included, so a port that respawned cells would have
rewritten every accepted snapshot.

`Card(slot)` still exists beside `PooledCell(slot)`. The browser's own code and
its tests query `Card`, and collapsing the two is a rename across both crates
for no behaviour.

## 5. What a Lua test's click aims at

`examples/machine`'s `sorter/sort.lua` went red in this pass, and the cause was
not in this package: real fonts arrived under `assets/fonts/`, every label
re-measured a frame or two after the screen opened, and the injected button
moved out from under a pointer that had already been aimed. The gesture was
delivered where the button used to be.

`LuaTestDriver::act` now settles before it resolves a locator. That is the
right default -- a test that says "click this button" means the button, not the
pixel -- but it is a workaround for a sharper problem: a pointer gesture is
delivered at a position while an assertion is written against a node, and any
asynchronous relayout can separate the two. Re-reading the rect between the
pointer's move and its press would close the remaining window. The Phase 6
entry about recordings storing a locator beside each pointer position is the
same problem seen from the replay side.
