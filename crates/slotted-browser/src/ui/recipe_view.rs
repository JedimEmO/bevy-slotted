//! `slotted:recipe_view`: category tabs, the laid-out recipe, transfer
//! button, history and "used in". Package B.
//!
//! The view keeps a fixed skeleton of five regions and refills them whenever
//! `BrowserRuntime::open` changes. The tab row, the slot area and the "used
//! in" list carry no `SemanticRole`, so the screen tree lifts their children
//! and reads `RecipeView > Tab*, RecipeSlot*, Button*, Panel[browser.uses]`.

use bevy::input_focus::tab_navigation::TabIndex;
use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use bevy::ui::InteractionDisabled;
use bevy::ui::ui_transform::UiTransform;
use bevy::ui_widgets::Activate;
use slotted_ecs::{OpenMenu, Registries};
use slotted_model::Inventories;
use slotted_registry::Value;
use slotted_theme::{ActiveTheme, Motion, Theme, Themed};
use slotted_ui::{
    ItemView, ScreenRoot, SemanticLabel, SemanticRole, SpawnCtx, Tags, TestId, UiNodeDef, Widget,
    widgets::SLOT_SIZE,
};

use super::panel::BrowserPanel;
use super::{ShowsIngredient, ingredient_key, ingredient_stack, roles};
use crate::category::{CategoryId, RecipeLayout, RecipeRef, RecipeSlotIx, SlotRole};
use crate::events::{RecipeNav, TransferFailed, TransferRequested};
use crate::ingredient::{Ingredient, IngredientCtx, IngredientTypes, Subtypes};
use crate::recipes::{Categories, RecipeStore};
use crate::runtime::{BrowserRuntime, PageMode, RecipePage};
use crate::transfer::TransferHandlers;

/// On the view root: the page it last rendered.
#[derive(Component, Debug, Default, Clone)]
pub struct RecipeViewRoot {
    /// What is drawn right now; `None` means the view is closed.
    pub rendered: Option<RecipePage>,
    /// Set when something other than the page invalidates the drawing.
    pub dirty: bool,
}

/// On a recipe slot node: which layout slot and which alternative shows.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecipeSlotNode {
    /// Index into the current `RecipeLayout`.
    pub ix: RecipeSlotIx,
    /// Which alternative is showing.
    pub alternative: usize,
}

/// The concrete ingredients one recipe slot cycles through.
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub struct SlotAlternatives(pub Vec<Ingredient>);

/// On the `+` button: the recipe a press would transfer.
#[derive(Component, Debug, Default, Clone, Copy)]
pub struct TransferButton {
    /// The open recipe, when there is one.
    pub recipe: Option<RecipeRef>,
}

/// On a category tab.
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub struct RecipeTab(pub CategoryId);

/// Markers for the view's five regions.
#[derive(Component, Debug, Default, Clone, Copy)]
pub struct TabRow;
/// The area recipe slots and the arrow are positioned in.
#[derive(Component, Debug, Default, Clone, Copy)]
pub struct SlotArea;
/// The "used in" list.
#[derive(Component, Debug, Default, Clone, Copy)]
pub struct UsesPanel;
/// The focus's name in the header.
#[derive(Component, Debug, Default, Clone, Copy)]
pub struct RecipeTitle;
/// The arrow's moving part.
#[derive(Component, Debug, Default, Clone, Copy)]
pub struct ArrowProgress;
/// History buttons.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct HistoryButton {
    /// Forward rather than back.
    pub forward: bool,
}

/// The widget.
#[derive(Debug, Default, Clone, Copy)]
pub struct RecipeViewWidget;

