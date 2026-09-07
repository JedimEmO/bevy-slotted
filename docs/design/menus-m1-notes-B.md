# Menus M1 package B notes: value store, activation, value controls

What package B (the value store, focused actions, the button and the five
value controls) built for the M1 contract (`menus-m1-contract.md` sections
1.1, 1.2 and 3), the decisions the contract left open, what changed in shared
files, the tests, and what is deferred. Nothing here changes a signature the
contract fixes; every addition is on top.

## 1. What was built

### `slotted-ui/src/values.rs` (1.1, 3.1)

- **`apply_set_values`** in `ValueApply` (first inside `Navigate`). Per
  `SetValue`: the `ValueRule` snaps to `min + round((v - min) / step) * step`
  and then clamps (an `Int` stays an `Int`, a `Text` must be one of
  `options` when the list is non-empty), every guard is asked in order with
  the store as it is before the commit (the first `Err` refuses, an `Ok` may
  rewrite), then `commit` bumps `version` and one `ValueChanged` is written,
  or one `ValueRefused`. A write that changes nothing still commits and still
  announces itself: the store is the truth and says so every time. A refusal
  never moves `version`.
- **`ValueBinding::from_def(&BindDef, menu)`**: `bind` wins, `property`
  needs the screen's menu (a `property` on a menu-less screen is logged and
  dropped). `ValueBinding::key()` for the store side.
- **`BoundValue { entity, value }`** (new `EntityEvent`): the value arriving at
  a control. `sync_value_bindings` (`ValueSync`, inside `Render`) triggers it
  for every store binding that is `Added` or whenever `ValueStore` changed
  (the `version` moved), and for every property binding on `Added`, reading
  the `MenuProperty` child as `Value::Int`. `on_bound_property_changed`
  (observer on `slotted_ecs::PropertyChanged`) covers the menu side after
  that. Each control observes `BoundValue` and copies it into its state; the
  paint systems run `.after(ValueSync)` so the value that arrived this frame
  is painted this frame. No control reads the store. See 2.1.
- **`ValueWriter`** (new `SystemParam`): a control's one write path.
  `write(source, &binding, value)` is a `SetValue { source }` for a store
  binding and a `slotted_ecs::SetProperty` for a property (`Bool` as 0/1,
  `Float` rounded, `Text` logged and dropped).

### `slotted-ui/src/nav.rs` (1.2)

- **`dispatch_focused_actions`** triggers one `FocusedAction` per
  `UiActionEvent` on the `InputFocus` entity when it is `Focusable`, not
  `InteractionDisabled` and not under a `FocusMask` (`is_masked` walks
  `ChildOf`; public for the ring). An action already claimed this frame is
  not delivered: it was consumed (see 2.3).
- **`directional_nav_actions`** now skips a claimed direction. The contract
  says a slider that consumed `Left` claims it and nav leaves it alone; the
  M0 system never read the claims, so this is the missing half.
- **`accept_focused` is gone.** Its slot half is `on_slot_accept`, a global
  observer on `FocusedAction`: a fresh `Accept` on a `SlotRef` is a left
  `SlotClicked` with the modifiers held, from any device, claimed. Its
  button half is `widgets::on_button_accept` (below).
- **`track_text_entry_focus`** keeps counting `EditableText` and the
  `TextField` role, except an M1 `text_field` row that is not `editing`
  (`TextFieldState::editing`): a focused row must still see `Accept` to
  start editing (contract 4.1). The M0 test that spawns a bare `TextField`
  role and the browser's search field are unchanged by this.

### `slotted-ui/src/widgets.rs` (3.2)

