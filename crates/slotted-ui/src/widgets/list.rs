//! The list (menus M1 contract 4.3): a one-column virtual grid of focusable
//! rows.
//!
//! A list is a spawn-time configuration of the virtual grid: one full-width
//! column of `control_height_compact` rows, whose cells the grid spawns from
//! the [`VirtualGridSource`](super::virtual_grid::VirtualGridSource) exactly
//! as it does for a slot grid. [`spawn_row`] is the hook the grid calls
//! before each cell it spawns; the cell goes under the themed, focusable
//! [`ListRow`] it returns. `Up`/`Down` walk the rows and move the window when the target
//! is outside it; `Accept` (or a pointer click) selects the row, writes the
//! index to the binding as an `Int` and triggers `Activate` on the row.

use bevy::input_focus::tab_navigation::TabIndex;
use bevy::input_focus::{FocusCause, InputFocus};
use bevy::picking::events::{Click, Pointer};
use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use bevy::ui::auto_directional_navigation::AutoDirectionalNavigation;
use slotted_theme::{Themed, roles};

use crate::actions::{UiAction, UiActionClaims};
use crate::def::{BindDef, DataSourceId, Tags};
use crate::nav::FocusedAction;
use crate::screen::SpawnCtx;
use crate::semantic::{SemanticLabel, SemanticRole, WidgetNode};
use crate::values::{BindingTarget, SetValue, Value, ValueBinding, ValueStore};
use crate::widgets::kinds;
use crate::widgets::virtual_grid::{
    GridShape, VirtualCell, VirtualGridParams, VirtualGridState, spawn_shaped_virtual_grid,
};

/// The list's state, for tests.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ListState {
    /// The selected row, if any.
    pub selected: Option<usize>,
    /// Rows in the source.
    pub len: usize,
}

/// On every row of a list: which list, and which source index.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ListRow {
    /// The list root.
    pub list: Entity,
    /// The row's index into the source.
    pub index: usize,
}

/// On a list root: focus the row at this index once the grid has spawned it.
/// Written by the `Up`/`Down` walk when the target row is outside the window
/// and the window has to move first.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ListFocusRequest(pub usize);

/// The tag every list row carries with its source index.
pub const ROW_TAG: &str = "row";

/// The `ValueBinding` for a `BindDef`, if it binds anything (menus M1
/// contract 1.3). A menu property binds only when the screen drives a menu.
pub(crate) fn binding_for(ctx: &SpawnCtx<'_>, bind: &BindDef) -> Option<ValueBinding> {
    if let Some(key) = &bind.bind {
        return Some(ValueBinding {
            target: BindingTarget::Store(key.clone()),
        });
    }
    let (Some(id), Some(menu)) = (bind.property, ctx.menu) else {
        return None;
    };
    Some(ValueBinding {
        target: BindingTarget::Property { menu, id },
    })
}

/// Spawns a list.
pub fn spawn_list(
    ctx: &mut SpawnCtx<'_>,
    source: &DataSourceId,
    rows: u16,
    bind: &BindDef,
) -> Entity {
    let tokens = ctx.tokens();
    let shape = GridShape {
        columns: RepeatedGridTrack::fr(1, 1.0),
        row_height: tokens.sizes.control_height_compact,
        gap: tokens.spacing.xs,
        role: slotted_theme::Role::new_static("invisible"),
    };
    let params = VirtualGridParams {
        source: source.clone(),
        cols: 1,
        rows: rows.max(1),
    };
    let entity = spawn_shaped_virtual_grid(ctx, &params, &Tags::new(), &shape);
    let binding = binding_for(ctx, bind);
    let mut e = ctx.world.entity_mut(entity);
    e.insert((
        SemanticRole::List,
        SemanticLabel::default(),
        WidgetNode(kinds::list()),
        ListState {
            selected: None,
            len: 0,
        },
        StoreSeen(0),
    ));
    if let Some(mut node) = e.get_mut::<Node>() {
        node.width = Val::Percent(100.0);
        node.flex_shrink = 0.0;
    }
    if bind.disabled {
        e.insert(bevy::ui::InteractionDisabled);
    }
    if let Some(binding) = binding {
        e.insert(binding);
    }
    entity
}

