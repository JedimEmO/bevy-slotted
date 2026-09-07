# Menus M1 contract: type scale, rich text, localisation arguments, controls, value store

Status: v1.1, 2026-09-07 (v1.1: skeleton facts folded in, see 1.5). Bevy 0.19.1. Companion to `docs/design/menus-proposal.md` (v0.3)
sections 4.5 to 4.7 and to the M0 contract, whose vocabulary (`UiAction`, `UiActionEvent`,
`UiActionClaims`, `Focusable`, `ScreenStack`, `Layout`) this uses without restating. Four packages:
**A** (typography, rich text, localisation arguments), **B** (value store, activation, the value
controls), **C** (text field, containers, scrolling) run in parallel without talking to each other;
**D** (harness, demo screen, docs) runs after. The skeleton compiles and carries the data types, the
node variants, the widget kinds, the roles and the module wiring; a body marked `// M1-IMPL: A|B|C|D`
is that package's to fill. Signatures change only by amending this file. Each package writes
`docs/design/menus-m1-notes-{A,B,C,D}.md`.

## 0. Ground rules

- **Controls read actions, not keys.** Bevy's headless widgets (`bevy_ui_widgets`) hard-code Enter,
  Space and the arrows with no rebinding hook, so no M1 control uses `bevy_ui_widgets::{Button,
  Checkbox, Slider, RadioGroup, ListBox, Menu}`. A control gets its keyboard and gamepad input from
  [`FocusedAction`](#12-focused-actions) and its pointer input from Bevy picking. From
  `bevy_ui_widgets` we take `ScrollArea`, `ScrollIntoView` and `Scrollbar`; from `bevy_ui` the
  interaction markers `InteractionDisabled`, `Checked`, `Pressed`. `Activate` stays the event a
  consumer observes on a button, because `slotted-packs` routes it to Lua.
- **A control paints from its binding, never from its own memory.** Every value control carries a
  `ValueBinding`; the store is the truth, the control is a view. A write is a `SetValue` the store
  validates and commits or refuses; the control repaints from the store either way, so a refused
  write snaps back with no code in the control.
- **Every control is headless-inspectable.** State that tests assert on is a plain component
  (`SliderState`, `ToggleState`, `SelectState`, `TabsState`, `TextFieldState`, `ListState`), never
  something only the paint knows.
- **Theme, not screen, decides control size.** `tokens.sizes.control_height` (44 glass, 36 paper,
  40 neon) and `control_height_compact` (28, 24, 28). A control node's `height` is one of these; a
  screen file never writes a control's height.
- Roles follow the M0 convention: `<control>`, `<control>.hover`, `<control>.focus`,
  `<control>.active`, `<control>.disabled`, resolved through the dotted fallback, so a theme may
  define `control` once and override per control. The skeleton adds every role to `roles::ALL`
  (38 becomes 89) and to the three themes; packages adjust the materials of the roles they own.
- No wall clock; repeat and reveal timing is virtual.

## 1. Data (done in the skeleton)

### 1.1 Values (`values.rs`)

```rust
#[derive(Serialize, Deserialize)] #[serde(untagged)]
pub enum Value { Bool(bool), Int(i64), Float(f64), Text(String) }   // an option id is Text
#[derive(Resource, Default)] pub struct ValueStore { values: BTreeMap<String, Value>, version: u64 }
impl ValueStore { get(&str) -> Option<&Value>; version() -> u64; insert(key, value) /* game-side seed, no validation, no event */; keys() }
#[derive(Serialize, Deserialize, Default)] pub struct ValueRule { min: Option<f64>, max: Option<f64>, step: Option<f64>, options: Vec<String> }
#[derive(Resource, Default)] pub struct ValueRules(BTreeMap<String, ValueRule>);
pub trait ValueGuard: Send + Sync { fn check(&self, key: &str, proposed: &Value, store: &ValueStore) -> Result<Value, String>; }
#[derive(Resource, Default)] pub struct ValueGuards(Vec<Arc<dyn ValueGuard>>);
#[derive(Message)] pub struct SetValue { pub key: String, pub value: Value, pub source: Option<Entity> }
#[derive(Message)] pub struct ValueChanged { pub key: String, pub old: Option<Value>, pub new: Value, pub source: Option<Entity> }
#[derive(Message)] pub struct ValueRefused { pub key: String, pub value: Value, pub reason: String, pub source: Option<Entity> }
#[derive(Component)] pub struct ValueBinding { pub target: BindingTarget }
pub enum BindingTarget { Store(String), Property { menu: Entity, id: PropertyId } }
```

`apply_set_values` in `SlottedUiSet::Navigate` (before the stack systems): for each `SetValue`,
clamp and snap by the `ValueRule` when one exists (a `Text` value must be one of `options` when
`options` is non-empty), then ask every guard in order (the first `Err` refuses; an `Ok(v)` may
rewrite the value), then commit, bump `version` and write `ValueChanged`, or write `ValueRefused`.
A `Property` binding writes `slotted_ecs::SetProperty` instead and reads `PropertyChanged`, as
`tank.rs::bind_properties` does; `Value::Int` only.

### 1.2 Focused actions

```rust
#[derive(EntityEvent)] pub struct FocusedAction { pub entity: Entity, pub action: UiAction, pub device: InputDevice, pub repeat: bool }
```

`dispatch_focused_actions` (`nav.rs`, `Input` after `UiActionEmit`) triggers one `FocusedAction`
on the `InputFocus` entity for every `UiActionEvent` this frame, when that entity is `Focusable`
and not `InteractionDisabled`. A control observes it and claims what it consumed with
`UiActionClaims`. M0's `accept_focused` becomes two observers: a slot's Accept → `SlotClicked`,
and the M1 `button`'s Accept → `Activate` (all devices; the button no longer carries
`bevy_ui_widgets::Button`, closing the "rebound Accept" follow-up). Nav (`Up/Down/Left/Right`)
still goes to `directional_nav_actions` unless a control claims the action first: the dispatch
runs before it and a slider that consumed `Left` claims it.