- **`spawn_button`** rewritten: `control_height` (or compact) tall, padding
  `md` × `sm`, an icon child through `icon_image` before the label, the
  label a `LocText` in `control.label`, `ButtonState { variant, disabled,
  pressed }`, `Focusable`, `TabIndex(0)`, `InteractionDisabled` when
  disabled, `Themed(button | button.primary | button.danger)`. **No
  `bevy_ui_widgets::Button`.** Its own observers: `Pointer<Press>` inserts
  `Pressed` and sets `pressed`, release / drag end / cancel clear it,
  `Pointer<Click>` triggers `Activate` unless disabled (propagation stopped
  like Bevy's). `on_button_accept` (global observer) turns a fresh `Accept`
  from any device into `Activate` and claims it. This closes the M0
  "rebound Accept" follow-up: `a_rebound_accept_key_activates_a_button`
  binds `Accept` to `K`, presses Enter (nothing), presses `K` (once).
- **`button_roles`** (`Render`) swaps `<variant>.{disabled|pressed|focus|hover}`
  through `controls::state_role_with`, disabled winning.
- **`slotted:close`** as a `widget` on a typed `button` node attaches
  `on_close_activate` (pops the stack) inside `spawn_button`; the registry's
  `CloseWidget` reaches the same code. See 2.4.
- `ButtonParams` is now `ButtonOpts` itself, so `slotted:button` through the
  registry takes icon, variant, disabled and compact like the typed node.

### `slotted-ui/src/widgets/controls.rs` (shared plumbing, 0)

`ControlLook { hovered, focused, active, disabled }` and
`state_role(base, look)` / `state_role_with(base, look, active_suffix)` are
the one place role precedence lives (disabled > active > focus > hover).
`spawn_control_row` is every control's root: `control_height`, a flex row,
`Focusable`, `TabIndex(0)`, `Hovered`, the semantic bundle, `InteractionDisabled`
when disabled, the label through `spawn_control_label`. `spawn_control_spacer`
pushes the value part to the row's end. `fraction_along` is the slider's
pointer maths. `build(app)` registers every observer and paint system;
`plugin.rs` calls it after `values::build`.

### The controls (3.3 to 3.6)

Every control: the row is the focusable, the value part is a child that
carries the roles (the row itself is not themed, see 2.2), the state
component lives on the row beside a `*Parts` component naming the children
tests reach for, and a `BoundValue` observer copies the value in.

- **`toggle`**: switch = a `toggle`/`toggle.on` track (`lg + md + 2·xs` wide)
  with a `toggle.thumb` child that slides `spacing.md` on a `Translate`
  tween of `MotionPreset::Hover` (the `fast` tier); checkbox = a
  `checkbox`/`checkbox.on` square with a `✓` tick child shown when on.
  `Checked` mirrors `on`. Accept flips and claims; a primary click anywhere
  on the row flips. `ToggleThumb { on }` on the thumb remembers where it was
  painted so a flip slides from there and the first paint snaps.
- **`slider`**: `slider` track (`flex_grow`, `sm + 2` tall, half a thumb of
  horizontal margin so the thumb can overhang), `slider.fill` (absolute,
  `width: %`), `slider.thumb` (absolute, `left: %`, self-centred through a
  percent `UiTransform`), `slider.text` readout. `format_readout` handles
  `{value}`, `{min}`, `{max}` and `{name:.N}`; a whole number prints
  without decimals, else up to two with trailing zeros dropped. `Left`/`Right`
  step by `step` (1 % of the range when 0; repeats from `emit_ui_actions`
  count), `PagePrev`/`PageNext` a tenth; all four claimed. `Pointer<Press>`
  on the track or its children jumps and starts a scrub, `Pointer<Drag>`
  writes on every move, release or drag end ends it. Every write is a
  `SetValue` with the slider as `source`, snapped to the step grid first.
- **`select`**: a `select` pill with `‹`, the current option's `LocText`,
  `›`. `Left`/`Right` cycle with wrap and write. Accept in `InputMode::Pointer`
  (or a primary click on the row) triggers `OpenSelectPopup`: an absolute
  `select.popup` panel spawned under the screen root, placed below the pill
  in the root's padding-box coordinates, `ZIndex(1)`, one `Focusable`
  `select.option`/`select.option.active` row per option (`SemanticRole::ListItem`,
  `SelectOptionNode`). Focus moves to the active option. On an option,
  `Up`/`Down` wrap through the siblings, `Accept` writes and closes, `Back`
  closes; all claimed. `CloseSelectPopup` despawns and focuses the row.
  Because the popup is under the screen root, `enforce_focus_scope` leaves
  it alone; because `Back` is claimed, `pop_on_back` does not pop.
- **`radio_group`**: one `radio`/`radio.active` segment per option in a row
  at the right, each with a `LocText`. `Left`/`Right` move without wrapping
  (the ends are ends) and write at once; both claimed. A primary click on a
  segment writes.
- **`key_binding`**: label defaults to the action's data name, a
  `key_binding`/`key_binding.capturing` cell with a `text.key` child showing
  `rich::key_glyph_text(action, mode_for(device), bindings)`. Accept enters
  capture and claims. `capture_key_bindings` runs **before**
  `dispatch_focused_actions`: any `Back` event cancels and claims `Back`;
  else the first `just_pressed` key (never Escape) or gamepad button
  replaces the action's first binding, every action that press was bound to
  is claimed, and `BindingChanged` is written. `paint_key_bindings`
  refreshes on `UiBindings` change.

### `slotted-ui/src/widgets/icon_button.rs`

`on_icon_button_key` reads `FocusedAction::Accept` instead of
`FocusedInput<KeyboardInput>` with `Enter | Space`, and claims. Contract 6's
"no control reads `KeyCode`" gate names only `key_binding.rs`, and the icon
button was the last `KeyCode::` in `widgets/`. Its tests are unchanged.

## 2. Decisions the contract left open

### 2.1 `BoundValue` rather than a per-control query in `sync_value_bindings`

The contract says `sync_value_bindings` copies the value "into the control's
state component". Four state types of B's plus three of C's in one system
would be a seven-way `Option<&mut>` query that has to know every control. An
entity event keeps the sync ignorant of the controls: it delivers, and each
control's observer converts (a toggle takes `Bool` or a non-zero `Int`, a
select takes an option id or an index). Observers run when the sync's
commands flush, before the next system, so ordering the paint systems after
`ValueSync` is enough. C's controls read the store directly with their own
`StoreSeen` bookkeeping; they could switch to `BoundValue` without a contract
change.

