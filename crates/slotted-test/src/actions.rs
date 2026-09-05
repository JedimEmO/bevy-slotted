//! Semantic and pointer actions.
//!
//! Semantic actions trigger the widgets' own entity events and skip layout.
//! Pointer actions move the `PointerId::Mouse` pointer and write
//! `PointerInput` messages, so hit-testing, z-order and `Hovered` are all
//! exercised. Each pointer step costs one frame; callers see none of that.

use bevy::camera::NormalizedRenderTarget;
use bevy::input::ButtonState;
use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::input_focus::{FocusCause, InputFocus};
use bevy::math::CompassOctant;
use bevy::picking::pointer::{Location, PointerAction, PointerButton, PointerId, PointerLocation};
use bevy::prelude::*;
use bevy::window::WindowRef;
use slotted_ecs::{MenuAction, Modifiers, SlotClicked};
use slotted_model::{Button, ClickAction};
use slotted_ui::{TooltipRequest, TooltipTier};

use crate::harness::UiHarness;

impl UiHarness {
    // ---- semantic -----------------------------------------------------------

    /// Trigger `bevy_ui_widgets::Activate` on a button as Enter would.
    pub fn activate(&mut self, entity: Entity) {
        self.world_mut()
            .trigger(bevy::ui_widgets::Activate { entity });
        self.step(1);
    }

    /// Trigger [`SlotClicked`] on a slot entity directly.
    pub fn click_slot(&mut self, slot: Entity, button: Button, modifiers: Modifiers) {
        self.world_mut().trigger(SlotClicked {
            entity: slot,
            button,
            modifiers,
        });
        self.step(1);
    }

    /// Trigger a fully formed [`MenuAction`] on a menu entity.
    pub fn menu_action(&mut self, menu: Entity, action: ClickAction) {
        self.world_mut().trigger(MenuAction {
            entity: menu,
            action,
        });
        self.step(1);
    }

    /// Ask for a tooltip on `entity` without moving the pointer.
    pub fn request_tooltip(&mut self, entity: Entity, tier: TooltipTier) {
        self.world_mut().trigger(TooltipRequest { entity, tier });
        self.step(1);
    }

    // ---- pointer ------------------------------------------------------------

    fn location(&self, pos: Vec2) -> Location {
        Location {
            target: NormalizedRenderTarget::Window(
                WindowRef::Primary
                    .normalize(Some(self.window))
                    .expect("primary window exists"),
            ),
            position: pos,
        }
    }

    fn pointer_input(&mut self, action: PointerAction) {
        let loc = self.location(self.pointer_pos);
        let id = *self
            .world()
            .get::<PointerId>(self.pointer)
            .expect("pointer entity has PointerId");
        self.world_mut()
            .write_message(bevy::picking::pointer::PointerInput::new(id, loc, action));
        self.step(1);
    }

    /// Move the pointer to logical window coordinates.
    pub fn pointer_move_to(&mut self, pos: Vec2) {
        let delta = pos - self.pointer_pos;
        self.pointer_pos = pos;
        self.pointer_input(PointerAction::Move { delta });
    }

    /// Press a pointer button where the pointer is.
    pub fn pointer_press(&mut self, button: PointerButton) {
        self.pointer_input(PointerAction::Press(button));
    }

    /// Release a pointer button where the pointer is.
    pub fn pointer_release(&mut self, button: PointerButton) {
        self.pointer_input(PointerAction::Release(button));
    }

    /// Move, press, release at `pos`.
    pub fn click_at(&mut self, pos: Vec2, button: PointerButton) {
        self.pointer_move_to(pos);
        self.pointer_press(button);
        self.pointer_release(button);
    }

    /// Left click the centre of `entity`.
    pub fn click(&mut self, entity: Entity) {
        let pos = self.center_of(entity);
        self.click_at(pos, PointerButton::Primary);
    }

    /// Right click the centre of `entity`.
    pub fn right_click(&mut self, entity: Entity) {
        let pos = self.center_of(entity);
        self.click_at(pos, PointerButton::Secondary);
    }

    /// Middle click the centre of `entity`.
    pub fn middle_click(&mut self, entity: Entity) {
        let pos = self.center_of(entity);
        self.click_at(pos, PointerButton::Middle);
    }

    /// Shift held, left click.
    pub fn shift_click(&mut self, entity: Entity) {
        self.hold(KeyCode::ShiftLeft);
        self.click(entity);
        self.release(KeyCode::ShiftLeft);
    }

    /// Two left clicks within the interpreter's double-click window.
    pub fn double_click(&mut self, entity: Entity) {
        self.click(entity);
        self.click(entity);
    }

    /// Move the pointer over `entity` and wait one frame.
    pub fn hover(&mut self, entity: Entity) {
        let pos = self.center_of(entity);
        self.pointer_move_to(pos);
    }

    /// Press on `from`, move to `to`, release.
    pub fn drag(&mut self, from: Entity, to: Entity) {
        let a = self.center_of(from);
        let b = self.center_of(to);
        self.pointer_move_to(a);
        self.pointer_press(PointerButton::Primary);
        self.pointer_move_to(b);
        self.pointer_release(PointerButton::Primary);
    }

    /// Press on the first slot, sweep over the rest, release on the last.
    pub fn drag_paint(&mut self, slots: &[Entity]) {
        let Some((first, rest)) = slots.split_first() else {
            return;
        };
        let pos = self.center_of(*first);
        self.pointer_move_to(pos);
        self.pointer_press(PointerButton::Primary);
        for s in rest {
            let pos = self.center_of(*s);
            self.pointer_move_to(pos);
        }
        self.pointer_release(PointerButton::Primary);
    }

