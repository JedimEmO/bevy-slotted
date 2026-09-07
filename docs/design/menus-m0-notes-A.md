# Menus M0 notes: package A (input actions, input mode, focus ring, nav graph)

What A built, what the contract left to A and how A decided it, what changed
outside A's own files, and what B and C need to know. Contract:
`docs/design/menus-m0-contract.md`, sections 1.1, 2.1 to 2.6.

## What is built

### `slotted-ui/src/actions.rs` (2.1, 2.2)

- `emit_ui_actions` clears `UiActionClaims`, then gathers one `Evidence` per
  `UiAction` from the keyboard, every `Gamepad` entity's buttons and the left
  stick, and writes at most one `UiActionEvent` per action per frame. A fresh
  press (`just_pressed` on any source) is `repeat: false` and names the
  device that made it, keyboard first when both did. Held `Up/Down/Left/Right`
  repeat after `repeat_delay` (default `durations.hover_delay`) then every
  `repeat_every` (default `durations.fast`), on `Time<Virtual>`; nothing else
  repeats. Keyboard evidence for anything but `Back` is dropped while
  `TextEntryFocused` is set; gamepad evidence never is.
- Stick: `stick_directions` engages a direction past `stick_deadzone` and
  keeps it until the axis drops under half of it; `ActionRepeat::stick_held`
  is the hysteresis state. A direction that engages this frame counts as a
  fresh press, so the stick and the d-pad produce the same event.
- `track_input_mode` reads `CursorMoved` (what the window writes) *and* the
  mouse's `PointerInput` moves and presses (what a headless harness or a
  replay writes; winit writes both, so a real player is counted once). A
  `CursorMoved` counts when it is more than `POINTER_MOVE_THRESHOLD` (2
  logical px) from the last sample. The stick switches the mode once when it
  crosses the deadzone, not every frame it stays there.
- New resource `InputModeTracker` (last cursor sample, stick-over flag),
  registered in `actions::build`.

### `slotted-ui/src/focus_ring.rs` (2.3)

- `spawn_focus_ring` spawns the ring hidden at startup when
  `SlottedUiConfig::spawn_layers`. `update_focus_ring` in `Render` sets the
  target (`InputFocus` when that entity is `Focusable`), visibility (target
  and `InputMode != Pointer`), and the rect (the target's computed rect in
  logical px through `UiUnits`, grown by `spacing.xs` on every side).
- A change of target slides with a `Translate` tween from the old rect to the
  new one and a `Size` tween between the two sizes, both `durations.fast` with
  `Easing::Standard`. It snaps under `Motion::reduced`, on the first placement,
  when the ring was hidden, and when the new target sits under a different
  screen root. The same target moving through layout is followed without a
  tween.
- `Focusable` is on `spawn_button`, `spawn_slot`, the icon button, the side
  tab header, virtual grid cells (pooled and non-pooled paths) and the
  browser's search field and cards.

### `slotted-ui/src/nav.rs` (2.4)

- `directional_nav_actions` reads directional `UiActionEvent`s. From the
  focused entity it walks up through `ChildOf` to the screen root looking for a
  `NavLinks` with a link in that direction, resolves the id by `TestId` under
  the same root, and focuses that node's first `Focusable` descendant (itself
  if focusable). An unresolved id is logged once per screen spawn (component
  `NavLinkWarned` on the root) and falls through to `AutoDirectionalNavigator`,
  as does a node with no link. Focus outside any screen root goes straight to
  the navigator, which is what keeps `keyboard_focus_and_arrow_navigation_work`
  green.
- `focus_on_spawn` (observer on `ScreenSpawned`) does nothing for an
  `overlay`, or for a screen the stack holds below its top. Otherwise it
  resolves `initial_focus` (first focusable descendant of the named node, or
  the first `Focusable` in tree order when the id is missing or unnamed),
  writes `ScreenRoot::initial_focus`, and sets `InputFocus` when the resource
  exists. The record is written even without `InputFocus`, so the stack's
  restore-on-pop has something to fall back to.
- Two helpers are public for B and C: `nav::screen_root_of` and
  `nav::first_focusable`.

### `slotted-theme`, `assets/themes` (2.3)

`roles::FOCUS_RING` (`focus.ring`) and `roles::SCRIM` (`scrim`); `ALL` is 38.
Glass: `Solid` with `$accent` border and `radii.md` (12) and no fill. Paper:
`Dashed` ink stroke, width 2. Neon: `Solid` magenta border, `elevation: low`.
Scrims as the contract lists them. The ring node sets its own 2 px border
width (`RING_BORDER`); `Solid` has no width field, so the theme only colours it.

### Claims (2.5)

- `slotted-browser/src/ui/hotkeys.rs`: `browser_hotkeys` now runs after
  `UiActionEmit` (the whole `BrowserSet::Input` chain does, in `ui/mod.rs`).
  A `Back` event closes an open recipe view and is claimed; with nothing open
  it is left unclaimed so the stack can pop the screen. The raw `close` key is
  skipped whenever it is one of `Back`'s bound keys (Escape by default), which
  is the contract's "when it is Escape" generalised to a rebound `Back`.
- `slotted-ui/src/hud_editor.rs`: `cancel_hud_drag` reads `Back` instead of
  `KeyCode::Escape`, claims it only while a drag is in progress, and the chain
  it lives in is ordered after `UiActionEmit` in `plugin.rs`.
- No `KeyCode::Escape` literal remains outside `UiBindings::default()`,
  `hud_editor.rs`'s edit toggle, the browser's `KeyMappings`, and test or
  harness key tables (contract 5).

## Decisions the contract left to A

