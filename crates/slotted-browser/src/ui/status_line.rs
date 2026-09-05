//! `slotted:status_line`: the panel's footer. Package B.
//!
//! The moodboard's `.bfoot` row: a result count on the left and the hotkey
//! hint on the right. It is also the panel's loading affordance, because the
//! card grid has nothing to bind while `IndexState::Building` and an empty
//! grid with no explanation reads as "no items" rather than "not yet".
//!
//! Neither node carries a [`SemanticRole`](slotted_ui::SemanticRole), so the
//! screen tree elides the whole footer and the contract's tree shape is
//! unchanged.

use bevy::prelude::*;
use slotted_registry::Value;
use slotted_theme::Themed;
use slotted_ui::{SpawnCtx, TestId, UiNodeDef, Widget};

use super::roles;
use crate::index::IndexState;
use crate::runtime::BrowserRuntime;

/// Marker on the count text, the left half of the footer.
#[derive(Component, Debug, Default, Clone, Copy)]
pub struct StatusText;

/// The widget.
#[derive(Debug, Default, Clone, Copy)]
pub struct StatusLineWidget;

impl Widget for StatusLineWidget {
    fn spawn(&self, ctx: &mut SpawnCtx<'_>, _params: &Value, _children: &[UiNodeDef]) -> Entity {
        let row = ctx.spawn_node((
            Node {
                width: percent(100),
                flex_direction: FlexDirection::Row,
                justify_content: JustifyContent::SpaceBetween,
                align_items: AlignItems::Center,
                column_gap: px(8),
                flex_shrink: 0.0,
                ..default()
            },
            Pickable::IGNORE,
        ));
        ctx.world.spawn((
            Node::default(),
            Text::new("indexing…"),
            Themed(roles::STATUS),
            TestId::new("browser.status"),
            StatusText,
            Pickable::IGNORE,
            ChildOf(row),
        ));
        ctx.world.spawn((
            Node::default(),
            Text::new("R recipes / U uses / A bookmark"),
            Themed(roles::HINT),
            Pickable::IGNORE,
            ChildOf(row),
        ));
        row
    }
}

/// `BrowserSet::Render`: the footer says whether the index has landed and, once
/// it has, how many entries the query left.
pub fn render_status(
    index: Res<IndexState>,
    runtime: Res<BrowserRuntime>,
    mut texts: Query<&mut Text, With<StatusText>>,
) {
    let want = if index.ready().is_some() {
        let n = runtime.visible.len();
        if n == 1 {
            "1 item".to_owned()
        } else {
            format!("{n} items")
        }
    } else {
        "indexing…".to_owned()
    };
    for mut text in &mut texts {
        if text.0 != want {
            text.0.clone_from(&want);
        }
    }
}
