//! The radio group (menus M1 contract 3.5): a segmented control over the
//! same data as a select.
//!
//! The row is the focusable; each option is a segment child. `Left`/`Right`
//! move the active segment and write at once; a click on a segment writes.

use bevy::picking::events::{Click, Pointer};
use bevy::picking::hover::Hovered;
use bevy::picking::pointer::PointerButton;
use bevy::prelude::*;
use slotted_theme::{Themed, roles};

use crate::actions::{UiAction, UiActionClaims};
use crate::def::SelectOption;
use crate::def::{BindDef, LocKey};
use crate::nav::FocusedAction;
use crate::screen::SpawnCtx;
use crate::semantic::{LocText, SemanticRole};
use crate::values::{BoundValue, Value, ValueBinding, ValueWriter};
use crate::widgets::controls::{self, ControlLook};
use crate::widgets::kinds;

/// The group's state, for tests.
#[derive(Component, Debug, Clone, PartialEq)]
pub struct RadioState {
    /// Index into the options.
    pub index: usize,
    /// The options.
    pub options: Vec<SelectOption>,
    /// Inert.
    pub disabled: bool,
}

impl RadioState {
    /// The index of the option with `id`.
    pub fn index_of(&self, id: &str) -> Option<usize> {
        self.options.iter().position(|o| o.id == id)
    }

    /// The current option's id.
    pub fn id(&self) -> Option<&str> {
        self.options.get(self.index).map(|o| o.id.as_str())
    }
}

/// One segment of a radio group.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct RadioSegment {
    /// The group it belongs to.
    pub group: Entity,
    /// Which option.
    pub index: usize,
}

/// Spawns a radio group row.
pub fn spawn_radio_group(
    ctx: &mut SpawnCtx<'_>,
    label: Option<&LocKey>,
    options: &[SelectOption],
    bind: &BindDef,
) -> Entity {
    let tokens = ctx.tokens();
    let entity = controls::spawn_control_row(
        ctx,
        SemanticRole::RadioGroup,
        kinds::radio_group(),
        label,
        bind.disabled,
    );
    controls::spawn_control_spacer(ctx.world, entity);
    let height = tokens.sizes.control_height - 2.0 * tokens.spacing.sm;
    let segments = ctx
        .world
        .spawn((
            Node {
                height: Val::Px(height),
                align_items: AlignItems::Stretch,
                column_gap: Val::Px(tokens.spacing.xs),
                flex_shrink: 0.0,
                ..default()
            },
            Pickable::default(),
            ChildOf(entity),
        ))
        .id();
    for (index, option) in options.iter().enumerate() {
        let segment = ctx
            .world
            .spawn((
                Node {
                    padding: UiRect::axes(Val::Px(tokens.spacing.md), Val::ZERO),
                    border: UiRect::all(Val::Px(crate::widgets::BORDER_WIDTH)),
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::Center,
                    border_radius: BorderRadius::all(Val::Px(tokens.radii.sm)),
                    ..default()
                },
                Themed(if index == 0 {
                    roles::RADIO_ACTIVE
                } else {
                    roles::RADIO
                }),
                RadioSegment {
                    group: entity,
                    index,
                },
                Hovered::default(),
                Pickable::default(),
                ChildOf(segments),
            ))
            .id();
        ctx.world.spawn((
            Node::default(),
            Text::new(option.label.0.clone()),
            Themed(roles::CONTROL_LABEL),
            LocText::new(option.label.clone()),
            Pickable::IGNORE,
            ChildOf(segment),
        ));
        ctx.world.entity_mut(segment).observe(on_segment_click);
    }
    let mut row = ctx.world.entity_mut(entity);
    row.insert(RadioState {
        index: 0,
        options: options.to_vec(),
        disabled: bind.disabled,
    });
    if let Some(binding) = ValueBinding::from_def(bind, ctx.menu) {
        row.insert(binding);
    }
    row.observe(on_radio_action);
    entity
}

/// Asks the binding for the option at `index`, or takes it when unbound.
fn choose(
    entity: Entity,
    state: &mut RadioState,
    binding: Option<&ValueBinding>,
    index: usize,
    writer: &mut ValueWriter,
) {
    let Some(option) = state.options.get(index) else {
        return;
    };
    match binding {
        Some(binding) => writer.write(entity, binding, Value::Text(option.id.clone())),
        None => state.index = index,
    }
}

/// Observer: `Left`/`Right` move the active segment (no wrap: the ends are
/// ends) and write; both are claimed.
pub fn on_radio_action(
    action: On<FocusedAction>,
    mut rows: Query<(&mut RadioState, Option<&ValueBinding>)>,
    mut claims: ResMut<UiActionClaims>,
    mut writer: ValueWriter,
) {
    let Ok((mut state, binding)) = rows.get_mut(action.entity) else {
        return;
    };
    if state.disabled || state.options.is_empty() {
        return;
    }
    let next = match action.action {
        UiAction::Left => state.index.saturating_sub(1),
        UiAction::Right => (state.index + 1).min(state.options.len() - 1),
        _ => return,
    };
    claims.claim(action.action);
    if next != state.index {
        choose(action.entity, &mut state, binding, next, &mut writer);
    }
}

/// Observer on a segment: a primary click picks it.
pub fn on_segment_click(
    click: On<Pointer<Click>>,
    segments: Query<&RadioSegment>,
    mut rows: Query<(&mut RadioState, Option<&ValueBinding>)>,
    mut writer: ValueWriter,
) {
    if click.event().button != PointerButton::Primary {
        return;
    }
    let Ok(segment) = segments.get(click.entity).copied() else {
        return;
    };
    let Ok((mut state, binding)) = rows.get_mut(segment.group) else {
        return;
    };
    if state.disabled {
        return;
    }
    choose(
        segment.group,
        &mut state,
        binding,
        segment.index,
        &mut writer,
    );
}

/// Observer on [`BoundValue`]: the store's option id (or a property's
/// index) becomes `index`.
pub fn on_radio_value(event: On<BoundValue>, mut rows: Query<&mut RadioState>) {
    let Ok(mut state) = rows.get_mut(event.entity) else {
        return;
    };
    let index = match &event.value {
        Value::Text(id) => state.index_of(id),
        Value::Int(i) => usize::try_from(*i)
            .ok()
            .filter(|i| *i < state.options.len()),
        Value::Bool(_) | Value::Float(_) => None,
    };
    if let Some(index) = index
        && state.index != index
    {
        state.index = index;
    }
}

/// `SlottedUiSet::Render`, after the value sync: segment roles.
pub fn paint_radio_groups(
    focus: Option<Res<bevy::input_focus::InputFocus>>,
    rows: Query<(Entity, &RadioState)>,
    mut segments: Query<(&RadioSegment, &Hovered, &mut Themed)>,
) {
    let focused = focus.and_then(|f| f.get());
    for (segment, hovered, mut themed) in &mut segments {
        let Ok((group, state)) = rows.get(segment.group) else {
            continue;
        };
        let base = if segment.index == state.index {
            "radio.active"
        } else {
            "radio"
        };
        let role = controls::state_role(
            base,
            ControlLook {
                hovered: hovered.get(),
                focused: focused == Some(group) && segment.index == state.index,
                active: false,
                disabled: state.disabled,
            },
        );
        if themed.0 != role {
            themed.0 = role;
        }
    }
}
