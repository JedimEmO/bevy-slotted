# Menus M2 package B notes: templates, menu actions, pause, confirm, toast, page, hint bar

What package B built for the M2 contract (`menus-m2-contract.md` section 3),
the decisions the contract left open, what changed outside B's files, the
tests, and what is deferred. Nothing here changes a signature section 1
fixes; every addition is on top.

## 1. What was built

### `strings.rs` (0, 2.1)

`MenuStrings::resolve` substitutes `{name}` arguments by plain textual
replacement (`substitute`): each argument's `rich::value_text` replaces its
`{name}`, a `{name}` nothing names stays as written so `{key:accept}` in a
rich string reaches the markup parser untouched. No escaping: the English
table is ours and holds no brackets. `install_strings` pushes it as a
`Localization` fallback in `PostStartup`, after the templates register.

### `plugin.rs` (3.2)

- **`route_menu_actions`** (global observer on `Activate`): the entity's
  `menu` tag, the screen root above it, the stack entry for that root; then
  the built-in handling, then one `MenuChoice { id, screen, entity }` in
  every case, so the game always hears the choice. `resume`/`close`/`back`
  pop; `settings` pushes `MenuConfig.settings_kind` from `Screens`, or warns
  naming the kind; `accept`/`cancel` on a root carrying `PendingConfirm`
  write `ConfirmResult`, remove the `PendingConfirm` (so the close observer
  stays quiet) and pop. A button outside a stack screen writes nothing.
- **`pause_on_menu`** (`SlottedUiSet::Input` after `UiActionEmit`): a fresh,
  unclaimed `Menu` with `stack.top()` empty (no page or modal; overlays do
  not count) pushes `pause_kind` and claims; with `pause_kind` on top it
  pops and claims. Anything else on top (a page, another modal) is left
  alone. An unregistered `pause_kind` warns.
- **`install_strings`** also fills the main menu header: a clone of the
  registered `slotted:main_menu` gets `title` from `MenuConfig.title` and
  `version` from `slotted.menu.version` with `{version}`; an empty version
  removes the `version` node rather than drawing "Version ". The def is
  re-registered (`Owner::Game`) only when something changed. A game that
  registered its own main menu with `title`/`version` ids gets the same
  treatment, which is what "the registered main menu" means.

### `confirm.rs` (3.3)

`confirm` queues a command: clone the registered `slotted:confirm`,
`rewritten` for the spec (title and message through `set_text`, the message
with `args`; the button labels through `set_button_label`, see 2.1; the
unwanted accept button removed by id), `PushScreen` applied directly so the
root is known, then `PendingConfirm(id)` inserted on it. `on_confirm_closed`
answers `false` for a root that still carries `PendingConfirm` when
`ScreenClosed` fires, which is the `Back` path. A confirm over a confirm
stacks; each answers its own id. `ConfirmSpec` gained `danger()`, `title()`,
`buttons()` and `arg()` builders.

### `toast.rs` (3.4)

`toast` queues a command: with fewer than `Toasts.max_visible` `Toast`s
alive, `spawn_toast`; else the spec goes to `ToastQueue`. The host is
created lazily: an absolute full-window `Pickable::IGNORE` node in
`zbands::TOAST` (`ToastHost`, flex column, `justify: end`, `align: center`,
`spacing.lg` bottom padding) holding a `ToastColumn` in `ColumnReverse`
with `spacing.sm` gaps, so the first toast sits at the bottom edge and every
later one stacks above it. A toast is `toast.node.ron` through a `SpawnCtx`
whose `screen` and `parent` are the column and whose kind is
`slotted:toast`, with the panel role swapped for `ToastLevel::role()` and
the `toast.text` node's key and args rewritten (`toast_def`); it carries
`Toast { level, remaining }`, `WidgetNode(slotted:toast)` and
`ToastArriving`. `remaining` defaults to ten `tokens.durations.slow`.

`start_toast_arrivals` runs after `SlottedThemeSet::Apply`, like the stack's
`start_push_transitions`: once the theme has painted the panel it sets the
alpha to zero and tweens it back through the `Fade` preset; under reduced
motion the toast simply appears. `tick_toasts` (`Render`) subtracts
`Time<Virtual>::delta` from every toast, starts a `Fade` tween to zero and
marks `ToastFading` when one runs out (or despawns it at once under reduced
motion), despawns a fading toast whose tween is gone, and dequeues into the
room that made, oldest first. Toasts never touch the stack or focus.

