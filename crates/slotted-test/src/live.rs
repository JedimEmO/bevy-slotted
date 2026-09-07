//! Perform test ops against a running app, one per frame. Phase 6 contract
//! section 3.2: the playground's Tests tab drives this from a resource. It
//! shares [`crate::lua_tests::ops`] with the harness driver.

use bevy::camera::NormalizedRenderTarget;
use bevy::input::ButtonState;
use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::picking::pointer::{Location, PointerAction, PointerButton, PointerId, PointerInput};
use bevy::prelude::*;
use bevy::ui::ui_transform::UiGlobalTransform;
use bevy::window::{PrimaryWindow, WindowRef};
use slotted_model::Value;
use slotted_script::{TestLocator, TestOp};
use slotted_ui::UiAction;

use crate::lua_tests::{StepOutcome, TestDriver, ops};

/// Quiet frames in a row that make a `Settle` done.
const QUIET_FRAMES: u32 = 2;

/// A driver over the live world. `Settle` is pending until `ActiveMotions ==
/// 0 && PendingRoundTrips == 0` for two consecutive frames; `Step` counts
/// frames; `OpenScreen` uses the screen that is open and fails when its kind
/// differs.
#[derive(Debug, Default)]
pub struct LiveDriver {
    /// Frames the current op has waited.
    pub waited: u32,
    /// Quiet frames seen in a row while settling.
    pub quiet: u32,
}

impl LiveDriver {
    /// Forgets where the current op had got to. Called whenever one finishes,
    /// so the next op starts from its first sub-step.
    fn finish(&mut self, value: Result<Value, String>) -> StepOutcome {
        self.waited = 0;
        self.quiet = 0;
        StepOutcome::Done(value)
    }

    /// One more frame on the same op.
    fn wait(&mut self) -> StepOutcome {
        self.waited += 1;
        StepOutcome::Pending
    }

    /// A click as three frames: move, press, release. Hit testing, z-order
    /// and `Hovered` all run in between, which is the point of driving the
    /// live app rather than poking components.
    fn click(
        &mut self,
        world: &mut World,
        loc: &TestLocator,
        button: PointerButton,
    ) -> StepOutcome {
        let entity = match ops::resolve_one(world, loc) {
            Ok(entity) => entity,
            Err(message) => return self.finish(Err(message)),
        };
        let Some(position) = center_of(world, entity) else {
            return self.finish(Err(format!("{entity} is not laid out yet")));
        };
        match self.waited {
            0 => {
                send_pointer(world, position, PointerAction::Move { delta: Vec2::ZERO });
                self.wait()
            }
            1 => {
                send_pointer(world, position, PointerAction::Press(button));
                self.wait()
            }
            _ => {
                send_pointer(world, position, PointerAction::Release(button));
                self.finish(Ok(Value::Null))
            }
        }
    }
}

impl LiveDriver {
    /// `Action`: the first key bound to the named action, pressed and
    /// released this frame.
    fn action(&mut self, world: &mut World, action: &str) -> StepOutcome {
        let Some(action) = ops::ui_action(action) else {
            return self.finish(Err(format!("no UI action named {action:?}")));
        };
        let Some(code) = world.resource::<slotted_ui::UiBindings>().first_key(action) else {
            return self.finish(Err(format!("UiBindings binds no key to {action:?}")));
        };
        send_key(world, code, logical_key(code), ButtonState::Pressed);
        send_key(world, code, logical_key(code), ButtonState::Released);
        self.finish(Ok(Value::Null))
    }

    /// `Gamepad`: press one frame, release the next. A press and a release
    /// in the same frame never show up as `just_pressed`.
    fn gamepad(&mut self, world: &mut World, button: &str) -> StepOutcome {
        let Some(button) = ops::gamepad_button(button) else {
            return self.finish(Err(format!("no gamepad button named {button:?}")));
        };
        let pressed = self.waited == 0;
        crate::cursor::feed(
            world,
            &slotted_ui::RecordedInput::GamepadButton { button, pressed },
        );
        if pressed {
            self.wait()
        } else {
            self.finish(Ok(Value::Null))
        }
    }
}

