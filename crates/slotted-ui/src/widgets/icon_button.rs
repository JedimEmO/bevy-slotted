//! `icon_button`: a button cycling through named states. Phase 6 contract
//! section 1.4. Not a `bevy_ui_widgets::Button`: the state must change before
//! `Activate` fires so `WidgetActivate.tags["state"]` is the new state.

use bevy::input::ButtonState;
use bevy::input::keyboard::KeyboardInput;
use bevy::input_focus::FocusedInput;
use bevy::input_focus::tab_navigation::TabIndex;
use bevy::picking::events::{Click, Pointer};
use bevy::picking::hover::Hovered;
use bevy::picking::pointer::PointerButton;
use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use slotted_model::PropertyId;
use slotted_theme::{Themed, roles};

use crate::def::{IconButtonState as StateDef, Tags};
use crate::screen::SpawnCtx;
use crate::semantic::{SemanticLabel, SemanticRole, WidgetNode};
use crate::widgets::{BORDER_WIDTH, SLOT_SIZE};

/// Icon button state on the root. `Tags["state"]` mirrors
/// `states[current].id`.
#[derive(Component, Debug, Clone, PartialEq)]
pub struct IconButtonState {
    /// The states, in cycle order.
    pub states: Vec<StateDef>,
    /// Index into `states`.
    pub current: usize,
    /// Property mirroring `current`, when bound.
    pub property: Option<PropertyId>,
    /// The menu the property lives on.
    pub menu: Option<Entity>,
}

impl IconButtonState {
    /// The current state's def.
    pub fn state(&self) -> &StateDef {
        &self.states[self.current.min(self.states.len().saturating_sub(1))]
    }

    /// The index one step forward or back, wrapping. `None` when the button
    /// has no states to cycle through.
    pub fn next(&self, forward: bool) -> Option<usize> {
        let len = self.states.len();
        if len == 0 {
            return None;
        }
        Some(if forward {
            (self.current + 1) % len
        } else {
            (self.current + len - 1) % len
        })
    }
}

/// Marker on the icon child of an icon button.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct IconButtonIcon;

/// Cycle an icon button. Targets the button root. Pointer and keyboard
/// observers trigger it; the harness's `cycle` does too. The observer then
/// triggers `bevy_ui_widgets::Activate` itself.
#[derive(EntityEvent, Debug, Clone, Copy, PartialEq, Eq)]
pub struct IconButtonCycle {
    /// The button root.
    pub entity: Entity,
    /// `true` advances, `false` (shift) goes back.
    pub forward: bool,
}

/// Parameters of `slotted:icon_button`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct IconButtonParams {
    /// At least one state.
    #[serde(default)]
    pub states: Vec<StateDef>,
    /// Bound property.
    #[serde(default)]
    pub property: Option<PropertyId>,
}

/// Spawns an icon button. Contract 1.4.
pub fn spawn_icon_button(ctx: &mut SpawnCtx<'_>, params: &IconButtonParams, tags: &Tags) -> Entity {
    if params.states.is_empty() {
        tracing::warn!("icon_button has no states; it will not cycle");
    }
    let mut tags = tags.clone();
    if let Some(first) = params.states.first() {
        tags.0.insert("state".to_owned(), first.id.clone());
    }
    let menu = ctx.menu;
    let radius = ctx.tokens().radii.sm;
    let label = params
        .states
        .first()
        .map_or_else(String::new, |s| s.label.0.clone());
    let entity = ctx.spawn_node((
        Node {
            width: px(SLOT_SIZE),
            height: px(SLOT_SIZE),
            border: UiRect::all(px(BORDER_WIDTH)),
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            border_radius: BorderRadius::all(px(radius)),
            ..default()
        },
        Themed(roles::ICON_BUTTON),
        SemanticRole::Button,
        SemanticLabel(label),
        WidgetNode(crate::widgets::kinds::icon_button()),
        IconButtonState {
            states: params.states.clone(),
            current: 0,
            property: params.property,
            menu,
        },
        tags,
        TabIndex(0),
        crate::focus_ring::Focusable,
        Hovered::default(),
        Pickable::default(),
        crate::widgets::tank::TooltipSource,
    ));
    let icon = params
        .states
        .first()
        .map(|s| crate::widgets::icon_image(ctx.world, &s.icon))
        .unwrap_or_default();
    ctx.world.spawn((
        Node {
            width: percent(70),
            height: percent(70),
            ..default()
        },
        icon,
        IconButtonIcon,
        Pickable::IGNORE,
        ChildOf(entity),
    ));
    let mut e = ctx.world.entity_mut(entity);
    e.observe(on_icon_button_click);
    e.observe(on_icon_button_key);
    e.observe(crate::tooltip::on_slot_over);
    entity
}

/// Observer: a primary click cycles forward, shift-click backwards.
pub fn on_icon_button_click(
    click: On<Pointer<Click>>,
    keys: Res<ButtonInput<KeyCode>>,
    buttons: Query<&IconButtonState>,
    mut commands: Commands,
) {
    if click.event().button != PointerButton::Primary || buttons.get(click.entity).is_err() {
        return;
    }
    commands.trigger(IconButtonCycle {
        entity: click.entity,
        forward: !crate::widgets::modifiers_from(&keys).shift,
    });
}

