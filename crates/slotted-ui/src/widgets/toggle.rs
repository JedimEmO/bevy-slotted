//! The toggle (menus M1 contract 3.3): a switch or a checkbox bound to a
//! `Bool`.
//!
//! The row is the focusable; the switch (a track with a sliding thumb) or
//! the checkbox (a square with a tick) is a child that carries the roles.
//! Accept and a click anywhere on the row flip it; the flip is a write to
//! the binding, and the row repaints from the value that comes back.

use bevy::picking::events::{Click, Pointer};
use bevy::picking::hover::Hovered;
use bevy::picking::pointer::PointerButton;
use bevy::prelude::*;
use slotted_theme::{Motion, MotionPreset, Themed, TweenTarget, roles};

use crate::actions::UiAction;
use crate::def::{BindDef, LocKey, ToggleStyle};
use crate::nav::FocusedAction;
use crate::screen::SpawnCtx;
use crate::semantic::SemanticRole;
use crate::values::{BoundValue, Value, ValueBinding, ValueWriter};
use crate::widgets::controls::{self, ControlLook};
use crate::widgets::kinds;

/// The toggle's state, for tests.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToggleState {
    /// On.
    pub on: bool,
    /// Switch or checkbox.
    pub style: ToggleStyle,
    /// Inert.
    pub disabled: bool,
}

/// The toggle's painted children, on the row.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToggleParts {
    /// The switch track or the checkbox square.
    pub track: Entity,
    /// The switch thumb (`None` for a checkbox).
    pub thumb: Option<Entity>,
    /// The checkbox tick (`None` for a switch).
    pub tick: Option<Entity>,
}

/// On the thumb: where it was last painted, so a flip slides from there.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToggleThumb {
    /// Painted at the `on` end.
    pub on: bool,
}

/// Spawns a toggle row.
#[allow(clippy::too_many_lines)]
pub fn spawn_toggle(
    ctx: &mut SpawnCtx<'_>,
    label: Option<&LocKey>,
    style: ToggleStyle,
    bind: &BindDef,
) -> Entity {
    let tokens = ctx.tokens();
    let entity = controls::spawn_control_row(
        ctx,
        SemanticRole::Toggle,
        kinds::toggle(),
        label,
        bind.disabled,
    );
    controls::spawn_control_spacer(ctx.world, entity);
    let d = tokens.spacing.lg;
    let inset = tokens.spacing.xs;
    let parts = match style {
        ToggleStyle::Switch => {
            let track = ctx
                .world
                .spawn((
                    Node {
                        width: Val::Px(d + tokens.spacing.md + 2.0 * inset),
                        height: Val::Px(d + 2.0 * inset),
                        border: UiRect::all(Val::Px(crate::widgets::BORDER_WIDTH)),
                        position_type: PositionType::Relative,
                        flex_shrink: 0.0,
                        ..default()
                    },
                    Themed(roles::TOGGLE),
                    Pickable::default(),
                    ChildOf(entity),
                ))
                .id();
            let thumb = ctx
                .world
                .spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Px(inset - crate::widgets::BORDER_WIDTH),
                        top: Val::Px(inset - crate::widgets::BORDER_WIDTH),
                        width: Val::Px(d),
                        height: Val::Px(d),
                        ..default()
                    },
                    Themed(roles::TOGGLE_THUMB),
                    ToggleThumb { on: false },
                    Pickable::IGNORE,
                    ChildOf(track),
                ))
                .id();
            ToggleParts {
                track,
                thumb: Some(thumb),
                tick: None,
            }
        }
        ToggleStyle::Checkbox => {
            let track = ctx
                .world
                .spawn((
                    Node {
                        width: Val::Px(d + 2.0 * inset),
                        height: Val::Px(d + 2.0 * inset),
                        border: UiRect::all(Val::Px(crate::widgets::BORDER_WIDTH)),
                        align_items: AlignItems::Center,
                        justify_content: JustifyContent::Center,
                        flex_shrink: 0.0,
                        ..default()
                    },
                    Themed(roles::CHECKBOX),
                    Pickable::default(),
                    ChildOf(entity),
                ))
                .id();
            let tick = ctx
                .world
                .spawn((
                    Node::default(),
                    Text::new("✓"),
                    Themed(roles::CONTROL_LABEL),
                    Visibility::Hidden,
                    Pickable::IGNORE,
                    ChildOf(track),
                ))
                .id();
            ToggleParts {
                track,
                thumb: None,
                tick: Some(tick),
            }
        }
    };
    let mut row = ctx.world.entity_mut(entity);
    row.insert((
        ToggleState {
            on: false,
            style,
            disabled: bind.disabled,
        },
        parts,
    ));
    if let Some(binding) = ValueBinding::from_def(bind, ctx.menu) {
        row.insert(binding);
    }
    row.observe(on_toggle_action);
    row.observe(on_toggle_click);
    entity
}

