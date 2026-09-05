//! `slotted:bookmarks_strip`: the bookmark row. Package B.

use bevy::prelude::*;
use slotted_registry::Value;
use slotted_ui::{SemanticRole, SpawnCtx, UiNodeDef, Widget};

use crate::bookmarks::Bookmarks;

/// Marker on the strip root.
#[derive(Component, Debug, Default, Clone, Copy)]
pub struct BookmarksStrip;

/// The widget.
#[derive(Debug, Default, Clone, Copy)]
pub struct BookmarksStripWidget;

impl Widget for BookmarksStripWidget {
    fn spawn(&self, ctx: &mut SpawnCtx<'_>, _params: &Value, _children: &[UiNodeDef]) -> Entity {
        ctx.spawn_node((
            Node {
                flex_direction: FlexDirection::Row,
                flex_wrap: FlexWrap::Wrap,
                ..default()
            },
            SemanticRole::Panel,
            BookmarksStrip,
        ))
    }
}

/// `BrowserSet::Render`: one `SemanticRole::Bookmark` node per entry when
/// `BrowserRuntime::bookmarks_version` changes.
pub fn render_bookmarks(_bookmarks: Res<Bookmarks>, _strips: Query<Entity, With<BookmarksStrip>>) {
    // PHASE3-IMPL: B
}
