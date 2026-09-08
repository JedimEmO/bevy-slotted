# Menus M2 notes: package D (facade, examples, harness, docs)

What D built on top of A, B and C, the decisions the contract left open,
what departs from it, the tests, and what is left. Contract:
`docs/design/menus-m2-contract.md`, section 5 and the "Done when" in
section 6. Every deferral from the four packages is under "Menus M2" in
`docs/FOLLOWUPS.md`.

## 1. What is built

### The `Menu` binding and the shared Escape (2.6, moved to D)

`UiBindings::default()` binds `Menu` to `Escape` and `Start`; Tab is bound
to nothing. Two things had to change with it in `slotted-menu`'s
`pause_on_menu` (B's file), both found by the first test that pressed
Escape rather than `h.action(Menu)` with Tab:

- **The push claims `Back` too.** The press that pauses is also `Back`, and
  `pop_on_back` runs later in `Navigate` after the push command has
  applied, so it popped the pause the frame it opened. Claiming `Back` on
  the push is the fix; with nothing open `Back` had nothing else to do.
- **The pop defers to `pop_on_back`.** With the pause on top the same press
  arrives as `Back` and `Menu`; popping in `pause_on_menu` and again in
  `pop_on_back` took the screen under the pause with it. When a `Back`
  event is in the frame and the pause's back policy is `pop`, the crate
  only claims `Menu` and leaves the pop to whoever owns `Back` (a select
  popup would close instead, which is right). `Start` on a pad is `Menu`
  alone and still pops.

`templates.rs::escape_on_the_pause_pops_only_the_pause_and_start_pops_it_too`
pins both, over a page and over nothing. The harness's Tab test passes again
because Tab opens nothing.

### Directional navigation scoped to the screen (not in the contract)

The flow test's first `Down` from the settings' resolution select over the
pause went nowhere. Bevy's `AutoDirectionalNavigator` is z-agnostic (its own
docs say so): the pause's Settings button, centred under the modal, scored
better than the slider, `enforce_focus_scope` bounced the focus back, and
the net effect was nothing. `nav.rs::directional_nav_actions` now does what
Bevy's navigator does in two steps of its own: a manual
`DirectionalNavigationMap` edge first, then `find_best_candidate` (Bevy's
public scorer) over the `AutoDirectionalNavigation` nodes under the focused
node's `ScreenRoot`, same camera, visible, non-empty. `focusable_area`
reproduces Bevy's bounds computation, including the private
`get_rotated_bounds`. Every M0, M1 and M2 nav test is unchanged and green;
the half-plane follow-up (`min_alignment_factor`) stays open, this only
narrows the candidate set.

### The chest example

`ChestMenuPlugin` (was `ChestSettingsPlugin`): `SettingsDemoPlugin`,
`showcase::menus::menu_config` pointing the pause's Settings at
`demo:settings`, and `QuitPlugin`. `open_settings_on_menu` is gone from the
showcase; `open_settings` stays for the `--settings` shot. `--pause` pops
the chest, waits for the stack to empty, pushes `MenuConfig::pause_kind`
and forces `InputMode::Keyboard` every frame from then on (the real mouse
over the window flipped it back and hid the ring and the hint bar in the
first captures); `--confirm` then calls `confirm_quit` half a second before
the shot. Six shots: `chest-pause{,-paper,-neon}.png`,
`chest-confirm{,-paper,-neon}.png`. `just shot-chest` and
`shot-chest-themes` capture them.

### `showcase::menus`

Shared by both examples: `menu_config(title, version)`, `confirm_quit`
(`QUIT_CONFIRM`, danger), `quit_on_menu_choice`, `exit_on_quit_confirmed`,
`QuitPlugin`. The demo strings (`demo-quit-*`, `demo-menus-*`) are in
`assets/locale/en-US.ftl`, which `SettingsDemoPlugin::install_locale`
compiles in.

### `examples/menus`

