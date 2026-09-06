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
pub mod status_line;

use bevy::prelude::*;
use slotted_ecs::Registries;
use slotted_model::ItemStack;
use slotted_theme::Role;
use slotted_ui::{Layout, LayoutDirection, ScreenKind, Side, Tags, UiNodeDef, WidgetKind};

pub use card_grid::{Card, CardGrid};
pub use dock::BrowserLayout;
pub use panel::BrowserPanel;

use crate::index::EntryId;
use crate::ingredient::{Ingredient, IngredientTypes, IngredientValue};
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
    /// `slotted:status_line`.
    pub fn status_line() -> WidgetKind {
        WidgetKind::new("slotted:status_line")
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
    /// The search field while it holds keyboard focus.
    pub const SEARCH_FOCUS: Role = Role::new_static("browser.search.focus");
    /// A recipe slot with no source, after a failed transfer.
    pub const SLOT_MISSING: Role = Role::new_static("browser.slot.missing");
    /// A recipe slot at rest.
    pub const SLOT: Role = Role::new_static("browser.slot");
    /// The arrow between a recipe's inputs and its output.
    pub const ARROW: Role = Role::new_static("browser.recipe.arrow");
    /// The moving part of the arrow.
    pub const ARROW_PROGRESS: Role = Role::new_static("browser.recipe.arrow.progress");
    /// One entry in the bookmark strip.
    pub const BOOKMARK: Role = Role::new_static("browser.bookmark");
    /// A recipe category tab.
    pub const TAB: Role = Role::new_static("browser.tab");
    /// The open recipe category tab.
    pub const TAB_ACTIVE: Role = Role::new_static("browser.tab.active");
    /// The mod-namespace badge on a card.
    pub const BADGE: Role = Role::new_static("browser.badge");
    /// A button the current state refuses, such as `+` with no handler.
    pub const BUTTON_DISABLED: Role = Role::new_static("browser.button.disabled");
    /// A card's display name.
    pub const CARD_NAME: Role = Role::new_static("browser.card.name");
    /// The rarity strip across the top of a card.
    pub const RARITY: Role = Role::new_static("browser.rarity");
    /// A chip's or tab's label text.
    pub const CHIP_TEXT: Role = Role::new_static("browser.chip.text");
    /// A selected chip's or tab's label text.
    pub const CHIP_TEXT_ACTIVE: Role = Role::new_static("browser.chip.text.active");
    /// The recipe view's title.
    pub const TITLE: Role = Role::new_static("browser.title");
    /// The footer's result count and index state.
    pub const STATUS: Role = Role::new_static("browser.status");
    /// The footer's hotkey hint and the "used in" line.
    pub const HINT: Role = Role::new_static("browser.hint");
    /// A small pill button: transfer, back, forward.
    pub const PILL: Role = Role::new_static("browser.pill");
    /// A small pill button's label.
    pub const PILL_TEXT: Role = Role::new_static("browser.pill.text");
    /// The stand-in glyph an iconless card draws.
    pub const GLYPH: Role = Role::new_static("browser.glyph");
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
            gap: dock::PANEL_GAP,
            padding: dock::PANEL_PADDING,
            ..Layout::default()
        },
        children: vec![
            custom(kinds::search_field(), "browser.search"),
            custom(kinds::chip_row(), "browser.chips"),
            custom(kinds::card_grid(), "browser.cards"),
            custom(kinds::bookmarks_strip(), "browser.bookmarks"),
            custom(kinds::recipe_view(), "browser.recipes"),
            custom(kinds::status_line(), "browser.footer"),
        ],
        tags: Tags::new().with("side", side.as_str()),
    }
}

/// The theme role of a card's rarity strip: `browser.rarity.<rarity>`, the
/// same naming the slot's rarity ring uses one level up.
pub fn rarity_strip_role(rarity: slotted_registry::Rarity) -> Role {
    Role::new(format!("browser.rarity.{}", rarity.as_str()))
}

/// Which entry a card or recipe slot shows right now; `None` is an empty cell.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ShowsEntry(pub Option<EntryId>);

/// Which ingredient a card, bookmark or recipe slot currently shows.
///
/// Hotkeys, tooltips and the ghost drag all resolve what is under the pointer
/// through this one component, so a card, a bookmark and a recipe slot look
/// the same to them.
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub struct ShowsIngredient(pub Ingredient);

/// The stack an ingredient renders as, or `None` when its type has none.
pub fn ingredient_stack(
    types: &IngredientTypes,
    ing: &Ingredient,
    count: u32,
) -> Option<ItemStack> {
    types.get(&ing.ty).and_then(|ty| ty.as_stack(ing, count))
}

/// The stable string an `entry=` tag carries: `minecraft:coal` for an item,
/// `#c:ingots` for a tag, the namespaced id for anything else.
///
/// Locators match on this (`by::role(Card).tag("entry", "minecraft:coal")`),
/// so it has to read well and not depend on dense ids.
pub fn ingredient_key(registries: Option<&Registries>, ing: &Ingredient) -> String {
    match &ing.value {
        IngredientValue::Item(id) => registries
            .and_then(|r| r.items.name_of(*id).map(ToString::to_string))
            .unwrap_or_else(|| format!("item#{}", id.0)),
        IngredientValue::Fluid(ns) | IngredientValue::Info(ns) => ns.to_string(),
        IngredientValue::Tag(ns) => format!("#{ns}"),
    }
}

/// Registers widgets, observers and systems. Called by the plugin when
/// `BrowserConfig::ui` is on.
pub fn register(app: &mut App) {
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
        registry.register(kinds::status_line(), status_line::StatusLineWidget);
    }
    if let Some(mut sources) = app
        .world_mut()
        .get_resource_mut::<slotted_ui::VirtualGridSources>()
    {
        sources.register(card_grid::card_source_id(), card_grid::CardSource);
    }
    app.init_resource::<hotkeys::HoverTarget>()
        .add_observer(panel::attach_panel)
        .add_observer(panel::detach_panel)
        .add_observer(ghost_drag::on_ghost_drop)
        .add_systems(
            Update,
            (
                (
                    hotkeys::track_keyboard_focus,
                    hotkeys::track_hover_target,
                    hotkeys::browser_hotkeys,
                    card_grid::page_card_grid,
                    search_field::diff_search_field,
                )
                    .chain()
                    .in_set(BrowserSet::Input),
                (
                    card_grid::size_card_pool,
                    card_grid::card_state_roles,
                    chip_row::render_chips,
                    bookmarks_strip::render_bookmarks,
                    recipe_view::render_recipe_view,
                    recipe_view::toggle_browse_regions,
                    recipe_view::cycle_alternatives,
                    recipe_view::animate_arrow,
                    recipe_view::update_transfer_button,
                    recipe_view::paint_missing_slots,
                    search_field::search_field_state_role,
                    status_line::render_status,
                )
                    .chain()
                    .in_set(BrowserSet::Render)
                    // `size_card_pool` writes the grid's window; the shared
                    // pooling pass reads it and rebinds the cards.
                    .before(slotted_ui::refresh_virtual_grids),
            ),
        )
        .add_systems(PostUpdate, dock::dock_panels.in_set(BrowserSet::Layout));
}
