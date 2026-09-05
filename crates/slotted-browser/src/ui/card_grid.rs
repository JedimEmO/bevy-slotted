//! `slotted:card_grid`: the virtualised entry grid. Package B.
//!
//! A pool of `cols * rows` card entities is rebound to
//! `BrowserRuntime::visible[page * per_page ..]` whenever the runtime, the
//! page or the dock changes; cards past the end are hidden rather than
//! despawned, so entities stay stable and screen-tree snapshots do too. The
//! pool is resized only when the dock gives the panel a different grid.

use bevy::input_focus::tab_navigation::TabIndex;
use bevy::picking::events::{Click, Pointer, Scroll};
use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use slotted_ecs::OpenMenu;
use slotted_model::GiveTarget;
use slotted_registry::Value;
use slotted_theme::Themed;
use slotted_ui::{
    ItemView, ScreenRoot, SemanticLabel, SpawnCtx, TooltipRequest, TooltipTier, UiNodeDef, Widget,
};
use slotted_ui::{SemanticRole, Tags};

use super::dock::{BrowserLayout, CARD_GAP, CARD_HEIGHT, CARD_WIDTH};
use super::panel::{BrowserPanel, panel_ancestor};
use super::{ShowsEntry, ShowsIngredient, ingredient_key, ingredient_stack, roles};
use crate::events::{GiveRequested, OpenRecipes, OpenUses};
use crate::index::IndexState;
use crate::ingredient::IngredientTypes;
use crate::runtime::BrowserRuntime;

/// Most columns a pool ever gets, however wide the strip is.
pub const MAX_COLS: u16 = 8;
/// Most rows a pool ever gets, however tall the strip is.
pub const MAX_ROWS: u16 = 6;

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
        let grid = ctx.spawn_node((
            Node {
                display: Display::Grid,
                width: percent(100),
                // The grid takes whatever height the panel has left over and
                // scrolls inside it, so a tall result set can never push the
                // bookmark strip and the footer off the bottom of the window.
                flex_grow: 0.0,
                flex_shrink: 1.0,
                min_height: px(0),
                overflow: Overflow::scroll_y(),
                // Fixed rows and a start-aligned content box: without both, a
                // grid inside a column panel stretches its rows to fill the
                // strip and the cards drift apart.
                align_content: AlignContent::Start,
                justify_content: JustifyContent::Start,
                row_gap: px(CARD_GAP),
                column_gap: px(CARD_GAP),
                grid_template_columns: RepeatedGridTrack::px(1, CARD_WIDTH),
                grid_auto_rows: vec![GridTrack::px(CARD_HEIGHT)],
                ..default()
            },
            SemanticRole::Grid,
            CardGrid::default(),
        ));
        ctx.world
            .entity_mut(grid)
            .observe(on_card_grid_scroll)
            .insert(Pickable::default());
        grid
    }
}

/// `BrowserSet::Render`: resize the card pool to the docked grid.
pub fn size_card_pool(world: &mut World) {
    let shapes: Vec<(Entity, u16, u16)> = {
        let mut query = world.query::<(Entity, &CardGrid)>();
        let grids: Vec<Entity> = query.iter(world).map(|(e, _)| e).collect();
        grids
            .into_iter()
            .filter_map(|grid| {
                let panel = panel_ancestor(world, grid)?;
                let layout = world.get::<BrowserLayout>(panel)?;
                Some((
                    grid,
                    layout.cols.clamp(1, MAX_COLS),
                    layout.rows.clamp(1, MAX_ROWS),
                ))
            })
            .collect()
    };
    for (grid, cols, rows) in shapes {
        resize_pool(world, grid, cols, rows);
    }
}

fn resize_pool(world: &mut World, grid: Entity, cols: u16, rows: u16) {
    let want = usize::from(cols) * usize::from(rows);
    let kids: Vec<Entity> = world
        .get::<Children>(grid)
        .map(|children| children.iter().collect())
        .unwrap_or_default();
    let existing: Vec<(Entity, u16)> = kids
        .into_iter()
        .filter_map(|c| world.get::<Card>(c).map(|card| (c, card.0)))
        .collect();
    if existing.len() == want {
        set_columns(world, grid, cols);
        return;
    }
    for (entity, index) in &existing {
        if usize::from(*index) >= want {
            world.entity_mut(*entity).despawn();
        }
    }
    #[allow(clippy::cast_possible_truncation)]
    for index in existing.len()..want {
        spawn_card(world, grid, index as u16);
    }
    set_columns(world, grid, cols);
    if let Some(mut card_grid) = world.get_mut::<CardGrid>(grid)
        && card_grid.per_page != want
    {
        card_grid.per_page = want;
        card_grid.page = 0;
    }
}