### 1.3 Node variants (`def.rs`) and kinds

Every row is a `UiNodeDef` variant (`type:` in data) and a `Custom` kind with the same params
(`kinds::all()` is 29, including `slotted:close`). All carry `tags`. `bind` is `Option<String>` (store key); `property` is
`Option<PropertyId>`; a node with both is a load error naming the node.

| `type` | Fields | Package |
|---|---|---|
| `text` (grown) | `key, style: TextRole, args: BTreeMap<String, Value> = {}, wrap = true, align: TextAlign = left, max_lines: Option<u16>` | A |
| `rich_text` | `key, style = body, args, wrap = true, align, inline = false` | A |
| `button` (grown) | `widget: Option<WidgetKind>, label: Option<LocKey>, icon: Option<IconDef>, variant: ButtonVariant = secondary, disabled = false, compact = false` | B |
| `toggle` | `label: Option<LocKey>, style: ToggleStyle = switch, bind, property, disabled` | B |
| `slider` | `label, min: f64, max: f64, step = 0, format = "{value}", bind, property, disabled` | B |
| `select` | `label, options: Vec<SelectOption { id, label: LocKey }>, bind, property, disabled` | B |
| `radio_group` | `label, options, bind, property, disabled` | B |
| `key_binding` | `label, action: UiAction, device: InputDevice (Keyboard or Gamepad), disabled` | B |
| `text_field` | `label, placeholder: Option<LocKey>, filter: TextFilter = any, max_len: Option<u16>, bind, disabled` | C |
| `list` | `source: DataSourceId, rows = 6, bind` (selected index) | C |
| `scroll` | `layout: Layout, scrollbar = true, children` | C |
| `tabs` | `tabs: Vec<TabDef { id, label: LocKey, icon: Option<IconDef> }>, bind, children` (one child per tab, in order) | C |
| `separator` | `direction: LayoutDirection = row` | C |
| `spacer` | `size: Length = fill` (grow when `fill`) | C |
| `image` | `path: String, width: Length, height: Length` | C |

`TextRole` grows to `display, title, heading, body, muted, label, caption, count` with roles
`text.display, panel.title, text.heading, text, text.muted, text.label, text.caption, count`.
`ButtonVariant = primary | secondary | danger`, `ToggleStyle = switch | checkbox`,
`TextFilter = any | numeric | integer`, `TextAlign = left | center | right`. `select`'s
`Value` is the option id as `Text`; `radio_group` the same; `tabs` binds the active tab id;
`list` binds the selected row index as `Int`; `slider` `Float`; `toggle` `Bool`; `text_field`
`Text`.

`button.widget` keeps meaning "behaviour kind" for the rail (`slotted:sort`) and the new
`slotted:close` (pops the stack, B). A button with no `widget` is a plain `Activate` source whose
tags reach Lua as before.

### 1.4 Typography tokens (`slotted-theme`)

