//! The one action vocabulary (menus contract 2.1, 2.2, 2.6): keys, buttons
//! and the left stick become `UiActionEvent`s, held directions repeat on
//! virtual time, text entry gates the keyboard, and `InputMode` follows the
//! last device the player touched.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::time::Duration;

use bevy::input::ButtonState;
use bevy::input::gamepad::{
    Gamepad, GamepadAxis, GamepadButton, RawGamepadAxisChangedEvent, RawGamepadButtonChangedEvent,
    RawGamepadEvent,
};
use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::input_focus::{FocusCause, InputFocus};
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use slotted_test::prelude::*;
use slotted_ui::{
    InputDevice, InputMode, InputModeChanged, SemanticRole, UiAction, UiActionEmit, UiActionEvent,
    UiBindings,
};

/// Every `UiActionEvent` the app emitted, in order, until drained. A
/// message buffer only lives two frames, so a test that advances virtual
/// time by hundreds of milliseconds has to collect as it goes.
#[derive(Resource, Default, Debug)]
struct Captured(Vec<UiActionEvent>);

fn capture(mut events: MessageReader<UiActionEvent>, mut captured: ResMut<Captured>) {
    captured.0.extend(events.read().copied());
}

struct CapturePlugin;

impl Plugin for CapturePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Captured>()
            .add_systems(Update, capture.after(UiActionEmit));
    }
}

fn harness() -> UiHarness {
    UiHarness::builder()
        .plugins((SlottedPlugins::headless(), CapturePlugin))
        .registries(TestRegistries::basic())
        .theme("glass")
        .build()
}

/// Clears the capture so a test starts counting from here.
fn reset(h: &mut UiHarness) {
    h.world_mut().resource_mut::<Captured>().0.clear();
}

/// Everything emitted since the last drain or reset.
fn drain(h: &mut UiHarness) -> Vec<UiActionEvent> {
    std::mem::take(&mut h.world_mut().resource_mut::<Captured>().0)
}

fn spawn_gamepad(h: &mut UiHarness) -> Entity {
    h.world_mut().spawn(Gamepad::default()).id()
}

fn button(h: &mut UiHarness, pad: Entity, button: GamepadButton, pressed: bool) {
    let value = if pressed { 1.0 } else { 0.0 };
    h.world_mut()
        .write_message(RawGamepadEvent::Button(RawGamepadButtonChangedEvent::new(
            pad, button, value,
        )));
    h.step(1);
}

fn stick_x(h: &mut UiHarness, pad: Entity, value: f32) {
    h.world_mut()
        .write_message(RawGamepadEvent::Axis(RawGamepadAxisChangedEvent::new(
            pad,
            GamepadAxis::LeftStickX,
            value,
        )));
    h.step(1);
}

/// Press several keys in the same frame, then step once.
fn press_together(h: &mut UiHarness, codes: &[KeyCode]) {
    let window = h
        .world_mut()
        .query_filtered::<Entity, With<PrimaryWindow>>()
        .single(h.world())
        .expect("primary window");
    for code in codes {
        h.world_mut().write_message(KeyboardInput {
            key_code: *code,
            logical_key: Key::Unidentified(bevy::input::keyboard::NativeKey::Unidentified),
            state: ButtonState::Pressed,
            text: None,
            repeat: false,
            window,
        });
    }
    h.step(1);
}

fn only(events: &[UiActionEvent], action: UiAction) -> Vec<UiActionEvent> {
    events
        .iter()
        .copied()
        .filter(|e| e.action == action)
        .collect()
}

// ---------------------------------------------------------------------------
// Emission
// ---------------------------------------------------------------------------

#[test]
fn default_bindings_emit_one_event_per_action_from_each_device() {
    let mut h = harness();
    let pad = spawn_gamepad(&mut h);
    let bindings = h.world().resource::<UiBindings>().clone();
    reset(&mut h);
    h.step(1);
    drain(&mut h);

    for action in UiAction::ALL {
        let key = bindings.first_key(action).expect("a key per action");
        h.key(key);
        let got = drain(&mut h);
        assert_eq!(
            only(&got, action),
            vec![UiActionEvent {
                action,
                device: InputDevice::Keyboard,
                repeat: false,
            }],
            "{action:?} from {key:?}: {got:?}"
        );

        let b = bindings.first_button(action).expect("a button per action");
        button(&mut h, pad, b, true);
        button(&mut h, pad, b, false);
        let got = drain(&mut h);
        assert_eq!(
            only(&got, action),
            vec![UiActionEvent {
                action,
                device: InputDevice::Gamepad,
                repeat: false,
            }],
            "{action:?} from {b:?}: {got:?}"
        );
    }
}