    /// Scroll over `entity`.
    pub fn scroll(&mut self, entity: Entity, delta: Vec2) {
        self.hover(entity);
        self.pointer_input(PointerAction::Scroll {
            unit: bevy::input::mouse::MouseScrollUnit::Line,
            x: delta.x,
            y: delta.y,
            phase: bevy::input::touch::TouchPhase::Moved,
        });
    }

    /// Spawn a second pointer. It clicks and presses correctly but never
    /// drives `Hovered` (ADR 0002).
    pub fn spawn_custom_pointer(&mut self) -> Entity {
        self.world_mut()
            .spawn((
                PointerId::Custom(uuid::Uuid::new_v4()),
                PointerLocation::default(),
            ))
            .id()
    }

    /// Where the primary pointer is.
    pub fn pointer_pos(&self) -> Vec2 {
        self.pointer_pos
    }

    // ---- keyboard -----------------------------------------------------------

    fn key_event(&mut self, key_code: KeyCode, logical_key: Key, state: ButtonState) {
        let window = self.window;
        self.world_mut().write_message(KeyboardInput {
            key_code,
            logical_key,
            state,
            text: None,
            repeat: false,
            window,
        });
        self.step(1);
    }

    /// Press and release a key.
    pub fn key(&mut self, key_code: KeyCode) {
        let logical = logical_key_for(key_code);
        self.key_with(key_code, logical);
    }

    /// Press and release a key with an explicit logical key.
    pub fn key_with(&mut self, key_code: KeyCode, logical: Key) {
        self.key_event(key_code, logical.clone(), ButtonState::Pressed);
        self.key_event(key_code, logical, ButtonState::Released);
    }

    /// Hold a key across later actions.
    pub fn hold(&mut self, key_code: KeyCode) {
        if !self.held.contains(&key_code) {
            self.held.push(key_code);
            let logical = logical_key_for(key_code);
            self.key_event(key_code, logical, ButtonState::Pressed);
        }
    }

    /// Release a held key.
    pub fn release(&mut self, key_code: KeyCode) {
        self.held.retain(|k| *k != key_code);
        let logical = logical_key_for(key_code);
        self.key_event(key_code, logical, ButtonState::Released);
    }

    /// Keys currently held.
    pub fn held_keys(&self) -> &[KeyCode] {
        &self.held
    }

    /// Type text into the focused field, one `Key::Character` per char.
    pub fn type_text(&mut self, text: &str) {
        for ch in text.chars() {
            let window = self.window;
            for state in [ButtonState::Pressed, ButtonState::Released] {
                self.world_mut().write_message(KeyboardInput {
                    key_code: KeyCode::Unidentified(
                        bevy::input::keyboard::NativeKeyCode::Unidentified,
                    ),
                    logical_key: Key::Character(ch.to_string().into()),
                    state,
                    text: (state == ButtonState::Pressed).then(|| ch.to_string().into()),
                    repeat: false,
                    window,
                });
                self.step(1);
            }
        }
    }

    // ---- focus --------------------------------------------------------------

    /// Tab.
    pub fn focus_next(&mut self) {
        self.key(KeyCode::Tab);
    }

    /// Shift+Tab.
    pub fn focus_prev(&mut self) {
        self.hold(KeyCode::ShiftLeft);
        self.key(KeyCode::Tab);
        self.release(KeyCode::ShiftLeft);
    }

    /// Arrow key in `dir` (cardinal directions only).
    pub fn focus_dir(&mut self, dir: CompassOctant) {
        let code = match dir {
            CompassOctant::North => KeyCode::ArrowUp,
            CompassOctant::South => KeyCode::ArrowDown,
            CompassOctant::East => KeyCode::ArrowRight,
            CompassOctant::West => KeyCode::ArrowLeft,
            other => panic!("focus_dir takes a cardinal direction, got {other:?}"),
        };
        self.key(code);
    }

    /// Set or clear focus directly.
    pub fn set_focus(&mut self, entity: Option<Entity>) {
        let mut f = self.world_mut().resource_mut::<InputFocus>();
        match entity {
            Some(e) => f.set(e, FocusCause::Navigated),
            None => f.clear(),
        }
        self.step(1);
    }
}

fn logical_key_for(code: KeyCode) -> Key {
    match code {
        KeyCode::ShiftLeft | KeyCode::ShiftRight => Key::Shift,
        KeyCode::ControlLeft | KeyCode::ControlRight => Key::Control,
        KeyCode::AltLeft | KeyCode::AltRight => Key::Alt,
        KeyCode::Tab => Key::Tab,
        KeyCode::Enter => Key::Enter,
        KeyCode::Escape => Key::Escape,
        KeyCode::Space => Key::Space,
        KeyCode::ArrowUp => Key::ArrowUp,
        KeyCode::ArrowDown => Key::ArrowDown,
        KeyCode::ArrowLeft => Key::ArrowLeft,
        KeyCode::ArrowRight => Key::ArrowRight,
        KeyCode::Digit1 => Key::Character("1".into()),
        KeyCode::Digit2 => Key::Character("2".into()),
        KeyCode::Digit3 => Key::Character("3".into()),
        KeyCode::Digit4 => Key::Character("4".into()),
        KeyCode::Digit5 => Key::Character("5".into()),
        KeyCode::Digit6 => Key::Character("6".into()),
        KeyCode::Digit7 => Key::Character("7".into()),
        KeyCode::Digit8 => Key::Character("8".into()),
        KeyCode::Digit9 => Key::Character("9".into()),
        KeyCode::KeyQ => Key::Character("q".into()),
        KeyCode::KeyE => Key::Character("e".into()),
        _ => Key::Unidentified(bevy::input::keyboard::NativeKey::Unidentified),
    }
}