impl LiveDriver {
    /// `TypeInto`, one frame per sub-step: focus the row, the `Accept` key
    /// down, up, every character (a press and a release each), Enter down,
    /// Enter up. Spread over frames because a press and a release in one
    /// frame never show up as `just_pressed`.
    fn type_into(&mut self, world: &mut World, loc: &TestLocator, text: &str) -> StepOutcome {
        let entity = match ops::resolve_one(world, loc) {
            Ok(entity) => entity,
            Err(message) => return self.finish(Err(message)),
        };
        let chars: Vec<char> = text.chars().collect();
        let Some(accept) = world
            .resource::<slotted_ui::UiBindings>()
            .first_key(UiAction::Accept)
        else {
            return self.finish(Err("UiBindings binds no key to Accept".to_owned()));
        };
        let last = 4 + chars.len();
        match self.waited as usize {
            0 => {
                world
                    .resource_mut::<bevy::input_focus::InputFocus>()
                    .set(entity, bevy::input_focus::FocusCause::Navigated);
                self.wait()
            }
            1 => {
                send_key(world, accept, logical_key(accept), ButtonState::Pressed);
                self.wait()
            }
            2 => {
                send_key(world, accept, logical_key(accept), ButtonState::Released);
                self.wait()
            }
            n if n - 3 < chars.len() => {
                let key = Key::Character(chars[n - 3].to_string().into());
                let code =
                    KeyCode::Unidentified(bevy::input::keyboard::NativeKeyCode::Unidentified);
                send_key(world, code, key.clone(), ButtonState::Pressed);
                send_key(world, code, key, ButtonState::Released);
                self.wait()
            }
            n if n < last => {
                send_key(world, KeyCode::Enter, Key::Enter, ButtonState::Pressed);
                self.wait()
            }
            _ => {
                send_key(world, KeyCode::Enter, Key::Enter, ButtonState::Released);
                self.finish(Ok(Value::Null))
            }
        }
    }
}

impl TestDriver for LiveDriver {
    fn perform(&mut self, world: &mut World, op: &TestOp) -> StepOutcome {
        if let Some(answer) = ops::query(world, op) {
            return self.finish(answer);
        }
        match op {
            TestOp::OpenScreen { kind, .. } => {
                let answer = open_screen(world, kind);
                self.finish(answer)
            }
            TestOp::Click { loc } => self.click(world, loc, PointerButton::Primary),
            TestOp::RightClick { loc } => self.click(world, loc, PointerButton::Secondary),
            TestOp::ShiftClick { loc } => {
                if self.waited == 0 {
                    send_key(world, KeyCode::ShiftLeft, Key::Shift, ButtonState::Pressed);
                }
                let outcome = self.click(world, loc, PointerButton::Primary);
                if matches!(outcome, StepOutcome::Done(_)) {
                    send_key(world, KeyCode::ShiftLeft, Key::Shift, ButtonState::Released);
                }
                outcome
            }
            TestOp::Hover { loc } => {
                let answer = ops::resolve_one(world, loc).and_then(|entity| {
                    let position = center_of(world, entity)
                        .ok_or_else(|| format!("{entity} is not laid out yet"))?;
                    send_pointer(world, position, PointerAction::Move { delta: Vec2::ZERO });
                    Ok(Value::Null)
                });
                self.finish(answer)
            }
            TestOp::Cycle { loc, forward } => {
                let answer = ops::resolve_one(world, loc).map(|entity| {
                    world.trigger(slotted_ui::IconButtonCycle {
                        entity,
                        forward: *forward,
                    });
                    Value::Null
                });
                self.finish(answer)
            }
            TestOp::Key { key } => {
                let Some(code) = ops::key_code(key) else {
                    return self.finish(Err(format!("no key named {key:?}")));
                };
                send_key(world, code, logical_key(code), ButtonState::Pressed);
                send_key(world, code, logical_key(code), ButtonState::Released);
                self.finish(Ok(Value::Null))
            }
            TestOp::Action { action } => self.action(world, action),
            TestOp::Gamepad { button } => self.gamepad(world, button),
            TestOp::SetValue { key, value } => {
                // Written this frame, applied by `apply_set_values` on the
                // next, which is when the test may read it back.
                if self.waited == 0 {
                    if let Err(message) = ops::set_value(world, key, value) {
                        return self.finish(Err(message));
                    }
                    self.wait()
                } else {
                    self.finish(Ok(Value::Null))
                }
            }
            TestOp::TypeInto { loc, text } => self.type_into(world, loc, text),
            TestOp::TypeText { text } => {
                for ch in text.chars() {
                    let key = Key::Character(ch.to_string().into());
                    let code =
                        KeyCode::Unidentified(bevy::input::keyboard::NativeKeyCode::Unidentified);
                    send_key(world, code, key.clone(), ButtonState::Pressed);
                    send_key(world, code, key, ButtonState::Released);
                }
                self.finish(Ok(Value::Null))
            }
            TestOp::Settle => {
                if is_quiet(world) {
                    self.quiet += 1;
                } else {
                    self.quiet = 0;
                }
                if self.quiet >= QUIET_FRAMES {
                    self.finish(Ok(Value::Null))
                } else {
                    StepOutcome::Pending
                }
            }
            TestOp::Step { frames } => {
                if self.waited + 1 >= *frames {
                    self.finish(Ok(Value::Null))
                } else {
                    self.wait()
                }
            }
            // `ops::query` answered every other variant above.
            other => self.finish(Err(format!("{other:?} is not an action the app performs"))),
        }
    }
}