### `page.rs` (3.5)

`open_page` clones `slotted:page`, sets `title` and `body` (with args)
through `set_text`, and applies `PushScreen`. `PageSpec::arg` added.

### `hint_bar.rs` (3.6)

`HintBarWidget` spawns the row (`hint.bar` role, `HintBar`, `HintEntries`,
`HintRendered`, `Pickable::IGNORE`); the def's tags land on it through
`spawn_child`, so `hint.always` is read from `Tags` at update time.
`update_hint_bars` (`Render`) returns early unless `InputFocus`,
`InputMode`, `GlyphSet`, `UiBindings`, `ScreenStack`, `ActiveTheme` or
`Assets<Theme>` changed or a bar was added. Per bar: the screen root above
it; the verb from the focused node's `SemanticRole` only when that node
sits under the bar's screen (`verb_for`: Button/Tab/ListItem → Select,
Toggle/Chip → Toggle, Slider → Adjust on `Left` and `Right`,
Select/RadioGroup → Change, TextField → Edit, KeyBinding → Rebind, Slot →
Pick up); a `hint.accept` tag on the node replaces the label of every verb
entry (or adds an Accept entry to a node with no verb); `Back` with
`slotted.menu.close` for a modal and `slotted.menu.back` for a page when the
screen's `BackPolicy` is `Pop`; `TabPrev`/`TabNext` when a `TabsState`
lives under the root. Accept is always first. The glyph text per entry is
`key_glyph_text(action, mode, bindings, resolved_glyph_set(set, mode,
gamepads))`. Visibility is `mode != Pointer || hint.always`.

The children are rebuilt only when the entries, the glyph texts or the
bracket flag changed (`HintRendered` remembers the last render):
consecutive entries with the same label form one group, `[←][→] Adjust`,
each glyph a pill node in `hint.glyph` holding a `Text` in
`hint.glyph.text` (`HintGlyph` marker) and the label a `LocText` in
`hint.label` (`HintLabel` marker). See 2.2 for the pill and the brackets.

### Templates (3.1)

Unchanged in structure from the skeleton; every one opens in three themes
and the walk reaches every button (the settings frame's empty tabs node
means its first focusable is Reset, with Done to its right). Widget kind
`slotted:hint_bar` is registered by `hint_bar::build` into the registry
`SlottedUiPlugin` created, so `MenuPlugin` must come after it, as it does
in the facade group.

### Theme materials

- The eleven new roles kept, with `hint.glyph` reworked: glass and neon
  draw it as an accent pill (`Solid` fill, radius 999) with the text in a
  new `hint.glyph.text` entry (dark on the accent); paper keeps `hint.glyph`
  a `Text` material (orange mono) and defines `hint.glyph.text` the same,
  since the shipped themes must define the same key set.
- `button.primary.{hover,focus,pressed}` and `button.danger.{hover,focus,
  pressed}` in all three: the accent fill stays through the states (before,
  `button.primary.focus` fell back to `button.primary`, so a focused primary
  button, which is the first thing every template shows, looked exactly
  like an unfocused one). Glass brightens the gradient and whitens the
  border, neon swaps the border to magenta on focus and shades the fill,
  paper heavies the shadow on focus and drops it on press.
- `control.label.inverse` and `text.label.inverse` in all three (see 2.3).

## 2. Decisions the contract left open

### 2.1 `set_text` does not reach a button label

`ScreenDef::set_text` rewrites `text` and `rich_text` nodes, which is what
the contract asked A for; a button's label is `ButtonOpts.label`. The
confirm dialog's `accept` and `cancel` labels therefore go through
`confirm::set_button_label`, a `find_mut` on the `Button` variant. Worth
folding into `set_text` in `slotted-ui` later; not B's file.

### 2.2 The glyph pill and the brackets come from the material kind