fn set_columns(world: &mut World, grid: Entity, cols: u16) {
    let want = RepeatedGridTrack::px(u16::max(cols, 1), CARD_WIDTH);
    if let Some(mut node) = world.get_mut::<Node>(grid)
        && node.grid_template_columns != want
    {
        node.grid_template_columns = want;
    }
}

fn spawn_card(world: &mut World, grid: Entity, index: u16) {
    let card = world
        .spawn((
            Node {
                width: px(CARD_WIDTH),
                height: px(CARD_HEIGHT),
                // A long display name ("#minecraft:planks") must stay inside
                // its own tile rather than run over the card beside it.
                overflow: Overflow::clip(),
                ..default()
            },
            Themed(roles::CARD),
            SemanticRole::Card,
            SemanticLabel::default(),
            Tags::default(),
            ItemView::new(None),
            ShowsEntry::default(),
            Card(index),
            Hovered::default(),
            Pickable::default(),
            TabIndex(0),
            Visibility::Hidden,
            ChildOf(grid),
        ))
        .id();
    slotted_ui::item::spawn_item_view_children(world, card);
    reshape_item_view(world, card);
    // The rarity strip: the moodboard's `.rb`, a 2 px bar tucked under the
    // card's top edge rather than the ring a slot draws.
    world.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: px(8),
            right: px(8),
            top: px(0),
            height: px(2),
            ..default()
        },
        Themed(super::roles::RARITY),
        RarityStrip,
        Pickable::IGNORE,
        Visibility::Hidden,
        ChildOf(card),
    ));
    world.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: px(3),
            right: px(3),
            top: px(ICON_BOX + 10.0),
            height: px(26),
            ..default()
        },
        Text::new(String::new()),
        TextLayout::new(Justify::Center, LineBreak::WordBoundary),
        Themed(roles::CARD_NAME),
        CardLabel::Name,
        Pickable::IGNORE,
        Visibility::Hidden,
        ChildOf(card),
    ));
    world.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: px(3),
            right: px(3),
            bottom: px(5),
            ..default()
        },
        Text::new(String::new()),
        TextLayout::new(Justify::Center, LineBreak::WordBoundary),
        Themed(roles::BADGE),
        CardLabel::Badge,
        Pickable::IGNORE,
        Visibility::Hidden,
        ChildOf(card),
    ));
    world.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: px(0),
            width: percent(100),
            // Centred in the icon box by hand: the text sits at its own node's
            // top-left, so the node is placed where a 22 px line wants to be.
            top: px(8.0 + (ICON_BOX - 22.0) * 0.5),
            ..default()
        },
        Text::new(String::new()),
        TextLayout::new(Justify::Center, LineBreak::WordBoundary),
        Themed(roles::GLYPH),
        CardLabel::Glyph,
        Pickable::IGNORE,
        Visibility::Hidden,
        ChildOf(card),
    ));
    world
        .entity_mut(card)
        .observe(on_card_click)
        .observe(super::ghost_drag::on_card_drag_start)
        .observe(super::ghost_drag::on_card_drag_end)
        .observe(on_card_hover);
}

/// The icon box on a card, px.
const ICON_BOX: f32 = 34.0;

/// Repositions the shared item-view children for a card.
///
/// `spawn_item_view_children` lays an icon out for a square slot: 80 % of the
/// node, anchored top-left. A card is taller than it is wide and keeps its
/// icon in a box above the name, so the icon child is moved and the rarity
/// ring, which would draw around the whole card, is dropped in favour of the
/// strip.
fn reshape_item_view(world: &mut World, card: Entity) {
    let children: Vec<Entity> = world
        .get::<Children>(card)
        .map(|c| c.iter().collect())
        .unwrap_or_default();
    for child in children {
        if world.get::<slotted_ui::item::ItemIcon>(child).is_some()
            && let Some(mut node) = world.get_mut::<Node>(child)
        {
            node.width = px(ICON_BOX);
            node.height = px(ICON_BOX);
            node.left = px((CARD_WIDTH - ICON_BOX) / 2.0);
            node.top = px(8);
        }
        if world.get::<slotted_ui::item::RarityRing>(child).is_some() {
            world.entity_mut(child).despawn();
        }
    }
}

/// The rarity strip across the top of a card.
#[derive(Component, Debug, Default, Clone, Copy)]
pub struct RarityStrip;

/// Which of a card's three text children a node is. One component rather than
/// three markers, so [`rebind_cards`] needs one query instead of three
/// mutually-`Without` ones.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub enum CardLabel {
    /// The display name.
    Name,
    /// The `@mod` namespace badge.
    Badge,
    /// The stand-in glyph an iconless entry draws.
    Glyph,
}

