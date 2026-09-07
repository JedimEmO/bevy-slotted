# Menus M0 notes: package C (harness, migration, docs)

What C built on top of A and B, the decisions the contract left open, what
departs from it, the tests, and what is left. Contract:
`docs/design/menus-m0-contract.md`, section 4 and the "Done when" in
section 5. A's and B's notes are `menus-m0-notes-A.md` and
`menus-m0-notes-B.md`; the follow-ups they recorded are now in
`docs/FOLLOWUPS.md` under "Menus M0".

## 1. What is built

### `slotted-test` (4.1)

- **`UiHarness::open(screen) -> Entity`** pushes a menu-less screen through
  the stack, steps a frame and takes the conservation baseline, so
  `assert_conserved` works on a test that opens no menu. **`open_screen`**
  pushes too; `Opened.screen` is a stack entry and the census is untouched.
- **Gamepad.** The builder spawns one `Gamepad` entity
  (`cursor::ensure_gamepad`, which finds the first pad or spawns one) and
  `gamepad_entity()` names it. `gamepad(button)` is a press frame then a
  release frame; `gamepad_hold` and `gamepad_release` are the halves, with a
  `held_buttons()` list like `held_keys()`; `stick(Vec2)` writes both
  left-stick axes and leaves them there. All of it is `RawGamepadEvent`,
  A's finding: Bevy's processing system reads only the raw stream, so the
  `Gamepad` component a consumer queries never saw a hand-written
  `GamepadEvent`. **`cursor.rs`'s `feed` and `replay.rs`'s replay both wrote
  the wrong one and are fixed**; `replay.rs` now goes through `feed`, so
  there is one gamepad path. `feed` spawns a pad rather than dropping the
  event when the world has none.
- `action(UiAction)` presses `UiBindings::first_key` and panics when the
  action has no key. `set_input_mode` writes the resource and an
  `InputModeChanged`, then steps a frame so the ring reacts.
- Readers: `input_mode()`, `stack()` (kinds bottom to top),
  `focus_ring()` (`FocusRingState`, or the default when no ring was
  spawned). `focused()` already existed in `queries.rs`.
- **Lua `slotted.test`:** `gamepad(button)`, `action(name)`, `focused()`.
  `TestOp` gains `Gamepad { button }`, `Action { action }` and `Focused`;
  button names are matched on the `GamepadButton` `Debug` form, case
  insensitively, from a list like `KEYS`; action names are `UiAction`'s
  data-file spelling through `FromStr`. `focused()` answers the focused
  node's `Tags` as a table, `nil` when nothing has focus or the focused
  entity carries no tags (Bevy parks focus on the window at startup). Both
  drivers route them: the harness through the functions above, the live
  driver (`live.rs`) through `cursor::feed` with the press on one frame and
  the release on the next, because a press and a release in one frame never
  show up as `just_pressed`. `docs/guide/api/` and the Luau stubs are
  regenerated.

### `Accept` on the focused node (not in any package's section; see 2.1)

`nav::accept_focused`, in `Input` after `UiActionEmit`: a fresh `Accept` on a
focused slot triggers `SlotClicked { Left, modifiers held }` and claims the
action, from the keyboard or the pad; on a focused `Button` it triggers
`Activate` for a gamepad-sourced press only, and claims it.

### Migration (4.2)

- `examples/showcase/src/chest.rs`: `open_over` and the `E` reopen call
  `push_screen`. `toggle_chest_screen` no longer reads Escape; it forgets
  `ChestBinding::open` when the recorded root is no longer a `ScreenRoot`,
  whichever way the screen went, so `E` can reopen after a pop and the
  chest example's `binding.open.is_none()` assertion still holds.
- `examples/machine/src/main.rs`, `examples/modded/src/lib.rs`, and the
  playground's chest, browser, machine, mods, testing (the rewind) and
  multiplayer scenes push. `scenes::teardown` still closes through
  `close_screen`, and the stack's observer drops the entries.
- `assets/screens/demo_chest.screen.ron` and
  `examples/machine/screens/furnace.screen.ron` declare `presentation` and
  `initial_focus` (`chest_grid`, `input`); the copper chest's `data.lua`
  declares both in the Lua table (`chest_grid`).
