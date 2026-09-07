# Menus M1 notes: package D (harness, demo screen, docs)

What D built on top of A, B and C, the decisions the contract left open,
what departs from it, the tests, and what is left. Contract:
`docs/design/menus-m1-contract.md`, section 5 and the "Done when" in
section 6. The follow-ups from all four packages are in `docs/FOLLOWUPS.md`
under "Menus M1".

## 1. What is built

### `slotted-test` (5, harness)

- **Values.** `value(key) -> Option<Value>` reads the store. `set_value(key,
  value)` writes one `SetValue` with no source and steps a frame, so the
  key's rule and every guard apply exactly as they do to a control's write;
  a test that wants to bypass them seeds `ValueStore::insert` itself.
- **Controls.** `drag_slider(row, fraction)` presses on the track where the
  thumb is, moves to the midpoint and then to `fraction` (two moves, so a
  drag to where the thumb already sits is still a drag Bevy's picking
  recognises), and releases; every move is a write with the slider as
  source. `select_option(row, id)` clicks the row (which opens the popup),
  settles, and clicks the `SelectOptionNode` of `id`; it panics naming the
  options when `id` is not one. `type_into(row, text)` is `set_focus`,
  `Accept`, `type_text`, Enter. `switch_tab(tabs, id)` clicks the
  `TabButton` of `id` and settles. `capture_key(row, key)` is `set_focus`,
  `Accept`, one frame, the key, one frame. All take the control's row (the
  node carrying its `*State`) and panic when it is not one.
- **`by::control(kind)`**: a `Locator` over `WidgetNode`; a bare name is
  `slotted:<name>`, a namespaced id is taken as written. `by::widget_kind`
  stays for a `WidgetKind` in hand.