/// Spawns the row for cell `index` of `grid` when `grid` is a list; the
/// grid then spawns the source's node under it. `None` for any other grid.
/// Called by `refresh_virtual_grids` for every non-pooled cell.
pub fn spawn_row(world: &mut World, grid: Entity, index: usize) -> Option<Entity> {
    world.get::<ListState>(grid)?;
    let disabled = world.get::<bevy::ui::InteractionDisabled>(grid).is_some();
    let tokens = crate::screen::active_tokens(world);
    let height = tokens.sizes.control_height_compact;
    let row = world
        .spawn((
            Node {
                height: Val::Px(height),
                min_height: Val::Px(height),
                width: Val::Percent(100.0),
                flex_shrink: 0.0,
                align_items: AlignItems::Center,
                column_gap: Val::Px(tokens.spacing.sm),
                padding: UiRect::axes(Val::Px(tokens.spacing.sm), Val::Px(0.0)),
                border: UiRect::all(Val::Px(crate::widgets::BORDER_WIDTH)),
                border_radius: BorderRadius::all(Val::Px(tokens.radii.sm)),
                overflow: Overflow::clip(),
                ..default()
            },
            ListRow { list: grid, index },
            Themed(roles::LIST_ROW),
            SemanticRole::ListItem,
            SemanticLabel::default(),
            TabIndex(0),
            Hovered::default(),
            AutoDirectionalNavigation::default(),
            Pickable::default(),
            Tags::new().with(ROW_TAG, &index.to_string()),
            ChildOf(grid),
        ))
        .id();
    if disabled {
        world.entity_mut(row).insert(bevy::ui::InteractionDisabled);
    }
    Some(row)
}

/// Selects `index` on `list`: records it, writes the binding, and triggers
/// `Activate` on `row`.
fn select(
    commands: &mut Commands,
    writes: &mut MessageWriter<SetValue>,
    lists: &mut Query<(&mut ListState, Option<&ValueBinding>)>,
    list: Entity,
    row: Entity,
    index: usize,
) {
    let Ok((mut state, binding)) = lists.get_mut(list) else {
        return;
    };
    if state.selected != Some(index) {
        state.selected = Some(index);
    }
    match binding.map(|b| &b.target) {
        Some(BindingTarget::Store(key)) => {
            writes.write(SetValue {
                key: key.clone(),
                value: Value::Int(i64::try_from(index).unwrap_or(i64::MAX)),
                source: Some(list),
            });
        }
        Some(BindingTarget::Property { menu, id }) => {
            commands.trigger(slotted_ecs::SetProperty {
                entity: *menu,
                id: *id,
                value: i32::try_from(index).unwrap_or(i32::MAX),
            });
        }
        None => {}
    }
    commands.trigger(bevy::ui_widgets::Activate { entity: row });
}

/// Observer: a [`FocusedAction`] on a row. `Up`/`Down` walk, `Accept`
/// selects; each is claimed when the row acted on it.
#[allow(clippy::too_many_arguments)]
pub fn on_list_row_action(
    action: On<FocusedAction>,
    rows: Query<(&ListRow, Has<bevy::ui::InteractionDisabled>)>,
    cells: Query<(Entity, &VirtualCell, &ChildOf)>,
    mut grids: Query<&mut VirtualGridState>,
    mut lists: Query<(&mut ListState, Option<&ValueBinding>)>,
    mut focus: Option<ResMut<InputFocus>>,
    mut claims: ResMut<UiActionClaims>,
    mut writes: MessageWriter<SetValue>,
    mut commands: Commands,
) {
    let Ok((row, disabled)) = rows.get(action.entity) else {
        return;
    };
    if disabled {
        return;
    }
    match action.action {
        UiAction::Accept => {
            if action.repeat {
                return;
            }
            claims.claim(UiAction::Accept);
            select(
                &mut commands,
                &mut writes,
                &mut lists,
                row.list,
                action.entity,
                row.index,
            );
        }
        UiAction::Up | UiAction::Down => {
            let Ok(mut grid) = grids.get_mut(row.list) else {
                return;
            };
            let target = if action.action == UiAction::Up {
                row.index.checked_sub(1)
            } else {
                Some(row.index + 1).filter(|i| *i < grid.total)
            };
            let Some(target) = target else {
                // The edge of the list: the action is left to directional
                // navigation, so focus can leave the list.
                return;
            };
            claims.claim(action.action);
            let spawned = cells
                .iter()
                .find(|(_, cell, parent)| cell.0 == target && parent.parent() == row.list)
                .map(|(e, _, _)| e);
            if let Some(cell) = spawned {
                if let Some(focus) = focus.as_deref_mut() {
                    focus.set(cell, FocusCause::Navigated);
                }
            } else {
                let delta = if action.action == UiAction::Up { -1 } else { 1 };
                grid.scroll_by(delta);
                commands.entity(row.list).insert(ListFocusRequest(target));
            }
        }
        _ => {}
    }
}

