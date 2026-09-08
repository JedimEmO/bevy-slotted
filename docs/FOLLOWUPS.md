# Follow-ups

Decisions deferred during phases. Each entry names the phase that should pick it up.

Entries the gap-closing round closed have been deleted rather than struck
through; what closed them is recorded in `docs/design/gaps-notes-{A,B,C}.md`
and summarised below. What is left here is open.

## Menus M3, 2026-09-09

The dialogue runner and screen (`docs/design/menus-m3-contract.md`,
`docs/design/menus-m3-notes-{A,B,C,D}.md`). What the four packages deferred.

- **The Lua bridge is M4's.** The three messages carry strings and a `usize`
  and the five commands take strings, so each is a one-line binding. Where
  the forwarding lives is M4's decision: `slotted-packs` does not depend on
  `slotted-menu`, so it is either an optional dependency there or a bridge
  in the facade.
- **A dialogue shipped inside a mod pack waits for a `RegistryKind::Dialogues`.**
  Today a pack's `.dialogue.ron` is not discovered; a game loads it through
  `DialogueAssets` or registers it by hand.
- **`IconDef` does not read its own map form.** A `.dialogue.ron` portrait
  reads `(image: "..")` through a private `deserialize_icon` shim in
  `DialogueNode::Say`, because `IconDef`'s derived `Deserialize` under RON's
  typed reader wants `image("..")`. A custom `Deserialize` on `IconDef`
  accepting both spellings (a `slotted-ui` change) would drop the shim.
- **A value store change while a choice is up does not re-enable its
  buttons.** The runner re-checks the condition on `choose`, so a stale
  button is a no-op with a warning rather than a wrong jump; re-presenting
  the column on a `ValueStore` change is a small addition.
- **A `jump` to a choice while a modal sits above** spawns the buttons and
  leaves focus alone; the modal's pop lands on nothing focused until the
  player moves.
- **The text box's `min_height` is a fixed 66 px** (three body lines at the
  shipped themes' metrics). A `min_lines` on `rich_text` would make it
  theme-proof.
- **The hidden header keeps its height on a choice.** The portrait and the
  speaker are `Visibility::Hidden`, not collapsed, so a choice shows an
  empty band where the header was; the panel is anchored at the bottom and
  grows upward with the buttons anyway. Collapsing the row (`Display::None`
  when both are hidden) is a presenter change with the snapshots to match.
- **The history page hides the dialogue** (`slotted:page` is a page). A modal
  variant would keep the conversation visible underneath; a game can
  register its own `slotted:page` kind with `mode: "modal"` today.
- **The "Continue" hint and the `{key:accept} continue` caption both show**
  on a revealed line in keyboard and gamepad mode, saying the same thing at
  both ends of the footer. The contract asked for both; the caption alone
  would do once the bar is there, or the bar alone in pointer mode.
- **A `.ftl` argument in the history is baked in.** `HistoryLine.text` is
  resolved when the node is entered, so a language switch does not rewrite
  lines already said; that is the recorded transcript, by contract.
- **`Secondary` to history has no runner-level test** in `tests/dialogue.rs`
  (B ran while `open_history` was a `todo!()`); C's screen test covers it
  end to end.
- **The missing-text-role warning is per paint for plain `text`** (spawn,
  theme change) and once per node for `rich_text`; a marker in
  `slotted-theme` would make them match.
- **A theme that defines `dialogue` but not `dialogue.speaker` paints the
  speaker with the panel material**, because the dotted lookup finds
  `dialogue` before `ThemedFallback` is consulted. Documented in `themes.md`;
  a fallback that prefers a text material over a panel one is the fix.
- **`DialogueEvent`s are ordered per frame, not per write.** The recorder
  reads three message types through three readers, so within one frame the
  order is chosen, entered, ended; a `Replaced` end lands after the new
  dialogue's first node when both happen in one frame.
- **A dialogue over the main menu overlaps the menu's panel.** The dialogue
  is `90%` wide and centred at the bottom; the example's title panel sits at
  the left and the two cross in the shots. Cosmetic; a game places its
  title elsewhere or narrows the dialogue.
- **Two injections at one anchor sit side by side** (the anchor is a row),
  so the example's About and Talk buttons share one injection. Noted under
  M2 as "an injected node sizes to its content"; a column anchor would fix
  both.
- **`examples/menus` loads the greeting through the asset server** in the
  headless flow test too, and waits for it to register; a game whose test
  wants no asset I/O registers from `include_str!` instead.
- **The `ScreenStack` ordering of `route_choices` against the `menu` tag's
  `Activate` observer is unspecified** within `SlottedUiSet::Input`: the
  example's Talk starts its dialogue either the same frame or the next. The
  runner's reader drain (D's fix) makes both orders behave.

## Menus M2, 2026-09-08

The `slotted-menu` crate (`docs/design/menus-m2-contract.md`,
`docs/design/menus-m2-notes-{A,B,C,D}.md`). What the four packages deferred,
plus what the M2 review left open. M3 closed and deleted four of the M2
entries: the unused `menu.title` and `menu.version` roles (A's `role` on a
text node, used by the main menu template), `set_text` not reaching a
button label (A), `rich-text.md` not mentioning `GlyphSet` (D), and overlay
screens' bars saying Back (D: the bar lists no stack Back on an overlay
whatever its policy, since `pop_on_back` never pops one; a `focus: true`
overlay names its own through the `hint.back` tag).

- **A confirm answers `false` on a hot-reload respawn of `slotted:confirm`.**
  `respawn_screens` closes the old root (which triggers `ScreenClosed`, and
  `on_confirm_closed` answers) and the new root has no `PendingConfirm`, so
  the visible dialog can never answer. Carry the component across the
  respawn, or have the respawn path skip the close observers.
- **`resolved_glyph_set` takes an `InputMode` it ignores.** Every caller
  threads one through, and `paint_key_bindings` invents `Gamepad` to satisfy
  it. Drop the parameter.
- **`screen_def_over` clones the frame root's shape** because the inheritance
  rule gives the child root the last word on shape. A merge rule that lets a
  shape-less child root defer to the ancestor would make the generated
  settings screen a plain `inherits` plus the `settings.tabs` node.
- **A toast re-parses `toast.node.ron` per toast** and `toast()` builds fresh
  query states to find the column. Parse once into a resource; keep the
  column entity in `ToastQueue`.
- **`directional_nav_actions` walks every navigable node in the world** to
  find the ones under the focused root, then fetches each again. Walk the
  root's descendants once.
