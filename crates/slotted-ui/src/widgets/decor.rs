//! Separator, spacer and image (menus M1 contract 4.5).

use bevy::prelude::*;
use slotted_theme::{Themed, roles};

use crate::def::{LayoutDirection, Length};
use crate::layers::Decorative;
use crate::screen::SpawnCtx;
use crate::semantic::{SemanticLabel, SemanticRole, WidgetNode};
use crate::widgets::kinds;

/// Spawns a hairline: 1 px thick, across the cross axis of `direction`.
pub fn spawn_separator(ctx: &mut SpawnCtx<'_>, direction: LayoutDirection) -> Entity {
    let (width, height) = match direction {
        LayoutDirection::Row => (Val::Percent(100.0), Val::Px(1.0)),
        LayoutDirection::Column => (Val::Px(1.0), Val::Percent(100.0)),
    };
    ctx.spawn_node((
        Node {
            width,
            height,
            flex_shrink: 0.0,
            ..default()
        },
        Themed(roles::SEPARATOR),
        SemanticRole::Decor,
        WidgetNode(kinds::separator()),
        Decorative,
        Pickable::IGNORE,
    ))
}

/// Spawns empty space: `flex_grow: 1` for `fill`, else the given length
/// along the parent's main axis.
pub fn spawn_spacer(ctx: &mut SpawnCtx<'_>, size: Length) -> Entity {
    let spacing_sm = ctx.tokens().spacing.sm;
    let grow = matches!(size, Length::Percent(p) if (p - 100.0).abs() < f32::EPSILON);
    let val = crate::widgets::length_val(Some(size), spacing_sm);
    ctx.spawn_node((
        Node {
            flex_grow: if grow { 1.0 } else { 0.0 },
            flex_shrink: 0.0,
            flex_basis: if grow { Val::Auto } else { val },
            ..default()
        },
        SemanticRole::Decor,
        WidgetNode(kinds::spacer()),
        Decorative,
        Pickable::IGNORE,
    ))
}

/// Spawns an image from the asset server. Without one (a bare test world)
/// the node still lays out with nothing to draw.
pub fn spawn_image(ctx: &mut SpawnCtx<'_>, path: &str, width: Length, height: Length) -> Entity {
    let spacing_sm = ctx.tokens().spacing.sm;
    let image = crate::widgets::icon_image(ctx.world, &crate::def::IconDef::Image(path.to_owned()));
    ctx.spawn_node((
        Node {
            width: crate::widgets::length_val(Some(width), spacing_sm),
            height: crate::widgets::length_val(Some(height), spacing_sm),
            flex_shrink: 0.0,
            ..default()
        },
        image,
        SemanticRole::Decor,
        SemanticLabel(path.to_owned()),
        WidgetNode(kinds::image()),
        Decorative,
        Pickable::IGNORE,
    ))
}
