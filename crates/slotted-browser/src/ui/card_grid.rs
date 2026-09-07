//! `slotted:card_grid`: the virtualised entry grid. Package B.
//!
//! The grid is a [`slotted_ui::VirtualGridSource`] in
//! [pooled](slotted_ui::VirtualGridSource::pooled) mode: `slotted-ui` keeps
//! `cols * rows` card entities alive with stable entity ids and hands each one
//! to [`CardSource::rebind`], which paints it from
//! `BrowserRuntime::visible[first_row * cols ..]`. Cards past the end of the
//! result set are hidden rather than despawned, so screen-tree snapshots list
//! the whole pool. A browser page is `rows` rows of that window, so turning a
//! page is a window move.

use bevy::input_focus::tab_navigation::TabIndex;
use bevy::picking::events::{Click, Pointer, Scroll};
use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use slotted_ecs::OpenMenu;
use slotted_model::{GiveTarget, Namespaced};
use slotted_registry::Value;
use slotted_theme::Themed;
use slotted_ui::{
    DataSourceId, ItemView, LocKey, ScreenRoot, SemanticLabel, SpawnCtx, TextRole, TooltipRequest,
    TooltipTier, UiNodeDef, VirtualGridSource, VirtualGridState, Widget,
};
use slotted_ui::{SemanticRole, Tags};

use super::dock::{BrowserLayout, CARD_GAP, DockMetrics};
use super::panel::{BrowserPanel, panel_ancestor};
use super::{ShowsEntry, ShowsIngredient, ingredient_key, ingredient_stack, roles};
use crate::events::{GiveRequested, OpenRecipes, OpenUses};
use crate::index::{EntryId, IndexState};
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
        let metrics = DockMetrics::from(&ctx.tokens());
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
                grid_template_columns: RepeatedGridTrack::px(1, metrics.card_width),
                grid_auto_rows: vec![GridTrack::px(metrics.card_height)],
                ..default()
            },
            SemanticRole::Grid,
            CardGrid::default(),
            // The shared pooling machinery: `size_card_pool` gives it the
            // docked shape and the open page, `refresh_virtual_grids` keeps
            // that many cards alive and rebinds them through `CardSource`.
            VirtualGridState {
                source: card_source_id(),
                cols: 1,
                rows: 0,
                first_row: 0,
                total: 0,
                version: 0,
            },
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
    let metrics = DockMetrics::from(&slotted_ui::active_tokens(world));
    for (grid, cols, rows) in shapes {
        resize_pool(world, grid, cols, rows, metrics);
    }
}

/// Hands the docked shape and the open page to the grid's
/// [`VirtualGridState`]. The cards themselves are spawned, kept and rebound by
/// `slotted-ui`, on the next `refresh_virtual_grids`.
fn resize_pool(world: &mut World, grid: Entity, cols: u16, rows: u16, metrics: DockMetrics) {
    set_columns(world, grid, cols, metrics);
    let want = usize::from(cols) * usize::from(rows);
    if let Some(mut card_grid) = world.get_mut::<CardGrid>(grid)
        && card_grid.per_page != want
    {
        card_grid.per_page = want;
        card_grid.page = 0;
    }
    let page = world.get::<CardGrid>(grid).map_or(0, |g| g.page);
    let Some(mut state) = world.get_mut::<VirtualGridState>(grid) else {
        return;
    };
    if state.cols != cols {
        state.cols = cols;
    }
    if state.rows != rows {
        state.rows = rows;
    }
    // One page is `rows` rows of the window, so the open page *is* the
    // window's first row.
    let first_row = page * usize::from(rows.max(1));
    if state.first_row != first_row {
        state.first_row = first_row;
    }
}

fn set_columns(world: &mut World, grid: Entity, cols: u16, metrics: DockMetrics) {
    let want = RepeatedGridTrack::px(u16::max(cols, 1), metrics.card_width);
    if let Some(mut node) = world.get_mut::<Node>(grid)
        && node.grid_template_columns != want
    {
        node.grid_template_columns = want;
    }
}

