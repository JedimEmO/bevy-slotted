# Menus M3 notes: package D (harness, example, docs)

What D built on top of A, B and C, the decisions the contract left open,
what departs from it, the tests, and what is left. Contract:
`docs/design/menus-m3-contract.md`, section 5 and the "Done when" in
section 6. No signature in the contract changed; there is no v1.3
amendment. Every deferral from the four packages is under "Menus M3" in
`docs/FOLLOWUPS.md`.

## 1. What is built

### Harness

`crates/slotted-test/src/menu.rs`, behind the existing `menu` feature:
`dialogue()`, `dialogue_revealed()`, `dialogue_text()` (the `TextSpan`s
under the `text` node whose `TextColor` alpha is above zero, A's rule),
`dialogue_options()` (the `choices` column's children carrying
`DialogueOption`, with `ButtonState::disabled` inverted), `dialogue_advance()`
(`Accept` and a frame), `dialogue_choose(id)` (the button with test id
`option_test_id(id)` within the dialogue root, `activate` and a frame; a
panic names the id and mentions `choice_delay`), `dialogue_history()`,
`dialogue_events()` (drains a `DialogueEventLog` the new `DialogueRecorder`
plugin fills in `Last`; the builder adds it beside `MenuChoiceRecorder`, and
it registers the three messages itself so an app without `MenuPlugin` still
builds) and `start_dialogue(id)` (the runner's command, a flush and a
frame). `DialogueEvent::{Entered, Chosen, Ended}` is in the prelude.

Tests: `crates/slotted-test/tests/dialogue.rs` (2, a dialogue registered by
hand, no example) and the example's `tests/flow.rs` (below) use every
helper. Both, as the lead allowed either; the harness test is the one a
consumer reads.

### The example

`examples/menus`: About and Talk share **one** injection at `buttons_end`
(two injections at one anchor sit side by side, since the anchor is a row;
the first attempt halved About's width). `load_dialogue` (`Startup`) loads
`assets/dialogue/greeting.dialogue.ron` through `DialogueAssets`; the
headless flow test waits for `Dialogues` to hold it rather than registering
from `include_str!`, so the asset path is what the tests cover. `talk`
starts it; `route_choices` answers `talk`; `thank_on_yes` answers the `yes`
`DialogueChoice` with an info toast. The file is the contract's 3.1 sample
with the portrait on the `yes` line too and the condition on
`demo.found_key`. Strings `demo-elder`, `demo-greeting-{hello,ask,yes,
secret}`, `demo-yes`, `demo-no`, `demo-secret`, `demo-menus-talk`,
`demo-menus-thanks` and the settings tab's two in `assets/locale/en-US.ftl`;
`assets/portraits/elder.png` was already there.

`showcase::settings::spec` grew a fourth tab, `demo`, with the one
`demo.found_key` toggle (`FOUND_KEY`). A row in an existing tab broke the
gamepad walk's Down-from-the-last-row assertions; a tab of its own broke
only the wrap assertion, which now visits it.

`main.rs`: `--dialogue`, `--dialogue-choice`, `--dialogue-history`, staged
with the runner's own commands (`talk`, retried until the asset registered;
`advance_dialogue`; `choose_dialogue("yes")`; `open_history`) at fixed
times, capturing at three seconds instead of two, keyboard mode forced as
the other shots do. Five shots: `menus-dialogue{,-paper,-neon}.png`,
`menus-dialogue-choice.png`, `menus-dialogue-history.png`; `just shot-menus`
captures them; `README.md` shows them.

### Docs

`docs/guide/dialogue.md` (new: the asset, conditions, starting and
answering, input, the screen and its ids and anchors, a template of your
own, the typewriter tokens, history, the harness), `menus.md` (the sixth
template row, the bar's own tags, the link), `screens.md` (`focus` on
`Presentation`, `focus_top` in the focus rule, `role` on `text` and
`rich_text`, `close_stacked` and `focus_top` in the stack table),
`rich-text.md` (a `Reveal` section, the glyph set in the key glyph table's
prose, the alpha rule in the test section), `themes.md` (105 roles, the
dialogue group, the disabled label rule, `role` on text with A's
dotted-fallback caveat and the three themes' dialogue looks,
`Sizes.portrait`, the `dialogue` tokens), `input.md` (the focus rule and the
same-frame clear), `testing.md` (`stack_top` skipping overlays, a `Dialogue`
section with every helper), `docs/guide/README.md` (the row), `docs/PLAN.md`
(v2.4, M3 done, 5.4), `CHANGELOG.md` ("Menus M3" above M2), `FOLLOWUPS.md`
("Menus M3, 2026-09-09"; the four M2 entries M3 closed deleted and named
in the M2 header).

## 2. What the shots showed, and what changed for them

Every shot was looked at.

- **Glass, paper, neon `--dialogue`**: the panel, the portrait in its
  frame, the accent (glass), lime (neon) and bold mono (paper) speaker, the
  line revealed, `Enter Continue` and `X History` in the bar, the caption at
  the right. Nothing to fix in the materials. `[i]Few[/i]` draws upright
  (the fonts have no italic; an M2 follow-up already says so).
- **`--dialogue-choice`**: the locked `I found the key` read exactly like
  `Not now`. `button.disabled` in glass is a fainter fill that vanishes over
  the dialogue's own sheet, and the label kept its full colour. Fixed in
  `slotted-ui`: `invert_active_labels` paints a label under a `*.disabled`
  role with `<rest>.disabled` (`control.label.disabled`, new in the three
  themes) when the theme defines it, else `text.muted`; `controls::
  disabled_of`; `controls.rs::a_disabled_button_is_inert_and_drawn_as_such`
  pins the label role. The recapture shows the third option dimmed.
- **`--dialogue-choice`** also shows an empty band above the prompt where
  the hidden header sits (`Visibility::Hidden` keeps layout). Left as it
  is: collapsing the row is a presenter change with C's visibility
  assertions and six snapshots to follow; in FOLLOWUPS.
- **`--dialogue-history`**: the page over the scene with both lines under
  their bold speakers, Done focused, the toast from `yes` at the bottom.
- The dialogue and the title panel overlap in every glass and neon shot
  (the title sits at the left, the dialogue is 90% wide and centred).
  Cosmetic, in FOLLOWUPS.

## 3. Deviations and fixes outside D's files

- **`dialogue_actions` drains its reader while it is not listening** (B's
  `dialogue.rs`). The first flow test found the first line already revealed
  the frame after Talk: the reader persisted the `Accept` that activated the
  Talk button, the early return skipped reading it, and the next frame,
  with the dialogue the focus top, read it as fresh and advanced.
  `events.clear()` on the not-listening path. Pinned by
  `tests/dialogue.rs::the_accept_that_started_the_dialogue_does_not_skip_its_first_line`
  (under `// M3-TEST: D`; a pad press, since a keyboard Enter activates on
  its release a frame later, and the game system in `PreUpdate` so the
  start is one frame after the choice, which is the order the example gets
  when `route_choices` happens to run before the `menu` tag's observer).
- **The hint bar lists no stack Back on an overlay** (C's `hint_bar.rs`,
  M2 file). Contract 6 lists "overlay screens' bars say Back" among the M2
  items M3 closes, and A's notes say the label was outside A's edit
  permission; `entries_for` now skips the entry for `PresentationMode::
  Overlay` whatever the `back` policy, since `pop_on_back` never pops one.
  `templates.rs::the_hint_bar_on_an_overlay_lists_no_back_even_with_a_pop_policy`.
- **The disabled label rule** (A's crate, above).
- **`slotted-test/tests/settings.rs`** and its three snapshots: the fourth
  tab (five toggles, four scroll pages, `demo` in the anchor list, the walk
  visits it, `TabNext` from `controls` lands on `demo` before wrapping).
- **The `menus` flow test drives the dialogue with `gamepad` + `step`**
  where it wants to watch a line type, since `settle()` runs until the
  typewriter is done (C's note); `pad()` elsewhere.

## 4. Tests

The workspace is at **1274 passing** (`just test`), 7 added by D (A, B and
C left it at 1267):

- `crates/slotted-test/tests/dialogue.rs` (2):
  `the_helpers_read_and_drive_a_dialogue_registered_by_hand` (an unknown id
  starts nothing; start, the overlay on the stack and not `stack_top`, the
  first `Entered`, the history line recorded on entry, a partial then the
  whole line, the choice and its three buttons with the locked one, a
  refused `dialogue_choose` on it, a flag set after the buttons spawned not
  re-enabling them, `go` and its `Chosen` then `Entered`, narration with no
  speaker, `next: None` finishing and closing) and
  `a_flag_set_before_the_choice_enables_its_option_and_stop_ends_early`.
- `examples/menus/tests/flow.rs` (3): `talk_walks_the_greeting_by_gamepad`
  (three Downs to Talk, South; the overlay over the title with `stack_top`
  the title; `Entered(hello)`; speaker `Elder`, the portrait visible; the
  line typing at 300 ms and whole on South with the `[i]` rendered; the
  bar reading Continue and History; South to `ask`, the prompt, no buttons
  before `choice_delay`, then `yes`, `no`, `secret` locked; the walk
  `yes → no → yes` skipping the locked one; South on Yes: `Chosen` then
  `Entered(yes)`, one toast; the `{ $name }` argument in the revealed text;
  the two-line history with markup kept; West opening the History page
  with Done focused and East closing it with the runner still on `yes`;
  South ending with `Entered(bye)`, `Ended(Finished)`, the overlay gone and
  focus back on Talk, no `MenuChoice` from the dialogue),
  `the_found_key_toggle_unlocks_the_secret_answer` (`set_value` on
  `demo.found_key` is saved to the memory store; `secret` enabled and
  chosen through `dialogue_choose`; speaker and portrait hidden on the
  narration line; no toast; the end) and
  `the_dialogue_trees_match_in_three_themes` (six snapshots: a revealed
  line and a choice with its buttons, per theme). The three main menu
  snapshots gained the Talk button.
- `crates/slotted-menu/tests/dialogue.rs` (1) and `tests/templates.rs` (1):
  above.
- `crates/slotted-ui/tests/controls.rs`: the disabled label assertion
  inside an existing test.

Gates, all green: `just fmt-check`, `just lint`, `just test` (1274),
`just doc`, `just gen-docs-check`, `just deny`, `just server-check`,
`just wasm-check` (including `slotted-test` with `--no-default-features
--features script`), and `cargo check -p slotted-menu --target
wasm32-unknown-unknown`.

## 5. Section 6, bullet by bullet

- **`just ci` green on native and wasm; `slotted-menu` checks on wasm:**
  pass, as above.
- **A game registers a `.dialogue.ron`, calls `start_dialogue`, and gets a
  typed conversation with portraits, choices gated by its value store, a
  history page, and messages telling it what was chosen, with no screen
  file of its own; a game that registers `slotted:dialogue` first gets its
  own look:** pass. The example is exactly that (one `.dialogue.ron`, one
  `Startup` load, one system answering `DialogueChoice`); the
  registered-first rule is B's template registration, unchanged from M2,
  and `dialogue.md` says what a template of your own has to keep.
- **`examples/menus` walks the conversation by gamepad headless and the
  reference shots exist for three themes:** pass; `tests/flow.rs`, and
  `shots/menus-dialogue{,-paper,-neon}.png` plus glass for the choice and
  the history.
- **FOLLOWUPS M2 items closed:** pass; the four are deleted from the M2
  list and named in its header: `menu.title` / `menu.version` unused (A),
  overlay bars say Back (D, above), `set_text` on a button label (A),
  `rich-text.md` and `GlyphSet` (D).

## 6. Deferrals

All in `docs/FOLLOWUPS.md` under "Menus M3, 2026-09-09", with A's, B's and
C's. D's own: the hidden header's empty band on a choice, the doubled
Continue hint and caption, the dialogue overlapping the title panel, two
injections at one anchor sitting side by side, `DialogueEvent` order being
per frame across three readers, the example loading its dialogue through
the asset server in the headless test, the unspecified `Input`-set order of
a game's `MenuChoice` reader against the `menu` tag observer, and the M4
forwarding-location and `RegistryKind::Dialogues` notes from contract 5.
