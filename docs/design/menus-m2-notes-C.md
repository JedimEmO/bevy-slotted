# Menus M2 package C notes: settings

What package C built for `docs/design/menus-m2-contract.md` section 4 (spec,
generated screen, persistence, reset, key conflicts, the showcase demo), the
decisions the contract left open, what changed outside the package's own
files, the tests, and what is deferred.

## 1. What was built

### `crates/slotted-menu/src/settings.rs`

- **`SettingsRow`** gained `key()`, `default_value()`, `rule()`, `test_id()`
  and `node()`. A value row's node is the M1 control bound to its key with
  `test_id` = the key (`settings.ui_scale`); a binding row's id is
  `bind.<device>.<action>` (`bind.keyboard.accept`); a `Custom` node keeps
  its own id; headings and separators carry none. A slider's rule is
  `min`/`max`/`step` (`step: 0` becomes `None`, so the store does not snap
  what the slider draws as one-percent steps); a select's and a radio's rule
  is its option ids; a toggle and a text field imply no rule.
- **`SettingsTab`** gained `start_anchor()`, `end_anchor()` and `page()`: a
  `scroll` (`column`, `gap: 1`, `max_height: 70%`, `grow: 1`, scrollbar)
  with id `settings.<tab>.page`, the `settings.<tab>.start` anchor, the rows,
  the `settings.<tab>.end` anchor.
- **`SettingsSpec::screen_def()`** parses the embedded `slotted:settings`
  frame and hands it to **`screen_def_over(&frame)`**, which clones the
  frame's root shape, replaces its children with the one `tabs` node
  (`test_id: settings.tabs`, `bind: <kind path>.tab`, one `TabDef` and one
  page per tab) and sets `inherits`, `initial_focus: settings.tabs` and a
  default `Presentation` (which the merge reads as "keep the frame's"). See
  2.1 for why the root shape is copied. `TABS_ID`, `rows()` and
  `tab_by_id()` are small helpers on top.
