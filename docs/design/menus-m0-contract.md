# Menus M0 contract: input model, focus ring, screen stack, layout model

Status: v1.1, 2026-09-07 (v1.1: nav links are tags, not a node field; observer signatures in the skeleton are final). Bevy 0.19.1. Companion to `docs/design/menus-proposal.md` (v0.3)
sections 4.1 to 4.4, which this makes binding and does not restate. Three packages: **A** (input
actions, input mode, focus ring, nav graph, theme roles) and **B** (screen stack, presentation,
layout model, HUD visibility) run in parallel without talking to each other; **C** (harness,
example migration, docs) runs after both. The skeleton compiles and carries the data types and the
module and system-set wiring; a body marked `// M0-IMPL: A|B|C` is that package's to fill.
Signatures change only by amending this file. Each package writes `docs/design/menus-m0-notes-{A,B,C}.md`.

## 0. Ground rules

- Dependency direction unchanged. Nothing new in `slotted-ui` needs a renderer; every new entity
  lays out headless and is inspectable through plain components.
- **One action vocabulary.** No system outside `actions.rs` reads `ButtonInput<KeyCode>` or a
  gamepad for a UI decision after M0, except text entry (Bevy's `EditableText`), the hotbar digit
  swap, the HUD edit toggle and the browser's letter hotkeys, which stay on raw keys because they
  are not navigation. The two hard-coded Escapes (browser recipe view, HUD editor drag cancel)
  become `Back` claims.
- **Claims decide who owns an action.** A consumer that acts on an action calls
  `UiActionClaims::claim(action)` in `SlottedUiSet::Input` after `UiActionEmit`. The stack's
  `pop_on_back` runs in `SlottedUiSet::Navigate` and pops only an unclaimed `Back`. Claims clear
  when the next frame emits.
- **Bevy's `InputFocus` stays the source of truth for focus.** A adds a ring and a policy, B adds
  a scope. Neither replaces the navigator.
- **`spawn_screen` and `close_screen` stay** as the low-level path and keep their signatures. The
  stack is a layer on top; a screen spawned directly is not in the stack and behaves as it does
  today, except that HUD visibility (5.4) treats it as a `page`.
- No wall clock: repeat timers, ring motion and transitions read `Time<Virtual>` through the
  existing motion machinery.
- Every enum written in data is a lowercase `string_enum!` (`def.rs`), never a RON variant.

## 1. Data (`def.rs`, `stack.rs`, `actions.rs`: done in the skeleton)

### 1.1 Actions

```rust
pub enum UiAction { Accept, Back, Secondary, Up, Down, Left, Right, TabPrev, TabNext, PagePrev, PageNext, Menu }
pub enum InputDevice { Pointer, Keyboard, Gamepad }
#[derive(Message)] pub struct UiActionEvent { pub action: UiAction, pub device: InputDevice, pub repeat: bool }
#[derive(Resource)] pub struct UiBindings {
    pub keys: BTreeMap<UiAction, Vec<KeyCode>>,
    pub buttons: BTreeMap<UiAction, Vec<GamepadButton>>,
    pub stick_deadzone: f32,          // 0.5
    pub repeat_delay: Option<Duration>, // None = tokens.durations.hover_delay
    pub repeat_every: Option<Duration>, // None = tokens.durations.fast
}
#[derive(Resource)] pub enum InputMode { Pointer, Keyboard, Gamepad }   // Default: Pointer
#[derive(Message)] pub struct InputModeChanged { pub from: InputMode, pub to: InputMode }
#[derive(Resource, Default)] pub struct UiActionClaims(BTreeSet<UiAction>);
impl UiActionClaims { pub fn claim(&mut self, a: UiAction); pub fn is_claimed(&self, a: UiAction) -> bool; }
```

`UiBindings::default()`: Accept = Enter, Space, South; Back = Escape, East; Secondary = KeyX,
West; Up/Down/Left/Right = arrows, WASD, d-pad, left stick; TabPrev/TabNext = KeyQ/KeyE,
LeftTrigger/RightTrigger (the bumpers, Bevy's names); PagePrev/PageNext = PageUp/PageDown,
LeftTrigger2/RightTrigger2; Menu = Tab, Start. `UiBindings` is `Serialize + Deserialize` (keys
and buttons through their Bevy `serialize` feature derives). `NavKeys` is deleted; nothing outside
the crate used it (verified: grep before deleting, fix any hit).

### 1.2 Node fields

Nav links are the reserved tag keys `nav.up`, `nav.down`, `nav.left`, `nav.right` (`Tags::NAV_*`),
on any node with tags, so no node variant changes shape and Lua writes them like any tag. Values
are node ids in the sense of `UiNodeDef::id` (a `test_id` tag). `SpawnCtx` turns them into a
`NavLinks { up, down, left, right: Option<String> }` component on the node (done in the skeleton).

`ScreenDef` gains `initial_focus: Option<String>` and `presentation: Presentation` (both serde
default). Both are the child's on inheritance when set, else the ancestor's.

### 1.3 Presentation

```rust
pub struct Presentation { pub mode: PresentationMode, pub scrim: Option<bool>, pub transition: Transition, pub back: BackPolicy }
string_enum! PresentationMode { Page = "page", Modal = "modal", Overlay = "overlay" }   // default Page
string_enum! Transition { Fade = "fade", SlideUp = "slide_up", SlideLeft = "slide_left", None = "none" } // default Fade
string_enum! BackPolicy { Pop = "pop", Ignore = "ignore" }   // default Pop
impl Presentation { pub fn scrim(&self) -> bool /* scrim.unwrap_or(mode == Modal) */ }
```

Semantics: `page` hides every stack entry below it and takes focus; `modal` keeps entries below
visible, draws a scrim, traps focus, takes focus; `overlay` neither takes focus nor blocks input
nor is counted by `pop_on_back` or HUD visibility.

### 1.4 Layout

`Layout` becomes:

| Field | Type | Default | Node mapping |
|---|---|---|---|
| `direction` | `LayoutDirection` | column | `flex_direction` |
| `gap` | `f32` steps | 0 | `row_gap`, `column_gap` |
| `padding` | `Padding` | 0 | `padding`; a bare number is all sides in steps, `(t, r, b, l)` per side in steps |
| `width`, `height` | `Option<Length>` | none | `width`, `height` |
| `min_width`, `max_width`, `min_height`, `max_height` | `Option<Length>` | none | `min_*`, `max_*` |
| `align` | `Align` | start | `align_items`: start, center, end, stretch |
| `justify` | `Justify` | start | `justify_content`: start, center, end, space_between |
| `grow` | `f32` | 0 | `flex_grow` |
| `wrap` | `bool` | false | `flex_wrap` |
| `overflow` | `Overflow` | visible | `overflow`: visible, scroll (`scroll_y`, plus `ScrollPosition` on the node) |
| `place` | `Option<Place { anchor: NineAnchor, offset: Vec2 }>` | none | `position_type: Absolute` and the nine-anchor inset/`UiTransform` the HUD wrapper uses |
| `center` | `bool` | false | deprecated alias: `center: true` with `align` unset means `align: center` |

`Length` is untagged serde: a number is `Px`, a string is `"50%"` (`Percent`), `"fill"`
(`Val::Percent(100)`), `"auto"`, or `"3s"` (`Steps`, multiplied by `spacing.sm`). `NineAnchor` moves
from `hud.rs` to `def.rs`; `hud.rs` re-exports it so nothing breaks. `layout_node(layout, spacing_sm)`
keeps its signature and maps all of the above.

## 2. Package A: input, focus ring, nav graph (`slotted-ui`, `slotted-theme`, `slotted-browser`, `assets/themes`)

### 2.1 Emitting actions (`actions.rs`)

- `emit_ui_actions` in `SlottedUiSet::Input`, in the labelled set `UiActionEmit` (first in `Input`).
  Reads `ButtonInput<KeyCode>`, every `Gamepad` entity, and `UiBindings`; writes one
  `UiActionEvent` per action per frame at most (two keys bound to `Up` both pressed = one event).
  `just_pressed` = `repeat: false`. Held directional actions (Up/Down/Left/Right only) repeat after
  `repeat_delay` then every `repeat_every`, `repeat: true`, on virtual time. Stick: an axis past
  `stick_deadzone` counts as pressed in that direction with hysteresis at half the deadzone.
- Clears `UiActionClaims` first thing.
- Keyboard-sourced actions other than `Back` are not emitted while `TextEntryFocused` is set;
  gamepad-sourced ones always are.
- `directional_nav_keys` becomes `directional_nav_actions`: drives `AutoDirectionalNavigator`
  from `Up/Down/Left/Right` events, after checking `NavLinks` (2.4). Runs in `Input` after
  `UiActionEmit`.

### 2.2 Input mode (`actions.rs`)

- `track_input_mode` before `UiActionEmit`: `Pointer` on a `CursorMoved` beyond 2 logical px since
  the last sample or any mouse button press; `Keyboard` on any `KeyCode` press; `Gamepad` on any
  gamepad button press or axis past the deadzone. Writes `InputModeChanged` on change.
- Nothing branches on `InputMode` except the ring (2.3) and, later, the hint bar.

### 2.3 Focus ring (`focus_ring.rs`)

- One entity, spawned at startup when `SlottedUiConfig::spawn_layers`, marked `FocusRing`, in
  `zbands::FOCUS` (new, 900), `Pickable::IGNORE`, `Themed(roles::FOCUS_RING)`, absolute,
  `Visibility::Hidden`. Component `FocusRingState { target: Option<Entity>, visible: bool }` is
  what tests read.
- `update_focus_ring` in `SlottedUiSet::Render`: target = `InputFocus` if that entity has
  `Focusable`; visible = target is some and `InputMode != Pointer`. Rect = target's computed rect
  grown by `tokens.spacing.xs` on every side, in logical pixels through `UiUnits`. The ring moves
  with a `Translate` and `Size` tween of `durations.fast`, `Easing::Standard`; `Motion.reduced` or
  a target on a different screen root snaps. Visibility toggles through the `Fade` preset.
- `Focusable` is a marker component inserted by `spawn_button`, `spawn_slot`, the icon button,
  the side tab header, the virtual grid cells and the browser's search field and cards. `TabIndex`
  alone does not make a node focusable for the ring.
- Roles: `focus.ring` and `scrim` added to `roles::ALL` (38) and to all three themes. Glass: a
  2 px accent `Solid` border with `radii.md` and no fill; paper: 2 px ink `Dashed`; neon: magenta
  `Solid` with `elevation: low`. `scrim`: glass `Solid` `#0B0E1499`; paper `#00000033`; neon
  `#000000B3`.

### 2.4 Nav graph (`nav.rs`)

- `NavLinks` component from the node field. On a directional event, walk from the focused entity
  up through `ChildOf` to the screen root looking for a `NavLinks` with a link in that direction;
  the first found wins. Resolve the id to an entity by `TestId` inside the same screen root, then
  focus its first `Focusable` descendant (itself if focusable). If no link resolves, fall through to
  the auto navigator. Log once per unresolved id per screen spawn.
- `initial_focus`: on `ScreenSpawned`, if the screen's `ScreenDef` names one, focus that node's
  first focusable descendant; else the first `Focusable` in tree order. Only for `page` and `modal`
  (an `overlay` never takes focus) and only when the screen is the top of the stack or not in the
  stack. Record the choice in `ScreenRoot::initial_focus: Option<Entity>` (new field).

### 2.5 Claims in the browser and the HUD editor

- `slotted-browser`: when the recipe view is open, `Back` closes it and is claimed. The existing
  `KeyMappings.close` remains a raw key for people who rebind it; when it is `Escape` (the default)
  the raw path is skipped so the same press is not handled twice.
- `hud_editor.rs`: while a drag is in progress, `Back` cancels it and is claimed; the raw Escape
  read goes.

### 2.6 Tests (A)

`crates/slotted-ui/tests/actions.rs` and `focus_ring.rs`, headless through `slotted-testutils`:
default bindings emit one event per action; two keys bound to one action emit once; repeat fires
after the delay then every interval on virtual time and not for `Accept`; stick past the deadzone
emits `Right` and releases at half; text entry suppresses keyboard `Accept` and not `Back` and not
gamepad `Accept`; `InputMode` follows the last device; ring is hidden in `Pointer`, shown on the
first key press, follows focus, snaps under reduced motion; a `NavLinks` link beats the navigator;
an unresolved link falls through; `initial_focus` lands where named and on the first focusable
otherwise. Theme completeness test covers 38 roles in three themes.

## 3. Package B: stack, presentation, layout (`slotted-ui`)

### 3.1 The stack (`stack.rs`)

```rust
#[derive(Resource, Default)] pub struct ScreenStack { entries: Vec<StackEntry> }
pub struct StackEntry { pub root: Entity, pub kind: ScreenKind, pub presentation: Presentation, pub menu: Option<Entity>, pub focus: Option<Entity> }
impl ScreenStack {
    pub fn entries(&self) -> &[StackEntry];
    pub fn top(&self) -> Option<&StackEntry>;              // topmost non-overlay
    pub fn top_any(&self) -> Option<&StackEntry>;          // topmost of all
    pub fn is_open(&self, kind: &ScreenKind) -> bool;
    pub fn kinds(&self) -> Vec<ScreenKind>;
}
pub fn push_screen(commands, def: Arc<ScreenDef>, menu: Option<Entity>) -> Entity;
pub fn replace_screen(commands, def, menu) -> Entity;     // pop top (non-overlay) then push
pub fn pop_screen(commands);                               // top non-overlay
pub fn pop_to(commands, kind: &ScreenKind);                // pops until `kind` is top; no-op if absent
pub fn clear_screens(commands);
#[derive(Message)] pub struct StackChanged { pub kinds: Vec<ScreenKind> }
```

- Each is a `Command`. Push resolves through `Screens` like `SpawnScreen` (call `spawn_screen`
  under the hood so the two paths never diverge), records the entry, then applies 3.2. Pop closes
  the root with `close_screen` and, when the entry has a menu, queues
  `slotted_ecs::close_menu(menu, OpenMenu.id)` so the carried stack returns exactly as it does
  today. `close_screen` on a root the stack holds removes the entry (an observer on
  `ScreenClosed`), so a game that mixes the two paths cannot leave a dangling entry.
- Every change writes `StackChanged`.

### 3.2 Presentation effects

- Z: entry `i` gets `GlobalZIndex(zbands::SCREEN + 2 * i)`; its scrim, when `presentation.scrim()`,
  is a sibling full-window node at `SCREEN + 2 * i - 1`, `Themed(roles::SCRIM)`, `Pickable` blocking,
  marked `Scrim { for_root }`, despawned with the entry.
- Visibility: after any change, every entry below the topmost `page` is `Visibility::Hidden`, the
  rest `Inherited`. An entry hidden this way keeps its state; nothing despawns.
- Focus scope: `enforce_focus_scope` in `SlottedUiSet::Navigate`: if `InputFocus` is set to an
  entity whose screen root is not `top()` (and is in the stack), reset it to that top entry's
  recorded `focus` or initial focus. A screen not in the stack is never corrected.
- Focus record: `record_stack_focus` in `Navigate` writes `InputFocus` into the owning entry's
  `focus` each frame. Pop restores `InputFocus` to the new top's `focus`, falling back to its
  `ScreenRoot::initial_focus`.
- Transitions on push only (pop is immediate in M0; recorded as a follow-up): `Fade` tweens the
  root's alpha 0 to 1 over `durations.normal`; `SlideUp` and `SlideLeft` add a `Translate` from
  `spacing.xl` in the named direction; `None` nothing. `Motion.reduced` collapses all to `Fade` of
  `durations.fast`. Use the existing `MotionTarget`/`TweenTarget` machinery in `motion.rs`; add
  `MotionPreset::Slide` in `slotted-theme` with the three themes' values.

### 3.3 Back

`pop_on_back` in `Navigate`: for each unclaimed `Back` event this frame, if `top()` exists and its
`presentation.back == Pop`, `pop_screen`. One pop per frame. A screen not in the stack is untouched
(the game handles its own Escape as before).

### 3.4 HUD visibility

`hud_screen_visibility` hides `hide_with_screen` layers when the stack has a `page` or `modal`
entry, or when any `ScreenRoot` exists that the stack does not hold.

### 3.5 Layout

Implement 1.4 in `layout_node` and `spawn_panel`; lift the nine-anchor inset maths into a
`pub fn nine_anchor_node(anchor, offset, size) -> Node` in `def.rs` or `layers.rs` and make
`HudAnchor::node` call it. `overflow: scroll` inserts `ScrollPosition::default()`; wheel scrolling
of such a panel is M1.

### 3.6 Tests (B)

`crates/slotted-ui/tests/stack.rs` and `layout.rs`: push/pop order and `StackChanged`; pop closes
the menu and the carried stack returns (reuse the Phase 2 conservation assertion); page hides the
entry below and pop reveals it; modal keeps it visible and spawns a scrim above it and below the
modal in z; overlay takes no focus and `Back` pops the screen under it; `pop_on_back` respects
`Ignore` and a claim; `enforce_focus_scope` bounces focus back; focus is restored on pop;
`close_screen` on a stacked root removes the entry; `pop_to` an absent kind is a no-op; layout
maps every field (assert on `Node`), `Length` parses each spelling and rejects `"12px"` with a
message, `place: bottom_right` puts the rect in the corner headless, `center: true` still centres.

## 4. Package C: harness, migration, docs (after A and B)

### 4.1 `slotted-test`

- `UiHarness::open(&mut self, screen: impl ScreenSource) -> Entity`: pushes a menu-less screen
  through the stack and steps one frame. `open_screen` (with a fixture) now pushes through the
  stack too, so `Opened.screen` is a stack entry; conservation census unchanged.
- Actions: `gamepad(GamepadButton)` (press and release across two frames), `gamepad_hold`,
  `gamepad_release`, `stick(Vec2)` (left stick, held until `stick(Vec2::ZERO)`), `action(UiAction)`
  (presses the first keyboard binding of that action), `set_input_mode(InputMode)`.
- Readers: `focused() -> Option<Entity>`, `input_mode() -> InputMode`, `stack() -> Vec<ScreenKind>`,
  `focus_ring() -> FocusRingState`. The harness spawns one `Gamepad` entity at build if the replay
  cursor did not already; reuse `cursor.rs`'s creation.
- The Lua `slotted.test` API gains `gamepad(button)`, `action(name)` and `focused()` returning the
  focused node's tags, routed through the same functions.

### 4.2 Migration

- `examples/showcase/src/chest.rs`, `examples/machine`, `examples/modded`, and every web-playground
  scene push through the stack. The chest's own Escape handler goes; `E` still reopens. Multiplayer
  and testing scenes: same, keep their assertions green. HUD scene: unchanged.
- The chest, machine and modded screen files gain `initial_focus` on their first grid and a
  `presentation` line even where it is the default, so the files document the field.

### 4.3 Docs

- `docs/guide/screens.md`: `presentation`, `initial_focus`, `nav`, the full layout table, the stack
  API with a push/pop example, and the note that direct `spawn_screen` is the low-level path.
- New `docs/guide/input.md`: actions, bindings, input mode, claims, the ring, and the harness
  gamepad API. `docs/guide/testing.md` links to it.
- `docs/PLAN.md`: a "Menus" section pointing at the proposal and this contract; status line bumps.
- `CHANGELOG.md` entries under Unreleased for every public change, including the `NavKeys` removal
  and the `Layout` field additions.

### 4.4 Tests (C)

`crates/slotted-test/tests/gamepad.rs`: open the chest headless, `set_input_mode(Gamepad)`, walk
every slot with `gamepad(DPadRight)` and assert the ring follows; `gamepad(South)` on a slot picks
up; `gamepad(East)` pops the screen and the menu closes with items conserved. `examples/showcase`
tests: Escape closes the chest through the stack. `just ci` green on native and wasm.

## 5. Done when

- `just ci` passes; test count grows in each package's notes.
- The chest screen opens through the stack, Escape or East closes it with items conserved, and a
  gamepad walks its slots with a visible ring in all three themes.
- A menu-less screen from a RON file opens through `UiHarness::open`, `initial_focus` lands, and
  `place: bottom_right` lays out in the corner headless.
- No `KeyCode::Escape` literal remains outside `UiBindings::default()`, `hud_editor.rs`'s edit
  toggle and the browser's `KeyMappings`.
