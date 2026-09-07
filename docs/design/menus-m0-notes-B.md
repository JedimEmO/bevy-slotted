# Menus M0 package B notes: stack, presentation, layout

What package B (the screen stack, presentation effects, the layout model and HUD visibility)
built for the M0 contract (`menus-m0-contract.md` section 3), the decisions the contract left
open, the tests, and what is deferred. Nothing here changes a signature the contract fixes;
every addition is on top.

## 1. What was built

### `slotted-ui/src/stack.rs`

- **The four commands.** `PushScreen` resolves the def through `Screens` *once*, pushes the entry
  with the resolved kind and presentation, and only then calls `SpawnScreen::apply` with the
  flattened def. The order matters: `ScreenSpawned` fires inside `SpawnScreen`, and package A's
  `focus_on_spawn` asks the stack whether the new screen is on top, so the entry has to exist
  first. `PopScreen` removes the top non-overlay entry, despawns its scrim, triggers
  `ScreenClosed` and despawns the root, then applies `slotted_ecs::CloseMenu` directly when the
  entry has a menu, so the carried stack lands in `Dropped` exactly as the chest's Escape handler
  did. `PopTo` pops entries of any kind (overlays included) until `kind` is `top_any()`, and is a
  no-op without a message when the kind is not open. `ClearScreens` pops everything from the top.
- **`apply_presentation(world)`** (new, public) is the one place z, scrims and visibility are
  written. It is idempotent and runs after every change: entry `i` at `zbands::SCREEN + 2i`,
  its scrim at `2i - 1`, every entry below the topmost `page` hidden. A scrim is a top-level
  full-window node with `Pickable::default()` (blocks lower), `Themed(roles::SCRIM)` and
  `Scrim { for_root }`, and follows its entry's visibility so a modal hidden under a later page
  does not leave a dark sheet behind.
- **`on_screen_closed`** removes the entry, despawns its scrim, writes `StackChanged`, and queues a
  command that re-applies presentation and restores focus. `PopScreen` removes the entry before
  triggering `ScreenClosed`, so the observer never double-handles a pop.
- **Focus.** `record_stack_focus` writes `InputFocus` into the owning entry every frame.
  `enforce_focus_scope` walks `ChildOf` to the screen root; a focused node inside a stacked root
  that is not `top()` is sent to the top's recorded focus, else its `ScreenRoot::initial_focus`,
  else focus is cleared. Pop restores the same way and clears a focus that died with the popped
  screen when nothing is left. Roots outside the stack are never corrected.
- **`pop_on_back`** pops once per frame on any unclaimed `Back` when `top()` exists and its policy
  is `pop`. `top()` skips overlays, so `Back` under a toast pops the page beneath it.
- **Transitions: `PushTransition(Transition)`** (new component) goes on the root at push;
  `start_push_transitions` (new system, after `SlottedThemeSet::Apply`) turns it into tweens on
  the first frame the theme has painted. See 2.2 for why it runs there.

### `slotted-ui/src/def.rs`

- **`nine_anchor_node(anchor, offset, size: Option<Vec2>) -> Node`** and
  **`nine_anchor_transform(anchor, scale) -> UiTransform`** (new, public). `HudAnchor::node` and
  `HudAnchor::transform` are now one-liners over them, and `layout_node`'s `place` uses them with
  `size: None`. The `Option` is the contract's `size` argument made honest: the HUD knows the
  window, a `place`d panel does not know its parent. See 2.1.

### `slotted-ui/src/widgets.rs`

- `layout_node` maps `overflow` (`scroll` = `Overflow::scroll_y()`) and `place` (absolute
  position, the nine-anchor insets and margin). `spawn_panel` inserts `ScrollPosition::default()`
  for `overflow: scroll` and the nine-anchor `UiTransform` for `place`.

### `slotted-ui/src/hud.rs`

- `hud_screen_visibility` takes `Res<ScreenStack>`: hidden while any non-overlay entry is on the
  stack, or while any `ScreenRoot` exists that the stack does not hold.

### `slotted-theme`

- `MotionPreset::Slide`, tier `normal`. Glass 180 ms `Standard`, paper 160 ms `EaseInOut` (a page
  turn, no spring), neon 120 ms `Snap`.

## 2. Decisions the contract left open

### 2.1 `place` without a parent size

The HUD centres an axis with `left: px(window / 2 + offset)`. A `place`d panel has no window to
read at spawn, and `Val` cannot add a percent to a pixel offset. The helper therefore uses
`left: 50%` plus `margin.left: px(offset)` when `size` is `None`; Taffy adds an absolute child's
margin to its inset, and the `-50%` self-translation the HUD already used centres the content.
`place: center` and `place: top` are asserted through real layout in `tests/layout.rs`. Insets are
measured inside the parent's 1 px panel border, which the tests account for.

### 2.2 What "the root's alpha" means

