# Menus M3 notes: package B (model, registry, loader, runner)

Status: done, 2026-09-08. Files: `crates/slotted-menu/src/dialogue.rs`,
`crates/slotted-menu/tests/dialogue.rs`, `crates/slotted-menu/tests/fixtures/dialogue/greeting.dialogue.ron`.
Gates: `cargo clippy -p slotted-menu --all-targets -- -D warnings`, `cargo test -p slotted-menu`
(24 in `dialogue.rs`), `RUSTDOCFLAGS="-D warnings" cargo doc -p slotted-menu --no-deps`, all clean.

## What was built

- **Model.** `Condition::parse` accepts `key` and `!key` (whitespace trimmed either side of the
  `!`); empty, a bare `!`, a key with whitespace or a second `!` is `BadCondition`. `holds`: `Bool`
  is itself, `Int`/`Float` non-zero, `Text` non-empty, a missing key false; `negate` flips.
  `Dialogue::from_ron` parses through `ron::Options` with `IMPLICIT_SOME` as a default extension
  (a file without the `#![enable]` line still reads) and calls `validate`, which checks in this
  order: `MissingStart`, then per node in `BTreeMap` order a `Say`'s `next` (`MissingNode`), a
  `Choice`'s `EmptyChoice`, `DuplicateOption` and each option's `next`.
- **Registry and loader.** `Dialogues` as in the skeleton. `apply_dialogue_assets` registers on
  `Added` and `Modified`, skipping a parse equal to what is registered (the `apply_screen_assets`
  rule). A running dialogue holds its own `Arc`, so a hot edit only changes what the next
  `start_dialogue` picks up (tested).
- **Runner.** Every public transition queues one `Command` closure; the work is in four private
  world functions: `start_with` (ends a running one `Replaced`, clones the `DialogueConfig.kind`
  def through `templates::cloned`, `PushScreen::apply` with a fresh root, `DialogueScreen` on the
  root, inserts `ActiveDialogue`, enters the start), `enter` (a `Say` resolves speaker and text
  with args through `Localization` into a `HistoryLine`, `revealed = false`; a `Choice` sets
  `revealed = true`; `End` finishes; each writes `DialogueNodeEntered` through
  `World::write_message`), `follow` (`next` or finish `Finished`) and `finish` (removes
  `ActiveDialogue` *first*, then writes `DialogueEnded`, then `CloseStacked(root).apply`, so the
  `ScreenClosed` observer finds nothing to cancel). `ActiveDialogue` is always touched through
  `resource_mut`, so C's `present_dialogue_node` sees a change on every node entry and on the
  reveal flip.
- **Input.** `dialogue_actions` runs only while `stack.focus_top()` is the dialogue root, skips
  repeats and already-claimed actions, and: `Accept` on a `Say` advances and claims (on a
  `Choice` it is left to the option buttons); `Back` with `back_cancels` ends `Cancelled` and
  claims; `Secondary` with `history` calls `dialogue_screen::open_history` and claims.
- **Closed from outside.** `on_dialogue_closed` matches `closed.entity` against
  `ActiveDialogue.root`, writes `DialogueEnded { Cancelled }` and removes the resource.

## Decisions and deviations (contract amended in place, "v1.1 (B)")

1. **`ActiveDialogue::options` takes `Option<&ValueStore>`**, as the skeleton already had it
   (the contract text said `&ValueStore`); `None` means every option is enabled. The contract
   line now says so.
2. **The portrait's `(image: ..)` form is read by hand.** Contract 1.4 assumed `IconDef` reads
   the one-key map; it only *writes* it (`map_variant_serialize!`), and its derived `Deserialize`
   under RON's typed reader wants `image("..")`. The screen loader is not affected because it goes
   `ron::Value` -> model -> typed, and `ron::Value` drops struct names, which is exactly why a
   dialogue file (`say(..)`, `choice(..)`, `end`) cannot take that route. So `DialogueNode::Say
   { portrait }` carries `deserialize_with = "deserialize_icon"`, a private `IconMap { image,
   item }` with `deny_unknown_fields`, one of the two required. The `image("..")` spelling is not
   accepted; serialising an `IconDef` still writes the map, so a round trip is consistent.