impl Widget for RecipeViewWidget {
    fn spawn(&self, ctx: &mut SpawnCtx<'_>, _params: &Value, _children: &[UiNodeDef]) -> Entity {
        let root = ctx.spawn_node((
            Node {
                display: Display::None,
                flex_direction: FlexDirection::Column,
                row_gap: px(10),
                ..default()
            },
            SemanticRole::RecipeView,
            RecipeViewRoot::default(),
            Visibility::Hidden,
        ));
        let world = &mut *ctx.world;
        // The header: a back pill and the focus's name, the moodboard's
        // `.recipe .hdr`. It carries no `SemanticRole`, so the tree lifts the
        // button and the title text is elided.
        let header = world
            .spawn((
                Node {
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    column_gap: px(10),
                    ..default()
                },
                ChildOf(root),
            ))
            .id();
        spawn_view_button(
            world,
            header,
            "browser.back",
            "<",
            HistoryButton { forward: false },
        );
        world.spawn((
            Node::default(),
            Text::new(String::new()),
            Themed(roles::TITLE),
            RecipeTitle,
            Pickable::IGNORE,
            ChildOf(header),
        ));
        world.spawn((
            Node {
                flex_direction: FlexDirection::Row,
                column_gap: px(6),
                row_gap: px(6),
                flex_wrap: FlexWrap::Wrap,
                ..default()
            },
            TabRow,
            ChildOf(root),
        ));
        world.spawn((
            Node {
                position_type: PositionType::Relative,
                align_self: AlignSelf::Center,
                min_height: px(SLOT_SIZE),
                ..default()
            },
            SlotArea,
            ChildOf(root),
        ));
        // The three buttons sit in a row that carries no `SemanticRole`, so
        // the screen tree still lifts them to the view root while the layout
        // keeps them side by side instead of stacked full width.
        let controls = world
            .spawn((
                Node {
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    column_gap: px(6),
                    ..default()
                },
                ChildOf(root),
            ))
            .id();
        spawn_view_button(
            world,
            controls,
            "browser.transfer",
            "+",
            TransferButton::default(),
        );
        spawn_view_button(
            world,
            controls,
            "browser.forward",
            ">",
            HistoryButton { forward: true },
        );
        world.spawn((
            Node {
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                flex_wrap: FlexWrap::Wrap,
                column_gap: px(6),
                row_gap: px(4),
                ..default()
            },
            SemanticRole::Panel,
            TestId::new("browser.uses"),
            UsesPanel,
            ChildOf(root),
        ));
        root
    }
}