Bevy UI has no opacity group: a screen root has no `BackgroundColor`, and tweening one on it
would fade nothing. The fade is the root panel's own `BackgroundColor` alpha, from zero to
whatever the theme painted (a glass panel ends at its authored translucency, not at 1.0). That
value only exists after `SlottedThemeSet::Apply` has run on the new tree, so
`start_push_transitions` runs after that set and writes the start value (alpha 0, the slide's
start translation) directly, so the first drawn frame is at the start of the motion rather than
flashing at rest. Text and slots inside the panel do not fade; a real opacity group is a Bevy
feature we do not have.

The slide is a `Translate` tween on the screen root, so the whole tree moves, from `spacing.xl`
below (`slide_up`) or to the right (`slide_left`) to zero. A root and a panel are two entities,
so a slide is two tweens and there is no combined target. Under `Motion.reduced` only the fade is
started and it completes on its first tick; `reduced` already zeroes every duration, so the
"fade of `durations.fast`" the contract names is the same thing.

### 2.3 Pop restores focus, and so does the observer

The contract puts focus restore on pop. A direct `close_screen` on a stacked root goes through
`on_screen_closed`, which cannot take `&mut World`; it queues a command that re-applies
presentation and restores focus, so both paths end in the same state one command later.

### 2.4 `pop_to` counts overlays

"Pops until `kind` is top" is read as `top_any()`: an overlay sitting above the target is popped
too. Reading it as `top()` would leave the overlay floating over a screen that was never under
it. One `StackChanged` is written for the whole `pop_to`, not one per entry.

## 3. Tests added

`crates/slotted-ui/tests/stack.rs` (18) and `crates/slotted-ui/tests/layout.rs` (7). Both build
the app with `UiHarness` and drive the stack through the `slotted_ui` commands; `Back` is written
as a `UiActionEvent`, and the claim is set by a test-owned `Input` system, so nothing depends on
package A's emitter.

- push/pop order, `top()`, `is_open`, `StackChanged` sequence, silent pop on empty;
- pop closes the menu, the carried stack is in `Dropped`, and a census of inventories, carried
  stacks and `Dropped` is unchanged;
- page hides the entry below and pop reveals it, no scrim, z bands;
- modal keeps the entry below visible, one full-window scrim between them in z, blocking the
  pointer, gone with the entry;
- overlay leaves focus alone, `top()` skips it, `Back` pops the page beneath and leaves it;
- `back: ignore`, a claim, and two `Back`s in one frame popping once;
- `Back` leaves a directly spawned screen alone;
- `enforce_focus_scope` bounces focus to the top's recorded focus; focus outside the stack is
  never corrected; pop restores focus; a pop with nothing left clears a dead focus;
- `close_screen` on a stacked root removes the entry, its scrim, and reveals the one below;
- `pop_to` an absent kind is a no-op and writes nothing; `pop_to` through an overlay;
- `clear_screens`; HUD hides for page and modal, not for an overlay, and for a direct spawn;
- fade tween on the panel that settles in under sixty frames, slide tween on the root from
  `spacing.xl`, reduced motion collapses a slide, `none` starts nothing;
- `layout_node` maps every field; `Length` parses each spelling, `None`, and rejects `"12px"`
  naming the accepted forms; `nine_anchor_node` for both size modes; `place: bottom_right` in
  the corner headless; `place: center` and `place: top` through the percent-and-margin path;
  `center: true` still centres and `fill` fills; `overflow: scroll` carries `ScrollPosition`,
  keeps its size and clips its tail.

`cargo test -p slotted-ui` is green except `screens.rs` (two) and, in the workspace, the chest
example's tree snapshot: all three see the first slot themed `slot.focus` / `focused: true`
because package A's `focus_on_spawn` now focuses it. That is A's to settle (or a snapshot
review), not a stack change. `tests/actions.rs` did not compile at the time of the run (A's
file, mid-edit).

## 4. Not finished, deferred

- **Pop transitions** and **wheel scrolling** of an `overflow: scroll` panel: deferred by the
  contract to follow-ups.
- **Hot reload drops a stacked screen out of the stack.** `respawn_screens` in `invalidate.rs`
  closes the root (the observer removes the entry) and spawns the new one through
  `spawn_screen`, so after a `*.screen.ron` edit or a mod reload the screen is open but no longer
  a stack entry, and `Back` stops closing it. The fix is for `respawn_screens` to re-push at the
  same index when the closed root was stacked; `invalidate.rs` is outside B's files. Pinned by
  nothing yet; the invalidation tests spawn directly.
- **Flex children of a scroll panel shrink** unless they carry `min_height`; that is Taffy's
  default and the layout test documents it. A list widget that sets `flex_shrink: 0` on rows
  belongs with M1's scrolling.
- **No opacity group**: the fade covers the root panel only (2.2).
