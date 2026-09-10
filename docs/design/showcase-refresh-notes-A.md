# Showcase refresh: package A notes

The Rust side of `docs/design/showcase-refresh-contract.md`: the Menus and
Dialogue scenes, Themes on a canvas of its own, the five exports, the native
tests, and the size line. Written after every gate was green.

## What was built

**The showcase crate** (`examples/showcase/`), shared with the web for real:

- `screens/main.screen.ron`: `showcase:main`, inheriting `slotted:main_menu`
  with a footer note, compiled in through `screens::MAIN_SCREEN_RON`.
- `dialogue/smith.dialogue.ron`: `showcase:smith`, the five nodes the
  contract names (`hello`, `ask` with `yes`/`no`/`secret` gated on
  `demo.found_key`, `yes` with a `{name}` argument, `secret`, `bye`).
  `src/dialogue.rs` registers it in `Dialogues` at `Startup` through
  `Dialogue::from_ron`, and answers `yes` with the `demo.showcase.thanks`
  toast.
- `locale/en-US.ftl`: the showcase's own catalogue (`demo-showcase-*`,
  `demo-smith*`, and the chest header's `demo-chest-*`), compiled in as
  `settings::SHOWCASE_EN_US`. `settings::install_locale` is now an exclusive
  system: it installs the base catalogue as before when nothing resolves the
  settings keys, then pushes the showcase catalogue as a *fallback* of the
  `Localization` port. A fallback survives a pack install and a mod reload,
  which only replace the primary, so the same keys resolve in the playground
  (locale from the pack install) and in the native examples.
- `src/menus.rs` gains the title screen (`main_kind`, `main_screen`,
  `about_injection`, `register_main_menu`, `open_main_menu`) and the leave
  confirm (`LEAVE_CONFIRM`, `confirm_leave`). The About button is an
  `Injection` at `buttons_end`, the way `examples/menus` does it.
- `src/lib.rs`: `ready: true` for Menus and Dialogue, `every_scene_is_ready`
  back to "every scene is real". Nothing else in the file changed.

**The playground** (`examples/web-playground/`):

- `Cargo.toml`: `menu` on the `slotted` dependency, and `menu` on
  `slotted-test` for the harness's toast, dialogue and confirm helpers.
- `src/settings_store.rs`: `PageSettings`, the `SettingsStore` over the bus.
  The bus is the one copy: `save` serialises `SavedSettings` to RON and
  publishes it (`Bus::set_settings`), `load` parses what the bus holds.
  `settings_ron()` reads the slot; `restore_settings(ron)` writes it
  synchronously *and* queues a request, so both the boot-time load at
  `PostStartup` and a running world are covered, and whichever runs second
  is a no-op.
- `src/scenes/menus.rs`: `MenusScene` and `MenusDemoPlugin` (the settings
  demo, the `MenuConfig`, the title and dialogue registrations, and the
  routing systems). `play` pops the title and opens `demo:chest` without the
  browser; `about` opens the page; `quit` on the pause asks to leave and
  pops back to the title, `quit` on the title asks with the danger button
  and an accepted confirm shows the title again with the `no_exit` toast.
  Nothing calls `AppExit`. `open(world, MenuScreen)` and
  `restore_settings(world, text)` are the world sides of the two exports.