#[test]
fn two_keys_bound_to_one_action_pressed_together_emit_once() {
    let mut h = harness();
    reset(&mut h);
    h.step(1);
    drain(&mut h);

    press_together(&mut h, &[KeyCode::ArrowUp, KeyCode::KeyW]);
    let got = drain(&mut h);
    assert_eq!(only(&got, UiAction::Up).len(), 1, "{got:?}");
    h.release(KeyCode::ArrowUp);
    h.release(KeyCode::KeyW);
}

#[test]
fn a_held_direction_repeats_after_the_delay_then_every_interval_on_virtual_time() {
    let mut h = harness();
    {
        let mut bindings = h.world_mut().resource_mut::<UiBindings>();
        bindings.repeat_delay = Some(Duration::from_millis(200));
        bindings.repeat_every = Some(Duration::from_millis(50));
    }
    let frame = h.frame_delta();
    reset(&mut h);
    h.step(1);
    drain(&mut h);

    h.hold(KeyCode::ArrowRight);
    let got = drain(&mut h);
    assert_eq!(
        only(&got, UiAction::Right),
        vec![UiActionEvent {
            action: UiAction::Right,
            device: InputDevice::Keyboard,
            repeat: false,
        }]
    );

    // Well inside the delay: silence.
    h.advance(Duration::from_millis(150));
    assert!(
        only(&drain(&mut h), UiAction::Right).is_empty(),
        "no repeat before the delay"
    );

    // Past the delay: the first repeat, and it is flagged as one.
    h.advance(Duration::from_millis(70));
    let got = only(&drain(&mut h), UiAction::Right);
    assert_eq!(got.len(), 1, "{got:?}");
    assert!(got[0].repeat);
    assert_eq!(got[0].device, InputDevice::Keyboard);

    // Then one every 50 ms: 300 ms is six, give or take frame quantisation.
    h.advance(Duration::from_millis(300));
    let got = only(&drain(&mut h), UiAction::Right);
    let expected = 300 / 50;
    assert!(
        (expected - 1..=expected + 1).contains(&got.len()),
        "{} repeats over 300 ms at {frame:?} per frame",
        got.len()
    );
    assert!(got.iter().all(|e| e.repeat));

    // Release: silence again.
    h.release(KeyCode::ArrowRight);
    h.advance(Duration::from_millis(300));
    assert!(only(&drain(&mut h), UiAction::Right).is_empty());
}

#[test]
fn accept_never_repeats_while_held() {
    let mut h = harness();
    {
        let mut bindings = h.world_mut().resource_mut::<UiBindings>();
        bindings.repeat_delay = Some(Duration::from_millis(50));
        bindings.repeat_every = Some(Duration::from_millis(20));
    }
    reset(&mut h);
    h.step(1);
    drain(&mut h);

    h.hold(KeyCode::Enter);
    h.advance(Duration::from_millis(500));
    let got = only(&drain(&mut h), UiAction::Accept);
    assert_eq!(got.len(), 1, "{got:?}");
    assert!(!got[0].repeat);
    h.release(KeyCode::Enter);
}

#[test]
fn the_stick_presses_past_the_deadzone_and_releases_at_half() {
    let mut h = harness();
    {
        let mut bindings = h.world_mut().resource_mut::<UiBindings>();
        // Keep repeats out of the picture.
        bindings.repeat_delay = Some(Duration::from_secs(60));
    }
    let pad = spawn_gamepad(&mut h);
    reset(&mut h);
    h.step(1);
    drain(&mut h);

    // Below the deadzone (0.5): nothing.
    stick_x(&mut h, pad, 0.4);
    assert!(only(&drain(&mut h), UiAction::Right).is_empty());

    // Past it: one `Right` from the pad.
    stick_x(&mut h, pad, 0.8);
    let got = only(&drain(&mut h), UiAction::Right);
    assert_eq!(got.len(), 1, "{got:?}");
    assert_eq!(got[0].device, InputDevice::Gamepad);
    assert!(!got[0].repeat);

    // Back under the deadzone but above half of it: still held, no new press.
    stick_x(&mut h, pad, 0.35);
    h.step(2);
    assert!(only(&drain(&mut h), UiAction::Right).is_empty());

    // Under half: released. Past the deadzone again: a fresh press.
    stick_x(&mut h, pad, 0.1);
    h.step(1);
    drain(&mut h);
    stick_x(&mut h, pad, 0.9);
    let got = only(&drain(&mut h), UiAction::Right);
    assert_eq!(got.len(), 1, "{got:?}");
    assert!(!got[0].repeat);

    // The other way is `Left`, and never `Right`.
    stick_x(&mut h, pad, 0.0);
    h.step(1);
    drain(&mut h);
    stick_x(&mut h, pad, -0.9);
    let got = drain(&mut h);
    assert_eq!(only(&got, UiAction::Left).len(), 1, "{got:?}");
    assert!(only(&got, UiAction::Right).is_empty());
}