### 2.2 The row is not themed

A `toggle` role is a `radius: 999` pill; painting it over a full-width row
would be wrong, and a `select.focus` border belongs on the pill. So every
control row carries no `Themed`, and the value part (track, pill, cell,
segments) carries the base role and the state suffixes. The focus ring still
frames the whole row, which is the `Focusable`. `button` is the exception:
it is its own value part.

### 2.3 The dispatch skips claimed actions

Without this, the key that ends a capture (`Enter` bound as the new
`Accept`) would reach the row as an `Accept` and start the next capture, and
a captured arrow would still move focus. Reading it as "a claimed action was
consumed" also lets the capture run before the dispatch and swallow the
press cleanly. `directional_nav_actions` reads claims for the same reason.

### 2.4 `slotted:close` on a typed `button` node

`UiNodeDef::Button { widget }` goes straight to `spawn_button`; the registry's
`CloseWidget` is only reached through a `custom` node. The pop observer is
therefore attached inside `spawn_button` when the kind is `slotted:close`,
and `CloseWidget` just calls `spawn_button`. The first version attached it in
`CloseWidget` only and the typed node did nothing, which
`a_close_button_pops_the_screen` caught.

### 2.5 Unbound controls keep their own value

"A control paints from its binding, never from its own memory" has nothing
to paint from when a node names neither `bind` nor `property`. Rather than a
dead control, an unbound one writes its state directly. Bound controls never
do.

### 2.6 A gamepad row cannot bind `Back`'s button

`Back` cancels a capture on either device, so East (or whatever `Back` is
bound to) can never be captured on a gamepad row. Escape is refused as a
key outright. Both are the contract's rule applied to both devices.

### 2.7 A write that changes nothing still commits

The alternative (skip the commit, write nothing) leaves a control whose
`SetValue` was a no-op with no signal that the store agreed, and a `version`
that did not move for a slider drag that landed on the same snapped value.
Committing is simpler and every consumer already tolerates a repeated value.

### 2.8 Labels carry no `SemanticRole`

`spawn_control_label` spawns a `LocText` in `control.label` with no
`SemanticRole::Text`, as the M0 button label did. The control's
`SemanticLabel` already names it; a label child would add a `Text` node
under every control in every tree snapshot for no information.

## 3. Changes to shared files

- `plugin.rs`: `dispatch_focused_actions` ordered after `track_text_entry_focus`;
  `capture_key_bindings` ordered before the dispatch; `accept_focused`
  removed from both places; `on_slot_accept` observer added;
  `widgets::controls::build(app)` called after `values::build`.
- `lib.rs`: exports `BoundValue`, `ValueWriter`, `ButtonState`,
  `OpenSelectPopup`, `CloseSelectPopup`, `SelectOptionNode`, `on_slot_accept`;
  `accept_focused` removed.
- The three theme files: untouched. The skeleton's first-pass materials for
  `button.*`, `toggle.*`, `checkbox.*`, `slider.*`, `select.*`, `radio.*`,
  `key_binding.*` were kept; every dotted state role the controls emit
  (`toggle.on.focus`, `radio.active.hover`, `button.primary.pressed`, ...)
  resolves through the fallback in all three, which `controls.rs` asserts.