- `src/scenes/dialogue.rs`: `DialogueScene` opens the furnace through the
  Machine scene's `open` (now `pub`, resources included) and starts the
  smith. `talk_again` restarts or says the smith is still talking.
  `log_transcript` writes `who: "dialogue"` lines, `smith: ...` on every
  node entered (the choice's prompt included) and `you: ...` on every
  choice. `shield_furnace_from_back` claims `Back` while the dialogue is the
  focus top; see the decisions below.
- `src/scenes/themes.rs`: `overlays_current` is gone; `enter` opens
  `demo:settings` through `showcase::settings::open_settings`, `leave` is
  the teardown. A theme swap with nothing open (after `Esc`) reopens the
  screen first.
- `src/showcase.rs`: `CanvasScene` and the overlay path are removed;
  `ActiveScene` is the one current scene. `MenuScreen` and the native twins
  of the five exports (`menu_open`, `settings_ron`, `restore_settings`,
  `talk_again`, `set_value`) do the validation and queue the request over
  any `Bus`; `bridge.rs` is one line over each, mapping the error to a
  `TypeError`. `set_value` parses a JSON bool, number or string without
  `serde_json` and coerces it to the kind the key's default has.
- `src/bus.rs`, `src/lib.rs`: the four new requests, the matching
  `SceneCommand`s, the plugin and the store in `build_app`.
- `current_screen() -> String`, the read-only export section 7 allows,
  asked for by package B: the kind on top of the stack, overlays included
  (`slotted:dialogue` over the furnace), empty when nothing is open.
  Published once a frame by `showcase::publish_screen` when the stack
  changed, the same push-pull as `current_scene`.

**Tests** (`tests/showcase.rs`, `tests/common/mod.rs`): the harness now
carries `MenusDemoPlugin` and a `PageSettings` store over the bus it hands
back (`harness_over`, `showcase_world_over` for the boot-time path). New
tests: `menus_scene_walks_the_stack_and_a_setting_reaches_the_bus`,
`menus_scene_quit_confirms_stay_in_the_tab_and_leave_is_clean`,
`dialogue_scene_talks_to_the_smith_over_the_furnace`,
`themes_scene_repaints_the_settings_screen_in_place`,
`saved_settings_round_trip_through_the_page_and_an_empty_restore_resets`;
`every_scene_enters_and_leaves_cleanly` walks nine and asserts no toast and
no dialogue follows a switch; `list_scenes_matches_the_page_and_the_registry`
expects nine real scenes. `tests/boot_order.rs` asserts the boot scene
*docks* the browser, which the skeleton's Chest already did; its old
assertion was the pre-refresh one.

## Decisions the contract left to me

- **The portrait is `assets/portraits/smith.png`**, the elder's file resized
  to 96 px, not `examples/web-playground/assets/`: nothing serves a
  playground-local directory (`xtask playground` copies the workspace
  `assets/`, `build.rs` bakes text only). Deviation from the ownership
  table, recorded in FOLLOWUPS.
- **`pause_on_menu` is on only while the Menus scene is active.** The
  `MenuConfig` has to be app-wide because `MenuPlugin` fills the title from
  it at `PostStartup`, but a pause screen appearing over the HUD scene on
  `Esc` is a screen that scene never asked for.
- **`menu_open("title")` pops back to the title** (or closes the chest and
  pushes one) rather than pushing a title over a pause. Pause and Settings
  push, unless already on top. Outside the Menus scene all three are a
  console line.
- **`Back` is claimed while the dialogue is the focus top.** `pop_on_back`
  pops the topmost non-overlay entry, which is the furnace under the
  dialogue; one `Esc` mid-conversation left the smith over an empty canvas.
  `Esc` on the history page still goes back. The library gap is in
  FOLLOWUPS.
- **`restore_settings("")`** forgets the saved settings, puts every declared
  key back to its default with `ValueStore::restore` (no `ValueChanged`, so
  nothing is saved straight back) and the bindings back to
  `DefaultBindings`; `settings_ron()` is then empty. A non-empty text is
  parsed, each value made to fit its rule, and restored the same way; text
  that is not settings is refused with nothing changed.
- **Transcript speaker names** are the resolved speaker, lowercased
  (`smith`); a line with no speaker would be `narrator`. The choice's prompt
  is logged as the smith's line when the node is entered.
- **The chest header warning is fixed by the same mechanism.**
  `demo.chest.title`, `demo.chest.capacity` and `demo.chest.inventory` are
  in the showcase catalogue, so "no catalogue defines this locale key" is
  gone on every chest the showcase opens; `fill_labels` still overwrites
  them every frame.
- **Restart into Menus or Dialogue** re-runs `enter` from the top, as the
  contract says. A snapshot taken with the chest open behind the title
  finds no menu on the fresh title and says so after its retry budget; the
  Dialogue scene's furnace takes it. FOLLOWUPS has the snapshot field that
  would close this.

## Size

`cargo run -p xtask -- playground`, `wasm-opt -O1`, 2026-09-10:

| | wasm | bindgen | wasm-opt |
|---|---|---|---|
| before (skeleton) | | | 40.36 MiB |
| after | 50.26 MiB | 45.32 MiB | 40.51 MiB |

`menu` in costs 0.15 MiB against a 3 MiB budget.

## Gates

- `cargo test -p web-playground -p showcase -p menus -p chest`: green
  (playground 23 unit, 28 showcase, plus boot_order, plumbing, restart,
  review_adversarial, smoke_page, tests_tab; showcase 9; menus 8; chest 13).
- `cargo clippy -p web-playground -p showcase --all-targets`: no warnings.
- `cargo check -p web-playground --target wasm32-unknown-unknown`: clean.
- `cargo fmt -p web-playground -p showcase --check`: clean.
- `cargo run -p xtask -- playground`: succeeded, size above.
- Not run, per the brief: the browser smoke, `just shot-showcase`, the dev
  server.

## Deferred

`docs/FOLLOWUPS.md`, "Showcase refresh, 2026-09-10": the portrait's home,
`Back` under a focus overlay, `ChestBinding` outliving its scene, raw markup
in the transcript, the Title button's pop-back, and the restart's stack.

## Files changed

Modified:

- `examples/showcase/src/lib.rs` (`ready` flips and the test only)
- `examples/showcase/src/menus.rs`
- `examples/showcase/src/screens.rs`
- `examples/showcase/src/settings.rs`
- `examples/web-playground/Cargo.toml`
- `examples/web-playground/src/bridge.rs`
- `examples/web-playground/src/bus.rs`
- `examples/web-playground/src/lib.rs`
- `examples/web-playground/src/scenes.rs`
- `examples/web-playground/src/scenes/machine.rs`
- `examples/web-playground/src/scenes/themes.rs`
- `examples/web-playground/src/showcase.rs`
- `examples/web-playground/tests/boot_order.rs`
- `examples/web-playground/tests/common/mod.rs`
- `examples/web-playground/tests/showcase.rs`
- `docs/FOLLOWUPS.md`

Added:

- `assets/portraits/smith.png`
- `examples/showcase/dialogue/smith.dialogue.ron`
- `examples/showcase/locale/en-US.ftl`
- `examples/showcase/screens/main.screen.ron`
- `examples/showcase/src/dialogue.rs`
- `examples/web-playground/src/scenes/dialogue.rs`
- `examples/web-playground/src/scenes/menus.rs`
- `examples/web-playground/src/settings_store.rs`
- `docs/design/showcase-refresh-notes-A.md`

Untouched: `examples/web-playground/build.rs` (the locale and the screens
reach the module through `include_str!` in the showcase crate; the portrait
through the copied `assets/`), `web/**`, `tests/smoke.mjs`,
`tests/smoke_page.rs`, `shots/`, `docs/guide/**`, README.