- **The hint bar recomputes `has_tabs` on every focus move.** It now walks up
  from each `tabs` node rather than down through the screen, which is cheap
  while tabs are few; a cache per stack change would remove it entirely.

- **`Auto` glyphs follow the first pad, not the last one used.** Two pads
  of different vendors show the first's glyphs. Recording the pad entity on
  `UiActionEvent` and resolving from it is the fix; the emitter is the place.
- **A `GamepadConnectionEvent` re-renders every key span and cell** even when
  the resolved set did not change (an Xbox pad reconnecting). Cheap; a
  resource caching the last resolved set would skip it.
- **The select row press is a no-op with the popup open**, unchanged from
  M1. A toggle wants the press to close and the click to be swallowed, which
  needs state that outlives the press.
- **The hint bar reads the theme's material kind** to decide on brackets. A
  theme that wants a pill *and* brackets, or text without them, needs a
  token or a tag; none exists.
- **A hint bar on a screen under a modal keeps its Back entry** and loses
  its verb (the focus left it). Hiding a covered screen's bar, or freezing
  it, is a design call; the chest's confirm shot shows the pause's bar
  reading `Esc Close` under the dialog.
- **A toast spawned before a catalogue loads keeps the key** until
  `render_rich_text`'s own `Localization` change check repaints it, which
  it does.
- **The theme materials were tuned blind and then by shot.** Every value was
  chosen from the palette and checked headless; D looked at the pause,
  confirm, main menu, page and toast in three themes and fixed the danger
  buttons. The settings screen's materials had M1's look at.
- **`max_height: 70%` on a settings page resolves against the tabs column**,
  whose height comes from the frame's `max_height: 85%`; at 720p the pages
  are about 225 px tall, so the display tab scrolls too. A theme-sized or
  frame-set page height is a design call.
- **Reset does not close a capture in progress** and does not scroll the
  pages.
- **`apply_settings` runs once per app.** A `Settings` resource replaced
  after it ran is not re-applied; removing `SettingsApplied` re-runs it.
- **No per-action display names** for the conflict toast: `binding_moved`'s
  `action` is the data name (`tab_next`). `slotted.menu.action.<name>` keys
  would fix it.
- **The main menu is a panel now**, placed at the left, so paper's ink reads
  on a dark scene; an invisible column looked right in glass and neon and
  unreadable in paper. A game whose scene suits bare text inherits and sets
  the root role back to `invisible`.
- **An injected node sizes to its content.** The anchor node is a plain flex
  row, so a button injected into a column is as wide as its label; the
  example wraps it in a `width: "100%"` column panel. The anchor could adopt
  its parent's direction and stretch, but every existing injection (the
  rail's Sort button) was laid out against the row.
- **A shot's input mode is forced every frame** in the examples' shot
  systems, because the real mouse over the window flips `InputMode` back to
  pointer and hides the ring and the hint bar. A `--shot` that spawns no
  winit pointer would be cleaner.
- **The pause quit in `examples/menus` goes back to the title**, the title's
  quit exits; the contract said "exits" for both. One id, two meanings by
  `MenuChoice::screen`, is the demo of that field.
- **Only a `LocText` reports a missing locale key.** `resolve_loc_text` warns
  once per dotted key through `MissingLocKeys`; the browser's own
  `Localization::resolve` callers (a card's title) still report nothing. A
  warning at the `Localization` level, after the chain, would cover them and
  needs interior mutability there.
- **The scroll panel's bar shows with nothing to scroll** (the About page's
  short body draws a full-height thumb). M1's scroll widget; hide the bar
  when the content fits.
- **`[i]` in the About body draws upright**: the shipped fonts have no
  italic face and the rich text has no synthetic slant.
- **The chest's `E` reopens the chest over the main menu** unless the game
  clears `ChestBinding::def`, which `examples/menus` does on leave. A stack
  policy ("a page ignores game keys") would be the general fix.

## Menus M1, 2026-09-07

