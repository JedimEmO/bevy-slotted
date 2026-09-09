# Menus M3 notes: the review round

`/code-review 9d4d7c3..HEAD high` over the five M3 commits, after packages A
to D landed. Five findings, none blocking. What was changed, what was
recorded, and the one finding that did not survive checking.

## Fixed

### A dialogue whose `start` names no node panicked a frame later

`start_with` inserted `ActiveDialogue { node: start, .. }` and then called
`enter`, which warned and returned when the node was missing. That left the
resource pointing at a node the dialogue does not hold, and
`ActiveDialogue::current` (an `expect`) panicked on the next frame in
`present`. Reachable without malformed data: `Dialogue`'s fields are public
and `Dialogues::register` and `start_dialogue_with` both take one that never
went through `from_ron`, which is the only path that validates.

Two guards, because they answer different holes. `start_with` now calls
`Dialogue::validate` and refuses to start on an error, so a broken
conversation never opens a screen at all rather than opening one and closing
it a frame later; and `enter` now `finish`es the run with
`EndReason::Cancelled` instead of returning, so `current()` cannot be reached
with a bad node even if something slips past validation. `jump_dialogue`
already checked before entering and is unchanged.

Test: `a_dialogue_whose_start_is_missing_never_opens_a_screen`.

### A `jump` back to the choice already showing did not rebuild it

`Presented` recorded the node and the history length, and `fresh` compared
both. A `Choice` pushes no history line, so `jump_dialogue` to the node
already current changed neither field and the presenter skipped the whole
rewrite: the option buttons, their enabled states and the focus stayed as
they were, even though the game had explicitly asked to re-enter.

That mattered more than it looks, because re-asking is exactly how a game
recovers from options gated off: set the value the condition reads, jump
back, watch the option unlock. `ActiveDialogue` gained an `entered: u64`
counter, bumped on every entry including a re-entry, and `Presented` now
holds only that. A reveal flip still changes no entry, so it still does not
re-present.

Test: `a_jump_back_to_the_current_choice_rebuilds_its_options`, which was
confirmed to fail against the old comparison before the fix went in.

## Recorded, not fixed

- **A choice with every option disabled strands the player**: nothing
  focusable, and no key leaves the screen under the default config. The
  presenter now logs an error naming the dialogue and the node, since it is
  a data mistake rather than a state the runner should paper over, and the
  `jump` fix above gives the game a way back. A runner-level escape is in
  `FOLLOWUPS.md`.
- **A hot reload of `slotted:dialogue` cancels the run**, the same shape as
  the M2 confirm entry. In `FOLLOWUPS.md` with the two candidate fixes.

## Checked and rejected

The review read `hint_bar.rs`'s new `PresentationMode::Overlay => None` arm
as a lie: it argued that `pop_on_back` never looks at `presentation.mode`, so
an overlay with the default `BackPolicy::Pop` is popped by `Back` while its
bar now hides the entry. It does not hold. `pop_on_back` reads
`ScreenStack::top`, and `top` is itself defined as the topmost entry whose
mode is not `Overlay`; `PopScreen` filters the same way. An overlay is
therefore never the entry `Back` pops, whatever its policy says, which is
what the comment claims. Two tests already pin it:
`stack.rs::an_overlay_takes_no_focus_and_back_pops_the_screen_under_it` and
`foundation_m3.rs::back_over_a_focusable_overlay_pops_nothing`. No change.

The review also confirmed as sound: the `split_revealed` unit accounting
against `RichReveal::units` including the inline icon path and the multi-byte
`char_indices` split, the `focus_top` switch across `restore_focus`,
`enforce_focus_scope`, `focus_on_spawn` and `tab_actions`, `CloseStacked`
against `on_screen_closed`, the `set_text` `Button` arm and its
`confirm::set_button_label` caller, the `typewriter` and `present` command
ordering, and the `Condition` paths against both fixtures.

## Gates

`just ci` green (fmt-check, lint, test, gen-docs-check, deny, server-check),
`just wasm-check` green.
