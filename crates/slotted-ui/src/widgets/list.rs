//! The list (menus M1 contract 4.3): a one-column virtual grid of focusable
//! rows.

use bevy::prelude::*;
use slotted_theme::{Themed, roles};

use crate::def::DataSourceId;
use crate::def::{BindDef, LocKey};
use crate::focus_ring::Focusable;
use crate::screen::SpawnCtx;
use crate::semantic::{SemanticLabel, SemanticRole, WidgetNode};
use crate::widgets::kinds;

/// The list's state, for tests.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ListState {
    /// The selected row, if any.
    pub selected: Option<usize>,
    /// Rows in the source.
    pub len: usize,
}

/// Spawns a list.
pub fn spawn_list(
    ctx: &mut SpawnCtx<'_>,
    source: &DataSourceId,
    rows: u16,
    bind: &BindDef,
) -> Entity {
    // M1-IMPL: C
    let _ = (source, rows, bind);
    let entity = placeholder_row(
        ctx,
        roles::LIST_ROW,
        SemanticRole::List,
        kinds::list(),
        None,
        true,
    );
    ctx.world.entity_mut(entity).insert(ListState {
        selected: None,
        len: 0,
    });
    entity
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