/// Flips the toggle: a write to the binding, or the state itself when the
/// row is bound to nothing.
fn flip(
    entity: Entity,
    state: &mut ToggleState,
    binding: Option<&ValueBinding>,
    writer: &mut ValueWriter,
) {
    if state.disabled {
        return;
    }
    match binding {
        Some(binding) => writer.write(entity, binding, Value::Bool(!state.on)),
        None => state.on = !state.on,
    }
}

/// Observer: a fresh `Accept` flips and is claimed.
pub fn on_toggle_action(
    action: On<FocusedAction>,
    mut rows: Query<(&mut ToggleState, Option<&ValueBinding>)>,
    mut claims: ResMut<crate::actions::UiActionClaims>,
    mut writer: ValueWriter,
) {
    if action.action != UiAction::Accept || action.repeat {
        return;
    }
    let Ok((mut state, binding)) = rows.get_mut(action.entity) else {
        return;
    };
    if state.disabled {
        return;
    }
    claims.claim(UiAction::Accept);
    flip(action.entity, &mut state, binding, &mut writer);
}

/// Observer: a primary click anywhere on the row flips.
pub fn on_toggle_click(
    click: On<Pointer<Click>>,
    mut rows: Query<(&mut ToggleState, Option<&ValueBinding>)>,
    mut writer: ValueWriter,
) {
    if click.event().button != PointerButton::Primary {
        return;
    }
    let Ok((mut state, binding)) = rows.get_mut(click.entity) else {
        return;
    };
    flip(click.entity, &mut state, binding, &mut writer);
}

/// Observer on [`BoundValue`]: the store's `Bool` (or a menu property's
/// non-zero) becomes `on`.
pub fn on_toggle_value(event: On<BoundValue>, mut rows: Query<&mut ToggleState>) {
    let Ok(mut state) = rows.get_mut(event.entity) else {
        return;
    };
    let on = match &event.value {
        Value::Bool(b) => *b,
        Value::Int(i) => *i != 0,
        Value::Float(f) => *f != 0.0,
        Value::Text(_) => return,
    };
    if state.on != on {
        state.on = on;
    }
}

/// `SlottedUiSet::Render`, after the value sync: roles, `Checked`, the tick,
/// and the thumb's slide over `durations.fast`.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn paint_toggles(
    focus: Option<Res<bevy::input_focus::InputFocus>>,
    motion: Res<Motion>,
    tokens: crate::tooltip::ThemeTokens,
    rows: Query<(
        Entity,
        &ToggleState,
        &ToggleParts,
        &Hovered,
        Has<bevy::ui::Checked>,
    )>,
    mut themed: Query<&mut Themed>,
    mut thumbs: Query<(&mut ToggleThumb, Option<&mut UiTransform>)>,
    mut visibility: Query<&mut Visibility>,
    mut commands: Commands,
) {
    let focused = focus.and_then(|f| f.get());
    let tokens = tokens.get();
    for (entity, state, parts, hovered, checked) in &rows {
        let base = match (state.style, state.on) {
            (ToggleStyle::Switch, false) => "toggle",
            (ToggleStyle::Switch, true) => "toggle.on",
            (ToggleStyle::Checkbox, false) => "checkbox",
            (ToggleStyle::Checkbox, true) => "checkbox.on",
        };
        let role = controls::state_role(
            base,
            ControlLook {
                hovered: hovered.get(),
                focused: focused == Some(entity),
                active: false,
                disabled: state.disabled,
            },
        );
        if let Ok(mut t) = themed.get_mut(parts.track)
            && t.0 != role
        {
            t.0 = role;
        }
        if checked != state.on {
            if state.on {
                commands.entity(entity).insert(bevy::ui::Checked);
            } else {
                commands.entity(entity).remove::<bevy::ui::Checked>();
            }
        }
        if let Some(tick) = parts.tick
            && let Ok(mut vis) = visibility.get_mut(tick)
        {
            let want = if state.on {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            };
            if *vis != want {
                *vis = want;
            }
        }
        if let Some(thumb) = parts.thumb
            && let Ok((mut painted, transform)) = thumbs.get_mut(thumb)
            && painted.on != state.on
        {
            painted.on = state.on;
            let travel = tokens.spacing.md;
            let to = if state.on { travel } else { 0.0 };
            let from = match transform.as_deref() {
                Some(tf) => match tf.translation.x {
                    Val::Px(x) => x,
                    _ => travel - to,
                },
                None => travel - to,
            };
            commands.entity(thumb).insert(motion.tween(
                MotionPreset::Hover,
                TweenTarget::Translate {
                    from: Vec2::new(from, 0.0),
                    to: Vec2::new(to, 0.0),
                },
                &tokens.durations,
            ));
        }
    }
}