/// Observer: a pointer click on a row selects it.
pub fn on_list_row_click(
    click: On<Pointer<Click>>,
    rows: Query<(&ListRow, Has<bevy::ui::InteractionDisabled>)>,
    mut lists: Query<(&mut ListState, Option<&ValueBinding>)>,
    mut focus: Option<ResMut<InputFocus>>,
    mut writes: MessageWriter<SetValue>,
    mut commands: Commands,
) {
    if click.button != bevy::picking::pointer::PointerButton::Primary {
        return;
    }
    let Ok((row, disabled)) = rows.get(click.entity) else {
        return;
    };
    if disabled {
        return;
    }
    if let Some(focus) = focus.as_deref_mut() {
        focus.set(click.entity, FocusCause::Pressed);
    }
    select(
        &mut commands,
        &mut writes,
        &mut lists,
        row.list,
        click.entity,
        row.index,
    );
}

/// The store version a control last painted from. On the list, the tabs
/// and the text field.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StoreSeen(pub u64);

/// `SlottedUiSet::Render`, after `refresh_virtual_grids`: mirrors the grid's
/// total into `ListState::len`, paints `selected` from the store when it
/// changed, and resolves a pending [`ListFocusRequest`] once the row exists.
#[allow(clippy::type_complexity)]
pub fn sync_lists(
    store: Res<ValueStore>,
    mut lists: Query<(
        Entity,
        &mut ListState,
        &mut StoreSeen,
        &VirtualGridState,
        Option<&ValueBinding>,
        Option<&ListFocusRequest>,
    )>,
    cells: Query<(Entity, &VirtualCell, &ChildOf)>,
    mut focus: Option<ResMut<InputFocus>>,
    mut commands: Commands,
) {
    for (entity, mut state, mut seen, grid, binding, request) in &mut lists {
        if state.len != grid.total {
            state.len = grid.total;
        }
        if let Some(BindingTarget::Store(key)) = binding.map(|b| &b.target)
            && seen.0 != store.version()
        {
            seen.0 = store.version();
            let want = store
                .get(key)
                .and_then(|v| match v {
                    Value::Int(i) => usize::try_from(*i).ok(),
                    _ => None,
                })
                .filter(|i| *i < grid.total.max(1));
            if state.selected != want {
                state.selected = want;
            }
        }
        if let Some(ListFocusRequest(index)) = request {
            let cell = cells
                .iter()
                .find(|(_, cell, parent)| cell.0 == *index && parent.parent() == entity)
                .map(|(e, _, _)| e);
            if let Some(cell) = cell {
                if let Some(focus) = focus.as_deref_mut() {
                    focus.set(cell, FocusCause::Navigated);
                }
                commands.entity(entity).remove::<ListFocusRequest>();
            }
        }
    }
}

/// Observer on `slotted_ecs::PropertyChanged`: a property-bound list follows
/// the menu.
pub fn on_list_property(
    event: On<slotted_ecs::PropertyChanged>,
    mut lists: Query<(&mut ListState, &ValueBinding)>,
) {
    for (mut state, binding) in &mut lists {
        let BindingTarget::Property { menu, id } = &binding.target else {
            continue;
        };
        if *menu != event.menu || *id != event.id {
            continue;
        }
        let want = usize::try_from(event.value).ok().filter(|i| *i < state.len);
        if state.selected != want {
            state.selected = want;
        }
    }
}

/// `SlottedUiSet::Render`: paints every row from selection, focus and hover.
pub fn list_row_roles(
    focus: Option<Res<InputFocus>>,
    lists: Query<&ListState>,
    mut rows: Query<(
        Entity,
        &ListRow,
        &Hovered,
        &mut Themed,
        Has<bevy::ui::InteractionDisabled>,
    )>,
) {
    let focused = focus.and_then(|f| f.get());
    for (entity, row, hovered, mut themed, disabled) in &mut rows {
        let selected = lists
            .get(row.list)
            .is_ok_and(|s| s.selected == Some(row.index));
        let role = if selected {
            roles::LIST_ROW_SELECTED
        } else if focused == Some(entity) && !disabled {
            roles::LIST_ROW_FOCUS
        } else if hovered.get() && !disabled {
            roles::LIST_ROW_HOVER
        } else {
            roles::LIST_ROW
        };
        if themed.0 != role {
            themed.0 = role;
        }
    }
}

/// Registers the list's observers and systems.
pub fn build(app: &mut App) {
    app.add_observer(on_list_row_action)
        .add_observer(on_list_row_click)
        .add_observer(on_list_property)
        .add_systems(
            Update,
            (
                crate::widgets::virtual_grid::page_virtual_grids
                    .after(crate::actions::UiActionEmit)
                    .after(crate::nav::dispatch_focused_actions)
                    .in_set(crate::plugin::SlottedUiSet::Input),
                (sync_lists, list_row_roles)
                    .chain()
                    .after(crate::widgets::virtual_grid::refresh_virtual_grids)
                    .in_set(crate::plugin::SlottedUiSet::Render),
            ),
        );
}