fn spawn_view_button(
    world: &mut World,
    parent: Entity,
    test_id: &str,
    label: &str,
    marker: impl Bundle,
) -> Entity {
    let button = world
        .spawn((
            Node {
                min_width: px(26),
                padding: UiRect::axes(px(9), px(4)),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            Themed(roles::PILL),
            SemanticRole::Button,
            SemanticLabel(label.to_owned()),
            TestId::new(test_id),
            bevy::ui_widgets::Button,
            Hovered::default(),
            TabIndex(0),
            Pickable::default(),
            marker,
            ChildOf(parent),
        ))
        .id();
    world.spawn((
        Node::default(),
        Text::new(label.to_owned()),
        Themed(roles::PILL_TEXT),
        Pickable::IGNORE,
        ChildOf(button),
    ));
    world
        .entity_mut(button)
        .observe(on_view_button)
        .observe(on_tab_or_button_click);
    button
}

/// Everything the view needs to draw one page, resolved once per rebuild.
struct Page {
    title: String,
    tabs: Vec<CategoryId>,
    recipe: Option<RecipeRef>,
    layout: Option<RecipeLayout>,
    uses: Vec<Ingredient>,
}

fn resolve(world: &World, page: &RecipePage) -> Page {
    let (Some(registries), Some(store), Some(categories), Some(types)) = (
        world.get_resource::<Registries>(),
        world.get_resource::<RecipeStore>(),
        world.get_resource::<Categories>(),
        world.get_resource::<IngredientTypes>(),
    ) else {
        return Page {
            title: String::new(),
            tabs: Vec::new(),
            recipe: None,
            layout: None,
            uses: Vec::new(),
        };
    };
    let grouped = match page.mode {
        PageMode::Recipes => store.recipes_for(&page.focus, registries, categories),
        PageMode::Uses => store.uses(&page.focus, registries, categories),
    };
    let tabs: Vec<CategoryId> = grouped.iter().map(|(id, _)| id.clone()).collect();
    let recipes = grouped
        .iter()
        .find(|(id, _)| *id == page.category)
        .or_else(|| grouped.first())
        .map(|(_, r)| r.clone())
        .unwrap_or_default();
    let recipe = recipes
        .get(page.recipe % recipes.len().max(1))
        .or_else(|| recipes.first())
        .copied();
    let layout =
        recipe.and_then(|r| store.layout(r, Some(&page.focus), registries, categories, types));
    let uses = store
        .uses(&page.focus, registries, categories)
        .into_iter()
        .flat_map(|(_, recipes)| recipes)
        .filter_map(|r| {
            store
                .layout(r, None, registries, categories, types)
                .and_then(|l| output_of(&l))
        })
        .collect();
    let loc = world
        .get_resource::<slotted_ui::Localization>()
        .cloned()
        .unwrap_or_default();
    let title = world
        .get_resource::<Subtypes>()
        .and_then(|subtypes| {
            types.get(&page.focus.ty).map(|ty| {
                ty.display_name(
                    &page.focus,
                    &IngredientCtx {
                        registries,
                        subtypes,
                        loc: &loc,
                    },
                )
            })
        })
        .unwrap_or_default();
    Page {
        title,
        tabs,
        recipe,
        layout,
        uses,
    }
}

/// The first alternative of a layout's first output slot.
fn output_of(layout: &RecipeLayout) -> Option<Ingredient> {
    layout
        .with_role(SlotRole::Output)
        .next()
        .and_then(|(_, slot)| slot.alternatives.first().cloned())
}

/// `BrowserSet::Render`: redraw whenever `BrowserRuntime::open` changes.
pub fn render_recipe_view(world: &mut World) {
    let open = world.resource::<BrowserRuntime>().open.clone();
    let roots: Vec<Entity> = {
        let mut query = world.query_filtered::<Entity, With<RecipeViewRoot>>();
        query.iter(world).collect()
    };
    for root in roots {
        let stale = world
            .get::<RecipeViewRoot>(root)
            .is_some_and(|state| state.rendered != open || state.dirty);
        if !stale {
            continue;
        }
        match &open {
            Some(page) => {
                let resolved = resolve(world, page);
                draw(world, root, page, &resolved);
            }
            None => clear(world, root),
        }
        if let Some(mut state) = world.get_mut::<RecipeViewRoot>(root) {
            state.rendered.clone_from(&open);
            state.dirty = false;
        }
    }
}

/// The first descendant of `root` carrying `T`. The view's regions are one or
/// two levels down, since the button row is an elided wrapper.
fn region<T: Component>(world: &mut World, root: Entity) -> Option<Entity> {
    let children: Vec<Entity> = world
        .get::<Children>(root)
        .map(|c| c.iter().collect())
        .unwrap_or_default();
    for child in &children {
        if world.get::<T>(*child).is_some() {
            return Some(*child);
        }
    }
    children
        .into_iter()
        .find_map(|child| region::<T>(world, child))
}

fn clear_children(world: &mut World, parent: Entity) {
    let children: Vec<Entity> = world
        .get::<Children>(parent)
        .map(|c| c.iter().collect())
        .unwrap_or_default();
    for child in children {
        world.entity_mut(child).despawn();
    }
}

fn clear(world: &mut World, root: Entity) {
    if let Some(title) = region::<RecipeTitle>(world, root)
        && let Some(mut text) = world.get_mut::<Text>(title)
    {
        text.0.clear();
    }
    for parent in [
        region::<TabRow>(world, root),
        region::<SlotArea>(world, root),
        region::<UsesPanel>(world, root),
    ]
    .into_iter()
    .flatten()
    {
        clear_children(world, parent);
    }
    if let Some(mut visibility) = world.get_mut::<Visibility>(root) {
        *visibility = Visibility::Hidden;
    }
    if let Some(mut node) = world.get_mut::<Node>(root)
        && node.display != Display::None
    {
        node.display = Display::None;
    }
    if let Some(button) = region::<TransferButton>(world, root)
        && let Some(mut state) = world.get_mut::<TransferButton>(button)
    {
        state.recipe = None;
    }
}

fn draw(world: &mut World, root: Entity, page: &RecipePage, resolved: &Page) {
    if let Some(mut visibility) = world.get_mut::<Visibility>(root) {
        *visibility = Visibility::Inherited;
    }
    if let Some(mut node) = world.get_mut::<Node>(root)
        && node.display != Display::Flex
    {
        node.display = Display::Flex;
    }
    if let Some(tabs) = region::<TabRow>(world, root) {
        clear_children(world, tabs);
        for id in &resolved.tabs {
            spawn_tab(world, tabs, id.clone(), *id == page.category);
        }
    }
    if let Some(area) = region::<SlotArea>(world, root) {
        clear_children(world, area);
        if let Some(layout) = &resolved.layout {
            draw_layout(world, area, layout);
        }
    }
    if let Some(button) = region::<TransferButton>(world, root)
        && let Some(mut state) = world.get_mut::<TransferButton>(button)
    {
        state.recipe = resolved.recipe;
    }
    if let Some(title) = region::<RecipeTitle>(world, root)
        && let Some(mut text) = world.get_mut::<Text>(title)
        && text.0 != resolved.title
    {
        text.0.clone_from(&resolved.title);
    }
    if let Some(uses) = region::<UsesPanel>(world, root) {
        clear_children(world, uses);
        // The moodboard's "Used in:" line, always present so an item that
        // feeds nothing says so rather than showing an empty row.
        world.spawn((
            Node::default(),
            Text::new(if resolved.uses.is_empty() {
                "Used in: nothing yet".to_owned()
            } else {
                "Used in:".to_owned()
            }),
            Themed(roles::HINT),
            Pickable::IGNORE,
            ChildOf(uses),
        ));
        for ingredient in &resolved.uses {
            spawn_uses_card(world, uses, ingredient);
        }
    }
}

fn spawn_tab(world: &mut World, row: Entity, id: CategoryId, active: bool) {
    let label = id.0.path().to_owned();
    let tab = world
        .spawn((
            Node {
                padding: UiRect::axes(px(10), px(4)),
                align_items: AlignItems::Center,
                ..default()
            },
            Themed(if active {
                roles::TAB_ACTIVE
            } else {
                roles::TAB
            }),
            SemanticRole::Tab,
            SemanticLabel(label.clone()),
            Tags::new().with("category", id.0.as_ref()),
            RecipeTab(id),
            Pickable::default(),
            ChildOf(row),
        ))
        .id();
    world.spawn((
        Node::default(),
        Text::new(label),
        Themed(if active {
            roles::CHIP_TEXT_ACTIVE
        } else {
            roles::CHIP_TEXT
        }),
        Pickable::IGNORE,
        ChildOf(tab),
    ));
    world.entity_mut(tab).observe(on_tab_or_button_click);
}

fn draw_layout(world: &mut World, area: Entity, layout: &RecipeLayout) {
    // Slots are placed absolutely inside the area, so the area has to be told
    // how tall the category's layout is or it collapses to nothing.
    let extent = layout
        .slots
        .iter()
        .map(|slot| slot.pos + Vec2::splat(SLOT_SIZE))
        .chain(layout.arrow.map(|r| r.max))
        .fold(Vec2::new(SLOT_SIZE, SLOT_SIZE), Vec2::max);
    if let Some(mut node) = world.get_mut::<Node>(area) {
        node.width = px(extent.x);
        node.height = px(extent.y);
    }
    if let Some(arrow) = layout.arrow {
        // The category hands over the whole region between the inputs and the
        // output; the moodboard draws a slim horizontal track centred in it.
        const TRACK: f32 = 10.0;
        let node = world
            .spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: px(arrow.min.x + 4.0),
                    top: px(arrow.min.y + (arrow.height() - TRACK) * 0.5),
                    width: px((arrow.width() - 8.0).max(1.0)),
                    height: px(TRACK),
                    overflow: Overflow::clip(),
                    ..default()
                },
                Themed(roles::ARROW),
                Pickable::IGNORE,
                ChildOf(area),
            ))
            .id();
        world.spawn((
            Node {
                width: percent(100),
                height: percent(100),
                ..default()
            },
            UiTransform::default(),
            Themed(roles::ARROW_PROGRESS),
            ArrowProgress,
            Pickable::IGNORE,
            ChildOf(node),
        ));
    }
    let types = world.resource::<IngredientTypes>().clone();
    let registries = world.get_resource::<Registries>().cloned();
    #[allow(clippy::cast_possible_truncation)]
    for (index, slot) in layout.slots.iter().enumerate() {
        let ix = RecipeSlotIx(index as u16);
        let first = slot.alternatives.first().cloned();
        let stack = first
            .as_ref()
            .and_then(|ing| ingredient_stack(&types, ing, slot.count.max(1)));
        let key = first
            .as_ref()
            .map(|ing| ingredient_key(registries.as_ref(), ing))
            .unwrap_or_default();
        let mut tags = Tags::new().with("role", slot.role.as_str());
        tags.0.insert("ix".to_owned(), index.to_string());
        if !key.is_empty() {
            tags.0.insert("entry".to_owned(), key.clone());
        }
        let node = world
            .spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: px(slot.pos.x),
                    top: px(slot.pos.y),
                    width: px(SLOT_SIZE),
                    height: px(SLOT_SIZE),
                    ..default()
                },
                Themed(roles::SLOT),
                SemanticRole::RecipeSlot,
                SemanticLabel(key),
                tags,
                ItemView::new(stack),
                RecipeSlotNode { ix, alternative: 0 },
                SlotAlternatives(slot.alternatives.clone()),
                Hovered::default(),
                Pickable::default(),
                ChildOf(area),
            ))
            .id();
        if let Some(ingredient) = first {
            world.entity_mut(node).insert(ShowsIngredient(ingredient));
        }
        slotted_ui::item::spawn_item_view_children(world, node);
    }
}

