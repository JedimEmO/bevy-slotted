//! `bar` and `progress`: a masked fill in one of four directions. Phase 6
//! contract section 1.2. Shares [`FillValue`](super::tank::FillValue) and the
//! property binding with the tank.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use slotted_model::PropertyId;

use crate::def::{Direction, Tags};
use crate::screen::SpawnCtx;
use crate::semantic::{SemanticRole, WidgetNode};
use crate::widgets::tank::{FillValue, PropertyBinding, TooltipSource};

/// Which pair of roles a bar paints with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BarStyle {
    /// `bar` / `bar.fill`.
    #[default]
    Bar,
    /// `progress` / `progress.fill`: the arrow between machine slots.
    Progress,
}

/// Bar state on the root.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct BarState {
    /// Fill direction.
    pub direction: Direction,
    /// Role pair.
    pub style: BarStyle,
    /// Whether a `bar.text` child exists.
    pub text: bool,
}

/// The optional `value / max` text child of a bar. Purely visual.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct BarText;

/// Parameters of `slotted:bar` and `slotted:progress`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BarParams {
    /// Value property.
    pub property: PropertyId,
    /// Maximum property.
    pub max: PropertyId,
    /// Fill direction.
    #[serde(default = "right")]
    pub direction: Direction,
    /// Show `value / max`. Ignored by `progress`.
    #[serde(default)]
    pub text: bool,
}

fn right() -> Direction {
    Direction::Right
}

impl Default for BarParams {
    fn default() -> Self {
        Self {
            property: PropertyId(0),
            max: PropertyId(1),
            direction: Direction::Right,
            text: false,
        }
    }
}

/// Spawns a bar or progress arrow. Root components per contract 1.2.
pub fn spawn_bar(
    ctx: &mut SpawnCtx<'_>,
    params: &BarParams,
    style: BarStyle,
    _tags: &Tags,
) -> Entity {
    // PHASE6-IMPL: A. Node 120x12 along `direction` (progress: 24x16),
    // Themed(BAR | PROGRESS), SemanticRole::Bar, SemanticLabel, FillValue,
    // PropertyBinding when ctx.menu is Some, Hovered, Pickable, TooltipSource,
    // on_slot_over; FillNode child with Themed(BAR_FILL | PROGRESS_FILL)
    // anchored to the fill origin; BarText child when `text`.
    let kind = match style {
        BarStyle::Bar => crate::widgets::kinds::bar(),
        BarStyle::Progress => crate::widgets::kinds::progress(),
    };
    let binding = ctx.menu.map(|menu| PropertyBinding {
        menu,
        value: params.property,
        max: params.max,
    });
    let entity = ctx.spawn_node((
        Node::default(),
        SemanticRole::Bar,
        WidgetNode(kind),
        FillValue::default(),
        BarState {
            direction: params.direction,
            style,
            text: params.text && style == BarStyle::Bar,
        },
        TooltipSource,
    ));
    if let Some(binding) = binding {
        ctx.world.entity_mut(entity).insert(binding);
    }
    entity
}
