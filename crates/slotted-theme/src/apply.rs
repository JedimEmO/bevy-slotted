//! Painting roles onto nodes.

use bevy::asset::{AssetEvent, Assets, Handle};
use bevy::prelude::*;

use crate::role::Role;
use crate::theme::Theme;

/// The theme in use. Swapping the handle repaints every [`Themed`] node.
#[derive(Resource, Debug, Clone, Default)]
pub struct ActiveTheme(pub Handle<Theme>);

/// The role a node plays. Widgets set this and never touch colours.
///
/// State changes are role changes: the slot widget swaps `slot` for
/// `slot.hover` when hovered. `Changed<Themed>` repaints only that node.
#[derive(Component, Debug, Clone, PartialEq, Eq, Hash)]
pub struct Themed(pub Role);

/// Repaints nodes whose role changed, and every node when the theme asset
/// loads, is modified on disk, or `ActiveTheme` changes.
///
/// Writes only: `BackgroundColor`, `BorderColor`, `BackgroundGradient`,
/// `BoxShadow`, `Node::border_radius`, `ImageNode` (Sliced), `TextColor`,
/// `TextFont::font_size`, and with `blur` the glass `MaterialNode`. Never any
/// other `Node` field, so layout stays the ui crate's.
// PHASE2-IMPL: agent B. Match on `Material`, resolve colours through
// `Theme::color`, elevation through `tokens.elevation`.
pub fn apply_theme(
    active: Res<ActiveTheme>,
    themes: Res<Assets<Theme>>,
    mut events: MessageReader<AssetEvent<Theme>>,
    changed: Query<Entity, Changed<Themed>>,
    all: Query<Entity, With<Themed>>,
) {
    let repaint_all = active.is_changed()
        || events.read().any(|e| match e {
            AssetEvent::Added { id }
            | AssetEvent::Modified { id }
            | AssetEvent::LoadedWithDependencies { id } => *id == active.0.id(),
            _ => false,
        });
    let Some(_theme) = themes.get(&active.0) else {
        return;
    };
    let count = if repaint_all {
        all.iter().count()
    } else {
        changed.iter().count()
    };
    if count > 0 {
        tracing::warn!(count, repaint_all, "apply_theme is not implemented yet");
    }
}