/// `BrowserSet::Render`: rebind the card pool to the visible page.
///
/// A bound card carries four things the moodboard asks for: the icon, the
/// display name, the `@mod` badge and a rarity strip. Nothing is despawned;
/// a card past the end of the page keeps its cell and goes
/// [`Visibility::Hidden`], so the panel does not reflow as the player types.
#[allow(clippy::too_many_arguments)]
pub fn rebind_cards(
    runtime: Res<BrowserRuntime>,
    index: Res<IndexState>,
    types: Res<IngredientTypes>,
    registries: Option<Res<slotted_ecs::Registries>>,
    grids: Query<(&CardGrid, &Children)>,
    mut cards: Query<(
        &Card,
        &mut ShowsEntry,
        &mut ItemView,
        &mut Visibility,
        &mut Tags,
        &mut SemanticLabel,
        &Children,
    )>,
    mut labels: Query<(&CardLabel, &mut Text, &mut Visibility), Without<Card>>,
    mut strips: Query<
        (&mut Themed, &mut Visibility),
        (With<RarityStrip>, Without<Card>, Without<CardLabel>),
    >,
    mut commands: Commands,
) {
    let Some(index) = index.ready() else {
        return;
    };
    for (grid, children) in &grids {
        let start = grid.page * grid.per_page.max(1);
        for child in children.iter() {
            let Ok((card, mut shows, mut view, mut visibility, mut tags, mut label, card_children)) =
                cards.get_mut(child)
            else {
                continue;
            };
            let wanted = runtime.visible.get(start + usize::from(card.0)).copied();
            if shows.0 != wanted {
                shows.0 = wanted;
            }
            if let Some(entry) = wanted.and_then(|id| index.get(id)) {
                let stack = ingredient_stack(&types, &entry.ingredient, 1);
                if view.stack != stack {
                    view.stack = stack;
                }
                let key = ingredient_key(registries.as_deref(), &entry.ingredient);
                if tags.get("entry") != Some(key.as_str()) {
                    tags.0.insert("entry".to_owned(), key);
                }
                if label.0 != entry.display {
                    label.0.clone_from(&entry.display);
                }
                commands
                    .entity(child)
                    .insert(ShowsIngredient(entry.ingredient.clone()));
                if *visibility != Visibility::Inherited {
                    *visibility = Visibility::Inherited;
                }
                let glyph = if view.stack.is_some() {
                    ""
                } else {
                    glyph_for(&entry.display)
                };
                write_labels(
                    &mut labels,
                    card_children,
                    &entry.display,
                    &entry.mod_ns,
                    glyph,
                );
                write_strip(&mut strips, card_children, Some(entry.rarity));
            } else {
                if view.stack.is_some() {
                    view.stack = None;
                }
                if tags.get("entry").is_some() {
                    tags.0.remove("entry");
                }
                if !label.0.is_empty() {
                    label.0.clear();
                }
                commands.entity(child).try_remove::<ShowsIngredient>();
                if *visibility != Visibility::Hidden {
                    *visibility = Visibility::Hidden;
                }
                write_labels(&mut labels, card_children, "", "", "");
                write_strip(&mut strips, card_children, None);
            }
        }
    }
}

/// The character an iconless entry shows: a tag keeps its `#`, anything else
/// takes a question mark. The tag's path still reads out in the name below it.
fn glyph_for(display: &str) -> &str {
    if display.starts_with('#') { "#" } else { "?" }
}

/// Writes a card's three text children, hiding the empty ones.
fn write_labels(
    labels: &mut Query<(&CardLabel, &mut Text, &mut Visibility), Without<Card>>,
    children: &Children,
    name: &str,
    namespace: &str,
    glyph: &str,
) {
    for child in children.iter() {
        let Ok((kind, mut text, mut visibility)) = labels.get_mut(child) else {
            continue;
        };
        let want = match kind {
            CardLabel::Name => name,
            CardLabel::Badge => namespace,
            CardLabel::Glyph => glyph,
        };
        if text.0 != want {
            want.clone_into(&mut text.0);
        }
        let visible = if want.is_empty() {
            Visibility::Hidden
        } else {
            Visibility::Inherited
        };
        if *visibility != visible {
            *visibility = visible;
        }
    }
}

/// Paints a card's rarity strip, hidden for `Common` and for an empty card.
fn write_strip(
    strips: &mut Query<
        (&mut Themed, &mut Visibility),
        (With<RarityStrip>, Without<Card>, Without<CardLabel>),
    >,
    children: &Children,
    rarity: Option<slotted_registry::Rarity>,
) {
    let role = rarity
        .filter(|r| *r != slotted_registry::Rarity::Common)
        .map(super::rarity_strip_role);
    for child in children.iter() {
        let Ok((mut themed, mut visibility)) = strips.get_mut(child) else {
            continue;
        };
        match &role {
            Some(role) => {
                if themed.0 != *role {
                    themed.0.clone_from(role);
                }
                if *visibility != Visibility::Inherited {
                    *visibility = Visibility::Inherited;
                }
            }
            None => {
                if *visibility != Visibility::Hidden {
                    *visibility = Visibility::Hidden;
                }
            }
        }
    }
}

