# Menus M3 notes: package C (the screen)

Status: done, 2026-09-09. Files: `crates/slotted-menu/src/dialogue_screen.rs`,
`crates/slotted-menu/src/hint_bar.rs` (the two tag verbs), `crates/slotted-menu/screens/
dialogue.screen.ron`, `crates/slotted-menu/tests/dialogue_screen.rs` (new, 15 tests) and its
`tests/snapshots/`, one assertion in B's `tests/dialogue.rs` (below). `strings.rs` and
`dialogue.rs` are untouched. Contract amended in place as "v1.2 (C)": additions only, no
signature of section 1 changed.

Gates: `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test -p slotted-menu -p
slotted-ui` (dialogue.rs 24, dialogue_screen.rs 15, templates.rs 39), `RUSTDOCFLAGS="-D warnings"
cargo doc --workspace --no-deps`, `cargo fmt --all`, and `examples/menus` tests, all clean.

## What was built

**Presenting a node** (`present_dialogue_node`, 4.2). A one-line system: when `ActiveDialogue`
changed, queue `present` on the world. `present` finds the template nodes under the root by
`test_id` (`speaker`, `text`, `portrait`, `choices`, `continue`; a game template that keeps the
ids keeps the behaviour, one it drops is skipped) and:

- decides whether this is a *new* node from a private `Presented { node, lines }` on the root
  (the node id and the history length, so a `jump` back to the same `Say` presents again while
  the `revealed` flip does not);
- on a `Say`: rewrites the speaker's `LocText` and `SemanticLabel` (hidden when none), empties
  the portrait panel and spawns an `ImageNode` child from `icon_image` sized to
  `sizes.portrait` (hidden when none), rewrites the `text` node's `RichText` key and args, and
  puts `RichReveal(Some(0))` plus `DialogueLine { total }` on it, `total` from
  `RichReveal::units(resolve_runs(..))`. Under `Motion::reduced` or `chars_per_second == 0` it
  puts `RichReveal::ALL` and marks the runner `revealed` instead;
- on a `Choice`: speaker and portrait hidden, `text` shows the prompt (or an empty key) with
  `RichReveal::ALL`, the column is emptied, marked `DialogueChoices`, hidden, and given a
  `DialogueChoicesPending { root, elapsed }` (or the buttons spawn at once under reduced motion
  or a zero `choice_delay`);
- on `End` nothing;
- and on *every* change syncs the state: the `continue` caption's `Visibility` (a revealed `Say`
  only) and the hint bar tags (below).

**The buttons** (`spawn_choices`) are `UiNodeDef::Button`s spawned through a `SpawnCtx` whose
`screen` is the dialogue root and `parent` the column, labelled with the option's key,
`disabled` from `ActiveDialogue::options(values)`, with `DialogueOption { id, index }`, the
`dialogue.option = id` tag and `test_id = option.<id>` (`option_test_id`). Focus goes to the
first enabled option, but only while the dialogue is the focus top (a modal above keeps its
own; its pop restores what the stack recorded). `on_option_activate` (an observer on
`Activate`) hands the id to `choose_dialogue`, which re-checks the condition, so a click and a
pad Accept both go through the runner.

**The gamepad walk.** `AutoDirectionalNavigation`'s geometric fallback does not skip
`InteractionDisabled`, so each *enabled* button gets `nav.up` / `nav.down` links to the
previous and next enabled one, wrapping at the ends (a lone enabled option links to itself).
The disabled one has no links and is never a target. Contract 4.2's "`nav` tags for a
vertical walk" is exactly this; the wrap is the choice I made, since a dialogue list is short
and a stop at the end feels dead on a pad.

**The typewriter** (`typewriter`, 4.3) runs after `present` in the same chain and before
`render_rich_text`, so a reveal written this frame is painted this frame. Per `DialogueLine`:
`elapsed += Time<Virtual>::delta`, `units = min(total, floor(elapsed * cps))`, and
`RichReveal(Some(units))` when it changed; at `total` the line is removed, `RichReveal::ALL`
left behind (not `Some(total)`, so a later catalogue switch that lengthens the text still shows
all of it) and the runner marked `revealed`. When the runner's `revealed` is already true
(Accept skipped the reveal) the line jumps to `total` the same frame. A line added this frame
is skipped once, so the first painted frame shows `Some(0)`. The choice column's
`DialogueChoicesPending` counts the same clock and queues `spawn_choices` past `choice_delay`;
`spawn_choices` re-checks that the runner is still on a `Choice` on that root.

