//! `slotted:chip_row`: one toggle chip per recipe category. Package B.

use bevy::prelude::*;
use slotted_registry::Value;
use slotted_ui::{SemanticRole, SpawnCtx, UiNodeDef, Widget};

use crate::recipes::Categories;

/// On a chip: which category it filters.
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub struct Chip(pub crate::category::CategoryId);

/// The widget.
#[derive(Debug, Default, Clone, Copy)]
pub struct ChipRowWidget;

impl Widget for ChipRowWidget {
    fn spawn(&self, ctx: &mut SpawnCtx<'_>, _params: &Value, _children: &[UiNodeDef]) -> Entity {
        ctx.spawn_node((
            Node {
                flex_direction: FlexDirection::Row,
                flex_wrap: FlexWrap::Wrap,
                ..default()
            },
            SemanticRole::Panel,
        ))
    }
}

/// `BrowserSet::Render`: spawn or update chips from `Categories`; a click
/// toggles a `%category` term in the search.
pub fn render_chips(_categories: Res<Categories>, _rows: Query<Entity, With<ChipRowMarker>>) {
    // PHASE3-IMPL: B
}

/// Marker on the chip row root.
#[derive(Component, Debug, Default, Clone, Copy)]
pub struct ChipRowMarker;