- **Hot reload re-pushes in place.** `stack::push_screen_at(world, index,
  def, menu)` (new, public) inserts the entry at `index` before spawning, so
  A's `focus_on_spawn` sees the right "is it on top", then applies
  presentation, restores focus and writes `StackChanged`. `respawn_screens`
  remembers the closed root's stack index and uses it; a root outside the
  stack still goes through `spawn_screen`. A world without a `ScreenStack`
  (the packs crate's tests build a bare one) takes the old path.

### Docs (4.3)

`docs/guide/screens.md` (the seven `ScreenDef` fields, `presentation`,
focus and nav links, the full layout table, the stack API with a push/pop
example, `spawn_screen` as the low-level path, hot reload keeping the
position), new `docs/guide/input.md`, `testing.md` and `README.md` link it;
`hot-reload.md`'s "what survives" row; `PLAN.md` status line and a "Menus"
subsection under the phases; `CHANGELOG.md` a "Menus M0" block at the top
of Unreleased with every public change from all three packages;
`FOLLOWUPS.md` a "Menus M0" heading.

## 2. Decisions the contract left open

### 2.1 Where `Accept` lands

The contract's tests (4.4) say `gamepad(South)` on a slot picks up, and no
section says who turns `Accept` into a click. Bevy's `Button` handles
Enter and Space on the focused entity itself, firing `Activate`; nothing on a
slot listens to `Activate`, and the pad never reaches Bevy's handler at all.
The first cut observed `Activate` on slots, which double-fired on a pointer
click (Bevy fires `Activate` on click too, beside our `Pointer<Release>`
path) and turned every click into a double click. So the slot path reads
the action directly, for both devices, and the button path forwards only
gamepad presses. The cost is in the follow-ups: a keyboard `Accept` rebound
away from Enter and Space reaches slots but not buttons.

### 2.2 The multiplayer scene

Two clients, both on the canvas. Two pages would hide the first, so the
second client is pushed as a `modal` with `scrim: Some(false)`: the entry
below stays visible, no dark sheet, and `Back` pops Bob then Alice. The
`enforce_focus_scope` bounce means a click on Alice's slot does not keep
focus there, which the scene never depended on. Recorded as a follow-up.

### 2.3 Presentation on respawn

`push_screen_at` takes the resolved def's presentation rather than the old
entry's, so an edit that changes `presentation` in the file takes effect on
the same reload as any other edit. No push transition on a respawn: the
screen was already there.

### 2.4 The Lua test needs a `step` past the double-click window

`t.gamepad("South")` twice on one slot inside 250 ms of virtual time is a
double click and collects; the Lua test steps twenty frames between them.
That is the click interpreter doing its job, not a harness quirk, and it is
what a pointer test has to do as well.

### 2.5 `KeyCode::Escape` literals (contract 5)

`grep -rn "KeyCode::Escape" crates examples` now finds: `UiBindings::default()`
(as `K::Escape`), the browser's `KeyMappings` default, the harness's
logical-key tables (`slotted-test/src/actions.rs`, `live.rs`) and the Lua
`KEYS` name list, and tests that press the physical key on purpose (the
browser's rebound-close test, the HUD editor's cancel test, A's text-entry
test, the chest example's Escape tests, and a replay fixture). None reads the
key for a UI decision. The HUD edit toggle turned out not to be an Escape.

## 3. Deviations from the contract

- **`replay.rs` was fixed too**, not only `cursor.rs`: both wrote
  `GamepadEvent`.
- **`accept_focused`** is a new system in `nav.rs`, registered in
  `plugin.rs` beside `directional_nav_actions`. Nothing in A's or B's
  signatures changed.
- **`push_screen_at`** is a new public function in `stack.rs` and the
  `slotted_ui` root, beside the contract's five commands.
- `docs/PLAN.md`'s test count line says 1054, which is the workspace count
  after this package.

## 4. Tests

Fourteen added; the workspace is at **1054 passing** (`cargo test
--workspace`), plus `just test-mods` (3 Lua tests) green. No snapshot changed:
`initial_focus: chest_grid` picks the same slot the first-focusable rule
already picked, and the tree ignores the stack.

- `crates/slotted-test/tests/gamepad.rs` (9), on the real
  `demo_chest.screen.ron`: opens as a stack entry with focus on the chest
  grid's first slot and no ring in pointer mode; `set_input_mode(Gamepad)`
  shows the ring, eight `DPadRight` presses walk the row with the ring's
  target and rect following each one, `DPadDown` and `DPadLeft` continue;
  a real button press flips the mode and shows the ring; `South` picks up
  and, on another slot, puts down; `East` pops the screen, the menu is gone,
  focus is cleared, the ring hides, the carried stack is conserved; the
  keyboard `Back` binding pops too; keyboard `Accept` picks up exactly once;
  the left stick moves once past the deadzone, repeats while held and stops
  on release; a menu-less modal pause screen opens through `open`, its
  `initial_focus` lands on the named button, the chest stays visible under
  it, and `East` pops it back to the chest with focus restored.
- `crates/slotted-test/tests/lua_tests.rs` (1): a Lua test reads `focused()`,
  walks with `gamepad`, picks up and puts down with `South`, closes with
  `action("back")`, and a bad button name and a bad action name fail the test
  with the harness's message.
- `crates/slotted-test/tests/replay.rs` (1): a recorded `DPadRight` press and
  release replays through the raw stream, moves focus and reaches the
  `Gamepad` component.
- `crates/slotted-ui/tests/invalidation.rs` (2): a stacked screen respawned
  by a base edit stays the one entry on the same menu with the new root and
  `Back` still pops it; a screen under a page is respawned under it, hidden,
  with the page still on top and focus not moved into it.
- `examples/chest/tests/ui.rs` (1): the chest opens as a stack entry, Escape
  pops it with a carried stack conserved and the binding cleared, `E` pushes
  again, East pops again.

Gates run: `just fmt-check`, `just lint`, `just test`, `just doc`,
`just gen-docs-check`, `just test-mods`, and
`cargo check --target wasm32-unknown-unknown` for `slotted-ui`,
`slotted-browser`, `slotted-test` (default and `--no-default-features
--features script`). `just deny` and `just server-check` were not run.

## 5. Not done, and follow-ups

All in `docs/FOLLOWUPS.md` under "Menus M0": the ring's visibility fade
(needs a border-alpha tween target), pop transitions, wheel scrolling of
`overflow: scroll`, scroll children shrinking without `min_height`, no
opacity group for the push fade, a rebound keyboard `Accept` not reaching
buttons (2.1), and the multiplayer scene's modal second client (2.2).

Nothing in the contract's section 4 is left out. The three-theme "visible
ring" of section 5 is asserted on glass in `gamepad.rs`; A's theme
completeness test covers the `focus.ring` role in all three, and the ring's
visibility does not depend on the theme.