**History** (`open_history`, 4.4). `transcript(history)` builds `[b]{speaker}[/b]\n{text}\n\n`
per line (speaker escaped through `rich::escape`; the text is already resolved markup and is
kept as it is), and the page is pushed the way `open_page` does it (the def cloned, `title` and
`body` rewritten, `PushScreen`), inline rather than through `open_page` so the closure needs
no second command flush. The body key is the transcript itself; a `rich_text` resolves
through `text_with`, which never warns, so the `looks_like_key` concern in the contract does
not arise. `slotted:page` is a *page*, not a modal: it hides the dialogue while it is up and
the dialogue reappears with its focus when Done pops it, which the test pins.

**Hint entries** (4.5). On a `Say` nothing is focused, so the verb cannot come from the focused
node; the bar itself carries the state. `hint_bar.rs` learned: `hint.accept` on the *bar* is
the Accept entry when the focused node contributed nothing, `hint.secondary` and `hint.back`
on the bar add those entries (constants `SECONDARY_TAG`, `BACK_TAG`), and a bar rebuilds when
its `Tags` change (`Option<Ref<Tags>>` in the query). `sync_state` writes the bar's tags per
state: `hint.accept` = `slotted.menu.dialogue.skip` while typing, `.next` ("Continue") when
revealed, absent on a `Choice` (the focused button's own "Select" shows); `hint.secondary` =
`.history` when `DialogueConfig.history`; `hint.back` = `.leave` when `back_cancels`. The
tags are written only when they differ, so the bar does not churn. Every hint bar under the
root gets them, so a game template with two bars works.

**Template.** As the skeleton had it, plus `wrap: true` on `text`, `wrap: false` on the
speaker, and a comment listing the ids the screen relies on. The portrait's 64 is a
placeholder the screen overwrites from `sizes.portrait` at present time. `min_height: 66` on
the text box is three body lines at the shipped themes' metrics; a theme with a taller body
style bounces once on the first short line, which is cosmetic.

## Decisions and deviations

1. **`present_dialogue_node` keeps the skeleton's signature** (`Option<Res<ActiveDialogue>>`,
   `Commands`) and does its work in a queued world closure: the spawning needs `&mut World`
   for `SpawnCtx`, and a queued command keeps the system's change detection honest.
2. **Ordering**: `(present_dialogue_node, typewriter).chain().before(render_rich_text)`, not
   `before(refresh_key_glyphs)` as the skeleton had it. The chain and the `before` give Bevy
   the sync points, so the `RichText`/`RichReveal` writes of the presentation and the
   typewriter's reveal both land before the render of the same frame. The runner's commands
   (Navigate) apply before Render, so an Accept reaches the typewriter the frame it is pressed.
3. **The `revealed` flip is a resource change** the presenter sees the next frame and answers
   with the caption and hint sync only (`Presented` is unchanged). One frame of lag on the
   caption, none on the paint.
4. **Choice buttons spawn after the delay, not hidden before it**: the focus and the hint
   verb should not exist before the player can see what they act on.
5. **The contract's "Continue" hint label is `slotted.menu.dialogue.next`** (already in
   `strings.rs`); `.continue` is the caption with the glyph. No string was added.
6. **B's `a_modal_pushed_above_silences_accept_and_popping_it_restores`** (in `tests/dialogue.rs`)
   asserted `!revealed` after an Accept under the modal, which only held while nothing
   painted; with the typewriter running the line reveals on its own inside `settle()`. The
   test now advances a second first and proves the silence by the node (`hello` still, where a
   heard Accept would have advanced to `ask`), and the "reaches the runner again" half asserts
   `ask`. Three lines; flagged here because the file is B's.
7. **The transcript escapes only the speaker.** `HistoryLine.text` is resolved markup by
   contract; escaping it would show `[b]` literally.

## Tests (`tests/dialogue_screen.rs`, `// M3-TEST: C`)

