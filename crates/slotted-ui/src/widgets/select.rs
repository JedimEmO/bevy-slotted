//! The select (menus M1 contract 3.5): a value pill that cycles with
//! `Left`/`Right` and opens a popup for the pointer.
//!
//! The popup is an absolute panel spawned under the screen root, below the
//! pill, with one focusable option per entry. Focus moves into it on open
//! and back to the row on close; `Up`/`Down` walk it, `Accept` picks,
//! `Back` closes and is claimed so the screen does not pop.

use bevy::input_focus::tab_navigation::TabIndex;
use bevy::input_focus::{FocusCause, InputFocus};
use bevy::picking::events::{Click, Pointer};
use bevy::picking::hover::Hovered;
use bevy::picking::pointer::PointerButton;
use bevy::prelude::*;
use slotted_theme::{Themed, roles};

use crate::actions::{InputMode, UiAction, UiActionClaims};
use crate::def::SelectOption;
use crate::def::{BindDef, LocKey};
use crate::focus_ring::Focusable;
use crate::nav::FocusedAction;
use crate::screen::SpawnCtx;
use crate::semantic::{LocText, ScreenRoot, SemanticLabel, SemanticRole};
use crate::values::{BoundValue, Value, ValueBinding, ValueWriter};
use crate::widgets::controls::{self, ControlLook};
use crate::widgets::kinds;

/// The select's state, for tests.
#[derive(Component, Debug, Clone, PartialEq)]
pub struct SelectState {
    /// Index into the options.
    pub index: usize,
    /// The options.
    pub options: Vec<SelectOption>,
    /// The popup is open.
    pub open: bool,
    /// Inert.
    pub disabled: bool,
}

impl SelectState {
    /// The index one step forward or back, wrapping. `None` without options.
    pub fn next(&self, forward: bool) -> Option<usize> {
        let len = self.options.len();
        if len == 0 {
            return None;
        }
        Some(if forward {
            (self.index + 1) % len
        } else {
            (self.index + len - 1) % len
        })
    }

    /// The index of the option with `id`.
    pub fn index_of(&self, id: &str) -> Option<usize> {
        self.options.iter().position(|o| o.id == id)
    }

    /// The current option's id.
    pub fn id(&self) -> Option<&str> {
        self.options.get(self.index).map(|o| o.id.as_str())
    }
}

/// Marks the select's popup panel.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct SelectPopup {
    /// The select it belongs to.
    pub select: Entity,
}

/// One option row inside a popup.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct SelectOptionNode {
    /// The select it belongs to.
    pub select: Entity,
    /// Which option.
    pub index: usize,
}

/// The select's painted children, on the row.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct SelectParts {
    /// The value pill.
    pub pill: Entity,
    /// The current option's label inside the pill.
    pub value: Entity,
    /// The open popup, while there is one.
    pub popup: Option<Entity>,
}

/// Spawns a select row.
pub fn spawn_select(
    ctx: &mut SpawnCtx<'_>,
    label: Option<&LocKey>,
    options: &[SelectOption],
    bind: &BindDef,
) -> Entity {
    let tokens = ctx.tokens();
    let entity = controls::spawn_control_row(
        ctx,
        SemanticRole::Select,
        kinds::select(),
        label,
        bind.disabled,
    );
    controls::spawn_control_spacer(ctx.world, entity);
    let pill_height = tokens.sizes.control_height - 2.0 * tokens.spacing.sm;
    let pill = ctx
        .world
        .spawn((
            Node {
                height: Val::Px(pill_height),
                min_width: Val::Px(tokens.spacing.xl * 4.0),
                padding: UiRect::axes(Val::Px(tokens.spacing.sm), Val::ZERO),
                border: UiRect::all(Val::Px(crate::widgets::BORDER_WIDTH)),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::SpaceBetween,
                column_gap: Val::Px(tokens.spacing.sm),
                border_radius: BorderRadius::all(Val::Px(tokens.radii.sm)),
                flex_shrink: 0.0,
                ..default()
            },
            Themed(roles::SELECT),
            Pickable::default(),
            ChildOf(entity),
        ))
        .id();
    let chevron = |world: &mut World, glyph: &str| {
        world.spawn((
            Node::default(),
            Text::new(glyph.to_owned()),
            Themed(roles::CONTROL_LABEL),
            Pickable::IGNORE,
            ChildOf(pill),
        ));
    };
    chevron(ctx.world, "‹");
    let first = options.first().map(|o| o.label.clone());
    let value = ctx
        .world
        .spawn((
            Node {
                flex_grow: 1.0,
                justify_content: JustifyContent::Center,
                ..default()
            },
            Text::new(first.as_ref().map(|k| k.0.clone()).unwrap_or_default()),
            TextLayout::justify(bevy::text::Justify::Center),
            Themed(roles::CONTROL_LABEL),
            Pickable::IGNORE,
            ChildOf(pill),
        ))
        .id();
    if let Some(key) = first {
        ctx.world.entity_mut(value).insert(LocText::new(key));
    }
    chevron(ctx.world, "›");
    let mut row = ctx.world.entity_mut(entity);
    row.insert((
        SelectState {
            index: 0,
            options: options.to_vec(),
            open: false,
            disabled: bind.disabled,
        },
        SelectParts {
            pill,
            value,
            popup: None,
        },
    ));
    if let Some(binding) = ValueBinding::from_def(bind, ctx.menu) {
        row.insert(binding);
    }
    row.observe(on_select_action);
    row.observe(on_select_click);
    entity
}

