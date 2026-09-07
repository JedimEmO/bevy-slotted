//! The key binding row (menus M1 contract 3.6): shows one action's binding
//! and captures a new one.

use bevy::prelude::*;
use slotted_theme::{Themed, roles};

use crate::actions::{InputDevice, UiAction};
use crate::def::LocKey;
use crate::focus_ring::Focusable;
use crate::screen::SpawnCtx;
use crate::semantic::{SemanticLabel, SemanticRole, WidgetNode};
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

/// Spawns a key binding row.
pub fn spawn_key_binding(
    ctx: &mut SpawnCtx<'_>,
    label: Option<&LocKey>,
    action: UiAction,
    device: InputDevice,
    disabled: bool,
) -> Entity {
    // M1-IMPL: B
    let fallback = LocKey(action.as_str().to_owned());
    let entity = placeholder_row(
        ctx,
        roles::KEY_BINDING,
        SemanticRole::KeyBinding,
        kinds::key_binding(),
        Some(label.unwrap_or(&fallback)),
        false,
    );
    ctx.world.entity_mut(entity).insert(KeyBindingState {
        action,
        device,
        capturing: false,
        disabled,
    });
    entity
}

/// `SlottedUiSet::Input`, after `UiActionEmit`: while a row is capturing,
/// the next fresh key or button press becomes the binding.
pub fn capture_key_bindings(
    _rows: Query<(Entity, &mut KeyBindingState)>,
    _keys: Res<ButtonInput<KeyCode>>,
    _gamepads: Query<&Gamepad>,
    _bindings: ResMut<crate::actions::UiBindings>,
    _claims: ResMut<crate::actions::UiActionClaims>,
    _changed: MessageWriter<BindingChanged>,
) {
    // M1-IMPL: B
}

/// The skeleton's placeholder: a control-height row with the label, so the
/// tree lays out and locators find the node before the package fills it.
#[allow(dead_code)]
fn placeholder_row(
    ctx: &mut SpawnCtx<'_>,
    role: slotted_theme::Role,
    semantic: SemanticRole,
    kind: crate::def::WidgetKind,
    label: Option<&LocKey>,
    compact: bool,
) -> Entity {
    use bevy::input_focus::tab_navigation::TabIndex;
    let tokens = ctx.tokens();
    let height = if compact {
        tokens.sizes.control_height_compact
    } else {
        tokens.sizes.control_height
    };
    let entity = ctx.spawn_node((
        Node {
            height: Val::Px(height),
            min_height: Val::Px(height),
            padding: UiRect::axes(Val::Px(tokens.spacing.md), Val::Px(tokens.spacing.xs)),
            border: UiRect::all(Val::Px(crate::widgets::BORDER_WIDTH)),
            align_items: AlignItems::Center,
            column_gap: Val::Px(tokens.spacing.sm),
            flex_shrink: 0.0,
            ..default()
        },
        Themed(role),
        semantic,
        SemanticLabel(label.map(|k| k.0.clone()).unwrap_or_default()),
        WidgetNode(kind),
        Focusable,
        TabIndex(0),
        bevy::picking::hover::Hovered::default(),
        Pickable::default(),
    ));
    if let Some(label) = label {
        let parent = std::mem::replace(&mut ctx.parent, entity);
        crate::widgets::text::spawn_text(
            ctx,
            label,
            crate::def::TextRole::Label,
            &crate::def::TextOpts::default(),
        );
        ctx.parent = parent;
    }
    entity
}