- **Two entities for the ring.** A node holds one `Tween`, and a move needs a
  `Translate` and a `Size` at once. The `FocusRing` entity is the absolute
  wrapper with `FocusRingState`, `GlobalZIndex(zbands::FOCUS)`,
  `Pickable::IGNORE` and the `Visibility` switch, and takes the `Translate`;
  its one child, `FocusRingFrame`, carries `Themed(roles::FOCUS_RING)`, the
  border and the `Size` tween. Tests and the hint bar read `FocusRingState`
  on the `FocusRing` entity as the contract says; a query for `Themed` on
  that entity finds nothing.
- **Same-frame device tie-break** in `track_input_mode`: gamepad beats
  keyboard beats pointer. A player who touches a key or a button wants the
  ring; a mouse that jogs at the same time must not take it away.
- **Repeat cadence** is anchored to the schedule (`next += every`) rather than
  to the frame the repeat was noticed on, so a slow frame does not drift the
  rhythm; a frame that arrives more than one interval late fires once, not
  once per missed interval.
- **Back also blurs the search field** and is claimed then. The contract only
  names the recipe view, but the existing "Esc blurs, does not close" rule had
  to move onto the action too, and leaving that press unclaimed would pop the
  screen out from under a player who only wanted to leave the field.
- **Ring duration** is the `fast` tier (`Motion::duration(Hover, durations)`),
  not a theme's per-preset override of `Hover`; the contract says
  `durations.fast`.
- **Initial focus while `InputFocus` is on the window.** Bevy's
  `InputFocusPlugin` parks focus on the primary window at startup; an overlay
  leaves it there, which is what `an_overlay_never_takes_focus` asserts.

## Changes to shared files

- `slotted-ui/src/plugin.rs`: one `.after(crate::actions::UiActionEmit)` on
  the HUD editor chain. No other registration moved.
- `slotted-ui/src/lib.rs`: `FocusRingFrame` exported beside `FocusRing`.
- `slotted-ui/tests/screens.rs`: two assertions that assumed no slot had
  focus after spawn now expect `slot.focus` on the first slot, or read the
  second slot. Every screen spawned through `spawn_screen` now focuses its
  first focusable (contract 2.4), which is also why three tree snapshots grew
  a `focused: true` / `(focused)`: `slotted-test/tests/snapshots/chest_screen__*`
  (both) and `examples/{chest,machine}/tests/snapshots/ui__*`. Accepted as is;
  C's migration to the stack does not change them again.
- `slotted-browser/tests/panel.rs`: two tests added (below).

## Deviations from the skeleton's signatures

The names and registrations are unchanged. Parameter lists that changed:

- `track_input_mode` gains `MessageReader<PointerInput>` and
  `ResMut<InputModeTracker>`.
- `emit_ui_actions` gains `ThemeTokens` (for the two default durations).
- `update_focus_ring` takes `Motion`, `ThemeTokens`, `ChildOf`/`ScreenRoot`
  queries, a frame query and `Commands`; the ring query adds
  `FocusRingRect`, `Children` and `Has<Tween>`.
- `directional_nav_actions` gains `Query<&mut NavLinkWarned>` and `Commands`.
- `focus_on_spawn` takes `&mut ScreenRoot` (to record the choice), gains
  `Query<&ChildOf>`, drops `Commands`.

## Tests

Twenty added, all headless through `slotted-test`:

- `crates/slotted-ui/tests/actions.rs` (8): one event per action from each
  device with the default bindings; two keys pressed together emit once; a
  held direction repeats after the delay then every interval on virtual time
  and stops on release; `Accept` never repeats; the stick presses past the
  deadzone, holds down to half, releases under it, and `-x` is `Left`; text
  entry suppresses keyboard `Accept` and arrows but not `Back` or the pad;
  `InputMode` follows the last device with an `InputModeChanged` per change
  and ignores a one-pixel jog; setting focus by hand flips nothing.
- `crates/slotted-ui/tests/focus_ring.rs` (10): hidden on the mouse and shown
  on the first key with the rect around the slot grown by `xs`; slides with a
  tween and settles on the new rect; snaps under reduced motion; a
  non-`Focusable` focus clears the target; a `nav.down` on a grid beats the
  navigator from any slot in it while an unlinked grid navigates as before;
  an unresolved `nav.left` falls through; `initial_focus` lands where named
  and on the first focusable otherwise, recorded in `ScreenRoot`; an overlay
  takes no focus; all three themes define the 38 roles including the ring
  and the scrim.
- `crates/slotted-browser/tests/panel.rs` (2): `Back` closes the recipe view
  and is claimed only then; a rebound `close` key keeps working beside `Back`.

Workspace: `cargo test --workspace` green (1040 tests) at the time of writing,
with B's stack and layout work in the same tree; `cargo clippy --workspace
--all-targets -- -D warnings` and `cargo fmt --check` clean;
`cargo check --target wasm32-unknown-unknown -p slotted-ui -p slotted-browser`
clean.

## Not done, and follow-ups

- **The ring's visibility does not fade.** The contract says it toggles
  through the `Fade` preset, but the only alpha the tween machinery drives is
  `BackgroundColor`, and the ring has no fill. Fading the border needs a
  `BorderAlpha` (or a colour) `TweenTarget` in `slotted-theme/src/motion.rs`,
  which is B's file this milestone. Visibility switches in one frame; the
  slide and the size change animate.
- A `Scroll` action is out of scope (contract 1.1 lists none).
- `slotted-test`'s replay cursor writes `GamepadEvent`, which Bevy's
  processing system does not read; the `Gamepad` component only updates from
  `RawGamepadEvent`. The new tests write raw events. C's harness gamepad API
  (4.1) should do the same and fix `cursor.rs` on the way.