```rust
pub struct TypeStyle { size: f32, font: Option<String>, weight: Option<u16>, line_height: Option<f32> }
Tokens { ..., typography: BTreeMap<String, TypeStyle>, sizes: Sizes { ..., control_height: f32, control_height_compact: f32 } }
Material::Text { color, size: ThemeSize, font, shadow, weight: Option<u16> }
pub struct ThemeSize(String);   // "15" or "$body"; ThemeSize::px(f32), ::typography(name), .typography_ref()
```

`Theme::size(&ThemeSize) -> f32` and `Theme::type_style(&ThemeSize) -> Option<&TypeStyle>` resolve
a reference; `Theme::dangling_typography_refs()` sits beside `dangling_palette_refs()`. A `$name`
reference takes the style's font and weight too unless the material names its own. The three
themes define `display, title, heading, body, label, caption` (glass 40/24/18/15/13/12 in Manrope,
display in Barlow Condensed; paper 36/22/17/14/12/11 IBM Plex; neon 42/26/19/15/13/12, display and
title in Rajdhani) and every `Text` role points at one.

### 1.5 What the skeleton settled

- In Rust the optional fields are grouped so literals can write `..Default::default()`: `Text` and
  `RichText` carry `opts: TextOpts { args, wrap, align, max_lines, inline }`; `Button` carries
  `widget: Option<WidgetKind>` and `opts: ButtonOpts { label, icon, variant, disabled, compact }`;
  every value control carries `bind: BindDef { bind, property, disabled }`. All three are
  `#[serde(flatten)]`, so the data shape is exactly the table in 1.3. Optional fields carry
  `skip_serializing_if`, which the untyped round trip needs.
- `UiAction` and `InputDevice` serialise as lowercase strings (`"accept"`, `"gamepad"`).
- `LocText` is now `LocText { key, args }` with `LocText::new(key)`; `resolve_loc_text` re-resolves
  on `Changed<LocText>`. `Localizer::resolve(key, args: &LocArgs)`; `LocArgs = BTreeMap<String,
  Value>`; `Localization::{resolve_with, text_with}`; `no_args()`.
- `SemanticRole` gained `Toggle, Slider, Select, RadioGroup, KeyBinding, List, ListItem,
  ScrollView, Tabs, Decor` with AccessKit roles.
- `spawn_button(ctx, widget: Option<&WidgetKind>, opts: &ButtonOpts)`; the rail passes
  `compact: true`. `spawn_text` moved to `widgets/text.rs` and takes `&TextOpts`.
- Each control module has a `placeholder_row` the package replaces; `widgets/controls.rs`
  registers every kind through a `VariantWidget` that re-tags params as the typed variant (so a
  `custom` node and `register_widget` templates work without per-control params structs), and
  `CloseWidget` for `slotted:close`.
- `FocusedAction`, `FocusMask` and `dispatch_focused_actions` live in `nav.rs`; the dispatch is
  registered before `directional_nav_actions` and `accept_focused`. B replaces `accept_focused`.
- `ThemeSize` (number or `"$name"`), `TypeStyle`, `tokens.typography`, `sizes.control_height` and
  `control_height_compact`, `Material::Text.weight`, `Theme::{size, type_style,
  dangling_typography_refs}` exist; `apply.rs` already writes `TextFont.weight` and takes the
  type style's font. The three themes define the scale and all 51 new roles with first-pass
  materials; packages tune the ones they own.
- The M1 messages `BindingChanged` and `TextEntryRequested` and the systems
  `capture_key_bindings`, `enforce_max_lines`, `scroll_focus_into_view`, `refresh_key_glyphs`,
  `apply_set_values` are registered in `plugin.rs` / the modules' `build`.

## 2. Package A: typography, rich text, localisation arguments

Owns: `slotted-theme` (`tokens.rs`, `material.rs`, `theme.rs`, `apply.rs`), the typography and
`text.*` role entries in the three theme files, `slotted-ui/src/rich.rs`, `loc.rs`,
`widgets/text.rs` (new: `spawn_text` and `spawn_rich_text` move there), `def.rs`'s `TextRole`,
`slotted-packs/src/locale.rs`, tests `crates/slotted-ui/tests/rich_text.rs` and
`crates/slotted-theme/tests/typography.rs`.

### 2.1 Text node