fn spawn_uses_card(world: &mut World, parent: Entity, ingredient: &Ingredient) {
    let types = world.resource::<IngredientTypes>().clone();
    let registries = world.get_resource::<Registries>().cloned();
    let key = ingredient_key(registries.as_ref(), ingredient);
    let stack = ingredient_stack(&types, ingredient, 1);
    let card = world
        .spawn((
            Node {
                width: px(SLOT_SIZE * 0.6),
                height: px(SLOT_SIZE * 0.6),
                ..default()
            },
            Themed(roles::CARD),
            SemanticRole::Card,
            SemanticLabel(key.clone()),
            Tags::new().with("entry", &key),
            ItemView::new(stack),
            ShowsIngredient(ingredient.clone()),
            Hovered::default(),
            Pickable::default(),
            ChildOf(parent),
        ))
        .id();
    slotted_ui::item::spawn_item_view_children(world, card);
}

/// `BrowserSet::Render`: advance every recipe slot to its next alternative
/// once per `durations.slow`, paused while Shift is held.
#[allow(clippy::too_many_arguments)]
pub fn cycle_alternatives(
    time: Res<Time<Virtual>>,
    keys: Res<ButtonInput<KeyCode>>,
    theme: Option<Res<ActiveTheme>>,
    themes: Option<Res<Assets<Theme>>>,
    types: Res<IngredientTypes>,
    registries: Option<Res<Registries>>,
    mut slots: Query<(
        &mut RecipeSlotNode,
        &SlotAlternatives,
        &mut ItemView,
        &mut Tags,
        &mut ShowsIngredient,
    )>,
    mut elapsed: Local<f32>,
) {
    if keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight) {
        return;
    }
    let period = theme
        .zip(themes)
        .and_then(|(active, assets)| assets.get(&active.0).map(|t| t.tokens.durations.slow))
        .unwrap_or(320);
    *elapsed += time.delta_secs();
    #[allow(clippy::cast_precision_loss)]
    let period_secs = period as f32 / 1000.0;
    if *elapsed < period_secs {
        return;
    }
    *elapsed = 0.0;
    for (mut node, alternatives, mut view, mut tags, mut shows) in &mut slots {
        if alternatives.0.len() < 2 {
            continue;
        }
        node.alternative = (node.alternative + 1) % alternatives.0.len();
        let ingredient = &alternatives.0[node.alternative];
        let stack = ingredient_stack(&types, ingredient, 1);
        if view.stack != stack {
            view.stack = stack;
        }
        let key = ingredient_key(registries.as_deref(), ingredient);
        if tags.get("entry") != Some(key.as_str()) {
            tags.0.insert("entry".to_owned(), key);
        }
        if shows.0 != *ingredient {
            shows.0 = ingredient.clone();
        }
    }
}

