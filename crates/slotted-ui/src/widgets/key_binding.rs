//! The key binding row (menus M1 contract 3.6): shows one action's binding
//! and captures a new one.
//!
//! Accept enters capture; the cell shows `…`; the next fresh key (keyboard
//! rows) or button (gamepad rows) becomes the action's first binding in
//! `UiBindings`. `Back` cancels. Every action the captured press produced
//! is claimed, and the capture runs before the focused-action dispatch, so
//! the press that ends a capture never reaches the row as a new `Accept`.

use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use slotted_theme::{Themed, roles};

use crate::actions::{InputDevice, InputMode, UiAction, UiActionClaims, UiActionEvent, UiBindings};
use crate::def::LocKey;
use crate::nav::FocusedAction;
use crate::screen::SpawnCtx;
use crate::semantic::SemanticRole;
use crate::widgets::controls::{self, ControlLook};
use crate::widgets::kinds;

/// The row's state, for tests.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyBindingState {
    /// Which action.
    pub action: UiAction,
    /// Keyboard or gamepad.
    pub device: InputDevice,
    /// Waiting for the next press.
    pub capturing: bool,
    /// Inert.
    pub disabled: bool,
}

/// A binding was rewritten by a capture.
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct BindingChanged {
    /// Which action.
    pub action: UiAction,
    /// On which device.
    pub device: InputDevice,
}

/// The row's painted children, on the row.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyBindingParts {
    /// The value cell.
    pub cell: Entity,
    /// The binding text inside the cell.
    pub text: Entity,
}

/// The mode whose glyphs a row's device shows.
fn mode_for(device: InputDevice) -> InputMode {
    match device {
        InputDevice::Gamepad => InputMode::Gamepad,
        InputDevice::Keyboard | InputDevice::Pointer => InputMode::Keyboard,
    }
}

/// The glyph set a row's cell renders in, resolved from the world at spawn
/// (menus M2 contract 2.4); `paint_key_bindings` follows it afterwards.
fn glyph_set(world: &mut World) -> crate::rich::GlyphSet {
    let set = world
        .get_resource::<crate::rich::GlyphSet>()
        .copied()
        .unwrap_or_default();
    let vendor = world
        .query::<&Gamepad>()
        .iter(world)
        .next()
        .and_then(Gamepad::vendor_id);
    for_cell(crate::rich::resolved_glyph_set_for_vendor(set, vendor))
}

/// A gamepad row's cell names the pad's button whatever the set: under
/// `Keyboard` (a text-only UI) it uses Bevy's names rather than showing
/// the action's keyboard key, which would be the wrong device's binding.
fn for_cell(set: crate::rich::GlyphSet) -> crate::rich::GlyphSet {
    match set {
        crate::rich::GlyphSet::Keyboard => crate::rich::GlyphSet::Generic,
        other => other,
    }
}

/// Spawns a key binding row.
pub fn spawn_key_binding(
    ctx: &mut SpawnCtx<'_>,
    label: Option<&LocKey>,
    action: UiAction,
    device: InputDevice,
    disabled: bool,
) -> Entity {
    let tokens = ctx.tokens();
    let fallback = LocKey(action.as_str().to_owned());
    let entity = controls::spawn_control_row(
        ctx,
        SemanticRole::KeyBinding,
        kinds::key_binding(),
        Some(label.unwrap_or(&fallback)),
        disabled,
    );
    controls::spawn_control_spacer(ctx.world, entity);
    let set = glyph_set(ctx.world);
    let glyph = ctx
        .world
        .get_resource::<UiBindings>()
        .map(|b| crate::rich::key_glyph_text(action, mode_for(device), b, set))
        .unwrap_or_default();
    let cell_height = tokens.sizes.control_height - 2.0 * tokens.spacing.sm;
    let cell = ctx
        .world
        .spawn((
            Node {
                height: Val::Px(cell_height),
                min_width: Val::Px(tokens.spacing.xl * 3.0),
                padding: UiRect::axes(Val::Px(tokens.spacing.sm), Val::ZERO),
                border: UiRect::all(Val::Px(crate::widgets::BORDER_WIDTH)),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                border_radius: BorderRadius::all(Val::Px(tokens.radii.sm)),
                flex_shrink: 0.0,
                ..default()
            },
            Themed(roles::KEY_BINDING),
            Pickable::default(),
            ChildOf(entity),
        ))
        .id();
    let text = ctx
        .world
        .spawn((
            Node::default(),
            Text::new(glyph),
            Themed(roles::TEXT_KEY),
            Pickable::IGNORE,
            ChildOf(cell),
        ))
        .id();
    let mut row = ctx.world.entity_mut(entity);
    row.insert((
        KeyBindingState {
            action,
            device,
            capturing: false,
            disabled,
        },
        KeyBindingParts { cell, text },
    ));
    row.observe(on_key_binding_action);
    entity
}