fn spawn_card(world: &mut World, grid: Entity, index: u16, metrics: DockMetrics) -> Entity {
    let card = world
        .spawn((
            Node {
                width: px(metrics.card_width),
                height: px(metrics.card_height),
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
            slotted_ui::Focusable,
            Visibility::Hidden,
            ChildOf(grid),
        ))
        .id();
    slotted_ui::item::spawn_item_view_children(world, card);
    reshape_item_view(world, card, metrics);
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
    card
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
fn reshape_item_view(world: &mut World, card: Entity, metrics: DockMetrics) {
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
            node.left = px((metrics.card_width - ICON_BOX) / 2.0);
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
/// three markers, so the rebind reads one component instead of three
/// mutually-exclusive ones.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub enum CardLabel {
    /// The display name.
    Name,
    /// The `@mod` namespace badge.
    Badge,
    /// The stand-in glyph an iconless entry draws.
    Glyph,
}

/// The id the card grid's [`CardSource`] is registered under.
pub fn card_source_id() -> DataSourceId {
    DataSourceId(Namespaced::parse("slotted:browser_cards").expect("well formed"))
}

/// The card pool as a pooled [`VirtualGridSource`]: `slotted-ui` owns the
/// windowing and the pool, this owns what one card shows.
///
/// The list is `BrowserRuntime::visible`, which lives in the world rather than
/// in the source, so the length comes from
/// [`len_in`](VirtualGridSource::len_in); `len` is the empty answer for a
/// world-less caller.
#[derive(Debug, Default, Clone, Copy)]
pub struct CardSource;

impl VirtualGridSource for CardSource {
    fn len(&self) -> usize {
        0
    }

    fn len_in(&self, world: &World) -> usize {
        world
            .get_resource::<BrowserRuntime>()
            .map_or(0, |runtime| runtime.visible.len())
    }

    fn version(&self) -> u64 {
        0
    }

    /// Never called: [`spawn_cell`](VirtualGridSource::spawn_cell) spawns
    /// every card, because a card is a hand-built node with observers rather
    /// than a tree the widget path can make.
    fn cell(&self, _index: usize) -> UiNodeDef {
        UiNodeDef::Text {
            opts: slotted_ui::TextOpts::default(),
            key: LocKey(String::new()),
            style: TextRole::Body,
            tags: Tags::new(),
        }
    }

    fn pooled(&self) -> bool {
        true
    }

    fn spawn_cell(&self, world: &mut World, grid: Entity, slot: usize) -> Option<Entity> {
        let metrics = DockMetrics::from(&slotted_ui::active_tokens(world));
        let slot = u16::try_from(slot).unwrap_or(u16::MAX);
        Some(spawn_card(world, grid, slot, metrics))
    }

    fn rebind(&self, world: &mut World, cell: Entity, index: Option<usize>) -> bool {
        rebind_card(world, cell, index);
        true
    }
}

/// What one card shows, lifted out of the world's resources so the card can be
/// written without holding a borrow on them.
struct CardBinding {
    ingredient: crate::ingredient::Ingredient,
    display: String,
    mod_ns: String,
    rarity: slotted_registry::Rarity,
    stack: Option<slotted_model::ItemStack>,
    key: String,
}

/// Paints one card from the entry at `index`, or empties it when the pool runs
/// past the end of the result set.
///
/// A bound card carries four things the moodboard asks for: the icon, the
/// display name, the `@mod` badge and a rarity strip. Nothing is despawned; a
/// card past the end of the page keeps its cell and goes
/// [`Visibility::Hidden`], so the panel does not reflow as the player types.
fn rebind_card(world: &mut World, card: Entity, index: Option<usize>) {
    // Nothing is bound while the index is still building: an unbound card
    // stays unbound rather than claiming an entry that does not exist yet.
    if world
        .get_resource::<IndexState>()
        .and_then(IndexState::ready)
        .is_none()
    {
        return;
    }
    let wanted = index.and_then(|index| {
        world
            .get_resource::<BrowserRuntime>()
            .and_then(|runtime| runtime.visible.get(index).copied())
    });
    let bound = wanted.and_then(|id| read_entry(world, id));

    if let Some(mut shows) = world.get_mut::<ShowsEntry>(card)
        && shows.0 != wanted
    {
        shows.0 = wanted;
    }

    let children: Vec<Entity> = world
        .get::<Children>(card)
        .map(|c| c.iter().collect())
        .unwrap_or_default();

    let Some(bound) = bound else {
        if let Some(mut view) = world.get_mut::<ItemView>(card)
            && view.stack.is_some()
        {
            view.stack = None;
        }
        if let Some(mut tags) = world.get_mut::<Tags>(card)
            && tags.get("entry").is_some()
        {
            tags.0.remove("entry");
        }
        if let Some(mut label) = world.get_mut::<SemanticLabel>(card)
            && !label.0.is_empty()
        {
            label.0.clear();
        }
        world.entity_mut(card).remove::<ShowsIngredient>();
        set_visibility(world, card, Visibility::Hidden);
        write_labels(world, &children, "", "", "");
        write_strip(world, &children, None);
        return;
    };

    let mut has_stack = false;
    if let Some(mut view) = world.get_mut::<ItemView>(card) {
        if view.stack != bound.stack {
            view.stack = bound.stack;
        }
        has_stack = view.stack.is_some();
    }
    if let Some(mut tags) = world.get_mut::<Tags>(card)
        && tags.get("entry") != Some(bound.key.as_str())
    {
        tags.0.insert("entry".to_owned(), bound.key);
    }
    if let Some(mut label) = world.get_mut::<SemanticLabel>(card)
        && label.0 != bound.display
    {
        label.0.clone_from(&bound.display);
    }
    world
        .entity_mut(card)
        .insert(ShowsIngredient(bound.ingredient));
    set_visibility(world, card, Visibility::Inherited);
    let glyph = if has_stack {
        ""
    } else {
        glyph_for(&bound.display)
    };
    write_labels(world, &children, &bound.display, &bound.mod_ns, glyph);
    write_strip(world, &children, Some(bound.rarity));
}

/// The built index's entry for `id`, copied out of the resources.
fn read_entry(world: &World, id: EntryId) -> Option<CardBinding> {
    let index = world.get_resource::<IndexState>()?.ready()?;
    let entry = index.get(id)?;
    let types = world.get_resource::<IngredientTypes>()?;
    Some(CardBinding {
        stack: ingredient_stack(types, &entry.ingredient, 1),
        key: ingredient_key(
            world.get_resource::<slotted_ecs::Registries>(),
            &entry.ingredient,
        ),
        ingredient: entry.ingredient.clone(),
        display: entry.display.clone(),
        mod_ns: entry.mod_ns.clone(),
        rarity: entry.rarity,
    })
}

fn set_visibility(world: &mut World, entity: Entity, want: Visibility) {
    if let Some(mut visibility) = world.get_mut::<Visibility>(entity)
        && *visibility != want
    {
        *visibility = want;
    }
}

/// The character an iconless entry shows: a tag keeps its `#`, anything else
/// takes a question mark. The tag's path still reads out in the name below it.
fn glyph_for(display: &str) -> &str {
    if display.starts_with('#') { "#" } else { "?" }
}

/// Writes a card's three text children, hiding the empty ones.
fn write_labels(world: &mut World, children: &[Entity], name: &str, namespace: &str, glyph: &str) {
    for child in children {
        let Some(kind) = world.get::<CardLabel>(*child).copied() else {
            continue;
        };
        let want = match kind {
            CardLabel::Name => name,
            CardLabel::Badge => namespace,
            CardLabel::Glyph => glyph,
        };
        if let Some(mut text) = world.get_mut::<Text>(*child)
            && text.0 != want
        {
            want.clone_into(&mut text.0);
        }
        let visible = if want.is_empty() {
            Visibility::Hidden
        } else {
            Visibility::Inherited
        };
        set_visibility(world, *child, visible);
    }
}

/// Paints a card's rarity strip, hidden for `Common` and for an empty card.
fn write_strip(world: &mut World, children: &[Entity], rarity: Option<slotted_registry::Rarity>) {
    let role = rarity
        .filter(|r| *r != slotted_registry::Rarity::Common)
        .map(super::rarity_strip_role);
    for child in children {
        if world.get::<RarityStrip>(*child).is_none() {
            continue;
        }
        match &role {
            Some(role) => {
                if let Some(mut themed) = world.get_mut::<Themed>(*child)
                    && themed.0 != *role
                {
                    themed.0.clone_from(role);
                }
                set_visibility(world, *child, Visibility::Inherited);
            }
            None => set_visibility(world, *child, Visibility::Hidden),
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