/// The live app has one screen open at a time, so `open_screen` is "the
/// screen you have open" (contract 3.2). A test that asks for another one
/// fails saying which is up.
fn open_screen(world: &mut World, kind: &str) -> Result<Value, String> {
    let wanted =
        slotted_model::Namespaced::parse(kind).map_err(|e| format!("screen {kind:?}: {e}"))?;
    let mut roots = world.query::<(Entity, &slotted_ui::ScreenRoot)>();
    let open: Vec<(Entity, slotted_ui::ScreenRoot)> = roots
        .iter(world)
        .map(|(entity, root)| (entity, root.clone()))
        .collect();
    let Some((entity, root)) = open.iter().find(|(_, root)| root.kind.0 == wanted) else {
        let found = if open.is_empty() {
            "no screen is open".to_owned()
        } else {
            open.iter()
                .map(|(_, root)| root.kind.0.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        };
        return Err(format!("expected screen {kind}, found {found}"));
    };
    let mut out = std::collections::BTreeMap::new();
    out.insert(
        "menu".to_owned(),
        root.menu
            .map_or(Value::Null, |m| Value::Int(i64::from(m.index().index()))),
    );
    out.insert(
        "screen".to_owned(),
        Value::Int(i64::from(entity.index().index())),
    );
    Ok(Value::Map(out))
}

/// Nothing is animating and nothing is waiting on the authority.
fn is_quiet(world: &World) -> bool {
    let pending = world
        .get_resource::<slotted_ecs::PendingRoundTrips>()
        .is_none_or(|p| p.0 == 0);
    let motions = world
        .get_resource::<slotted_theme::ActiveMotions>()
        .is_none_or(|m| m.0 == 0);
    pending && motions
}

/// Logical-pixel centre of a laid-out node.
fn center_of(world: &World, entity: Entity) -> Option<Vec2> {
    let node = world.get::<ComputedNode>(entity)?;
    let transform = world.get::<UiGlobalTransform>(entity)?;
    if node.size().x <= 0.0 || node.size().y <= 0.0 {
        return None;
    }
    Some(transform.translation * node.inverse_scale_factor())
}

/// Writes one `PointerInput` for the mouse pointer over the primary window.
fn send_pointer(world: &mut World, position: Vec2, action: PointerAction) {
    let Some(window) = primary_window(world) else {
        warn!("no primary window: the live test driver cannot move a pointer");
        return;
    };
    let Some(target) = WindowRef::Primary.normalize(Some(window)) else {
        return;
    };
    world.write_message(PointerInput::new(
        PointerId::Mouse,
        Location {
            target: NormalizedRenderTarget::Window(target),
            position,
        },
        action,
    ));
}

/// Writes one `KeyboardInput` for the primary window.
fn send_key(world: &mut World, key_code: KeyCode, logical_key: Key, state: ButtonState) {
    let Some(window) = primary_window(world) else {
        return;
    };
    let text = match (&logical_key, state) {
        (Key::Character(c), ButtonState::Pressed) => Some(c.clone()),
        _ => None,
    };
    world.write_message(KeyboardInput {
        key_code,
        logical_key,
        state,
        text,
        repeat: false,
        window,
    });
}

fn primary_window(world: &mut World) -> Option<Entity> {
    let mut query = world.query_filtered::<Entity, With<PrimaryWindow>>();
    query.iter(world).next()
}

/// The logical key a physical one produces, for the handful of keys a test
/// names. Anything else arrives unidentified, which is what a widget reading
/// `key_code` wants anyway.
fn logical_key(code: KeyCode) -> Key {
    match code {
        KeyCode::Enter => Key::Enter,
        KeyCode::Escape => Key::Escape,
        KeyCode::Space => Key::Space,
        KeyCode::Tab => Key::Tab,
        KeyCode::ArrowUp => Key::ArrowUp,
        KeyCode::ArrowDown => Key::ArrowDown,
        KeyCode::ArrowLeft => Key::ArrowLeft,
        KeyCode::ArrowRight => Key::ArrowRight,
        KeyCode::ShiftLeft | KeyCode::ShiftRight => Key::Shift,
        KeyCode::ControlLeft | KeyCode::ControlRight => Key::Control,
        KeyCode::AltLeft | KeyCode::AltRight => Key::Alt,
        _ => Key::Unidentified(bevy::input::keyboard::NativeKey::Unidentified),
    }
}