Helpers copied from B's file, plus `harness_with(theme, motion, extra)`, `start_greeting`
(one frame after the start, so the first line is presented and untyped), `start_at_ask` (to
the choice with its buttons up), `shown()` (A's rule: the spans with alpha above zero),
`options()`, `option(id)`, `hints()`, `cps()`, `choice_delay()`.

`the_template_opens_as_a_focusable_overlay_in_three_themes`,
`a_say_shows_speaker_and_portrait_and_types_the_text_at_chars_per_second` (three themes:
`Some(0)` on the first frame, `units == floor(elapsed * cps)` mid-way, the shown prefix, no
caption while typing, then `ALL`, no `DialogueLine`, `revealed`, the caption),
`accept_mid_reveal_shows_everything_and_the_next_accept_advances`,
`reduced_motion_reveals_at_once_and_shows_the_choices_at_once`,
`a_zero_chars_per_second_theme_reveals_at_once`,
`a_choice_spawns_its_buttons_after_the_delay_and_focuses_the_first_enabled` (three themes:
pending then spawned, the prompt where the line was, speaker hidden, focus on `yes`, the walk
`yes -> no -> yes` skipping the disabled `secret` both ways, South yields `DialogueChoice` and
the `yes` line types), `a_condition_that_holds_enables_the_option` (up from the first wraps
to `secret`; narration hides speaker and portrait),
`secondary_opens_the_history_and_done_returns_to_the_dialogue_with_its_focus` (three themes:
title "History", the body's rendered text, Done focused, the runner waiting, Done pops and
the `no` option has its focus back), `the_history_lists_every_line_said_so_far_in_order`
(and the transcript's escaping), `history_off_leaves_secondary_alone`,
`the_hint_entries_follow_the_state` (skip/next/select with history, then leave with
`back_cancels`), `the_text_height_does_not_change_during_a_reveal` (three themes, the text and
the panel), `a_pause_pushed_over_the_dialogue_takes_focus_and_the_dialogue_resumes_after_it_pops`
(Menu pauses, the pause holds focus, Accept under it never reaches the option, the pop gives
the option its focus back and South chooses it; then `Time<Virtual>::pause()` freezes the
next line and unpausing resumes it), `ending_the_dialogue_closes_the_screen_and_its_line`,
`a_say_and_a_choice_tree_match_in_three_themes` (six insta snapshots under
`tests/snapshots/`; the trees are identical across themes, and the hint bar's tags in them pin
the state).

## For D

- `dialogue_text()` is `shown()` on the `text` node; `dialogue_options()` is `options()`;
  `dialogue_revealed()` is the runner's flag. The option buttons are found by
  `by::tag(OPTION_TAG, id)` or `by::test_id(option_test_id(id))`.
- `stack_top()` is `None` while only the dialogue is up (an overlay); `focus_top()` is the
  dialogue.
- The harness starts with something focused outside every screen (entity 50 in the tests);
  the dialogue does not clear it on a `Say`, since it is under no stack entry. The tests
  assert "no *option* is focused" rather than `None`.
- The hint bar is hidden in pointer mode; the entries still compute, and the caption always
  shows.

## Deferrals

- **A value store change while a `Choice` is up does not re-enable its buttons**; the runner
  re-checks the condition on `choose`, so a stale button is a no-op with a warning rather
  than a wrong jump. Re-presenting the column on `ValueStore` change is a small addition.
- **A `jump` to a `Choice` while a modal sits above** spawns the buttons after the delay and
  leaves focus alone; the modal's pop lands on the entry's recorded focus (`None`) and then
  the root's `initial_focus` (`None`), so nothing is focused until the player moves. Rare
  enough to leave.
- **`min_height` of the text box is a fixed 66 px**; a `min_lines` on `rich_text` (a `Layout`
  length in line units) would make it theme-proof.
- **The history page hides the dialogue** (`slotted:page` is a page). A modal variant would
  keep the conversation visible underneath; a game can register its own `slotted:page` kind
  with `mode: "modal"` today.
- **The portrait PNG is absent in the tests**; the asset server logs a load error for
  `portraits/elder.png` and the `ImageNode` stays blank. D's example adds the file.