`src/lib.rs` is the headless part: `MAIN_SCREEN` (`menus:main`, inheriting
`slotted:main_menu` with a footer caption appended), `about_injection` (an
`Injection` at `buttons_end` carrying `menu: about`), `register_main_menu`,
`open_main_menu`, `confirm_leave` (`LEAVE_CONFIRM`, danger),
`route_choices` (`play` pops the title and opens the chest; `about` opens a
page; `quit` on the pause confirms leaving, on the title confirms exiting),
`leave_or_exit` (an accepted leave pops the pause, clears
`ChestBinding::def` so `E` stays quiet on the title, pushes the title),
`toast_on_quick_stack` (an observer on `MenuAction` for
`ToolbarAction::QuickStack`), `MenusDemoPlugin`, `settings_path`
(`temp_dir/slotted-menus/settings.ron`). `src/main.rs` is the chest's
window, backdrop and screenshot plumbing with `--main`, `--pause`,
`--confirm`, `--page`, `--toast` and `--theme`; the plugin inserts no
`SettingsStorage`, `main.rs` inserts the file store and the test a
`MemorySettings`. `examples/menus/assets` is a symlink to the workspace
assets like the chest's. Seven shots under `shots/`, a `README.md`,
`just run-menus`, `just shot-menus`.

### Harness

`crates/slotted-test/src/menu.rs`, behind a `menu` feature in the crate's
defaults that also turns the facade's `menu` on: `toasts()` (the
`ToastColumn`'s children carrying `Toast`, oldest first),
`hint_entries(bar)`, `confirm_accept()` / `confirm_cancel()` (the top
entry's root must carry `PendingConfirm`; the button is found by test id
within that root, `accept` falling back to `accept_danger`; `activate` and
one frame), `menu_choices()` (drains a `MenuChoiceLog` the
`MenuChoiceRecorder` plugin fills in `Last`; the builder adds the plugin
after the user's, and it registers the message itself so an app without
`MenuPlugin` still builds).

### Docs

`docs/guide/menus.md` (new), `input.md` (`Menu` is Escape and why, the
glyph sets, the scoped navigator), `screens.md` (injected node sizing,
`set_text` / `set_tag` / `remove_node`), `themes.md` (101 roles, the menu
group, inverse labels and the accent-fill contrast rule, the hint glyph
kind), `values.md` (snapshot and restore and why restore bypasses rules),
`testing.md` (`stack_top`, the menu helpers), `README.md` (the link),
`PLAN.md` (v2.3, M2 done, `examples/menus` in section 5), `CHANGELOG.md`
("Menus M2" above M1: every public change of A, B, C and D), `FOLLOWUPS.md`
("Menus M2" with every deferral from the three notes files and D's; the six
M1 entries M2 closed are deleted and named in the M1 header, following the
file's convention).

## 2. Decisions the contract left open

### 2.1 The pause's Quit returns to the title

The contract says Quit "confirms with a danger button and then exits". In
the menus example the title's Quit does exactly that; the pause's Quit
confirms ("Leave the game?") and goes back to the title, because a game
with a title screen never exits from its pause, and because one `quit` id
meaning two things by `MenuChoice::screen` is what that field is for. The
chest example has no title and its pause's Quit exits. Contract 5 says so
now (v1.2).

### 2.2 `buttons_end` moved inside the button column, and the menu is a panel

The template put `buttons_end` after the button column, so an injected
button sat below it with the column's gap and its own width. Inside the
column it lines up with the three above it. An injected node is a child of
the anchor's plain row, so it still sizes to its content; the About button
sits in a `width: "100%"` column panel to take the column's width, which is
what a mod would write too (documented in `screens.md`).

The main menu's root was an invisible column, which read well in glass and
neon and not at all in paper: ink on a dark scene. It is a `panel` placed at
the left now (`min_width: 340`, `offset: (48, 0)`), which is what the three
shots show. The contract's 3.1 row is amended.

### 2.3 Danger buttons are solid

The first confirm shots had an unreadable Quit in paper and neon and a
faint one in glass: `button.danger` was a translucent or sheet fill and
B's label inversion painted `control.label.inverse` (the sheet colour, the
charcoal) on it. The inversion is by role and is right for an accent fill,
so the fix is the fill: `$danger` solid in all three themes, with hover,
focus and pressed shades. `themes.md` states the rule (an accent fill has to
contrast with the inverse colour).

### 2.4 `LocaleTable` no longer warns

The first windowed run logged "no locale layer defines this key" for every
`slotted.menu.*` key the fallback then answered, once per key. The table is
the primary of a chain now, so a miss there is routine; the `Localizer` impl
resolves without warning and `resolve_or_warn` and the `missing` set are
gone. A key nothing in the chain defines is still reported once by the
`LocText` that draws it. The browser's direct `Localization::resolve`
callers lose their once-per-key warning; noted in the follow-ups with the
right place for it.