/// `BrowserSet::Render`: the browse regions and the recipe view take turns.
///
/// The moodboard's browser swaps the card grid for the recipe page rather than
/// stacking them, which is the only way both fit a 352 px column.
pub fn toggle_browse_regions(
    runtime: Res<BrowserRuntime>,
    mut grids: Query<&mut Node, With<super::card_grid::CardGrid>>,
    mut chips: Query<
        &mut Node,
        (
            With<super::chip_row::ChipRowMarker>,
            Without<super::card_grid::CardGrid>,
        ),
    >,
) {
    let open = runtime.open.is_some();
    // `Display::None` rather than `Visibility::Hidden`: a hidden node still
    // takes its space in the flex column, and the panel is only as tall as the
    // window, so the two halves have to leave the layout to make room.
    for mut node in &mut grids {
        set_display(&mut node, if open { Display::None } else { Display::Grid });
    }
    for mut node in &mut chips {
        set_display(&mut node, if open { Display::None } else { Display::Flex });
    }
}

/// Writes a node's `display` only when it changes, so a settled frame stays
/// settled.
fn set_display(node: &mut Mut<'_, Node>, want: Display) {
    if node.display != want {
        node.display = want;
    }
}

/// `BrowserSet::Render`: the arrow's progress sweeps left to right once per
/// `durations.slow`.
///
/// The sweep is a `UiTransform` scale rather than a width, and it is written
/// directly rather than through a `Tween`: a repeating `Tween` would hold
/// `ActiveMotions` above zero forever and `UiHarness::settle` would never
/// return, and a changing width would move the layout every frame.
pub fn animate_arrow(
    time: Res<Time<Virtual>>,
    motion: Option<Res<Motion>>,
    theme: Option<Res<ActiveTheme>>,
    themes: Option<Res<Assets<Theme>>>,
    mut arrows: Query<&mut UiTransform, With<ArrowProgress>>,
) {
    if arrows.is_empty() {
        return;
    }
    let reduced = motion.is_some_and(|m| m.reduced);
    let period = theme
        .zip(themes)
        .and_then(|(active, assets)| assets.get(&active.0).map(|t| t.tokens.durations.slow))
        .unwrap_or(320);
    #[allow(clippy::cast_precision_loss)]
    let period_secs = (period as f32 / 1000.0).max(0.001);
    let progress = if reduced {
        1.0
    } else {
        (time.elapsed_secs() % period_secs) / period_secs
    };
    for mut transform in &mut arrows {
        let scale = Vec2::new(progress.max(0.01), 1.0);
        // Scale only: `UiHarness::settle` watches translation and rects, so a
        // sweep that moved the node would keep every test spinning. The bar
        // therefore grows about its centre rather than from the left edge.
        if transform.scale != scale {
            transform.scale = scale;
        }
    }
}

