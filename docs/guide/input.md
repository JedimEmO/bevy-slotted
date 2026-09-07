# Input

Every navigation decision in the UI reads one vocabulary, `UiAction`. The
keyboard, the gamepad's buttons and its left stick all map onto it through
`UiBindings`, so a screen written once walks the same on a pad as on the arrow
keys, and a game that rebinds a key rebinds it in one place. Nothing outside
`slotted-ui`'s emitter reads a raw key or a gamepad for a UI decision; the
exceptions are text entry (Bevy's `EditableText`), the hotbar digit swap, the
HUD edit toggle and the browser's letter hotkeys, which are not navigation.

## Actions

| `UiAction` | Keyboard | Gamepad | What it means |
|---|---|---|---|
| `Accept` | Enter, Space | South | Activate the focused widget. On a focused slot, a left click. |
| `Back` | Escape | East | Cancel, close, go back. Unclaimed, it pops the screen stack. |
| `Secondary` | X | West | The widget's secondary action. |
| `Up`, `Down`, `Left`, `Right` | arrows, WASD | d-pad, left stick | Move focus. These repeat while held. |
| `TabPrev`, `TabNext` | Q, E | left bumper, right bumper | Previous and next tab. |
| `PagePrev`, `PageNext` | Page Up, Page Down | left trigger, right trigger | Previous and next page of a list. |
| `Menu` | Tab | Start | Open the menu. |

An action arrives as a `UiActionEvent { action, device, repeat }` message, at
most one per action per frame: two keys bound to `Up` pressed together are one
event. `device` is the `InputDevice` that produced it. A fresh press is
`repeat: false`; a held direction fires again after `repeat_delay` (the theme's
`durations.hover_delay` by default) and then every `repeat_every`
(`durations.fast`), on virtual time, so the keyboard and the pad walk a grid at
the same cadence. Nothing but the four directions repeats.

The left stick counts as pressed in a direction past `stick_deadzone` (0.5)
and releases at half of that, so a stick resting near the edge of the deadzone
does not chatter. A stick and a d-pad produce the same event.

While a text field has focus, keyboard-sourced actions other than `Back` are
not emitted: the field sees its own arrows and its own letters. Gamepad-sourced
actions always are.

## Bindings

`UiBindings` is a resource: a map from action to keys, a map from action to
gamepad buttons, the stick deadzone and the two repeat timings. It serialises,
so a game persists it with its settings and a key-binding control edits it in
place.

```rust
fn rebind(mut bindings: ResMut<UiBindings>) {
    bindings.keys.insert(UiAction::Back, vec![KeyCode::Backspace, KeyCode::Escape]);
    bindings.repeat_every = Some(Duration::from_millis(60));
}
```

`first_key(action)` and `first_button(action)` are what the harness and a hint
glyph read. The old `NavKeys` resource is gone; the arrow keys live here now.

## Consuming an action, and claims

A system that acts on an action reads `UiActionEvent` in `SlottedUiSet::Input`
after the `UiActionEmit` set, and tells the rest of the frame it did so:

```rust
fn close_my_popup(
    mut events: MessageReader<UiActionEvent>,
    mut claims: ResMut<UiActionClaims>,
    popup: Res<PopupOpen>,
    mut commands: Commands,
) {
    if popup.0 && events.read().any(|e| e.action == UiAction::Back) {
        claims.claim(UiAction::Back);
        commands.queue(close_popup);
    }
}

app.add_systems(Update, close_my_popup.in_set(SlottedUiSet::Input).after(UiActionEmit));
```

Claims decide who owns an action this frame. The screen stack's `pop_on_back`
runs later, in `SlottedUiSet::Navigate`, and pops only an unclaimed `Back`; a
search field that blurs on Escape, a recipe view that closes, a HUD drag that
cancels, all claim it, so one press does one thing. Claims clear when the next
frame emits. The same goes for `Accept`: a focused slot or button takes it and
claims it.

## Input mode

`InputMode` is a resource that remembers the last device the player touched:
`Pointer` (the default) on a mouse move past 2 logical pixels or a button
press, `Keyboard` on any key, `Gamepad` on any button or a stick past the
deadzone. `InputModeChanged { from, to }` is written on each change. Exactly two
things branch on it: the focus ring, and the hint bar's glyph set. Nothing else
should.

## Focus and the ring

Bevy's `InputFocus` stays the source of truth for what is focused. The crate
adds a policy and a visual on top:

- Every interactive widget carries a `Focusable` marker: slots, buttons, icon
  buttons, side tab headers, virtual grid cells, the browser's search field
  and its cards. `TabIndex` alone does not make a node focusable for the ring.
- A screen takes focus when it opens, on its `initial_focus` node or the first
  focusable in tree order ([screens.md](screens.md#focus-and-nav-links)), and
  only when it is a `page` or a `modal` on top of the stack.
- Inside a container, Bevy's directional navigator picks the neighbour.
  Between containers, `nav.up`, `nav.down`, `nav.left` and `nav.right` tags on
  a node name where focus goes next.
- The stack keeps focus inside the top entry and restores each entry's last
  focus when it regains the top.

The focus ring is one entity, `FocusRing`, in its own z band above every
screen, that follows the focused `Focusable`. It shows in `Keyboard` and
`Gamepad` mode and hides in `Pointer` mode, so a mouse user never sees it and
the first key press brings it up. It sits `spacing.xs` outside the focused
node's rectangle and slides between targets on the theme's `fast` duration;
under reduced motion, on its first placement, and when the target is on a
different screen, it snaps. The `focus.ring` theme role draws it (glass: a
solid accent border; paper: a dashed ink stroke; neon: a solid magenta border
with a low elevation), and the `scrim` role is what a modal draws under
itself. `FocusRingState` on
the ring entity is what a test reads: the target and whether it is visible.

## Driving it from a test

`slotted-test`'s harness owns one `Gamepad` entity and writes the same raw
events a real pad does, so everything above runs unchanged headless:

```rust
let (mut h, opened) = open_chest();
h.set_input_mode(InputMode::Gamepad);
assert!(h.focus_ring().visible);

h.gamepad(GamepadButton::DPadRight);          // press one frame, release the next
h.settle();
assert_eq!(h.focused(), Some(chest_slot(&h, 1)));
assert_eq!(h.focus_ring().target, h.focused());

h.gamepad(GamepadButton::South);              // Accept: pick the stack up
h.settle();
assert!(h.carried(opened.menu).is_some());

h.stick(Vec2::new(1.0, 0.0));                 // held until stick(Vec2::ZERO)
h.action(UiAction::Back);                     // the first key bound to Back
h.settle();
assert!(h.stack().is_empty());
h.assert_conserved();
```

`gamepad_hold` and `gamepad_release` split a press when a test wants the
repeat; `input_mode()`, `stack()`, `focused()` and `focus_ring()` are the
readers. A mod's Lua test has `slotted.test.gamepad(button)`,
`slotted.test.action(name)` and `slotted.test.focused()`; see
[testing.md](testing.md) and [api/lua.md](api/lua.md#slottedtest).

A recorded session (`--record`) stores gamepad presses too, and a replay feeds
them through the same raw stream, so a d-pad walk someone reproduced by hand
replays as a test.