3. **`dialogue_actions` is ordered before `pause_on_menu`** as well as before `pop_on_back`
   (from `dialogue::build`, so `plugin.rs` is untouched). Both read the same frame's Escape;
   without the order, `pause_on_menu` could push the pause and claim `Back` first, and a
   `back_cancels` dialogue would never see its `Back`. Discovered by the test.
4. **Escape over a dialogue pauses.** With `back_cancels: false`, `Back` is unowned, so
   `pause_on_menu` reads Escape as `Menu` over an overlay and pushes the pause above the dialogue
   (contract 2.1 says "`Menu` over an overlay still pauses"). The test pins this, and uses the
   pad's East for the pure `Back` that touches nothing.
5. **Asset loader test uses a fixture directory, not a temp dir**: `tests/fixtures/dialogue/
   greeting.dialogue.ron`, registered as a second asset source (`fixtures://`) through
   `AssetSourceBuilder::new(AssetSource::get_default_reader(dir))` in a plugin closure placed
   *before* `SlottedPlugins` in the harness tuple. The theme keeps loading from the workspace
   `assets/`, and nothing in `assets/` is created that D's example will want to own. The harness
   helper `harness_with(theme, extra)` is the hook; C can copy it.
6. **`start_dialogue_with` runs an unregistered dialogue** (no registry lookup), which is what
   a game constructing a `Dialogue` in code gets; tested.

## Tests (`tests/dialogue.rs`, under `// M3-TEST: B`)

Model: `from_ron_parses_the_sample` (also without the `#![enable]` line),
`from_ron_rejects_a_dangling_next` (say and option), `from_ron_rejects_a_missing_start`,
`from_ron_rejects_an_empty_choice`, `from_ron_rejects_a_duplicate_option_id`,
`from_ron_rejects_a_bad_condition` (surfaces as `DialogueError::Ron` with the `bad condition`
message inside, since serde's `try_from` failure is reported by the RON reader),
`condition_parses_both_forms_and_reads_truthiness`.

Runner: `a_registered_dialogue_starts_as_an_overlay_and_enters_its_nodes`,
`starting_an_unknown_dialogue_does_nothing`, `start_dialogue_with_runs_an_unregistered_dialogue`,
`advance_reveals_then_advances`,
`accept_advances_the_line_and_claims_while_the_dialogue_is_the_focus_top`,
`choose_writes_the_choice_and_follows_next`, `a_disabled_option_is_a_no_op_until_its_condition_holds`,
`choose_on_a_line_is_a_no_op`, `jump_from_a_message_answer_redirects` (a game system answering
`DialogueNodeEntered` on `ask` with `jump_dialogue(secret)`), `end_cancels`,
`starting_over_a_running_dialogue_replaces_it`,
`an_end_node_and_a_missing_next_both_finish_and_close_the_screen` (end node, `next: None` on a
line, `next: None` on an option), `clear_screens_cancels` (and `close_screen` on the root),
`back_leaves_an_overlay_alone_unless_it_cancels`,
`a_modal_pushed_above_silences_accept_and_popping_it_restores`,
`the_asset_loader_round_trips_and_apply_dialogue_assets_registers` (load, register, start, edit
the asset in place: the registry takes the edit and the run keeps its `Arc`).

## Deferrals

- **`Secondary` -> history is untested here.** `open_history` is C's and was `todo!()` while B
  ran, so pressing Secondary with `history: true` would panic; the claim path is one `match` arm
  identical to `Back`'s. C's `dialogue_screen.rs` test of the history page covers it end to end.
- **Nothing paints.** C's `present_dialogue_node` and `typewriter` were no-ops during B, so the
  runner tests assert on `ActiveDialogue`, the stack and the messages only; `revealed` is flipped
  by `advance_dialogue` itself.
- **Fluent args in history.** `HistoryLine.text` is `Localization::text_with(text, args)`, so a
  missing key records the key itself (the sample's `demo-*` keys resolve to themselves in the
  tests). A language switch does not rewrite lines already in the history; that is the recorded
  transcript, as the contract says.
- **`IconDef` reading the map form itself** would let the `deserialize_icon` shim go; that is a
  `slotted-ui` change (a custom `Deserialize` on `IconDef` accepting both spellings) for
  FOLLOWUPS.