/// Asks the binding for the option at `index`, or takes it when unbound.
fn choose(
    entity: Entity,
    state: &mut SelectState,
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

/// Observer: `Left`/`Right` cycle in place; a fresh `Accept` in pointer mode
/// opens the popup. All three are claimed.
pub fn on_select_action(
    action: On<FocusedAction>,
    mut rows: Query<(&mut SelectState, Option<&ValueBinding>)>,
    mode: Res<InputMode>,
    mut claims: ResMut<UiActionClaims>,
    mut writer: ValueWriter,
    mut commands: Commands,
) {
    let Ok((mut state, binding)) = rows.get_mut(action.entity) else {
        return;
    };
    if state.disabled || state.open {
        return;
    }
    match action.action {
        UiAction::Left | UiAction::Right => {
            claims.claim(action.action);
            if let Some(next) = state.next(action.action == UiAction::Right) {
                choose(action.entity, &mut state, binding, next, &mut writer);
            }
        }
        UiAction::Accept if !action.repeat && *mode == InputMode::Pointer => {
            claims.claim(UiAction::Accept);
            commands.trigger(OpenSelectPopup {
                entity: action.entity,
            });
        }
        _ => {}
    }
}

/// Observer: a primary click on the row opens the popup.
pub fn on_select_click(
    click: On<Pointer<Click>>,
    rows: Query<&SelectState>,
    mut commands: Commands,
) {
    if click.event().button != PointerButton::Primary {
        return;
    }
    let Ok(state) = rows.get(click.entity) else {
        return;
    };
    if state.disabled || state.open {
        return;
    }
    commands.trigger(OpenSelectPopup {
        entity: click.entity,
    });
}

/// Open the popup of a select. The harness's `select_option` may use it.
#[derive(EntityEvent, Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpenSelectPopup {
    /// The select row.
    pub entity: Entity,
}

/// Close the popup of a select, focusing the row again.
#[derive(EntityEvent, Debug, Clone, Copy, PartialEq, Eq)]
pub struct CloseSelectPopup {
    /// The select row.
    pub entity: Entity,
}

/// The rect of a node in logical px.
fn logical_rect(node: &ComputedNode, transform: &UiGlobalTransform) -> Rect {
    let size = node.size() * node.inverse_scale_factor();
    let center = transform.translation * node.inverse_scale_factor();
    Rect::from_center_size(center, size)
}