/// `BrowserSet::Render`: the `+` button's state, from the handler's dry run.
///
/// Exclusive because a dry run needs the open menu's definition, its
/// inventories and the registry lookup all at once.
pub fn update_transfer_button(world: &mut World) {
    let buttons: Vec<Entity> = {
        let mut query = world.query_filtered::<Entity, With<TransferButton>>();
        query.iter(world).collect()
    };
    for button in buttons {
        let enabled = transfer_is_possible(world, button);
        let disabled = world.get::<InteractionDisabled>(button).is_some();
        if enabled == disabled {
            let mut entity = world.entity_mut(button);
            if enabled {
                entity.remove::<InteractionDisabled>();
            } else {
                entity.insert(InteractionDisabled);
            }
            let role = if enabled {
                roles::PILL
            } else {
                roles::BUTTON_DISABLED
            };
            if let Some(mut themed) = world.get_mut::<Themed>(button) {
                themed.0 = role;
            }
        }
    }
}

fn transfer_is_possible(world: &World, button: Entity) -> bool {
    let Some(recipe) = world.get::<TransferButton>(button).and_then(|b| b.recipe) else {
        return false;
    };
    let Some(page) = world.resource::<BrowserRuntime>().open.clone() else {
        return false;
    };
    let Some((panel, screen)) = panel_and_screen(world, button) else {
        return false;
    };
    let _ = panel;
    let Some(kind) = world.get::<ScreenRoot>(screen).map(|r| r.kind.clone()) else {
        return false;
    };
    let Some(handler) = world
        .resource::<TransferHandlers>()
        .get(&kind, &page.category)
        .cloned()
    else {
        return false;
    };
    let Some(menu_entity) = world.get::<ScreenRoot>(screen).and_then(|r| r.menu) else {
        return false;
    };
    let Some(menu) = world.get::<OpenMenu>(menu_entity) else {
        return false;
    };
    let Some(registries) = world.get_resource::<Registries>() else {
        return false;
    };
    let types = world.resource::<IngredientTypes>();
    let subtypes = world.resource::<Subtypes>();
    let store = world.resource::<RecipeStore>();
    let categories = world.resource::<Categories>();
    let loc = world.resource::<slotted_ui::Localization>();
    let Some(layout) = store.layout(recipe, Some(&page.focus), registries, categories, types)
    else {
        return false;
    };
    let mut inventories = Inventories::new();
    for entity in &menu.inventories {
        match world.get::<slotted_ecs::menu::Inventory>(*entity) {
            Some(inventory) => {
                inventories.push(inventory.0.clone());
            }
            None => return false,
        }
    }
    let lookup = registries.lookup();
    let ctx = crate::transfer::TransferCtx {
        def: &menu.def,
        inventories: &inventories,
        state: &menu.state,
        actor: menu.actor,
        lookup: &lookup,
        types,
        ctx: IngredientCtx {
            registries,
            subtypes,
            loc,
        },
    };
    handler.dry_run(&ctx, &layout).is_ok()
}

