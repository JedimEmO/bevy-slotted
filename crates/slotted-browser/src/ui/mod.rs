//! The browser panel: a `ScreenDef`-compatible overlay docked beside the open
//! screen. Contract section 7. Package B owns everything under `ui/`.
//!
//! The panel is spawned from [`panel_def`] through `slotted_ui::SpawnCtx`,
//! so its widgets are ordinary `Custom` nodes registered in the
//! `WidgetRegistry` under the kinds in [`kinds`], and RON or Lua can restyle
//! it later.

pub mod bookmarks_strip;
pub mod card_grid;
pub mod chip_row;
pub mod dock;
pub mod ghost_drag;
pub mod hotkeys;
pub mod panel;
pub mod recipe_view;
pub mod search_field;

use bevy::prelude::*;
use slotted_theme::Role;
use slotted_ui::{Layout, LayoutDirection, ScreenKind, Side, Tags, UiNodeDef, WidgetKind};

pub use card_grid::{Card, CardGrid};
pub use dock::BrowserLayout;
pub use panel::BrowserPanel;

use crate::index::EntryId;
use crate::plugin::BrowserSet;

/// Widget kinds the panel is built from.
pub mod kinds {
    use slotted_ui::WidgetKind;

    /// `slotted:search_field`.
    pub fn search_field() -> WidgetKind {
        WidgetKind::new("slotted:search_field")
    }
    /// `slotted:chip_row`.
    pub fn chip_row() -> WidgetKind {
        WidgetKind::new("slotted:chip_row")
    }
    /// `slotted:card_grid`.
    pub fn card_grid() -> WidgetKind {
        WidgetKind::new("slotted:card_grid")
    }
    /// `slotted:bookmarks_strip`.
    pub fn bookmarks_strip() -> WidgetKind {
        WidgetKind::new("slotted:bookmarks_strip")
    }
    /// `slotted:recipe_view`.
    pub fn recipe_view() -> WidgetKind {
        WidgetKind::new("slotted:recipe_view")
    }
}

/// The screen kind of every browser panel root.
pub fn panel_kind() -> ScreenKind {
    ScreenKind::new("slotted:browser")
}

/// Theme roles the panel uses; the glass theme defines them under `browser.*`.
pub mod roles {
    use slotted_theme::Role;

    /// The panel background.
    pub const PANEL: Role = Role::new_static("browser.panel");
    /// A card at rest.
    pub const CARD: Role = Role::new_static("browser.card");
    /// A hovered card.
    pub const CARD_HOVER: Role = Role::new_static("browser.card.hover");
    /// A category chip.
    pub const CHIP: Role = Role::new_static("browser.chip");
    /// A selected chip or tab.
    pub const CHIP_ACTIVE: Role = Role::new_static("browser.chip.active");
    /// The search field.
    pub const SEARCH: Role = Role::new_static("browser.search");
    /// A recipe slot with no source, after a failed transfer.
    pub const SLOT_MISSING: Role = Role::new_static("browser.slot.missing");
}

/// The panel's tree. `side` becomes the root's `side=` tag.
pub fn panel_def(side: Side) -> UiNodeDef {
    let custom = |kind: WidgetKind, test_id: &str| UiNodeDef::Custom {
        kind,
        params: slotted_registry::Value::Unit,
        children: Vec::new(),
        tags: Tags::new().with(Tags::TEST_ID, test_id),
    };
    UiNodeDef::Panel {
        role: Role::new_static("browser.panel"),
        layout: Layout {
            direction: LayoutDirection::Column,
            gap: 1.0,
            padding: 1.0,
            ..Layout::default()
        },
        children: vec![
            custom(kinds::search_field(), "browser.search"),
            custom(kinds::chip_row(), "browser.chips"),
            custom(kinds::card_grid(), "browser.cards"),
            custom(kinds::bookmarks_strip(), "browser.bookmarks"),
            custom(kinds::recipe_view(), "browser.recipes"),
        ],
        tags: Tags::new().with("side", side.as_str()),
    }
}

/// Which entry a card or recipe slot shows right now; `None` is an empty cell.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ShowsEntry(pub Option<EntryId>);

/// Registers widgets, observers and systems. Called by the plugin when
/// `BrowserConfig::ui` is on.
pub fn register(app: &mut App) {
    // PHASE3-IMPL: B — register the five widgets in `WidgetRegistry`, the
    // attach/detach observers and every set's systems. The skeleton wires the
    // widgets and the no-op systems so the sets exist and the tree has shape.
    if let Some(mut registry) = app
        .world_mut()
        .get_resource_mut::<slotted_ui::WidgetRegistry>()
    {
        registry.register(kinds::search_field(), search_field::SearchFieldWidget);
        registry.register(kinds::chip_row(), chip_row::ChipRowWidget);
        registry.register(kinds::card_grid(), card_grid::CardGridWidget);
        registry.register(
            kinds::bookmarks_strip(),
            bookmarks_strip::BookmarksStripWidget,
        );
        registry.register(kinds::recipe_view(), recipe_view::RecipeViewWidget);
    }
    app.add_observer(panel::attach_panel)
        .add_observer(panel::detach_panel)
        .add_systems(
            Update,
            (
                (
                    hotkeys::track_keyboard_focus,
                    hotkeys::browser_hotkeys,
                    search_field::diff_search_field,
                )
                    .chain()
                    .in_set(BrowserSet::Input),
                (
                    card_grid::rebind_cards,
                    recipe_view::render_recipe_view,
                    recipe_view::cycle_alternatives,
                    bookmarks_strip::render_bookmarks,
                    chip_row::render_chips,
                )
                    .chain()
                    .in_set(BrowserSet::Render),
            ),
        )
        .add_systems(PostUpdate, dock::dock_panels.in_set(BrowserSet::Layout));
}