## 4. Tests

`crates/slotted-ui/tests/values.rs` (7) and `crates/slotted-ui/tests/controls.rs`
(18), plus two unit tests in `slider.rs` (readout format, snapping).

- Store, on a bare `MinimalPlugins` app around `values::build`: a write
  without a rule commits and bumps `version`, the second reports `old`; a
  rule clamps high and low, snaps from `min`, keeps an `Int` an `Int`;
  options refuse `ultra` with a reason naming it and do not move `version`;
  two guards refuse then rewrite in order, one message each; a guard sees
  the store before the commit; a seed bumps `version` with no message.
- Property round trip, through a one-property menu fixture: a `property`
  toggle seeds `on` from the initial `1`, follows a host `SetProperty`, and
  its Accept writes the `MenuProperty` back to `1` with nothing in the store.
- Controls, through `UiHarness` on a seeded store: every control paints
  from the store on open (`Checked`, the `40%` readout, a later seed
  repaints, every row is `control_height` tall, compact is compact); Accept
  on a button activates once from keyboard, gamepad and click, no
  `bevy_ui_widgets::Button` on it; a rebound `Accept` key activates; a
  disabled button is `button.disabled`, `InteractionDisabled`, and inert;
  roles walk `primary.focus` → `primary` → `.hover` → `.pressed` and every
  dotted state role resolves in three themes; `slotted:close` pops; a toggle
  flips on Accept (one `SetValue` with the row as source) and on a click, the
  checkbox has a tick and `checkbox.on.focus`; the switch thumb tweens by
  `spacing.md`; a disabled slider ignores arrows, page and click with no
  write; a slider steps, pages a tenth, keeps focus (claimed), writes with
  itself as source, and repeats on a held arrow over virtual time; a guard
  over 50 snaps it back; a press on the track jumps and focuses, a drag
  writes 60, 40, 20 and lands the thumb at a fifth; a select cycles with
  wrap on the arrows and shows the option label; keyboard Accept opens no
  popup, pointer-mode Accept opens one under the pill with focus on the
  active option, Down wraps, `Back` closes without popping and a second
  `Back` pops; a click opens, pad Down + South picks, a click on an option
  picks; a radio group stops at the ends without writing, writes on a move,
  a segment click picks and paints `radio.active.focus` then `.hover`; a
  key capture shows `…` and `key_binding.capturing`, Escape cancels without
  popping, `K` rebinds `Accept`, one `BindingChanged`, the row's text
  changes, and the button answers to `K`; a captured `ArrowUp` is claimed so
  focus stays.

`cargo test -p slotted-ui -p slotted-test -p slotted-packs` green (the
harness `gamepad.rs` Accept-on-slot and Accept-on-button tests included);
`cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --check`
clean; `cargo check --target wasm32-unknown-unknown -p slotted-ui` clean;
`cargo test --workspace` green at 1121 tests (one earlier run saw the chest
example's theme-independence test fail while another package was editing a
theme file mid-run; it passed on every rerun).

## 5. Not finished, deferred

- **Theme materials are the skeleton's first pass.** No `toggle.thumb`
  travels a different distance per theme, no `slider.thumb` is a different
  size in paper; the tokens decide the geometry. Tuning is a theme-file
  job for whoever styles the demo screen (D).
- **A select popup does not close on a click outside it.** `Back`, Accept
  and a click on an option close it; a click elsewhere leaves it open with
  focus on the option. Bevy's `click_to_focus` moves focus out, so the next
  `Back` pops the screen with the popup still drawn. A global
  `Pointer<Press>` observer that closes any open popup whose subtree was not
  pressed is the fix; not in the contract.
- **A key capture does not unbind the key elsewhere.** Binding `K` to
  `Accept` leaves `K` bound to whatever it was; two actions can share a key.
  The contract says "replaces the action's first binding" and nothing about
  conflicts.
- **Gamepad rows cannot capture the `Back` button** (2.6).
- **C's controls read the store directly** with a `StoreSeen` marker rather
  than through `BoundValue` (2.1). Either works; one mechanism would be
  tidier.
- **`ValueRule` is not derived from the slider's `min`/`max`/`step`.** A
  slider clamps and snaps its own proposals, so a screen that declares a
  range and forgets the rule still behaves; a Lua or harness `set_value`
  outside the range is only caught when the game declared the rule.