#[test]
fn text_entry_suppresses_keyboard_accept_but_not_back_or_the_gamepad() {
    let mut h = harness();
    let pad = spawn_gamepad(&mut h);
    let field = h
        .world_mut()
        .spawn((Node::default(), SemanticRole::TextField))
        .id();
    h.set_focus(Some(field));
    reset(&mut h);
    h.step(1);
    drain(&mut h);

    h.key(KeyCode::Enter);
    assert!(
        only(&drain(&mut h), UiAction::Accept).is_empty(),
        "keyboard Accept is the field's"
    );
    h.key(KeyCode::ArrowRight);
    assert!(
        only(&drain(&mut h), UiAction::Right).is_empty(),
        "arrows move the caret, not the focus"
    );

    h.key(KeyCode::Escape);
    let got = only(&drain(&mut h), UiAction::Back);
    assert_eq!(got.len(), 1, "{got:?}");
    assert_eq!(got[0].device, InputDevice::Keyboard);

    button(&mut h, pad, GamepadButton::South, true);
    button(&mut h, pad, GamepadButton::South, false);
    let got = only(&drain(&mut h), UiAction::Accept);
    assert_eq!(got.len(), 1, "{got:?}");
    assert_eq!(got[0].device, InputDevice::Gamepad);
}

// ---------------------------------------------------------------------------
// Input mode
// ---------------------------------------------------------------------------

#[test]
fn input_mode_follows_the_last_device_and_announces_each_change() {
    let mut h = harness();
    let pad = spawn_gamepad(&mut h);
    let mut changes = h
        .world()
        .resource::<Messages<InputModeChanged>>()
        .get_cursor_current();
    let drain = |h: &UiHarness,
                 c: &mut bevy::ecs::message::MessageCursor<InputModeChanged>|
     -> Vec<InputModeChanged> {
        c.read(h.world().resource::<Messages<InputModeChanged>>())
            .copied()
            .collect()
    };
    h.step(1);
    drain(&h, &mut changes);
    assert_eq!(*h.world().resource::<InputMode>(), InputMode::Pointer);

    h.key(KeyCode::ArrowDown);
    assert_eq!(*h.world().resource::<InputMode>(), InputMode::Keyboard);
    assert_eq!(
        drain(&h, &mut changes),
        vec![InputModeChanged {
            from: InputMode::Pointer,
            to: InputMode::Keyboard,
        }]
    );

    button(&mut h, pad, GamepadButton::South, true);
    button(&mut h, pad, GamepadButton::South, false);
    assert_eq!(*h.world().resource::<InputMode>(), InputMode::Gamepad);
    assert_eq!(
        drain(&h, &mut changes),
        vec![InputModeChanged {
            from: InputMode::Keyboard,
            to: InputMode::Gamepad,
        }]
    );

    // A one-pixel jog is not reaching for the mouse; a real move is.
    h.pointer_move_to(Vec2::new(100.0, 100.0));
    assert_eq!(*h.world().resource::<InputMode>(), InputMode::Pointer);
    h.key(KeyCode::ArrowUp);
    assert_eq!(*h.world().resource::<InputMode>(), InputMode::Keyboard);
    h.pointer_move_to(Vec2::new(101.0, 100.0));
    assert_eq!(
        *h.world().resource::<InputMode>(),
        InputMode::Keyboard,
        "a sub-threshold move keeps the keyboard"
    );
    h.pointer_move_to(Vec2::new(140.0, 100.0));
    assert_eq!(*h.world().resource::<InputMode>(), InputMode::Pointer);

    // The stick switches once when it crosses the deadzone.
    stick_x(&mut h, pad, 0.9);
    assert_eq!(*h.world().resource::<InputMode>(), InputMode::Gamepad);
    let got = drain(&h, &mut changes);
    assert_eq!(got.last().map(|c| c.to), Some(InputMode::Gamepad));
    stick_x(&mut h, pad, 0.0);
}

#[test]
fn the_same_key_press_does_not_flip_the_mode_when_focus_is_set_by_hand() {
    // Setting `InputFocus` directly is what a harness does; it is not a
    // device and must not show the ring.
    let mut h = harness();
    let e = h.world_mut().spawn(Node::default()).id();
    h.world_mut()
        .resource_mut::<InputFocus>()
        .set(e, FocusCause::Navigated);
    h.step(2);
    assert_eq!(*h.world().resource::<InputMode>(), InputMode::Pointer);
}