/// Observer: `Enter` or `Space` on the focused button cycles it. Shift goes
/// back, the same as a shift-click.
pub fn on_icon_button_key(
    event: On<FocusedInput<KeyboardInput>>,
    buttons: Query<&IconButtonState>,
    mut commands: Commands,
) {
    let input = &event.input;
    if input.state != ButtonState::Pressed || input.repeat {
        return;
    }
    if !matches!(input.key_code, KeyCode::Enter | KeyCode::Space) {
        return;
    }
    let entity = event.focused_entity;
    if buttons.get(entity).is_err() {
        return;
    }
    // `KeyboardInput` does not carry modifiers; the harness's `key_with` and
    // a real keyboard both leave them in `ButtonInput<KeyCode>`, which the
    // cycle observer cannot see from here, so keyboard activation always
    // advances. Shift-cycling is a pointer gesture.
    commands.trigger(IconButtonCycle {
        entity,
        forward: true,
    });
}

/// Observer: advances or rewinds `current`, rewrites the `state` tag, the
/// semantic label and the icon, triggers `Activate`, and `SetProperty` when
/// bound.
pub fn on_icon_button_cycle(
    event: On<IconButtonCycle>,
    mut buttons: Query<(
        &mut IconButtonState,
        &mut Tags,
        &mut crate::semantic::SemanticLabel,
    )>,
    children: Query<&Children>,
    mut icons: Query<&mut ImageNode, With<IconButtonIcon>>,
    icon_source: crate::widgets::IconImages,
    mut commands: Commands,
) {
    let entity = event.entity;
    let Ok((mut state, mut tags, mut label)) = buttons.get_mut(entity) else {
        return;
    };
    let Some(next) = state.next(event.forward) else {
        tracing::warn!(?entity, "icon_button has no states to cycle");
        return;
    };
    state.current = next;
    let def = state.state().clone();
    tags.0.insert("state".to_owned(), def.id.clone());
    if label.0 != def.label.0 {
        label.0.clone_from(&def.label.0);
    }
    for child in children.get(entity).into_iter().flatten() {
        if let Ok(mut image) = icons.get_mut(*child) {
            *image = icon_source.image(&def.icon);
        }
    }

    // The state has already changed, so `WidgetActivate` in slotted-packs
    // reads the new `state` tag. Contract 1.4 and deviation 3.
    commands.trigger(bevy::ui_widgets::Activate { entity });
    if let (Some(id), Some(menu)) = (state.property, state.menu) {
        commands.trigger(slotted_ecs::SetProperty {
            entity: menu,
            id,
            value: i32::try_from(next).unwrap_or(0),
        });
    }
}

/// Observer on `slotted_ecs::PropertyChanged`: sets `current` from the value
/// of a bound property without re-triggering `Activate`.
pub fn on_icon_button_property(
    event: On<slotted_ecs::PropertyChanged>,
    mut buttons: Query<(
        Entity,
        &mut IconButtonState,
        &mut Tags,
        &mut crate::semantic::SemanticLabel,
    )>,
    children: Query<&Children>,
    mut icons: Query<&mut ImageNode, With<IconButtonIcon>>,
    icon_source: crate::widgets::IconImages,
) {
    let slotted_ecs::PropertyChanged {
        menu, id, value, ..
    } = *event;
    for (entity, mut state, mut tags, mut label) in &mut buttons {
        if state.menu != Some(menu) || state.property != Some(id) {
            continue;
        }
        let Ok(next) = usize::try_from(value) else {
            continue;
        };
        if next >= state.states.len() || next == state.current {
            continue;
        }
        state.current = next;
        let def = state.state().clone();
        tags.0.insert("state".to_owned(), def.id.clone());
        if label.0 != def.label.0 {
            label.0.clone_from(&def.label.0);
        }
        for child in children.get(entity).into_iter().flatten() {
            if let Ok(mut image) = icons.get_mut(*child) {
                *image = icon_source.image(&def.icon);
            }
        }
    }
}

/// `SlottedUiSet::Render`: swaps `icon_button` for `icon_button.hover` while
/// the pointer is over a button, the way `slot_state_roles` does for slots.
pub fn icon_button_roles(mut buttons: Query<(&Hovered, &mut Themed), With<IconButtonState>>) {
    for (hovered, mut themed) in &mut buttons {
        let role = if hovered.get() {
            roles::ICON_BUTTON_HOVER
        } else {
            roles::ICON_BUTTON
        };
        if themed.0 != role {
            themed.0 = role;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::def::{IconDef, LocKey};

    fn state(id: &str) -> StateDef {
        StateDef {
            id: id.to_owned(),
            icon: IconDef::Image(format!("icons/{id}.png")),
            label: LocKey(id.to_owned()),
        }
    }

    fn three() -> IconButtonState {
        IconButtonState {
            states: vec![state("ignore"), state("low"), state("high")],
            current: 0,
            property: None,
            menu: None,
        }
    }

    #[test]
    fn cycling_wraps_in_both_directions() {
        let mut s = three();
        assert_eq!(s.next(true), Some(1));
        assert_eq!(s.next(false), Some(2));
        s.current = 2;
        assert_eq!(s.next(true), Some(0));
        assert_eq!(s.next(false), Some(1));
    }

    #[test]
    fn a_button_with_no_states_has_nothing_to_cycle_to() {
        let s = IconButtonState {
            states: Vec::new(),
            current: 0,
            property: None,
            menu: None,
        };
        assert_eq!(s.next(true), None);
    }
}
