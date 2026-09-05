//! `slotted:card_grid`: the virtualised entry grid. Package B.
//!
//! A fixed pool of `cols * rows` card entities is rebound to
//! `BrowserRuntime::visible[page * per_page ..]` whenever the runtime or the
//! layout changes; cards past the end are hidden. Entities stay stable, so
//! screen-tree snapshots do.

use bevy::prelude::*;
use slotted_registry::Value;
use slotted_ui::{ItemView, SemanticRole, SpawnCtx, UiNodeDef, Widget};

use super::ShowsEntry;
use super::dock::BrowserLayout;
use crate::index::IndexState;
use crate::runtime::BrowserRuntime;

/// On the grid root.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CardGrid {
    /// Current page.
    pub page: usize,
    /// Cards per page, from the layout.
    pub per_page: usize,
}

/// On a card: its position in the pool.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Card(pub u16);

/// The widget.
#[derive(Debug, Default, Clone, Copy)]
pub struct CardGridWidget;

impl Widget for CardGridWidget {
    fn spawn(&self, ctx: &mut SpawnCtx<'_>, _params: &Value, _children: &[UiNodeDef]) -> Entity {
        // PHASE3-IMPL: B — spawn the pool after the first `BrowserLayout`;
        // each card carries `Card`, `ShowsEntry`, `ItemView`, `SemanticRole::Card`,
        // a rarity strip child, a mod badge child and tooltip/click observers.
        ctx.spawn_node((
            Node {
                display: Display::Grid,
                ..default()
            },
            SemanticRole::Grid,
            CardGrid::default(),
        ))
    }
}

/// `BrowserSet::Render`: rebind the card pool on `Changed<BrowserRuntime>`,
/// `Changed<CardGrid>` or `Changed<BrowserLayout>`.
pub fn rebind_cards(
    _runtime: Res<BrowserRuntime>,
    _index: Res<IndexState>,
    _grids: Query<(Entity, &CardGrid), Or<(Changed<CardGrid>, Changed<BrowserLayout>)>>,
    _cards: Query<(&Card, &mut ShowsEntry, &mut ItemView, &mut Visibility)>,
) {
    // PHASE3-IMPL: B
}