### 2.5 The flow test's store is a `MemorySettings`

`MenusDemoPlugin` inserts no `SettingsStorage`; `main.rs` inserts the file
store and the test inserts a memory one after the plugin, and asserts the
save after the slider step. A file under `temp_dir` in a test would leak
state between runs.

## 3. Deviations from the contract and from the notes

- **`nav.rs` changed** (section 1): a foundation fix for the flow the
  contract requires, in the same spirit as M1 D's `AutoDirectionalNavigation`
  and `Display::None` fixes.
- **`pause_on_menu` changed** (B's file): the two claims above.
- **`main_menu.screen.ron` changed** (B's file): the anchor and the root.
- **The three theme files changed** (B's materials): the danger states.
- **`slotted-packs/src/locale.rs` changed** (A's crate): the warning.
- **The harness's `menu` feature is on by default**, so `slotted-test` pulls
  `slotted-menu` in; a consumer that wants neither turns it off with
  `default-features = false`.
- **`menu_choices()` rather than `menu_actions()`**, since the message is
  `MenuChoice` (skeleton 1.3).
- **`examples/menus` depends on the `chest` crate**, not only on the
  showcase, for `load_registries`, `assets_dir` and `ChestDemoPlugin`; the
  showcase has no filesystem.

## 4. Tests

The workspace is at **1202 passing** (`just test`), 19 added by D:

- `crates/slotted-menu/tests/templates.rs` (1): the shared Escape over a
  page and over nothing, and Start.
- `examples/chest/tests/ui.rs` (2, replacing the Tab test): Tab opens
  nothing, Escape pops the chest, pauses, the pause's Settings opens
  `demo:settings` with focus on its first tab and four sliders, Escape pops
  the settings and then the pause, `E` reopens the chest, conservation; Quit
  on the pause pushes a danger confirm titled `Quit?` with focus on Cancel,
  `confirm_cancel` returns to the pause with no exit, `confirm_accept`
  writes `AppExit::Success`.
- `examples/menus/src/lib.rs` (1): the screen file parses and inherits.
- `examples/menus/tests/flow.rs` (4): the whole flow by pad (title →
  Play → chest with 63 slots → East → Start → pause → Down, South →
  settings over the pause with the hint bar reading Adjust, Adjust, Close,
  Previous tab, Next tab on the slider → Right steps the UI scale to 1.25
  and the memory store holds it → East → pause with focus restored → Quit →
  the leave confirm on Cancel → East cancels → South, Right, South leaves →
  title → Quit → the exit confirm → `confirm_accept` → `AppExit`), with the
  `MenuChoice`s and their screens asserted at each step; the injected About
  button under the button column, aligned with Play, reached by three Downs,
  opening the page with focus on Done, East returning focus to About; a
  quick stack on the chest's rail showing one success toast with no stack
  or focus change; nine tree snapshots (main menu, pause, confirm in three
  themes).
- `crates/slotted-test` (the `harness.rs` Tab test): passes again.

Gates, all green: `just fmt-check`, `just lint`, `just test` (1202),
`just doc`, `just gen-docs-check`, `just deny`, `just server-check`, `cargo
check -p slotted-menu -p slotted-ui -p slotted-test --target
wasm32-unknown-unknown`, and `slotted-test` on wasm with
`--no-default-features --features script`.

## 5. Section 6, bullet by bullet

- **`just ci` green on native and wasm; `slotted-menu` checks on wasm:**
  pass, as above.
- **A game adds `SlottedPlugins::default()`, inserts `Settings { spec }` and
  `MenuConfig`, and gets a main menu, pause, settings with persistence,
  confirm, toast and page with no screen file of its own:** pass. The chest
  example is exactly that (its one screen file is the chest's); the menus
  example adds one file only to show inheriting. A game that registers
  `slotted:pause` first gets its own pause: B's
  `the_templates_register_at_startup_unless_a_game_registered_the_kind_first`.
- **`examples/menus` runs the whole flow by gamepad headless and the
  reference shots exist for three themes:** pass; `tests/flow.rs`, and
  `shots/menus-main{,-paper,-neon}.png` plus glass for pause, confirm, page
  and toast (the contract asked for three themes of `--main` and at least
  glass for the rest).
- **FOLLOWUPS M1 items 7, 8, 9, 12, 20, 21 closed:** pass; deleted from the
  M1 list and named in its header.