Type scale, rich text, localisation arguments, the value store and the
controls (`docs/design/menus-m1-contract.md`,
`docs/design/menus-m1-notes-{A,B,C,D}.md`). What the four packages deferred.
The M0 entries for wheel scrolling, shrinking scroll children and the rebound
keyboard `Accept` are closed by this milestone and deleted below. M2 closed
and deleted six of the M1 entries: the control materials tuned for the
templates (B's materials, D's shots), the select popup closing on an outside
press (A), key capture conflicts (C), `ValueRule` derived from a slider (C),
the settings demo being a fixture (C's `SettingsSpec`), and the chest
opening settings on Tab (D's `Menu` = Escape).

- **`max_lines` is a `text` feature only.** `TextOpts` carries it for
  `rich_text` too, and a `rich_text` ignores it: truncating spans needs the
  cut to land inside a run and keep the tail's styles. Build it when a
  design wants a clamped rich paragraph.
- **Truncation is proportional, not exact.** `enforce_max_lines` keeps
  `len × max / lines` characters and can land a few words short of the
  longest prefix that fits. An exact fit needs Parley's line breaks for a
  candidate string without a layout pass, which `TextPipeline` does not
  expose in 0.19.
- **Inline rich-text icons do not re-resolve when the icon source changes.**
  `reresolve_icons_on_source_change` covers `ItemView`s; a rich text's
  `ImageNode` is rebuilt only when the node, the catalogue or the theme
  changes. One more `is_changed` on `Icons` in `render_rich_text`, once that
  resource's change semantics are settled.
- **A Fluent message reference in a rich string renders raw.** A `{name}`
  without the `$` is an unknown placeholder to the parser, so the whole
  footer shows its markup, logged once. Correct, but a translator who forgot
  the `$` sees brackets; a friendlier failure would render the rest and mark
  the one placeholder.
- **A text shadow is per paragraph, not per span.** Bevy reads `TextShadow`
  on the root, so a `{key}` run in a `count` paragraph takes the count's
  shadow and a paragraph in a role without one has none. Nothing to do unless
  a direction wants a shadow under glyphs alone.
- **The chest header grew** with `panel.title` on the scale (24/22/26 px).
  If the moodboard wants the old 15 px header, that is the `title` step in
  the theme files, not code.
- **A gamepad row cannot capture `Back`'s button.** `Back` cancels a capture
  on either device, so East can never be bound on a pad row. The rule the
  contract set for Escape, applied to both devices; a capture that ignores
  `Back` for one press would let a player rebind it.
- **Two seeding mechanisms.** B's controls take their value through the
  `BoundValue` entity event; C's `text_field`, `list` and `tabs` read the
  store themselves with a `StoreSeen(version)` marker. Either works; one
  mechanism would be tidier, and the switch needs no contract change.
- **A numeric text filter is per character.** "One point, a leading minus"
  cannot be expressed through Bevy's `EditableTextFilter`, so `1-2.3.4`
  types. The store's rule is where the parse belongs; nothing pins it.
- **Horizontal scroll panels.** The viewport is `scroll_y` only; paging
  handles `x` when a node ever sets it, and nothing spawns one.
- **`ScrollIntoView` and padding.** A padded viewport scrolls its first child
  to the padding edge rather than the content edge, which is Bevy's
  measurement and why the viewport carries no border of its own.
- **A list inside a scroll panel pages the panel, not the list.** The
  panel's `PageNext` observer claims first. Which should win is a design call
  for the settings template (M2).
- **The text field's ring sits on the editable child while editing**, since
  that child is `Focusable` so `Back` can reach it. A ring on the frame reads
  better; the ring would have to know `TextFieldParts`.
- **Tab icons are sized from the compact control height**, because the
  tokens have no icon size.
- **No harness rect snapshot helper.** The contract's "harness rect snapshot"
  does not exist; the settings screen's snapshots are `insta` tree snapshots
  only, which exclude sizes and positions by design. A rect snapshot would
  need a stable serialisation of `rect_of` over the tree and a decision on
  rounding.

- **Bevy's directional navigator takes the whole half-plane.**
  `AutoNavigationConfig::min_alignment_factor` is 0, and the scorer accepts a
  candidate with no cross-axis overlap at all, so `Left` from a grid's first
  slot lands on a button 360 px below it and `Up` from a full-width settings
  row lands on whichever tab button is nearest the row's centre. Every
  control row and button is in the graph now (they were not, which is why
  the settings walk did nothing from a tab), so this shows more than it
  did. A small `min_alignment_factor` (or our own scorer) in
  `SlottedUiPlugin` is the fix; it changes every screen's walk, so it wants a
  pass over the M0 nav tests rather than a one-line edit at the end of M1.
- **`slotted-test`'s own tests never load a theme.** `.theme("glass")` in
  `crates/slotted-test/tests/*` resolves `assets/themes/glass.theme.ron`
  against the crate root, which has no `assets/`, and the load fails
  silently (a failed load is not "loading", so `settle` does not wait). The
  M0 gamepad tests and the chest-screen tests run on the default tokens.
  `settings.rs` passes `AssetPlugin { file_path }` through
  `SlottedPlugins::headless().set(..)`; a `UiHarnessBuilder::assets_dir` (or a
  symlink like `examples/chest/assets`) would make it the default for this
  crate, and a harness that warned when the active theme failed to load would
  have caught it a milestone ago.
- **The base locale file is read only by a pack install.**
  `slotted-packs` loads `assets/locale/<lang>.ftl` in `load_locales`, which
  runs when mods install; a game with no `PackLayout` never gets its own base
  catalogue, which is why the chest example fills its labels by hand and why
  `showcase::settings::install_locale` compiles `en-US.ftl` in and installs
  it at `Startup` when nothing resolves the settings keys. Loading the base
  file without a pack set, on native through the asset server and on wasm
  through the bundle, belongs in `slotted-packs`.
- **A rich placeholder in a Fluent string needs a string literal.** Braces
  are Fluent's placeable syntax, so `{key:back}` in an `.ftl` file is a
  parse error that drops the whole file; the footer writes
  `{"{key:back}"}`. Documented in `rich-text.md`; a friendlier route is a
  Fluent function (`{ KEY("back") }`) registered on the bundle.
- **`slotted.test.open_screen` always opens over a menu.** The Lua runner's
  `open_screen("demo:settings")` takes the `"empty"` fixture and opens a
  menu-less screen with an empty menu behind it. Harmless for store bindings;
  a `property` binding on such a screen would resolve against that empty
  menu. An `open` op for menu-less screens is a small addition.
- **The icon button still reads `ButtonInput<KeyCode>`**, for the shift
  modifier of a pointer click (`on_icon_button_click`), which is not a key
  decision and passes the contract's `KeyCode::` grep. A modifier state
  resource fed by the emitter would remove the last raw keyboard read from
  `widgets/`.
- **Tree snapshots show localisation keys, not text.** `SemanticLabel` on a
  control row is the label's key (`demo.settings.display.ui_scale`), so the
  settings snapshots are stable across catalogues but a reviewer cannot read
  the labels off them. Resolving the label through `Localization` at spawn
  (and on catalogue change) would make the tree what a screen reader hears.

## Menus M0, 2026-09-07

The foundation of the menus work (`docs/design/menus-m0-contract.md`,
`docs/design/menus-m0-notes-{A,B,C}.md`). What the three packages deferred.

- **The focus ring does not fade.** The contract says its visibility toggles
  through the `Fade` preset, but the only alpha the tween machinery drives is
  `BackgroundColor`, and the ring is a border with no fill. It needs a border
  alpha (or a colour) `TweenTarget` in `slotted-theme/src/motion.rs`; until
  then the ring appears and disappears in one frame while its slide and
  resize animate. M1.
- **Pop transitions.** A push fades or slides in; a pop is immediate. The
  popped root would have to outlive its entry for the length of the motion,
  which means the stack despawning on a timer rather than in the command. M1.
- **No opacity group.** The push fade covers the root panel's own background;
  text and slots inside it do not fade. A real opacity group is a Bevy
  feature we do not have.
- **The showcase's multiplayer scene pushes its second client as a scrim-less
  modal**, because two pages would hide the first; `Back` therefore pops one
  client, then the other. A side-by-side of two stacks (one per client) is
  the honest shape and is out of scope for a demo scene.
- **Tab with no screen open logs a warning.** Bevy's tab navigator writes
  "Tab navigation error: No tab groups found" every time Tab is pressed while
  nothing focusable exists (seen running `examples/chest` after Escape). Tab
  is also `UiAction::Menu`'s default key, so a pause screen will want it. Either
  give the HUD a `TabGroup` or gate Bevy's `TabNavigationPlugin` behind an open
  screen. M1 or M2.

## Showcase, 2026-09-06

The eight-scene showcase (`docs/design/showcase-contract.md`) landed as two
packages and was integrated in one pass. What that pass fixed is in the git
history; what it left open is here.

- **The HUD poll only writes `localStorage` when the layout it read back is
  non-empty.** `enterHud` in `playground.js` guards on length, so today only
  Reset can clear the stored key, and Reset clears it directly. That is correct
  as long as Reset is the only way to empty a layout. Give the scene a second
  one and the guard becomes wrong: a layout emptied by that route would keep
  the old RON in storage and come back on the next visit. The guard should then
  compare what it read against what is stored rather than test its length.
- **`arrow_and_digit_keys_do_not_overlap` failed once in three workspace runs.**
  `crates/slotted-ui/tests/review_adversarial.rs`, panicking in
  `crates/slotted-icons/src/shape.rs:249`, which is `inside()` doing
  `points.len() - 1` on what must have been an empty polygon: an arithmetic
  overflow, not an assertion. It has not reproduced since, in that test alone,
  in that crate with `--all-features`, or in two further full workspace runs, so
  the trigger is something about a parallel run rather than the test. A guard on
  the empty slice would turn the panic into a `false`, which is the right answer
  for "is this point inside nothing"; whoever owns `slotted-icons` should decide
  whether the empty polygon is itself the bug.
- **The replay's geometry check is reported in the browser and enforced in a
  test.** A recording carries pointer positions and no locators, so a canvas of
  another size lands the clicks elsewhere. `UiHarness::replay_recording`
  refuses; the page logs a `warn` and plays it anyway, because a visitor's
  window is whatever size it is and refusing would mean the scrubber never
  worked. Locators in recordings would close this properly.
- **The page's scrubber range trails the module by up to a second.**
  `replay_load` is a request the world applies next frame, so the click's own
  `replay_status` still reads zero frames and the range input keeps `max="0"`
  until the one-second poll. A value set in that window is clamped by the
  browser and the seek goes nowhere. `smoke.mjs` waits for the range to grow;
  a page that wanted to be exact would re-read the status a frame later.
- **`test-mods` finds one `screens/` directory by convention and the rest by
  argument.** `sorter` injects into `slotted:any` and so has test files for two
  different games, but lives in one directory. The runner still derives
  `<mods dir>/../screens`, and `--screens` names any others; the `justfile`
  passes `examples/machine/screens`. A mod that spanned three games would want
  the manifest to say so rather than the caller.

## External review round, 2026-09-06

Eight findings from a second, external review, landed as three packages
(`docs/design/review-notes-{A,B,C}.md`) and integrated as one round. All eight
are closed. Each line names the test that proves it.

| # | Finding | Closed by |
|---|---|---|
| 1 | The server acted on any message that named a menu it had. | `slotted-net/tests/sessions.rs::every_message_kind_from_a_foreign_peer_is_refused_and_changes_nothing` and `::access_revoked_mid_session_refuses_the_owners_own_messages`. The seam against finding 3 is `review_round2.rs::a_revoked_peer_retransmitting_an_old_sequence_is_refused_and_leaks_nothing`. |
| 2 | Viewers shared a `MenuState`, an actor and a set of inventories. | `sessions.rs::two_sessions_share_the_container_and_share_nothing_else` and `::a_container_change_reaches_each_session_at_its_own_slot_index`. The seam against closing is `review_round2.rs::a_peer_closing_mid_drag_leaves_no_phantom_and_destroys_nothing`. |
| 3 | A lost correction could not be recovered, and a snapshot retired the wrong submission. | `sessions.rs::a_lost_correction_is_repeated_as_a_correction_on_the_retry` and `::an_out_of_order_snapshot_retires_exactly_what_it_answers`. Reordering is `review_round2.rs::a_snapshot_answering_the_newer_click_arrives_before_the_older_ack`. |
| 4 | Three property writers, only two of which wrote both copies. | `slotted-ecs/tests/prediction.rs::a_resync_updates_the_property_child_and_fires_property_changed` and `::a_resync_that_moves_no_property_fires_nothing`; `slotted-ui/tests/machine_widgets.rs::a_tank_follows_a_property_delivered_by_a_snapshot`. Where it meets finding 2: `slotted-net/tests/review_round2.rs::one_property_change_on_a_shared_furnace_reaches_both_sessions_and_seeds_a_third`. |
| 5 | Invalidation matched screen kinds by name instead of by dependency. | `slotted-ui/tests/invalidation.rs`, and across the reload in `slotted-packs/tests/review_round2.rs::an_edit_to_an_inherited_game_screen_reaches_the_mod_screen_through_both_paths` and `::an_injection_retargeted_on_reload_leaves_one_screen_and_reaches_the_other`. |
| 6 | Nothing a mod registered could ever be removed. | `slotted-packs/tests/review_round2.rs::a_mod_removed_from_the_dir_loses_its_registrations_and_gives_the_game_screen_back` and `::a_reload_that_fails_leaves_the_previous_owned_set_intact`. |
| 7 | `default-features = false` was not a server graph. | `just server-check` greps `cargo tree` on **both** native and `wasm32-unknown-unknown`, then builds both, then runs `slotted/tests/server.rs` -- which compiles only under `--no-default-features --features server` and drives one click from a `RemoteAuthority` to an ack through a `MenuServer` pumped by the server app's own schedule. |
| 8 | `pages` could deploy over a red branch. | `.github/workflows/ci.yml`: `pages` needs `native`, `wasm`, `browser`, `server` and `docs`, and the `browser` job installs a Chrome rather than skipping itself. |

### Found and fixed during the integration review

- **A client closing a screen destroyed whatever was on its cursor.**
  `ClientMessage::CloseMenu` is documented to return the carried stack to the
  player's own inventory; the server dropped the session and the stack with it.
  Any client could sink items at will by pressing escape mid-click.
  `MenuServer::return_carried` now puts it back into a binding the store marks
  private to that peer, and the fan-out runs after the session is gone.
  Proved by `slotted-net/tests/review_round2.rs::a_peer_closing_mid_drag_leaves_no_phantom_and_destroys_nothing`.
- **A lost `CloseMenu` left a session open on the server for ever.** `close`
  was fire-and-forget, so a link that ate the request left a session bound to
  the player's private inventories with nobody who would ever close it, while
  the client had stopped thinking about it. It is now repeated on the same
  timer a click is, until a `Closed` or a refusal settles it. Surfaced by the
  10% loss fuzz in
  `slotted-net/tests/review_round2.rs::two_peers_survive_a_lossy_link_with_properties_and_a_session_that_reopens`.
- **`just server-check` only checked the native dependency tree.** Package C's
  notes claim both targets; the recipe grepped one. It now loops over native
  and `wasm32-unknown-unknown`, and runs the new facade server tests.
- The `slotted-net` and `slotted-packs` test harnesses moved into
  `tests/common/mod.rs` in each crate, so the round-two files drive the same
  world the first-round files do rather than a second copy of it.

### Left open by the round

- **A lost `SetProperty` has no repair path of its own.** Unlike a click it is
  not retransmitted, and unlike a slot it is not re-derived from a dirty mask,
  so a property update the link eats is repaired only by the next snapshot. A
  furnace that keeps burning heals itself on the next tick; one that has
  stopped stays stale until something asks for a resync. The fuzz asserts the
  snapshot path carries it. Making properties reliable on their own needs an
  acknowledgement the protocol does not have, which is a bigger change than
  this round.
- **`apply_resync` diffs the property vector against the vector, not against
  the child components.** `slotted-ecs/src/systems.rs` captures the vector
  before assigning the snapshot and calls `write_property` only for the
  positions that moved, which is deliberate: calling it for all of them would
  deliver a `property_changed` per property per resync to every script. The
  residual is that a `MenuProperty` child that had somehow drifted from the
  vector is not repaired by a resync carrying the value the vector already
  holds. Unreachable while `write_property` is the only writer, and it is; the
  audit for this round found no second one.
- **`Injections` still has a public tuple field.** Nothing in `src/` mutates it
  outside `reconcile_mod_injections`, but several `slotted-ui` tests push into
  `.0` directly to stand in for a game's own registration. A `register_game`
  method would let the field be private.
- **A mod may still register outside its own namespace with only a warning**,
  and the owner recorded for a screen or a widget is derived from the *kind's*
  namespace rather than from the mod that registered it
  (`Screens::load_from_registry`, `install.rs::publish_ui`). It is consistent,
  it is documented where it happens, and it is harmless while a reconcile
  replaces the whole mod-owned set at once. It would stop being harmless if
  reconciliation ever became per-mod.

## Gap-closing round, 2026-09-06

Three packages landed together and were integrated and reviewed as one round.
The notes are `docs/design/gaps-notes-A.md` (authority, validation, the
networked adapter), `-B.md` (UI, theme, browser) and `-C.md` (icons, fonts,
advisories, wasm size). What changed, in one list:

- **`slotted-net`.** A new crate: `ClientMessage`/`ServerMessage`, a predicting
  `RemoteAuthority` that implements `slotted_model::Authority`, an
  authoritative `MenuServer` that applies every click to a scratch copy at
  `ValidationLevel::Always`, and a `Transport` port with an in-process
  `Loopback` that can add latency, reorder and drop. A `bevy_replicon` adapter
  is designed in notes A section 6 and not built.
- **Hints out of the inventories.** `MenuState::hints`, keyed by menu slot, and
  `slotted_model::slot_view` as the one accessor for "what does this slot
  show". A recipe filter no longer inflates `Inventories::count_of`.
- **`ValidationLevel`.** `Off`/`Debug`/`Always`, chosen by `Authority::validation`
  rather than by a global.
- **Resync.** `Authority::request_resync` and `AuthorityEvent::Slot`, so a
  refused submission can ask for the truth instead of repainting from a local
  copy.
- **`ComponentPatch` across a reload.** Patch keys are rebuilt by name beside
  the item; a key the new registry lost is reported as
  `ModError::ComponentVanished`.
- **Sizes are theme tokens.** `Tokens::sizes` carries `slot_size`, `slot_gap`,
  `panel_width`, `card_width`, `card_height` and `chrome_height`. `UiUnits`
  converts the things Bevy measures in another space; a widget never multiplies
  a token by `UiScale`.
- **One virtual grid.** The browser's card grid registers a `VirtualGridSource`
  and `slotted-ui` owns the pooling, so there is no second implementation.
- **Screen inheritance and the `.screen.ron` loader.** `inherits` and `remove`,
  merged by node id, with cycles and missing ancestors reported rather than
  panicked; `ScreenDef` is a Bevy asset and an edit respawns the open screen.
- **glTF icons.** `IconDef::Model { path, stand_in }` takes an atlas cell like
  any other icon; with the `gltf` feature the GPU rig loads the scene, and
  without one the declared stand-in (or a box hashed from the path) is what the
  CPU bake draws.
- **Fonts.** Five OFL families under `assets/fonts/<family>/` with the
  `OFL.txt` each was published with, wired into the three themes.
- **`cargo deny` in CI.** A `just deny` recipe, in `just ci` and in the
  workflow, with a dated single-id ignore for RUSTSEC-2026-0192.

### Found and fixed during the integration review

- **The atlas was published while the rig was clearing it.** The GPU bake's
  camera cleared its render target to transparent on every frame it drew, and
  that target *was* the image the UI sampled. A mesh pipeline compiles
  asynchronously, so on a cold shader cache the atlas was wiped and not
  refilled before the chest example's fixed two-second capture: five captures
  gave one blank and four good ones. The rig now warms its pipelines on a
  scratch target and only switches the atlas camera on once every pipeline
  behind the view is compiled, so the clear and the refill happen in one frame.
  Twenty-five captures over five cold-cache rounds of `just shot-chest` all
  show icons. `tools/check-shot.py` (`just shot-check`, and run by
  `just shot-chest`) measures the icons in the captured pixels, and
  `slotted-icons/tests/review_gaps.rs` holds the headless half: every baked
  cell has ink in it, and the rig's target starts as the CPU bake rather than
  as zeroes.
- **A lost resync request was never repeated.** `Transport::send` reports `Ok`
  for a message the link then drops, which is what an unreliable transport
  does, and `RemoteAuthority` marked the menu as awaiting a resync on the send.
  One dropped `RequestResync` left the client waiting for a container that was
  never coming. Resync requests are now retransmitted on the same timer clicks
  are. Pinned by
  `slotted-net/tests/review_adversarial.rs::a_resync_request_the_link_eats_is_asked_for_again`.
- **`report_unmatched_injections` deduplicated with `Vec::dedup`,** which only
  collapses adjacent equal entries, so two mods aiming at the same missing
  anchor with a third injection registered between them were reported twice.
- **`resolve_loc_text` lived in `slotted-packs`.** `slotted-ui` owns `LocText`
  and the `Localization` port, but the system that repaints a label when the
  catalogue is replaced was registered by the packs plugin, so a game using
  `slotted-ui` with its own localiser got labels frozen at spawn time. The
  system moved to `slotted-ui::loc` and `SlottedUiPlugin` runs it;
  `slotted-packs` re-exports the name it published. It also now puts the key
  back when a key stops resolving, instead of leaving the previous language's
  text on screen.
- **The browser's status line resolved two locale keys and formatted a string
  every frame,** for every open browser, and threw all of it away: the write
  was guarded, the work was not. It is now composed only when the index state,
  the visible count or the catalogue moves.

### Left open by the round

- `bevy_replicon` has no adapter; the design is notes A section 6.
- The two bakes still author their *geometry* twice (see below).
- A model's stand-in cannot be derived from the glTF; it is declared or it is a
  box.
- The `ttf-parser` ignore has a review date (2027-03-06), not a fix.
- The GPU render tests in `slotted-icons` cannot run even with a GPU. libtest
  runs each test on a spawned thread, winit refuses to build an event loop off
  the main thread, and without `WinitPlugin` `RenderPlugin` leaves no
  `RenderDevice` in the main world, so `bevy_pbr`'s own systems panic on the
  first frame. `tests/gpu_bake.rs::the_rig_actually_draws_into_the_atlas` is
  marked `#[ignore = "needs a GPU"]` and fails when it is run with `--ignored`.
  The evidence for the render path is `just shot-chest` plus `just shot-check`
  until a headless render harness exists. **Phase 8.**

## From Phase 2

- **`SemanticLabel` on a slot reads the item's namespaced id, not its display name.** A screen
  reader hears "minecraft:cobblestone x64" while the tooltip beside it says "Cobblestone".
  `slotted_ui::item::semantic_label` should prefer `ItemDef::display_name` and fall back to the id,
  once localisation exists to say which name is the player-facing one. **Phase 3** (localisation),
  which also owns `LocKey` resolution: the same snapshot shows `Text` nodes labelled `chest.title`
  and rail buttons labelled `sort`. Changing this rewrites the three accepted `ScreenTree`
  snapshots in `slotted-test`.
- **`UiNodeDef::{Tank, Bar, SideTab, Viewport, VirtualGrid}` spawn a layout-only placeholder.**
  `SpawnCtx::spawn_placeholder` gives them a `Node` and `SemanticRole::Custom(<name>)` so a screen
  using one still opens and still lays out. **Phase 6** implements them; the property-bound ones
  (`Tank`, `Bar`) need the `MenuProperty` binding that Phase 2 only mirrors.
- **`slotted_icons::LiveIcons` returns `IconRef::Missing`.** The `live` feature compiles the type
  but there is no offscreen item-model bake yet. **Phase 6**, together with `Viewport`.
- **`Favorite` is mirrored as a marker component but is not a theme role swap.** The glass theme
  defines `slot.favorite` and nothing selects it. **Phase 3**, with the rest of the slot state
  machine.

## From the chest example

- **`SlottedPlugins::default()` is not enough on top of `DefaultPlugins`.** `slotted_ui`'s
  navigation systems need `TabNavigationPlugin` and `DirectionalNavigationPlugin`, and
  `DefaultPlugins` ships neither, so a windowed game panics on its first frame with
  `manual_directional_navigation ... Resource does not exist`. `examples/chest/src/main.rs` adds
  them by hand. The facade should either add them itself or ship a `SlottedPlugins::windowed()`
  companion to `headless()`. **Phase 3**, and it is a five-minute fix.
- **Nothing renders a rail button's or a text node's label in a human language.** `LocKey` is
  shown verbatim and rail buttons are labelled `quick_stack`, so the example carries a
  `fill_labels` system that substitutes strings by `test_id` and by the `action` tag. That is a
  reasonable thing for a game to own, but not for every game to have to write. **Phase 3**, with
  localisation; the same entry already appears above for `SemanticLabel`.
- **The durability bar reads `slotted:damage` and `slotted:max_damage` off a stack's patch, but a
  component key only exists if some `ItemDef` declares it.** The demo's three tools each list both
  keys in `components` purely so the freeze interns them. A first-class "component types" registry,
  or interning the keys the renderer needs unconditionally, would remove the trap. **Phase 3**.
- **`slotted_test::fixtures` cannot be reused by a data-driven example.** `TestRegistries::basic()`
  builds its item table in Rust and freezes it into its own dense ids, so an example that loads
  items from RON has to duplicate the table. The item rows want to live in a shared `assets/data/`
  directory that both the fixtures and the examples read. **Phase 3**.
- **`examples/chest/assets` is a symlink to the workspace `assets/`.** Bevy's `AssetPlugin`
  resolves against `CARGO_MANIFEST_DIR`, so without it `cargo test -p chest` cannot find
  `themes/glass.theme.ron`. A `UiHarnessBuilder::asset_root(path)` would let a consumer point the
  harness at their own asset directory instead. **Phase 3**.

## From the Phase 2 adversarial review

- **Bevy gives every `ImageNode` its own `AccessibilityNode`.** The contract calls the icon child
  of a slot purely visual and gives it no `SemanticRole`, but AccessKit still announces 27
  anonymous images inside the chest grid. The semantic tree and the AccessKit tree therefore have
  different node counts, and a screen reader hears the difference. Suppressing the child's node
  (or labelling it) belongs with the rest of the a11y pass. Pinned by
  `slotted-ui/tests/review_adversarial.rs::every_interactive_node_is_semantic_and_reaches_accesskit`.
  **Phase 3**.
- **Two motion presets are still unused.** `MotionPreset::Hover`, `Press`, `DropSquash` and
  `FlyToSlot` are wired into slots by `slotted_ui::motion`, but `Stagger` (children appearing one
  after another when a screen opens) and `Fade` (tooltip and panel entry) have no caller, and
  buttons animate nothing at all. `SQUASH_SCALE` and friends are constants in `slotted-ui` rather
  than theme tokens, so a theme cannot say how far a slot pops. **Phase 3**, with the slot state
  machine and the `SLOT_SIZE` token above.
- **A screen and its tooltips live in different trees.** Tooltips are spawned under
  `TooltipLayer`, not under the screen root, so despawning a screen used to leave its tooltip on
  screen forever. `despawn_orphan_tooltips` now sweeps them. The same shape applies to anything
  else a screen parks in a shared layer; a "owned by screen" relationship would be sturdier than
  a sweep. **Phase 3**.

## From Phase 3

- **`BrowserRuntime::visibility_version` invalidates nothing.** `apply_search` recomputes only
  when the query changed, the index landed or `HiddenEntries` changed; a bump of
  `visibility_version` alone leaves `visible` stale even though the field is part of the
  `SearchCache` key. Nothing in Phase 3 writes it (cheat mode and dev items are Phase 6), so no
  test can fail today. Add it to the dirty check when the first writer arrives. **Phase 6**.
- **The recipe view's header changes the contract's tree order.** Section 7 pins
  `RecipeView > Tab*, RecipeSlot*, Button[browser.transfer], Button[browser.back],
  Button[browser.forward], Panel[browser.uses]`. The moodboard's header puts the back button and
  the focus's title above the tabs, so `browser.back` now comes first and `browser.forward` sits
  beside `+` under the recipe. Amend section 7 to match, or move the header behind a theme
  option. **Phase 4**, with the first contract revision.
- **`chrome_height` is a declared number, not a measurement.** The card grid's row count is
  derived from a 190-unit allowance for the search field, chips, bookmarks and footer. It is a
  theme token now, so a theme that changes those font sizes can say so, but it still has to know
  to; deriving rows from the grid's own `ComputedNode` after the first layout would be exact and
  would need no token at all. Half closed in the gaps pass, package B. **Phase 8**.
- **The dev feature is still empty.** `slotted-browser`'s `dev` feature is declared but the
  exclusion highlighter, the id tooltips and copy-recipe-id are not implemented (notes B, section
  10). **Phase 6**.
- **`ScreenHandler::clickable_areas` has no caller.** No Phase 3 handler returns one and nothing
  reads them; the furnace arrow they exist for is a Phase 6 screen. **Phase 6**.

## From Phase 4

- **The per-frame script budget is a constant, not configuration.** `route::MAX_SCRIPT_CALLS_PER_FRAME`
  is 512 calls. `PacksConfig` has no field for it and its shape is shared, so making it
  configurable means amending the contract. **Phase 5** (packs notes B, item 10).
- **An untyped payload loses the width of its numbers.** Every def is now read through
  `slotted_model::Value`, whose only numeric variants are `i64` and `f64`, so a widget `params`
  payload that RON parsed as `U8(54)` comes back as `I64(54)`. Nothing reads a payload's numeric
  width, and both sides of a payload comparison go through the same conversion, so this shows only
  if something starts comparing a payload against a freshly parsed `ron::Value`. **Phase 6**.
- **A resource pack cannot override a script that lives at a mod's root.** `read_script` tries
  `scripts/<mod id>/<entry>` through the layering first and falls back to `<mod root>/<entry>`;
  `ModWatch` registers a handle only for the first form, so a root-level script reloads through an
  explicit `ReloadMod` and not the file watcher. **Phase 5** (packs notes B, item 4).

## From Phase 5

- **The item browser's status line wraps when the panel is squeezed.** On a canvas narrow enough
  that the free strip beside the chest is under about 260 px, `dock::choose` still docks the panel
  and shrinks it to one or two card columns; "6 items" and "R recipes / U uses / A bookmark" then
  wrap onto three lines each and the footer looks broken. It is legible, not wrong, and the fix is
  in `slotted-browser`'s status line rather than in the playground: either a narrow form of the
  hint text or a minimum width below which the panel hides. Seen in the playground screenshot at a
  1400 px window. **Phase 6**.
- **`wasm-opt` is still where most of the module goes.** Re-measured in the gap pass, package C:
  62.24 MiB out of `cargo build`, 54.39 after `wasm-bindgen`, 26.57 after `wasm-opt -Oz`. The
  profile is as small as the knobs make it (`opt-level = "z"`, fat LTO, one codegen unit,
  `panic = "abort"`, debuginfo stripped); `strip = "symbols"` is not one of them, because
  wasm-bindgen reads the symbol names and `wasm-opt` drops the name section afterwards anyway.
  Splitting the Bevy feature list by target was tried and saved ten kilobytes. The module is a
  whole Bevy renderer and the next real cut is Bevy's own feature surface. **Phase 8**.

## From the Phase 6 design

- **`Injections` has one wildcard (`slotted:any`) and no per-screen-type filter.** A mod that
  wants "every container screen but not settings pages" has no way to say so. Add an
  `InjectionTarget` enum when the second filter shape appears. **Phase 7**.

## From Phase 6

- **The side tab's header and content were spawned as siblings of the tab root, not its
  children.** `spawn_side_tab` used `SpawnCtx::spawn_node`, which parents to the *rail*, so a
  closed tab was a 44x2 line with two loose nodes beside it and an open tab's content sat in the
  rail's flow. Fixed here by parenting both to the root. What made it survive review is that every
  assertion the contract names (`side_tab_open`, the content's `Visibility`, the exclusion zone)
  is true of a detached content node too; only the screenshot showed it. A widget whose children
  matter should assert its own tree shape, not only its state. **Phase 7**: a
  `assert_widget_tree!`-style helper, or a debug check that a widget's spawned entities all
  descend from the root it returned.
- **`measure_side_tabs` writes the tab root's height every frame it disagrees.** A closed tab's
  content still takes part in layout (it is hidden and clipped, not removed), so the root cannot
  inherit its height and the system has to assert it. Removing the content from layout while
  closed (`Display::None`) would be cheaper and would drop the system, but it also drops the
  measurement `open_width` is computed from. **Phase 7**, with a measure-once cache.
- **A recording refuses to replay at another resolution or scale factor.** Contract 2.3 asked for
  a warning; a warning let a drifted replay report every input delivered and pass green while
  clicking the wrong nodes. `ReplayError` gained `Resolution` and `ScaleFactor`. The real fix is a
  recording that stores a locator beside each pointer position, so a replay can say "this click
  was meant for the sort button" and survive a layout change. **Phase 7**.
- **The built-in `hotbar` HUD layer now sets `hide_with_screen`.** A screen draws the player's
  hotbar row itself, so leaving the HUD one up put two hotbars on the glass. The contract listed
  `hide_with_screen` only against the crosshair; this is the deviation.
- **A tank carries a `BarText` readout child.** Contract 1.2 gave the tank a `SemanticLabel` and a
  tooltip and no visible reading, which left the machine screenshot with an unlabelled column of
  blue. The stacked three-line label (`tank_label`) is what fits a well one slot wide. A tank wide
  enough for the one-line form should use it; nothing measures that yet. **Phase 7**.
- **`open_screen` inside a Lua mod test closes whatever screen was open.** Two tests in one file
  each opening the same screen left two screens in the world, and every locator in the second test
  matched twice. `just test-mods examples/modded/mods` was red on exactly this. The headless driver
  now closes first; `LiveDriver` still means "the screen you have open", which is the contract's
  choice and unaffected.
- **`UiHarness::mod_layout` panics on a malformed `mod.toml`.** `test-mods` therefore aborts with a
  panic message rather than printing an error and exiting 1. The message names the file and the
  missing field, so it is actionable, and the exit code is still non-zero. Turning it into a
  reported error means a fallible `mod_layout`, which every existing caller would have to unwrap.
  **Phase 7**.
- **A tank warns once per unknown fluid id, through a marker component.** `UnknownFluidWarned`
  exists because the warning lives on the render path, which runs every frame. A crate-wide
  warn-once utility would be better than a component per case. **Phase 7**.

## From Phase 7 package C

- **The CPU bake and the GPU bake still author their *geometry* twice.** Narrowed in the gap pass,
  package C. What a cell draws is now one description, `shape::CellDraw`, read by both bakes, and
  the view angle (`shape::view_rotation`) and the cell fill (`shape::cell_fill`) moved beside the
  polygons that are authored to match them, so colour, orientation and size are decided once.
  What is left is the drawing itself: flat polygons in `shape.rs` against `Mesh` primitives in
  `gpu.rs`, so a seventh `ShapeKind` has to be added to both. Generating the silhouette by
  projecting the mesh would remove that, and it means moving `Mesh` construction into the
  no-renderer path and re-accepting every atlas snapshot. **Phase 8**.

## From the Phase 7 review

- **`Theme::missing_roles()` cannot see a missing child role.** A role falls back to its parent, so
  a theme that drops `tank.fill` still paints it, in the tank's own material, and completeness
  reports nothing. Only the cross-theme key-set comparison catches it, and nothing would catch a
  role all three themes dropped together. A `roles::ALL`-shaped check that distinguishes "resolved
  through a parent" from "declared" would. **Phase 8**, and documented in the guide meanwhile.
  Pinned by `slotted-theme/tests/review_adversarial.rs::a_theme_missing_a_role_is_caught_and_the_role_is_named`.
- **The machine's energy bar draws its readout over its own fill.** White text over the orange fill
  is legible but low contrast where the fill edge crosses a digit. A `bar.text` role that swaps
  colour over the filled part, or a readout beside the bar the way the tank's now is, would fix
  it. Cosmetic, present since Phase 6. **Phase 8**.

## From the gaps pass (package B)

- **A Lua test's pointer gesture now settles first.** `t.click`/`t.hover` in a mod's test file
  aimed at the node's rect as it stood, and the frame the fonts landed every label re-measured and
  the button moved out from under the pointer: `examples/machine`'s `sorter/sort.lua` clicked
  nothing and reported "control.lua logs the sort". `LuaTestDriver::act` settles before it
  resolves a locator. The deeper shape is that a gesture is delivered at a position while an
  assertion is written against a node, so any asynchronous relayout can separate them. A pointer
  action that re-reads the rect between the move and the press would be sturdier than settling
  first, and a recording that stores a locator beside each position (the Phase 6 entry above) is
  the same fix seen from the replay side.
- **`slotted_ui::Decorative` is honoured by the harness, not by the renderer.** It says "leave this
  node out of the settled fingerprint", which is a testing concept living on a rendering
  component. Nothing else reads it yet. If a second consumer appears (a screenshot that waits for
  motion to stop, say), it wants a name that is about the node rather than about the harness.

## From the luaur swap (ADR 0004)

- **There is no second script runtime any more, and luaur is 0.1.8 with one
  maintainer.** ADR 0004 deleted mlua deliberately, and the mitigation is the
  32-case conformance suite: it takes a `&mut dyn ScriptRuntime`, not a concrete
  type, so a replacement has a definition of done on day one. That is what made
  the piccolo removal a day's work. Worth revisiting only if luaur goes
  unmaintained or a case it cannot pass turns up; the thing to watch is the
  upstream repository, not this file.
- **luaur costs about 3.4 MiB of optimised wasm more than piccolo did.** Investigated in the gap
  pass, package C, and not attempted: `luaur-rt` compiles every chunk from text
  (`ChunkMode::Binary` is documented as unsupported) and depends on `luaur-compiler`
  unconditionally, so shipping a compiler-less VM needs two upstream changes; and the playground
  edits Lua in the browser, so it needs the compiler at runtime regardless. The design, and what
  would have to land upstream first, is in `docs/design/gaps-notes-C.md` section 6. The
  playground's module went from 23.05 MiB to 26.46 MiB. It is not the VM core
  (the Phase 0 spike measured luaur smaller than piccolo standalone) but the
  Luau lexer, parser and bytecode compiler a real embedding keeps live.
  Depending on `luaur-rt` instead of the `luaur` umbrella changed nothing, so
  the analyser was already being dropped. If the bundle ever becomes the binding
  constraint, the thing to try is precompiling each mod's chunk to bytecode at
  build time and shipping a VM without the compiler, which luaur's layering
  (`luaur-compiler` beside `luaur-vm`) would allow.
- **A trap inside the module's own animation frame is detected by a `window`
  error listener, not by the call wrapper.** `web/playground.js` guards every
  call it makes into the module, but Bevy drives its loop from a
  `requestAnimationFrame` the module registered itself, so the common case
  surfaces as an uncaught error and is matched by a string test
  (`/RuntimeError|unreachable|out of bounds|table index/i`). A page that threw an
  unrelated error whose text matched would restart for nothing. A narrower
  signal would be the module marking itself dead before it traps, which needs a
  hook luaur does not currently offer.
- **The restart rebuilds the whole app, not just the Lua state.** Everything the
  visitor did outside the chest — the camera, the console history, an open
  browser panel — is gone. Only the open menu's inventories are carried across.
  Restoring more would mean serialising more of the world, which is a bigger
  contract than a playground needs; a game that ships a browser build with
  untrusted mods will want to decide this for itself.
- **`install_error_reporter` is a process-wide `OnceLock`.** One host, one
  reporter, set at start-up and never replaced. That is the right shape for a
  page and the wrong one for a test that wants to assert on reports, which is
  why the adapter has exactly one test that installs one.
- **The reporter fires natively too, for errors that are also returned as
  `Err`.** A native host that installs one sees each error twice. Gating it on
  `cfg(target_arch = "wasm32")` inside the adapter would fix that and would also
  stop a native test from covering the path, so it is documented rather than
  gated.
