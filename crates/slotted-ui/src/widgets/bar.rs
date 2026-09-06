//! `bar` and `progress`: a masked fill in one of four directions. Phase 6
//! contract section 1.2. Shares [`FillValue`] and the
//! property binding with the tank.

use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use slotted_model::PropertyId;
use slotted_theme::{Role, Themed, roles};

use crate::def::{Direction, Tags};
use crate::screen::SpawnCtx;
use crate::semantic::{SemanticLabel, SemanticRole, WidgetNode};
use crate::widgets::BORDER_WIDTH;
use crate::widgets::tank::{
    FillNode, FillOrigin, FillValue, PropertyBinding, TooltipSource, fill_label, fill_node,
};

/// Which pair of roles a bar paints with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BarStyle {
    /// `bar` / `bar.fill`.
    #[default]
    Bar,
    /// `progress` / `progress.fill`: the arrow between machine slots.
    Progress,
}

impl BarStyle {
    /// The well and fill roles this style paints with.
    pub const fn roles(self) -> (Role, Role) {
        match self {
            Self::Bar => (roles::BAR, roles::BAR_FILL),
            Self::Progress => (roles::PROGRESS, roles::PROGRESS_FILL),
        }
    }

    /// The widget kind published as `WidgetNode`.
    pub fn kind(self) -> crate::def::WidgetKind {
        match self {
            Self::Bar => crate::widgets::kinds::bar(),
            Self::Progress => crate::widgets::kinds::progress(),
        }
    }

    /// Size along and across the fill axis, in logical px. The moodboard
    /// sizes a progress arrow at 40 px, which is one slot: it reads as the
    /// gap between two slots rather than as a hyphen.
    pub const fn size(self) -> (f32, f32) {
        match self {
            Self::Bar => (120.0, 12.0),
            Self::Progress => (40.0, 14.0),
        }
    }

    /// Corner radius, given the theme's small radius and the size across the
    /// fill axis. A progress arrow takes a fully rounded track.
    pub fn radius(self, small: f32) -> f32 {
        match self {
            Self::Bar => small,
            Self::Progress => self.size().1 / 2.0,
        }
    }
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
///
/// The root is `120x12` (a progress arrow `40x14`) measured along its own
/// fill direction, so an upward bar is tall rather than wide.
pub fn spawn_bar(
    ctx: &mut SpawnCtx<'_>,
    params: &BarParams,
    style: BarStyle,
    _tags: &Tags,
) -> Entity {
    let (well, fill_role) = style.roles();
    let (along, across) = style.size();
    let (width, height) = match params.direction {
        Direction::Right | Direction::Left => (along, across),
        Direction::Up | Direction::Down => (across, along),
    };
    let binding = ctx.menu.map(|menu| PropertyBinding {
        menu,
        value: params.property,
        max: params.max,
    });
    let text = params.text && style == BarStyle::Bar;
    let radius = style.radius(ctx.tokens().radii.sm);
    let fill = FillValue::default();
    let entity = ctx.spawn_node((
        Node {
            width: px(width),
            height: px(height),
            border: UiRect::all(px(BORDER_WIDTH)),
            position_type: PositionType::Relative,
            overflow: Overflow::clip(),
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            border_radius: BorderRadius::all(px(radius)),
            ..default()
        },
        Themed(well),
        SemanticRole::Bar,
        SemanticLabel(fill_label(&fill, "")),
        WidgetNode(style.kind()),
        fill,
        FillOrigin(params.direction),
        BarState {
            direction: params.direction,
            style,
            text,
        },
        Hovered::default(),
        Pickable::default(),
        TooltipSource,
    ));
    ctx.world.spawn((
        fill_node(params.direction, 0.0),
        Themed(fill_role),
        FillNode,
        Pickable::IGNORE,
        ChildOf(entity),
    ));
    if text {
        ctx.world.spawn((
            Node::default(),
            Text::new(fill_label(&fill, "")),
            Themed(roles::BAR_TEXT),
            BarText,
            Pickable::IGNORE,
            ChildOf(entity),
        ));
    }
    if let Some(binding) = binding {
        ctx.world.entity_mut(entity).insert(binding);
    }
    ctx.world
        .entity_mut(entity)
        .observe(crate::tooltip::on_slot_over);
    entity
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_progress_arrow_paints_with_the_progress_roles() {
        assert_eq!(
            BarStyle::Progress.roles(),
            (roles::PROGRESS, roles::PROGRESS_FILL)
        );
        assert_eq!(BarStyle::Bar.roles(), (roles::BAR, roles::BAR_FILL));
    }

    #[test]
    fn a_bar_is_measured_along_its_fill_direction() {
        assert_eq!(BarStyle::Bar.size(), (120.0, 12.0));
        assert_eq!(BarStyle::Progress.size(), (40.0, 14.0));
    }

    #[test]
    fn a_progress_track_is_a_pill_and_a_bar_takes_the_theme_radius() {
        assert!((BarStyle::Progress.radius(4.0) - 7.0).abs() < f32::EPSILON);
        assert!((BarStyle::Bar.radius(4.0) - 4.0).abs() < f32::EPSILON);
    }
}