- **`defaults()`** is the value rows' keys with their defaults; **`rules()`**
  is the rows' rules plus the tab key's option list (the tab ids); `keys()`
  is unchanged (the defaults' keys). The tab key is not a default: see 2.2.
- **`apply_settings`** (exclusive, `PostStartup` after the templates, and
  `Update` while `Settings` exists and `SettingsApplied` does not): returns
  when already applied or without `Settings`; registers the generated screen
  over the *registered* `slotted:settings` frame (a game's own, or the
  embedded one) unless the kind is registered; loads `SettingsStorage`;
  seeds every declared key with, in order of precedence, the saved value, a
  value the game already put in the store, the default, through
  `ValueStore::restore` (one version bump, no events); seeds the tab key
  with the first tab when absent; inserts every rule; replaces `UiBindings`
  with the saved ones when the file has them; inserts `SettingsApplied`.
  Saved keys the spec does not declare are never loaded.
- **`save_settings`** (`Last`): with `Settings`, `SettingsStorage` and
  `SettingsApplied` present, saves when a `ValueChanged` names a declared
  key, on any `BindingChanged`, or on `AppExit`. The tab key and undeclared
  keys do not trigger a save. **`saved_settings(spec, store, bindings)`**
  builds the `SavedSettings` (declared keys the store holds, `bindings:
  Some`). The readers are drained when the gate fails so nothing piles up.
- **`reset_on_menu_choice`** (`Update`) turns a `MenuChoice` with id `reset`
  into a `SettingsReset`; **`reset_settings`** (chained after it) writes
  every default as a `SetValue` with no source and sets `UiBindings` to the
  default. Rules and guards apply as to any write, which is the test's
  locked key staying locked.
- **`resolve_binding_conflicts`** (`Render`, the frame the capture lands):
  per `BindingChanged`, the action's new first key (or button) is removed
  from every other action's list on the same device, and one
  `slotted.menu.binding_moved` toast per moved action carries `action` = the
  action's data name (`tab_next`). The other device is untouched. Closes
  FOLLOWUPS M1 item 9.
- **`FileSettings`** load and save mirror `FileHudLayout`: a missing file is
  `None`, unreadable or non-RON is a warning and `None`, save creates the
  parent directory and warns on failure; wasm stubs unchanged.
- `build` registers `apply_settings` (run-if gated), the reset pair, and the
  conflict system in `Update`, `save_settings` in `Last`.

### `examples/showcase/src/settings.rs`

Rebuilt on `SettingsSpec`: `spec()` (display, audio, controls tabs, the
thirteen M1 keys minus the tab key, which the spec derives as
`settings.tab`), `defaults()` as `spec().defaults()`, the same `UiScaleGuard`
and `MAX_UI_SCALE`, `SettingsDemoPlugin` (adds `MenuPlugin` when the app has
none, inserts `Settings { spec }`, `Startup`: `install_guard`,
`install_locale`), `open_settings` and `open_settings_on_menu` unchanged.
The M1 footer survives as a `SettingsRow::Custom` rich text at the end of
the controls tab (`test_id: footer_hint`), which is also the demo of a
custom row. Gone: `SCREEN_PATH`, `screen()`, `rules()`, `register_screen`,
`seed_store`, `on_reset` (the frame's Reset does it), the
`assets/screens/demo_settings.screen.ron` file and the showcase's
`SETTINGS_SCREEN_RON`. `examples/showcase/Cargo.toml` adds the facade's
`menu` feature; `examples/chest/src/main.rs` no longer loads the deleted
screen file through `ScreenAssets`. `ChestSettingsPlugin` is untouched and
still opens `demo:settings` on `Menu`.

### `crates/slotted-test/tests/settings.rs` (kept green)

What the rebuild forced: ids are the new convention (`settings.<key>`,
`bind.keyboard.<action>`, `settings.tabs`, `settings.audio.page`); the count
assertions read three scrolls (one page per tab), no image, and the frame's
anchors; the gamepad walk lands on `reset` rather than `done` from the name
field (the frame's footer has other gaps and full-height buttons) and walks
`Right`/`Left` between the two; the rebound-Accept test re-captures `K`
after pressing Reset, because Reset now restores the default bindings. The
three tree snapshots are re-accepted. `lua_tests.rs` uses the new id for the
name field. The Tab-opens-settings parts are left for D.

## 2. Decisions the contract left open

### 2.1 The root's shape

`Screens::resolve` gives the child's root the last word on role, layout and
tags, so a generated root with a default `Layout` would throw away the
frame's 720 px column. `screen_def_over` copies the frame root's shape and
`apply_settings` passes the registered frame, so a game that registered its
own `slotted:settings` (contract 0, override by registering first) gets its
own root shape too. `screen_def()` with no world uses the embedded frame.

### 2.2 The tab key is UI state

`<kind path>.tab` is seeded (first tab), ruled (the tab ids) and bound, but it
is not in `defaults()`/`keys()`: it is not saved (a tab switch would write
the file), not reset (Reset from the audio tab should not jump to display)
and not required of the game. The M1 demo did the same for Reset.

### 2.3 Seeding precedence

Saved value > a value the game already inserted > the default. A game that
seeds the store itself before startup keeps what it seeded unless a file
overrides it; a saved value out of the current rule's range is loaded as is
(restore skips rules), which a later write corrects.

### 2.4 The toast argument

`binding_moved`'s `action` is the action's data name (`tab_next`), since the
fallback table has no per-action display names. A game that wants "Next tab
lost its key" overrides `slotted.menu.binding_moved` in its catalogue and
maps the name itself, or C adds `slotted.menu.action.<name>` keys in a later
round.

### 2.5 Every binding row is a `KeyBinding` on the frame's own control

No two-column grid (proposal 5.1): the M1 controls already carry their label
on the left and their value on the right, so a row is one control.

## 3. Shared files touched

`crates/slotted-menu/src/lib.rs`: nothing added (the skeleton's exports
cover the package). `assets/locale/en-US.ftl`: one comment line naming the
spec instead of the deleted file. `crates/slotted-test/tests/lua_tests.rs`:
one test id.

## 4. Tests

`crates/slotted-menu/tests/settings.rs`, ten tests, headless through
`UiHarness` with the glass theme from the workspace `assets/`:

- the generated def (kind, inherits, initial focus, one tabs child, bind,
  one scroll page per tab, start and end anchors first and last) and the
  opened screen (frame title, Reset, Done, tabs, page, every anchor, the
  binding-row id, focus on the first tab button);
- defaults and rules (values, the derived slider and option rules, the tab
  rule, no rule for text and toggle) and their effect in the app (clamp,
  snap, refused option);
- the `MemorySettings` round trip (opening saves nothing, a drag saves the
  four declared keys and the bindings, a tab switch and an undeclared key
  save nothing, a fresh app seeds from the file and the slider paints it);
- stale keys dropped and precedence (saved over game seed over default,
  stale never written back);
- bindings persist across apps;
- `FileSettings` round trip through a temp directory (missing file, created
  parent directory, RON equal to `MemorySettings::from_ron`, broken file);
- a `Settings` inserted after startup applies the next frame;
- Reset through a locking guard (values back, the locked key kept, slider
  repainted, bindings default, the reset saved, `SettingsReset` alone works);
- a rebind conflict (E from TabNext to Accept: TabNext loses E, the pad is
  untouched, one `Toast` entity, E now accepts rather than switching tabs, a
  free key toasts nothing, a pad capture of a free button toasts nothing);
- a mod-style `Injection` at `settings.general.end` lands inside the general
  page below the last spec row and binds the store.

`crates/slotted-test/tests/settings.rs`: 17 tests green against the generated
screen. `showcase` unit tests: the spec's keys and tab key, the guard.

## 5. Deferred

- **`max_height: 70%` on a page resolves against the tabs column**, whose
  height comes from the frame's `max_height: 85%`; at 720p the pages are
  about 225 px tall, so the display tab scrolls too. A theme-sized or
  frame-set page height is a design call once the templates are tuned.
- **Reset does not close a capture in progress** and does not scroll the
  pages; neither was asked for.
- **`apply_settings` runs once per app.** A `Settings` resource replaced after
  it ran is not re-applied; removing `SettingsApplied` re-runs it.
- **No per-action display names** for the conflict toast (2.4).
- **The harness's own `harness.rs` Tab test** fails under the facade's default
  features because `Menu` (still Tab) now pushes the pause screen through
  B's `pause_on_menu`; D's binding change (Escape + Start) resolves it. Not
  a C change.