/// Observer: a fresh `Accept` enters capture and is claimed.
pub fn on_key_binding_action(
    action: On<FocusedAction>,
    mut rows: Query<&mut KeyBindingState>,
    mut claims: ResMut<UiActionClaims>,
) {
    if action.action != UiAction::Accept || action.repeat {
        return;
    }
    let Ok(mut state) = rows.get_mut(action.entity) else {
        return;
    };
    if state.disabled || state.capturing {
        return;
    }
    claims.claim(UiAction::Accept);
    state.capturing = true;
}

/// `SlottedUiSet::Input`, after `UiActionEmit` and before the focused-action
/// dispatch: while a row is capturing, the next fresh key or button press
/// becomes the binding, or `Back` cancels. Escape is never bound.
#[allow(clippy::too_many_arguments)]
pub fn capture_key_bindings(
    mut rows: Query<(Entity, &mut KeyBindingState)>,
    keys: Res<ButtonInput<KeyCode>>,
    gamepads: Query<&Gamepad>,
    mut events: MessageReader<UiActionEvent>,
    mut bindings: ResMut<UiBindings>,
    mut claims: ResMut<UiActionClaims>,
    mut changed: MessageWriter<BindingChanged>,
) {
    let mut capturing = rows.iter_mut().filter(|(_, s)| s.capturing);
    let Some((entity, mut state)) = capturing.next() else {
        events.clear();
        return;
    };
    let back = events.read().any(|e| e.action == UiAction::Back);
    if back {
        state.capturing = false;
        claims.claim(UiAction::Back);
        tracing::debug!(?entity, "key capture cancelled");
        return;
    }
    match state.device {
        InputDevice::Gamepad => {
            let Some(button) = gamepads
                .iter()
                .find_map(|g| g.get_just_pressed().next().copied())
            else {
                return;
            };
            for (action, buttons) in &bindings.buttons {
                if buttons.contains(&button) {
                    claims.claim(*action);
                }
            }
            let slot = bindings.buttons.entry(state.action).or_default();
            if slot.is_empty() {
                slot.push(button);
            } else {
                slot[0] = button;
            }
        }
        InputDevice::Keyboard | InputDevice::Pointer => {
            let Some(key) = keys.get_just_pressed().next().copied() else {
                return;
            };
            if key == KeyCode::Escape {
                return;
            }
            for (action, codes) in &bindings.keys {
                if codes.contains(&key) {
                    claims.claim(*action);
                }
            }
            let slot = bindings.keys.entry(state.action).or_default();
            if slot.is_empty() {
                slot.push(key);
            } else {
                slot[0] = key;
            }
        }
    }
    state.capturing = false;
    changed.write(BindingChanged {
        action: state.action,
        device: state.device,
    });
}

/// `SlottedUiSet::Render`: the cell's role and its text (`…` while
/// capturing, else the first binding's glyph), refreshed whenever
/// `UiBindings` changes.
#[allow(clippy::too_many_arguments)]
pub fn paint_key_bindings(
    focus: Option<Res<bevy::input_focus::InputFocus>>,
    bindings: Res<UiBindings>,
    glyphs: Res<crate::rich::GlyphSet>,
    gamepads: Query<&Gamepad>,
    mut connections: MessageReader<bevy::input::gamepad::GamepadConnectionEvent>,
    rows: Query<(Entity, Ref<KeyBindingState>, &KeyBindingParts, &Hovered)>,
    mut themed: Query<&mut Themed>,
    mut texts: Query<&mut Text>,
) {
    let focused = focus.and_then(|f| f.get());
    // A new pad may change what `Auto` means (menus M2 contract 2.4).
    let glyphs_moved = glyphs.is_changed() || connections.read().count() > 0;
    let set = for_cell(crate::rich::resolved_glyph_set(
        *glyphs,
        InputMode::Gamepad,
        &gamepads,
    ));
    for (entity, state, parts, hovered) in &rows {
        let role = controls::state_role_with(
            "key_binding",
            ControlLook {
                hovered: hovered.get(),
                focused: focused == Some(entity),
                active: state.capturing,
                disabled: state.disabled,
            },
            "capturing",
        );
        if let Ok(mut t) = themed.get_mut(parts.cell)
            && t.0 != role
        {
            t.0 = role;
        }
        if !(state.is_changed() || bindings.is_changed() || glyphs_moved) {
            continue;
        }
        let want = if state.capturing {
            "…".to_owned()
        } else {
            crate::rich::key_glyph_text(state.action, mode_for(state.device), &bindings, set)
        };
        if let Ok(mut text) = texts.get_mut(parts.text)
            && text.0 != want
        {
            text.0 = want;
        }
    }
}