`spawn_text` writes `TextLayout { justify, linebreak }` from `align` and `wrap` (`NoWrap` when
`wrap: false`), and `max_lines` as a `MaxLines(u16)` component that a `Render` system enforces by
truncating the resolved string with `…` until the laid-out line count fits (Bevy has no max-lines;
measure through `ComputedTextBlock` or `TextLayoutInfo`, re-truncate when the node's width changes).
`apply.rs` writes `TextFont.weight` from the material or the type style.

### 2.2 Rich text (`rich.rs`)

- `parse(markup: &str) -> Result<Vec<RichRun>, RichError>`; `RichRun { text: String, style: RunStyle
  { bold, italic, color: Option<ThemeColor>, size: Option<ThemeSize> }, kind: RunKind::{Text,
  Key(UiAction), Icon(Namespaced), Arg(String)} }`. Tags: `[b]…[/b]`, `[i]…[/i]`, `[color=$accent]`,
  `[color=#RRGGBB]`, `[size=heading]`, `{key:accept}`, `{icon:demo:chest}`, `{name}` (a Fluent
  argument, substituted from `args` before parsing so a value containing `[` is escaped by the
  substitution). `[[` and `{{` are literals. An unclosed tag is an error naming the offset; a
  parse error renders the raw markup and logs once.
- Rendering: one `Text` entity with a `TextSpan` child per run. Bold and italic set
  `TextFont.weight = 700` and `FontStyle::Italic`; `[color]` sets `TextColor`, `[size]` the span's
  `font_size` from the typography map. `{key:accept}` renders the first binding of the action for
  the current `InputMode` as text in the `text.key` role (`Enter`, `A`, `Esc`, `D-pad ↑`); it
  re-renders on `InputModeChanged` and when `UiBindings` changes. `{icon:x}` renders the item's
  localised name in the `text.icon` role when `inline: false`; when `inline: true` the node is a
  flex row and the icon is a sibling `ImageNode` through `IconSource` at line height.
- Localisation happens before parsing: the `key` resolves with `args`, and the resolved string
  is the markup. So `.ftl` files may contain tags. The `text` node stays plain (tags are literal
  there).

### 2.3 Localisation arguments

`Localizer::resolve(&self, key: &LocKey, args: &LocArgs) -> Option<String>`, `LocArgs =
BTreeMap<String, Value>`; `NoLocalization` ignores them; `LocaleTable` converts to `FluentArgs`.
`LocText { key, args }`; `resolve_loc_text` re-resolves when `args` change (`Changed<LocText>`).
The browser's status line (`FOLLOWUPS` "resolves two loc keys and formats a string") switches to
one key with args.

### 2.4 Tests (A)

Parser: every tag, nesting, escapes, an unclosed tag, `{{`, argument substitution with a `[` in
the value. Render: span count and per-span weight/colour/size; `{key:accept}` changes text when
`InputMode` flips; inline icon spawns an image sibling; a paragraph wraps at the panel width and
`max_lines: 2` truncates with `…`; `wrap: false` never wraps. Typography: every theme resolves
every `$` size; a dangling ref is reported; `Theme::missing_roles` still empty for three themes.

## 3. Package B: value store, activation, value controls

Owns: `values.rs`, `nav.rs` (`dispatch_focused_actions`, the slot Accept observer), `widgets.rs`
`spawn_button` (rewrite) and the new `widgets/{toggle,slider,select,radio_group,key_binding}.rs`,
those controls' role entries in the three theme files, tests `crates/slotted-ui/tests/values.rs`
and `controls.rs`.

### 3.1 Store

Section 1.1 as written, plus `ValueStore::bind_property` plumbing: a `ValueBinding` on a control
inserts `Added` seeding in `sync_value_bindings` (`Render`, before the controls' paint systems):
copy the store's value (or the menu property) into the control's state component when `version`
or `PropertyChanged` says it changed. A control never reads the store itself.

### 3.2 Button

Node: `height = control_height` (`compact` → compact), padding `spacing.md` × `spacing.sm`, an
optional icon child (`IconDef` through `IconSource`) before the label, the label a `LocText`.
Components: `Themed(button | button.primary | button.danger)`, `Focusable`, `TabIndex(0)`,
`SemanticRole::Button`, `ButtonState { variant, disabled, pressed: bool }`, `InteractionDisabled`
when disabled. Pointer: `Pointer<Press>` sets `Pressed`, `Pointer<Click>` triggers `Activate`
unless disabled. `FocusedAction::Accept` (fresh, any device) triggers `Activate` and claims.
Roles swap on hover/focus/pressed/disabled in `button_roles` (`Render`), like `slot_state_roles`.
`slotted:close` is a `Widget` whose `Activate` observer calls `pop_screen`. The rail and the
side tab keep working; the browser's buttons (`browser.button.*`) are untouched.

### 3.3 Toggle

A row: label, then the switch (a track with a thumb that slides `spacing.md` over `durations.fast`)
or a checkbox (a square with a tick glyph). `ToggleState { on: bool, style, disabled }`;
`Checked` mirrors `on`. Accept flips; pointer click anywhere on the row flips. Roles `toggle`,
`toggle.on`, `toggle.thumb`, `checkbox`, `checkbox.on` plus hover/focus/disabled.

### 3.4 Slider

A row: label, track with fill and thumb, a value readout (`format` with `{value}`, `{min}`,
`{max}`; `{value:.1}` precision). `SliderState { value, min, max, step, dragging }`. `Left/Right`
step by `step` (or 1 % of the range when 0), repeats already come from `emit_ui_actions`;
`PagePrev/PageNext` step by 10 %. Pointer: press on the track jumps, `Pointer<Drag>` on thumb or
track scrubs; every intermediate write is a `SetValue` with `source = the slider`, so a guard sees
each one. Roles `slider`, `slider.fill`, `slider.thumb`, `slider.text` plus states.

### 3.5 Select and radio group

`select`: a row with label and a value pill showing the current option's label between two
chevrons. `Left/Right` cycle in place and write. Accept in `Pointer` mode (or pointer click on the
pill) opens a popup: an absolute panel of option buttons under the pill (`select.popup`,
`select.option`, `select.option.active`), `Up/Down` move, Accept picks, `Back` closes and claims;
`enforce_focus_scope` must not bounce focus out of it (the popup is a child of the screen root).
`SelectState { index, open }`. `radio_group`: the same data as a row of segments
(`radio`, `radio.active`); `Left/Right` (row) move and write at once.

### 3.6 Key binding

A row: label (the action's name by default), the current binding as text through the same glyph
text as `{key:...}`. Accept enters capture: `KeyBindingState::capturing = true`, the value cell
shows `…`, the next fresh key (keyboard) or button (gamepad) press replaces the action's first
binding in `UiBindings`, writes `BindingChanged { action, device }`, claims every action that press
produced, and leaves capture. `Back` cancels capture and is claimed. Escape cannot be bound while
capturing.

### 3.7 Tests (B)

Store: rule clamp and snap, options check, guard refuse and rewrite, `ValueChanged` and
`ValueRefused` once each, `version` bumps, property binding round trip through `SetProperty` and
`PropertyChanged`. Controls: every control paints from the store on open, writes on Accept/arrows
/click, snaps back when a guard refuses, does nothing when disabled, claims what it consumed (a
slider's `Left` does not move focus), select popup opens in pointer mode only and `Back` closes it
without popping the screen, key capture rebinds `UiBindings` and the next `Accept` on a button
uses the new key. Role completeness stays green.

## 4. Package C: text field, containers, scrolling

Owns: `widgets/{text_field,list,scroll,tabs,decor}.rs`, those roles in the three theme files,
`widgets/virtual_grid.rs` (list reuse, `flex_shrink: 0` rows), the browser's `search_field.rs`
(becomes a thin wrapper over the widget, or is deleted if the widget covers it), tests
`crates/slotted-ui/tests/text_field.rs` and `containers.rs`.

### 4.1 Text field

Lift `search_field.rs`. Node: `height = control_height`, `EditableText` inside a themed frame
(`text_field`, `.focus`, `.disabled`), placeholder child, label to the left when given.
`TextFieldState { text, editing: bool }`. Focus alone does not edit: a focused field shows the
ring; Accept (or pointer click) enters editing, which gives Bevy's `InputFocus` to the
`EditableText` and sets `TextEntryFocused` (so keyboard actions stop); `Back` or Enter leaves
editing (Enter commits, `Back` reverts to the bound value) and claims. `filter` installs an
`EditableTextFilter`; `max_len` sets `max_characters`. Writes: a `SetValue` on commit (and on every
change when `filter != any`, so a numeric field can show a guard's clamp live). A gamepad Accept
also writes `TextEntryRequested { entity }` (a `Message`) for a game's on-screen keyboard;
nothing else happens on a pad.

### 4.2 Scroll and scrolling

`scroll` = `spawn_panel` with `overflow: scroll`, `ScrollArea` (Bevy's wheel observer),
`ScrollPosition`, and a `Scrollbar` child when `scrollbar` (roles `scroll.bar`, `scroll.thumb`,
reusing the virtual grid's look). Focus follow: a `Render` system triggers `ScrollIntoView` for the
`InputFocus` entity whenever it changes and sits under a `ScrollArea`. `PagePrev/PageNext` on a
focused descendant scroll by the visible height and claim. Direct children get `flex_shrink: 0`.
The virtual grid keeps its row model; the card grid is the browser's business.

### 4.3 List

A `virtual_grid` with `cols: 1` whose cells are `Focusable` rows (`list.row`, `.hover`, `.focus`,
`.selected`), height `control_height_compact`. `ListState { selected: Option<usize>, len }`.
`Up/Down` move focus row to row and scroll the window; Accept selects (writes `bind` as `Int`) and
triggers `Activate` on the row with its tags (the row's `VirtualCell` index in a `row` tag). The
`VirtualGridSource` is unchanged; `list` is a spawn-time configuration of it.

### 4.4 Tabs

A tab bar (`tabs.bar`, `tab`, `tab.active`, `tab.hover`, `tab.focus`) over a page area. Exactly
one child page is visible (`Visibility`), the rest `Hidden` and skipped by focus (`Focusable`
descendants of hidden pages are excluded by `enforce_focus_scope`'s successor: a `FocusMask`
component on hidden pages that `dispatch_focused_actions` and the ring both respect). `TabPrev/
TabNext` from anywhere in the screen switch tabs and claim; pointer click on a tab switches.
`TabsState { active: usize }`; `bind` writes the tab id. The browser's `TabRow` stays.

### 4.5 Decor

`separator`: a 1 px hairline (`separator` role) across the cross axis. `spacer`: an empty node,
`flex_grow: 1` when `fill`, else the given length. `image`: an `ImageNode` from the asset path,
`Pickable::IGNORE`, `Decorative`.

### 4.6 Tests (C)

Text field: Accept enters editing and `TextEntryFocused` is set, typing changes state, Enter
commits and writes, `Back` reverts and does not pop the screen, numeric filter drops letters,
gamepad Accept writes `TextEntryRequested`. Scroll: wheel moves `ScrollPosition`, focus-follow
brings a row into view, page actions scroll and claim, rows do not shrink. List: Up/Down walk and
scroll, Accept selects and activates with the row tag. Tabs: `TabNext` cycles and wraps, hidden
pages take no focus, `bind` carries the id. Decor lays out headless.

## 5. Package D: harness, demo screen, docs

Owns `slotted-test`, `assets/screens/demo_settings.screen.ron` (a test and showcase fixture, not
yet the M2 template), `docs/`, `CHANGELOG.md`, snapshots.

- Harness: `value(key) -> Option<Value>`, `set_value(key, value)`, `drag_slider(entity, fraction)`,
  `select_option(entity, id)`, `type_into(entity, text)` (Accept, type, Enter), `switch_tab(entity,
  id)`, `capture_key(entity, KeyCode)`; a `Locator::control(kind)` filter; Lua `slotted.test`
  gains `value`, `set_value`, `type_into`. `just gen-docs` after.
- Demo screen: `demo:settings` with three tabs (display: select, slider, toggle; audio: three
  sliders; controls: four `key_binding` rows and a `text_field`), a `scroll` list of twelve rows on
  the audio tab, a `rich_text` footer with `{key:back}`, in all three themes. Snapshot tests at
  1280x720 in three themes (`insta` tree snapshots plus the harness rect snapshot), and a
  gamepad-only walk that reaches every control from `initial_focus` and back.
- Docs: `docs/guide/screens.md` node table rows for every control and the grown `text`;
  `docs/guide/rich-text.md` (markup, glyph text, icons, `.ftl` interplay); `docs/guide/values.md`
  (store, rules, guards, bindings, property mirror); `docs/guide/themes.md` typography and control
  sizes; `docs/guide/input.md` focused actions and claims from controls; `CHANGELOG.md`;
  `FOLLOWUPS.md` "Menus M1" heading with everything the packages deferred.

## 6. Done when

- `just ci` green on native and wasm.
- `demo:settings` opens headless in three themes, every control is reachable by gamepad from
  `initial_focus`, every control writes and reads through the store, a guard refusal snaps a
  slider back, `Back` from a select popup closes the popup and a second `Back` pops the screen.
- A rebound `Accept` key activates a button (the M0 follow-up closes).
- `{key:accept}` in a rich text footer reads `Enter` on the keyboard and `A` on a pad.
- No control reads `ButtonInput<KeyCode>`; `grep -rn "KeyCode::" crates/slotted-ui/src/widgets`
  shows only `key_binding.rs`'s capture.