/// Observer on [`OpenSelectPopup`]: spawns the panel under the screen root,
/// below the pill, and focuses the active option.
#[allow(clippy::too_many_arguments)]
pub fn on_open_select_popup(
    open: On<OpenSelectPopup>,
    mut rows: Query<(&mut SelectState, &mut SelectParts)>,
    parents: Query<&ChildOf>,
    roots: Query<(), With<ScreenRoot>>,
    nodes: Query<(&ComputedNode, &UiGlobalTransform)>,
    tokens: crate::tooltip::ThemeTokens,
    mut focus: Option<ResMut<InputFocus>>,
    mut commands: Commands,
) {
    let entity = open.entity;
    let Ok((mut state, mut parts)) = rows.get_mut(entity) else {
        return;
    };
    if state.open || state.options.is_empty() {
        return;
    }
    let Some(root) = crate::nav::screen_root_of(entity, &parents, &roots) else {
        tracing::warn!(?entity, "select outside a screen root has nowhere to open");
        return;
    };
    let tokens = tokens.get();
    // Below the pill, in the root's coordinates: an absolute child is
    // placed from the parent's padding box.
    let (left, top) = match (nodes.get(parts.pill), nodes.get(root)) {
        (Ok((pill_node, pill_tf)), Ok((root_node, root_tf))) => {
            let pill = logical_rect(pill_node, pill_tf);
            let root_rect = logical_rect(root_node, root_tf);
            let border = root_node.border() * root_node.inverse_scale_factor();
            (
                pill.min.x - root_rect.min.x - border.min_inset.x,
                pill.max.y - root_rect.min.y - border.min_inset.y + tokens.spacing.xs,
            )
        }
        _ => (0.0, 0.0),
    };
    let popup = commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(left),
                top: Val::Px(top),
                min_width: Val::Px(tokens.spacing.xl * 4.0),
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(Val::Px(tokens.spacing.xs)),
                row_gap: Val::Px(tokens.spacing.xs),
                border: UiRect::all(Val::Px(crate::widgets::BORDER_WIDTH)),
                ..default()
            },
            ZIndex(1),
            Themed(roles::SELECT_POPUP),
            SemanticRole::Panel,
            SemanticLabel::default(),
            SelectPopup { select: entity },
            Pickable::default(),
            ChildOf(root),
        ))
        .id();
    let mut first_focus = None;
    let row_height = tokens.sizes.control_height_compact;
    for (index, option) in state.options.iter().enumerate() {
        let role = if index == state.index {
            roles::SELECT_OPTION_ACTIVE
        } else {
            roles::SELECT_OPTION
        };
        let node = commands
            .spawn((
                Node {
                    height: Val::Px(row_height),
                    min_height: Val::Px(row_height),
                    padding: UiRect::axes(Val::Px(tokens.spacing.sm), Val::ZERO),
                    border: UiRect::all(Val::Px(crate::widgets::BORDER_WIDTH)),
                    align_items: AlignItems::Center,
                    flex_shrink: 0.0,
                    ..default()
                },
                Themed(role),
                SemanticRole::ListItem,
                SemanticLabel(option.label.0.clone()),
                SelectOptionNode {
                    select: entity,
                    index,
                },
                Focusable,
                TabIndex(0),
                Hovered::default(),
                Pickable::default(),
                ChildOf(popup),
            ))
            .observe(on_option_action)
            .observe(on_option_click)
            .id();
        commands.spawn((
            Node::default(),
            Text::new(option.label.0.clone()),
            Themed(roles::CONTROL_LABEL),
            LocText::new(option.label.clone()),
            Pickable::IGNORE,
            ChildOf(node),
        ));
        if index == state.index {
            first_focus = Some(node);
        }
    }
    state.open = true;
    parts.popup = Some(popup);
    if let (Some(focus), Some(target)) = (focus.as_deref_mut(), first_focus) {
        focus.set(target, FocusCause::Navigated);
    }
}

/// Observer on [`CloseSelectPopup`]: despawns the panel and returns focus
/// to the row.
pub fn on_close_select_popup(
    close: On<CloseSelectPopup>,
    mut rows: Query<(&mut SelectState, &mut SelectParts)>,
    mut focus: Option<ResMut<InputFocus>>,
    mut commands: Commands,
) {
    let entity = close.entity;
    let Ok((mut state, mut parts)) = rows.get_mut(entity) else {
        return;
    };
    if let Some(popup) = parts.popup.take() {
        commands.entity(popup).despawn();
    }
    if state.open {
        state.open = false;
    }
    if let Some(focus) = focus.as_deref_mut() {
        focus.set(entity, FocusCause::Navigated);
    }
}