fn panel_and_screen(world: &World, entity: Entity) -> Option<(Entity, Entity)> {
    let mut current = entity;
    loop {
        if let Some(panel) = world.get::<BrowserPanel>(current) {
            return Some((current, panel.screen));
        }
        current = world.get::<ChildOf>(current)?.parent();
    }
}

/// `BrowserSet::Render`: paint the slots a refused transfer named.
pub fn paint_missing_slots(
    mut failed: MessageReader<TransferFailed>,
    mut slots: Query<(&RecipeSlotNode, &mut Themed)>,
) {
    for event in failed.read() {
        let missing = &event.error.missing;
        for (node, mut themed) in &mut slots {
            let want = if missing.contains(&node.ix) {
                roles::SLOT_MISSING
            } else {
                roles::SLOT
            };
            if themed.0 != want {
                themed.0 = want;
            }
        }
    }
}

/// Observer on `+`, back and forward: `Activate` from a click or the
/// keyboard.
fn on_view_button(
    activate: On<Activate>,
    transfer: Query<&TransferButton>,
    history: Query<&HistoryButton>,
    keys: Res<ButtonInput<KeyCode>>,
    mut requests: MessageWriter<TransferRequested>,
    mut nav: MessageWriter<RecipeNav>,
) {
    let entity = activate.entity;
    if let Ok(button) = transfer.get(entity) {
        if let Some(recipe) = button.recipe {
            let max = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
            requests.write(TransferRequested { recipe, max });
        }
        return;
    }
    if let Ok(button) = history.get(entity) {
        nav.write(if button.forward {
            RecipeNav::Forward
        } else {
            RecipeNav::Back
        });
    }
}

/// Observer on a tab: switch category.
fn on_tab_or_button_click(
    click: On<bevy::picking::events::Pointer<bevy::picking::events::Click>>,
    tabs: Query<&RecipeTab>,
    mut nav: MessageWriter<RecipeNav>,
) {
    if let Ok(tab) = tabs.get(click.entity) {
        nav.write(RecipeNav::Category(tab.0.clone()));
    }
}