- **Lua.** `TestOp::{Value { key }, SetValue { key, value }, TypeInto { loc,
  text }}` in `slotted-script`; `slotted.test.value`, `set_value` and
  `type_into` in the prelude, documented with `---` blocks so `gen-docs`
  picks them up. `ops::query` answers `Value`; `ops::set_value`,
  `to_store_value` and `to_lua_value` convert between `slotted_model::Value`
  and `slotted_ui::Value` (a list or a table is the test's failure: "set_value
  takes a boolean, a number or a string, not a list"). The harness driver
  steps a frame after the write and `type_into` settles before resolving the
  row (layout must stand still before a focus-and-type); the live driver
  writes on one frame and finishes on the next, and spreads `type_into` over
  frames (focus, Accept down, up, one press-and-release per character, Enter
  down, up), because a press and a release in one frame never show up as
  `just_pressed`.

### The demo screen (5)

`assets/screens/demo_settings.screen.ron`, `demo:settings`, a modal with
`initial_focus: tabs` (the tabs node's first focusable is the first tab
button). A 720 px panel: the title, three tabs bound to `settings.tab`, a
separator, and a footer row with a 16 px `image` (`icons/hint_info.png`), a
`rich_text` hint in `muted` (`{key:back}` and `{key:tab_next}`), a `spacer`,
a plain `Reset` button and a `slotted:close` `Done` button. Display: `select`
(three resolutions), `slider` (UI scale 0.5 to 3.0 by 0.25, `{value:.2}×`),
`toggle` (reduced motion, switch), `radio_group` (colour mode, three
segments). Audio: a `scroll` 220 px tall with twelve rows: three headings,
three sliders (0 to 100 by 5, `{value}%`), a select (output device), three
toggles (a switch and two checkboxes) and two separators. Controls: four
`key_binding` rows (accept, back, tab_prev, tab_next; keyboard), a
separator, a `text_field` (player name, `max_len: 16`, placeholder). Every
control binds a `settings.*` key. The strings are in
`assets/locale/en-US.ftl`; the footer's placeholders are Fluent string
literals, `{"{key:back}"}`, because a bare `{` is Fluent's own placeable
syntax and a bare `{key:back}` drops the whole file as junk (2.1).

`examples/showcase/src/settings.rs`: `SETTINGS`, `SCREEN_PATH`,
`MAX_UI_SCALE` (2.0), `screen()` from the compiled-in
`screens::SETTINGS_SCREEN_RON`, `defaults()` (thirteen keys), `rules()`
(ranges for the four sliders, option lists for the two selects, the radio
group and the tabs), `UiScaleGuard` (refuses `settings.ui_scale` above 2.0
with a reason naming both numbers), `SettingsDemoPlugin` (`Startup`:
`register_screen` unless registered, `seed_store` leaving a key a game
already seeded alone, `install_locale`; an observer `on_reset` that writes
every default but the tab back through `SetValue` when the `reset` button
activates), `open_settings(commands)` (a push unless it is already on top)
and `open_settings_on_menu` (an unclaimed fresh `Menu` opens and claims).
`install_locale` compiles `assets/locale/en-US.ftl` in and installs it as
`Locales` and the `Localization` port when nothing already resolves
`demo.settings.title` (2.2).

`examples/chest`: `ChestSettingsPlugin` adds `SettingsDemoPlugin` and runs
`open_settings_on_menu` in `SlottedUiSet::Input` after `UiActionEmit`;
`main.rs` adds it and loads the settings file through `ScreenAssets` beside
the chest's, so it hot-reloads. `Tab` opens the settings over the chest,
`Esc` pops it.

### Docs (5)

`docs/guide/screens.md` (every M1 node, the grown `text` and `button`, the
value-control preamble, the new roles), new `rich-text.md` and `values.md`,
`themes.md` (typography, control sizes, the 89 roles and the state
fallback), `input.md` (focused actions, claims, capture, text entry),
`testing.md` (the helpers, `by::control`, the Lua ops, the asset-root
note), `README.md`, `PLAN.md` (v2.2, M1 landed, 1142 tests), `CHANGELOG.md`
(the M1 block above M0's: Added, Changed, Removed), `FOLLOWUPS.md` ("Menus
M1": every deferral from A, B and C plus D's own; the three M0 entries M1
closed are deleted). `docs/guide/api/lua.md` and `slotted.d.luau`
regenerated.

## 2. Decisions the contract left open

### 2.1 Fluent and the braces

Package A tested rich strings through a fake localiser, and the contract's
"`.ftl` files may contain tags" is true of the square brackets only. Fluent
parses `{key:back}` as a placeable and fails the message, and
`FluentResource::try_new` fails the resource, which `push_layer` turns into
"base locale file is not valid Fluent" and an empty catalogue. The footer
writes `{"{key:back}"}`; `rich-text.md` says so and `FOLLOWUPS.md` has the
friendlier route (a Fluent function). A unit test in `settings.rs` pins that
the shipped file parses and hands the braces through.

### 2.2 Where the base catalogue comes from

`slotted-packs` reads `assets/locale/<lang>.ftl` only when a pack set
installs, and the chest example installs none, which is why its labels are
filled by hand. The settings labels and the footer are Fluent strings, so
the showcase's settings module compiles the base file in and installs it at
`Startup` when the current `Localization` does not resolve the settings
title. A pack install replaces both resources with the layered catalogue,
which reads the same file from disk first. The general fix (load the base
file without a pack set) is in the follow-ups.

### 2.3 The theme actually loads in the settings tests

`.theme("glass")` in `slotted-test`'s own tests never loaded anything:
Bevy resolves the asset root against the crate, which has no `assets/`, and
a failed load is not a loading one, so `settle` does not wait for it. The
settings tests hand `SlottedPlugins::headless().set(AssetPlugin {
file_path })` the workspace directory and wait for `Assets<Theme>` to hold
the active handle, then assert every display-tab row is the theme's
`control_height` (44/36/40), which is the first assertion in this crate that
would notice a theme not loading. The M0 tests are left as they are; the
follow-up names the builder option that would make this the default.

### 2.4 The walk's shape is the navigator's

"Reaches every control on every tab and back" is read literally: from the
first tab, `Down` four times through the display page, `RightTrigger` into
the audio page (focus moves to its first control, since it sat in the old
page), `Down` six times to the last row with the scroll panel following,
`RightTrigger` into controls, `Down` four times to the field, `Down` to the
footer (`Done`, the nearest button to a full-width field), `Left` to
`Reset`, `RightTrigger` wraps to display (focus outside the pages stays),
`Up` four times to the first row, `Up` to the tab bar (the audio tab, whose
centre is nearest the row's), `Left` to the first tab. Each step asserts
focus, the ring's target and that the ring's rect frames the target.

### 2.5 The reset button is a plain `Activate` source

The contract wants "a rebound `Accept` key activates a button", observed as
`Activate`. A `slotted:close` button pops the screen, which is a fine second
assertion but a poor first one; the demo therefore has a plain `Reset`
button whose `Activate` the showcase module answers by writing the defaults
back, and the test observes both the `Activate` and the values. The `Done`
button then shows the same key popping the screen.

## 3. Deviations from the contract and from the notes

- **Two fixes outside D's files**, both needed by section 6's "every control
  reachable by gamepad":
  - `widgets/controls.rs::spawn_control_row` and `widgets.rs::spawn_button`
    insert `AutoDirectionalNavigation`. Without it `Down` from a tab button
    did nothing and the walk skipped every B row; only C's text field, list
    rows and tab buttons were in Bevy's graph. `focus_ring.rs`'s
    unresolved-nav-link test changed with it: a second `Left` from the first
    chest slot now lands on the `done` button below the grids, because
    Bevy's navigator accepts any candidate whose centre is in the pressed
    half-plane (the follow-up on `min_alignment_factor`); the test asserts
    that, then removes the button from the graph and asserts focus stays.
  - `widgets/tabs.rs` gives a hidden page `Display::None` beside
    `Visibility::Hidden` (`hide_page` / `show_page`). Hidden pages kept
    their height in the column, so the settings panel was 853 px tall in a
    720 px window and the footer was off screen; a click on `Reset` hit
    nothing. C's container tests are unchanged and green.
- **No rect snapshot.** The contract's "harness rect snapshot" names nothing
  that exists; the three snapshots are tree snapshots (`settings_tree_glass`,
  `_paper`, `_neon`), identical across themes by design. Recorded as a
  follow-up.
- **`slotted-test` has a dev-dependency on `showcase`**, so the settings
  tests drive the module the chest example uses rather than a copy of its
  seed. The path dependency is stripped on publish.
- **`drag_slider` makes two moves**, not one, and starts at the thumb rather
  than pressing at `fraction` (the contract does not say which; a press at
  the target would be a jump, not a drag).
- **`type_into` settles first** on the Lua harness driver, as `click` does,
  so a row that is still moving is focused where it is.

## 4. Tests

Twenty-one added; the workspace is at **1142 passing** (`just test`).

- `crates/slotted-test/tests/settings.rs` (17): opens in glass, paper and
  neon with focus on the first tab, the twelve control kinds counted once
  each through `by::control`, every display row the theme's
  `control_height`, the title resolved; three tree snapshots; the gamepad
  walk above (every control on every tab, the ring's target and rect each
  step, the scroll panel bringing the last audio row into view, `settings.tab`
  written by the trigger); a toggle writes on `Accept` and repaints from
  `set_value`; the UI-scale slider writes 1.5 from a drag, repaints 1.75
  from the store, and a `set_value` of 0.1 clamps to 0.5 on both sides; the
  audio sliders drag, seed and step (`Left` claimed, focus stays); the
  select writes from its popup, repaints from the store, and refuses
  `640x480` with a reason naming it; the radio group writes on `Right` and
  repaints; the text field is seeded `Steve`, `type_into` writes `Ada`,
  leaves editing with focus on the row and the screen still open, and a
  store write repaints `Grace`; `set_value("settings.tab")` switches the tab
  and `capture_key` rebinds `TabNext` to `N`, which then switches; `Reset`
  activates once and every default comes back through the store; the guard
  refuses a drag to the end (every refusal names the slider, the reason
  says "more than 2", the store never held more, the slider paints what
  the store kept) and refuses an arrow step past 2.0; `Back` closes the
  popup with the stack intact and a second `Back` pops the screen; the
  footer reads `Esc` and `E` in keyboard mode, `B` and the trigger's glyph
  in gamepad mode, flips on a real pad press, and reads `X` after `Back` is
  rebound; a rebound `Accept` (`K`) activates `Reset` without popping and
  activates `Done`, which pops.
- `crates/slotted-test/tests/lua_tests.rs` (1): a Lua test reads the seed,
  `nil` for an unknown key, sees 1.6 snap to 1.5 and 2.75 refused, flips a
  toggle, walks two tabs with `action("tab_next")`, types `Ada` into the
  field and reads the resolved title; a table as a value fails with the
  message above.
- `examples/chest/tests/ui.rs` (1): `Tab` pushes `demo:settings` over the
  chest, the chest stays visible, focus is the modal's first tab, the four
  sliders are up, a second `Tab` does nothing, `Escape` pops it and focus is
  back on a chest slot with the chest still open and every item accounted
  for.
- `examples/showcase/src/settings.rs` (2): the shipped catalogue parses and
  hands the braces through; the guard's edge and every option default is in
  its rule's list. `screens.rs`'s parse test covers the third file.

Gates run, all green: `just fmt-check`, `just lint`, `just test` (1142),
`just doc` (the rustdoc error B reported did not reproduce; nothing was
changed for it), `just gen-docs` then `just gen-docs-check`, `cargo check
-p slotted-ui -p slotted-browser -p slotted-test --target
wasm32-unknown-unknown` and the same for `slotted-test` with
`--no-default-features --features script`. Contract 6's grep: `KeyCode::`
under `widgets/` is `key_binding.rs:196` only (`icon_button.rs` reads
`ButtonInput<KeyCode>` for a click's shift modifier, which the grep does not
name and the follow-ups do).

## 5. Section 6, bullet by bullet

- `just ci` green on native and wasm: `fmt-check`, `lint`, `test`,
  `gen-docs-check`, `deny` and `server-check` all pass, and so do the wasm
  checks of `slotted-ui`, `slotted-browser` and `slotted-test`.
- `demo:settings` opens headless in three themes: yes, with the theme asset
  loaded (2.3).
- Every control reachable by gamepad from `initial_focus`: yes, after the
  two fixes in section 3.
- Every control writes and reads through the store: yes, one test per kind.
- A guard refusal snaps a slider back: yes, on a drag and on an arrow.
- `Back` from a select popup closes the popup and a second `Back` pops the
  screen: yes.
- A rebound `Accept` key activates a button: yes, a plain one and a close.
- `{key:accept}` reads `Enter` on the keyboard and `A` on a pad: the demo's
  footer uses `{key:back}` and `{key:tab_next}` (contract 5), asserted as
  `Esc` / `B` and `E` / the trigger's glyph; `{key:accept}` itself is A's
  `rich_text.rs` test (`Enter`, then `A`).
- No control reads `ButtonInput<KeyCode>`; the grep shows only
  `key_binding.rs`: the grep passes; the icon button's modifier read is the
  one `ButtonInput<KeyCode>` left, noted above.