The contract gives the hint bar one glyph role and wants paper to draw
bracketed mono text where glass and neon draw an accent pill. A `Text`
material cannot paint a fill, and a widget must not know colours, so:
the glyph is a pill node in `hint.glyph` with a text child in
`hint.glyph.text`, and the bar reads the *kind* of the theme's `hint.glyph`
material: a `Text` material means "no pill, draw `[A]`"; anything else
means "a pill, draw `A`". A theme decides the whole look with one entry.
`update_hint_bars` therefore re-renders when the theme loads or changes.

### 2.3 Inverse labels keep their size

While tuning the primary button materials the label inversion from M1 turned
out to flip the label back to `control.label` (light text) the moment a
primary button took focus, because `is_accent_fill` matched
`button.primary` exactly and the button had moved to `button.primary.focus`;
and `text.inverse` is a `$label` size, so an unfocused primary button's
label was two pixels smaller than its neighbour's. Both fixed in
`slotted-ui/src/widgets/controls.rs` (section 3): `is_accent_fill` takes
the state roles of primary and danger, and `invert_active_labels` uses
`<rest>.inverse` (`control.label.inverse`, `text.label.inverse`) when the
active theme defines that key directly, else `text.inverse`; it also looks
at every accent label again when the theme loads or changes, since the
dotted roles arrive with it. The three themes define both, at the rest
role's size and font, so only the colour flips. `text.inverse` stays in
`roles::ALL` as the fallback.

### 2.4 Toasts stack with the first at the bottom

"Stacks upward" is read as: the column grows upward, so the second toast
sits above the first. A `ColumnReverse` column does that with plain child
appends and keeps the oldest toast where the eye first landed.

### 2.5 An empty version removes the node

`MenuConfig::version` defaults to empty; drawing "Version " under the title
of a game that gave none is worse than drawing nothing, so the node goes.

### 2.6 A `MenuChoice` is written even for the built-in ids

The contract says the message reaches the game "after the built-in
handling". `resume`, `settings`, `accept` and `cancel` therefore also arrive
as `MenuChoice`s, which a game can log or ignore; `reset` reaches C's
handler that way.

## 3. Changes outside B's files

- `crates/slotted-ui/src/widgets/controls.rs`: `is_accent_fill` uses
  `starts_with` for `button.primary`/`button.danger`; `inverse_of(rest)`
  added; `invert_active_labels` takes the theme resources and picks the
  dotted inverse role when the theme defines it (2.3). `slotted-ui`'s
  control tests are unchanged and green.
- `assets/themes/*.theme.ron`: `hint.glyph` / `hint.glyph.text`, the six
  primary/danger state roles, `control.label.inverse`,
  `text.label.inverse`. `slotted-theme`'s same-key-set test passes.
- `crates/slotted-menu/tests/skeleton.rs` removed; its check is the first
  test of `templates.rs`.
- `crates/slotted-menu/src/lib.rs`: untouched.

## 4. Tests

`crates/slotted-menu/tests/templates.rs` (34) plus one unit test in
`strings.rs`. The harness is `SlottedPlugins::headless()` with the
workspace `assets/` as the asset root, `MenuPlugin` added unless the facade
already did, and `MenuConfig` inserted before the first frame (so
`install_strings` sees it); every test waits for the theme asset.

- Templates: every embedded file parses, names its kind, an initial focus
  and anchors; the five register at startup and a pause registered by the
  game before the plugin survives; main menu, pause, confirm and page open
  in glass, paper and neon with focus on `initial_focus` and a pad walks
  every button (Down, then Right when Down did not move); the settings frame
  opens with focus on Reset, Right reaches Done, South pops; `roles::ALL`
  is 101, no theme has a missing role, the eleven new roles and
  `hint.glyph.text` resolve in all three.
- Menu actions and strings: `play` and `quit` produce `MenuChoice`s with
  their ids, screen and entity and pop nothing; the English fallbacks fill
  Play/Settings/Quit and `Version 1.2.0`, the game's title key is drawn as
  written until its catalogue answers; an empty version removes the node; a
  game catalogue set as primary overrides `Play` and leaves `Quit` to the
  fallback.