/// `BrowserSet::Render`: a hovered card pops with `browser.card.hover`.
pub fn card_state_roles(mut cards: Query<(&Hovered, &mut Themed), With<Card>>) {
    for (hovered, mut themed) in &mut cards {
        let want = if hovered.get() {
            roles::CARD_HOVER
        } else {
            roles::CARD
        };
        if themed.0 != want {
            themed.0 = want;
        }
    }
}

/// `BrowserSet::Input`: `PageUp` and `PageDown` walk the grid, unless the search
/// field has the keyboard.
pub fn page_card_grid(
    keys: Res<ButtonInput<KeyCode>>,
    runtime: Res<BrowserRuntime>,
    mut grids: Query<&mut CardGrid>,
) {
    if runtime.has_keyboard_focus {
        return;
    }
    let delta = i32::from(keys.just_pressed(runtime.keys.next_page))
        - i32::from(keys.just_pressed(runtime.keys.prev_page));
    if delta != 0 {
        for mut grid in &mut grids {
            turn_page(&mut grid, delta, runtime.visible.len());
        }
    }
}

/// Moves `grid` by `delta` pages, clamped to what `total` entries fill.
pub fn turn_page(grid: &mut CardGrid, delta: i32, total: usize) {
    let per_page = grid.per_page.max(1);
    let last = i64::try_from(total.saturating_sub(1) / per_page).unwrap_or(i64::MAX);
    let now = i64::try_from(grid.page).unwrap_or(0);
    let next = usize::try_from((now + i64::from(delta)).clamp(0, last)).unwrap_or(0);
    if grid.page != next {
        grid.page = next;
    }
}

/// Observer on the grid: the wheel turns a page.
fn on_card_grid_scroll(
    scroll: On<Pointer<Scroll>>,
    runtime: Res<BrowserRuntime>,
    mut grids: Query<&mut CardGrid>,
) {
    let Ok(mut grid) = grids.get_mut(scroll.entity) else {
        return;
    };
    let delta = -scroll.event.y;
    if delta.abs() < f32::EPSILON {
        return;
    }
    let step = if delta > 0.0 { 1 } else { -1 };
    turn_page(&mut grid, step, runtime.visible.len());
}

/// Observer on a card: left opens recipes, right opens uses, Ctrl+left is a
/// cheat-give when the screen's actor may cheat.
#[allow(clippy::too_many_arguments)]
fn on_card_click(
    click: On<Pointer<Click>>,
    cards: Query<(&ShowsIngredient, &ChildOf)>,
    parents: Query<&ChildOf>,
    panels: Query<(&BrowserPanel, &ScreenRoot)>,
    menus: Query<&OpenMenu>,
    keys: Res<ButtonInput<KeyCode>>,
    mut recipes: MessageWriter<OpenRecipes>,
    mut uses: MessageWriter<OpenUses>,
    mut gives: MessageWriter<GiveRequested>,
) {
    let Ok((shows, _)) = cards.get(click.entity) else {
        return;
    };
    let ctrl = keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight);
    match click.event.button {
        bevy::picking::pointer::PointerButton::Secondary => {
            uses.write(OpenUses(shows.0.clone()));
        }
        bevy::picking::pointer::PointerButton::Primary if ctrl => {
            let can_cheat = panel_menu(click.entity, &parents, &panels)
                .and_then(|menu| menus.get(menu).ok())
                .is_some_and(|menu| menu.actor.can_cheat);
            if can_cheat {
                gives.write(GiveRequested {
                    ingredient: shows.0.clone(),
                    count: 1,
                    target: GiveTarget::Cursor,
                });
            } else {
                tracing::debug!("cheat-give refused: the actor may not cheat");
            }
        }
        bevy::picking::pointer::PointerButton::Primary => {
            recipes.write(OpenRecipes(shows.0.clone()));
        }
        bevy::picking::pointer::PointerButton::Middle => {}
    }
}

/// The menu the panel above `entity` drives.
pub fn panel_menu(
    entity: Entity,
    parents: &Query<&ChildOf>,
    panels: &Query<(&BrowserPanel, &ScreenRoot)>,
) -> Option<Entity> {
    let mut current = entity;
    loop {
        if let Ok((_, root)) = panels.get(current) {
            return root.menu;
        }
        current = parents.get(current).ok()?.parent();
    }
}

/// Observer on a card: ask the shared tooltip layer to draw it, so the
/// tooltip lands above the panel rather than inside it.
fn on_card_hover(over: On<Pointer<bevy::picking::events::Over>>, mut commands: Commands) {
    commands.trigger(TooltipRequest {
        entity: over.entity,
        tier: TooltipTier::Compact,
    });
}