/// Observer on an option: `Up`/`Down` move between options, `Accept` picks,
/// `Back` closes. Every one is claimed.
#[allow(clippy::too_many_arguments)]
pub fn on_option_action(
    action: On<FocusedAction>,
    options: Query<&SelectOptionNode>,
    siblings: Query<(Entity, &SelectOptionNode)>,
    mut rows: Query<(&mut SelectState, Option<&ValueBinding>)>,
    mut focus: Option<ResMut<InputFocus>>,
    mut claims: ResMut<UiActionClaims>,
    mut writer: ValueWriter,
    mut commands: Commands,
) {
    let Ok(option) = options.get(action.entity).copied() else {
        return;
    };
    match action.action {
        UiAction::Up | UiAction::Down => {
            claims.claim(action.action);
            let Ok((state, _)) = rows.get(option.select) else {
                return;
            };
            let len = state.options.len();
            if len == 0 {
                return;
            }
            let next = if action.action == UiAction::Down {
                (option.index + 1) % len
            } else {
                (option.index + len - 1) % len
            };
            let target = siblings
                .iter()
                .find(|(_, o)| o.select == option.select && o.index == next)
                .map(|(e, _)| e);
            if let (Some(focus), Some(target)) = (focus.as_deref_mut(), target) {
                focus.set(target, FocusCause::Navigated);
            }
        }
        UiAction::Accept if !action.repeat => {
            claims.claim(UiAction::Accept);
            if let Ok((mut state, binding)) = rows.get_mut(option.select) {
                choose(
                    option.select,
                    &mut state,
                    binding,
                    option.index,
                    &mut writer,
                );
            }
            commands.trigger(CloseSelectPopup {
                entity: option.select,
            });
        }
        UiAction::Back => {
            claims.claim(UiAction::Back);
            commands.trigger(CloseSelectPopup {
                entity: option.select,
            });
        }
        _ => {}
    }
}

/// Observer on an option: a primary click picks it and closes the popup.
pub fn on_option_click(
    click: On<Pointer<Click>>,
    options: Query<&SelectOptionNode>,
    mut rows: Query<(&mut SelectState, Option<&ValueBinding>)>,
    mut writer: ValueWriter,
    mut commands: Commands,
) {
    if click.event().button != PointerButton::Primary {
        return;
    }
    let Ok(option) = options.get(click.entity).copied() else {
        return;
    };
    if let Ok((mut state, binding)) = rows.get_mut(option.select) {
        choose(
            option.select,
            &mut state,
            binding,
            option.index,
            &mut writer,
        );
    }
    commands.trigger(CloseSelectPopup {
        entity: option.select,
    });
}

/// Observer on [`BoundValue`]: the store's option id (or a property's
/// index) becomes `index`. An id that is not an option is ignored.
pub fn on_select_value(event: On<BoundValue>, mut rows: Query<&mut SelectState>) {
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

/// `SlottedUiSet::Render`, after the value sync: the pill's label and
/// roles, the popup's active option.
#[allow(clippy::type_complexity)]
pub fn paint_selects(
    focus: Option<Res<InputFocus>>,
    rows: Query<(Entity, Ref<SelectState>, &SelectParts, &Hovered)>,
    mut themed: Query<&mut Themed>,
    mut labels: Query<(&mut Text, Option<&mut LocText>)>,
    options: Query<(Entity, &SelectOptionNode)>,
    mut commands: Commands,
) {
    let focused = focus.and_then(|f| f.get());
    for (entity, state, parts, hovered) in &rows {
        let role = controls::state_role(
            "select",
            ControlLook {
                hovered: hovered.get(),
                focused: focused == Some(entity),
                active: state.open,
                disabled: state.disabled,
            },
        );
        if let Ok(mut t) = themed.get_mut(parts.pill)
            && t.0 != role
        {
            t.0 = role;
        }
        if !state.is_changed() {
            continue;
        }
        if let Some(option) = state.options.get(state.index)
            && let Ok((mut text, loc)) = labels.get_mut(parts.value)
        {
            if let Some(mut loc) = loc {
                if loc.key != option.label {
                    loc.key = option.label.clone();
                }
            } else {
                text.0.clone_from(&option.label.0);
                commands
                    .entity(parts.value)
                    .insert(LocText::new(option.label.clone()));
            }
        }
        if parts.popup.is_some() {
            for (node, option) in &options {
                if option.select != entity {
                    continue;
                }
                let role = if option.index == state.index {
                    roles::SELECT_OPTION_ACTIVE
                } else {
                    roles::SELECT_OPTION
                };
                if let Ok(mut t) = themed.get_mut(node)
                    && t.0 != role
                {
                    t.0 = role;
                }
            }
        }
    }
}