- Pause: `Menu` pushes the pause with focus on Resume, `Menu` pops it,
  `Back` pops it; `Menu` over the main menu page does nothing;
  `pause_on_menu: false` does nothing; `resume` pops with one choice;
  `settings` pushes the configured kind over the pause and `Back` returns;
  an unregistered settings kind pushes nothing and still writes the choice.
- Confirm: title, cancel and accept labels rewritten, the message carries
  the args and renders `Jump lost its key`, focus on cancel, the danger
  button gone, `PendingConfirm` on the root; `accept`, `cancel` and `Back`
  each yield exactly one `ConfirmResult` and pop, the choice still reaches
  the game, nothing answers twice; `danger` keeps only the danger button; a
  confirm over a confirm stacks and each answers its own id.
- Toast: two toasts in the `TOAST` band, the second above the first, the
  first near the bottom edge and centred, level roles and text right, no
  stack entry; the first expires on virtual time and fades, then the
  second; past `max_visible` the third queues and takes the freed slot in
  order; a toast over a page and a modal is visible, moves no focus, and
  defaults to ten `slow`; reduced motion shows and removes without a tween;
  a toast arrives with an `Alpha { from: 0 }` `Fade` tween that settles to
  the painted alpha.
- Page: title and body (with an arg) rewritten, focus on Done, South pops;
  `Back` pops.
- Hint bar: Select and Close on a pause button with `Enter`/`Esc` glyphs and
  English labels; the main menu bar (`back: ignore`) has no Back and a
  page's says Back; a slider gives Adjust on `Left` and `Right` sharing one
  label and a `hint.accept` tag replaces the verb; pointer mode hides the
  plain bar and keeps the `hint.always` one, with its entries maintained;
  the glyph re-renders from `Enter` to the pad glyph on a pad press and
  follows `GlyphSet` through PlayStation, Switch and Generic (asserted
  against `key_glyph_text`, so it holds before and after A's tables land);
  paper brackets `[Enter]` and glass draws an unbracketed glyph on a filled
  pill; a screen with tabs lists Previous tab and Next tab; `verb_for` maps
  every contract role.
- Materials: in three themes a focused primary button is
  `button.primary.focus`, its label `control.label.inverse` at the same
  size as its secondary neighbour, and stays inverse at rest.

`cargo test -p slotted-menu --test templates` (also with `-p slotted`, which
unifies the facade's `menu` and `blur` features in) and `cargo test -p
slotted-ui` green; `cargo test --workspace --no-fail-fast` at the time of
writing: 1189 passed, 6 failed, none in B's targets: the harness `Tab` test
(section 5), `lua_tests`' `player_name` locator (C's demo screen rebuild),
and three in `slotted-registry`, `slotted-testutils` and
`slotted-script-luaur` that pass when run alone and look like fixture
contention between the three packages' concurrent builds; `cargo clippy -p slotted-menu --all-targets -- -D warnings` clean
for B's files (C's in-progress `settings.rs` and `tests/settings.rs` fail
it at the time of writing); `cargo check -p slotted-menu --target
wasm32-unknown-unknown` clean; `cargo test -p slotted-theme` green.

## 5. Not finished, deferred

- **`set_text` on a button.** The confirm dialog rewrites its button labels
  through `confirm::set_button_label`; `ScreenDef::set_text` should grow the
  `Button` arm (2.1).
- **The hint bar reads the theme's material kind** to decide on brackets
  (2.2). If a future theme wants a pill *and* brackets, or text without
  them, it needs a token or a tag; none exists.
- **Overlay screens' bars** say Back for `BackPolicy::Pop`, which
  `pop_on_back` does not honour for an overlay (it pops the top
  non-overlay). No template is an overlay.
- **Toast text does not localise late.** A toast spawned before a catalogue
  loads keeps the key until `render_rich_text`'s own `Localization` change
  check repaints it, which it does; nothing B-specific, noted for
  completeness.
- **No harness helpers for toasts or hints** (`toasts()`,
  `hint_entries()`, `confirm_accept()`): D's, per contract 5; the tests here
  read `Toast`, `ToastColumn` and `HintEntries` directly.
- **The theme materials are tuned blind.** Every value was chosen from the
  palette and checked headless for resolution and role walks, not by eye;
  D's reference shots are where they get looked at.
