//! `virtual_grid`: a windowed grid over a [`VirtualGridSource`]. Phase 6
//! contract section 1.5. Cells are respawned when the window moves, unless the
//! source [pools](VirtualGridSource::pooled) them — the browser's card grid
//! does — in which case the same cells are kept and rebound.

use std::collections::HashMap;
use std::sync::Arc;

use bevy::picking::events::{Pointer, Scroll};
use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use slotted_theme::{Role, Themed, roles};

use crate::def::{DataSourceId, Tags, UiNodeDef};
use crate::screen::SpawnCtx;
use crate::semantic::{ScreenRoot, SemanticRole, WidgetNode};

/// What a virtual grid shows. Cells are `UiNodeDef`s spawned through the
/// ordinary widget path, so a source can hand out slots, cards or anything.
///
/// A source that would rather keep its cells and rebind them — the browser's
/// card pool is one — overrides [`pooled`](Self::pooled),
/// [`spawn_cell`](Self::spawn_cell) and [`rebind`](Self::rebind). Everything
/// else keeps the respawn behaviour, which is the whole trait for a source
/// that only implements the four required methods.
pub trait VirtualGridSource: Send + Sync {
    /// Total number of cells.
    fn len(&self) -> usize;
    /// No cells at all.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
    /// Bump to force the visible window to rebuild.
    fn version(&self) -> u64;
    /// The cell at `index`.
    fn cell(&self, index: usize) -> UiNodeDef;

    /// [`len`](Self::len) for a source whose list lives in the world. The
    /// grid asks this one; the default hands the question back to `len`.
    fn len_in(&self, _world: &World) -> usize {
        self.len()
    }

    /// Whether the grid pools its cells.
    ///
    /// A pooled grid keeps `cols * rows` cells alive with stable entities,
    /// rebinds them through [`rebind`](Self::rebind) every frame and lets the
    /// source hide the ones past the end of the source; it never despawns a
    /// cell to move its window, and it does not clamp `first_row` for the
    /// source, since an over-scrolled window simply binds nothing. A
    /// non-pooled grid — the default — despawns and respawns the window.
    fn pooled(&self) -> bool {
        false
    }

    /// Spawns the pooled cell for pool position `slot`, as a child of `grid`.
    ///
    /// `None`, the default, spawns `cell(slot)` through the ordinary widget
    /// path instead. A source that wants a bespoke cell — observers, children
    /// laid out by hand — spawns it here and keeps the entity for the life of
    /// the pool.
    fn spawn_cell(&self, _world: &mut World, _grid: Entity, _slot: usize) -> Option<Entity> {
        None
    }

    /// Binds an existing `cell` to `index`, or to nothing when the pool runs
    /// past the end of the source. Returns whether the cell could be reused;
    /// `false` — the default — makes the grid despawn it and spawn a fresh
    /// one, which is what every non-pooled source does anyway.
    fn rebind(&self, _world: &mut World, _cell: Entity, _index: Option<usize>) -> bool {
        false
    }
}

/// Registered sources by id.
#[derive(Resource, Default, Clone)]
pub struct VirtualGridSources(pub HashMap<DataSourceId, Arc<dyn VirtualGridSource>>);

impl VirtualGridSources {
    /// Register or replace.
    pub fn register(&mut self, id: DataSourceId, source: impl VirtualGridSource + 'static) {
        self.0.insert(id, Arc::new(source));
    }
}

/// Width of the scrollbar column, in logical px.
pub const SCROLLBAR_WIDTH: f32 = 8.0;

/// Grid state on the root.
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub struct VirtualGridState {
    /// Source id.
    pub source: DataSourceId,
    /// Columns.
    pub cols: u16,
    /// Visible rows.
    pub rows: u16,
    /// First visible row.
    pub first_row: usize,
    /// `source.len()` at the last rebuild.
    pub total: usize,
    /// `source.version()` at the last rebuild.
    pub version: u64,
}

impl VirtualGridState {
    /// Rows the whole source needs.
    pub fn total_rows(&self) -> usize {
        let cols = usize::from(self.cols).max(1);
        self.total.div_ceil(cols)
    }

    /// The largest `first_row` that still fills the window.
    pub fn max_first_row(&self) -> usize {
        self.total_rows().saturating_sub(usize::from(self.rows))
    }

    /// Moves the window by `delta` rows, clamped. Returns whether it moved.
    pub fn scroll_by(&mut self, delta: isize) -> bool {
        let max = self.max_first_row();
        let next = self.first_row.saturating_add_signed(delta).min(max);
        let moved = next != self.first_row;
        self.first_row = next;
        moved
    }

    /// Index range of the visible window.
    pub fn window(&self) -> std::ops::Range<usize> {
        let cols = usize::from(self.cols).max(1);
        let start = self.first_row * cols;
        let end = (start + cols * usize::from(self.rows)).min(self.total);
        start..end.max(start)
    }
}

/// A spawned cell; `index` into the source. On a non-pooled grid the cell is
/// also tagged `cell=<index>`; a pooled cell keeps its own tags, since it
/// outlives any one index.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct VirtualCell(pub usize);

/// On a pooled cell: its fixed position in the pool, stable for the life of
/// the cell. [`VirtualCell`] says which index it currently shows, and is
/// absent while it shows none.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct PooledCell(pub usize);

/// The scrollbar track child. `SemanticRole::Custom("scrollbar")`.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct VirtualScrollbar;

/// The draggable thumb inside a [`VirtualScrollbar`].
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct VirtualScrollThumb;

/// Set on a grid whose source id is not registered, so the warning is logged
/// once instead of every frame.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct UnknownGridSource;

/// Parameters of `slotted:virtual_grid`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VirtualGridParams {
    /// Source id.
    pub source: DataSourceId,
    /// Columns.
    #[serde(default = "nine")]
    pub cols: u16,
    /// Visible rows.
    #[serde(default = "three")]
    pub rows: u16,
}

fn nine() -> u16 {
    9
}

fn three() -> u16 {
    3
}

impl Default for VirtualGridParams {
    fn default() -> Self {
        Self {
            source: DataSourceId(
                slotted_model::Namespaced::parse("slotted:empty").expect("well formed"),
            ),
            cols: 9,
            rows: 3,
        }
    }
}

/// How a grid lays its cells out: the slot grid's square cells, or the
/// list's full-width rows (menus M1 contract 4.3).
#[derive(Debug, Clone, PartialEq)]
pub struct GridShape {
    /// The cell columns.
    pub columns: RepeatedGridTrack,
    /// Height of one row, in logical px.
    pub row_height: f32,
    /// Gap between rows and columns, in logical px.
    pub gap: f32,
    /// The root's theme role.
    pub role: Role,
}

impl GridShape {
    /// `cols` square cells of the theme's slot size: the Phase 6 grid.
    pub fn slots(tokens: &slotted_theme::Tokens, cols: u16) -> Self {
        Self {
            columns: RepeatedGridTrack::px(cols, tokens.sizes.slot_size),
            row_height: tokens.sizes.slot_size,
            gap: tokens.slot_gap(),
            role: roles::VIRTUAL_GRID,
        }
    }
}

/// Spawns a virtual grid root and its first window of cells. Contract 1.5.
///
/// The cells themselves are spawned by [`refresh_virtual_grids`] on the next
/// `Render`, which is the same code path a scroll takes; there is only one
/// place that reads the source.
pub fn spawn_virtual_grid(
    ctx: &mut SpawnCtx<'_>,
    params: &VirtualGridParams,
    tags: &Tags,
) -> Entity {
    let shape = GridShape::slots(&ctx.tokens(), params.cols);
    spawn_shaped_virtual_grid(ctx, params, tags, &shape)
}

/// [`spawn_virtual_grid`] with the cell geometry chosen by the caller. The
/// list is this with one full-width column of compact rows.
pub fn spawn_shaped_virtual_grid(
    ctx: &mut SpawnCtx<'_>,
    params: &VirtualGridParams,
    _tags: &Tags,
    shape: &GridShape,
) -> Entity {
    let entity = ctx.spawn_node((
        Node {
            display: Display::Grid,
            grid_template_columns: vec![
                shape.columns.clone(),
                RepeatedGridTrack::px(1, SCROLLBAR_WIDTH),
            ],
            grid_template_rows: vec![RepeatedGridTrack::px(params.rows, shape.row_height)],
            row_gap: px(shape.gap),
            column_gap: px(shape.gap),
            overflow: Overflow::clip(),
            ..default()
        },
        Themed(shape.role.clone()),
        SemanticRole::Grid,
        WidgetNode(crate::widgets::kinds::virtual_grid()),
        VirtualGridState {
            source: params.source.clone(),
            cols: params.cols,
            rows: params.rows,
            first_row: 0,
            total: 0,
            version: 0,
        },
        Pickable::default(),
    ));
    spawn_scrollbar(ctx, entity, params.cols, params.rows);
    let mut e = ctx.world.entity_mut(entity);
    e.observe(on_virtual_grid_scroll);
    entity
}

fn spawn_scrollbar(ctx: &mut SpawnCtx<'_>, grid: Entity, cols: u16, rows: u16) -> Entity {
    let track = ctx
        .world
        .spawn((
            Node {
                grid_column: GridPlacement::start(column_line(cols)),
                grid_row: GridPlacement::start(1).set_span(rows.max(1)),
                width: px(SCROLLBAR_WIDTH),
                height: percent(100),
                position_type: PositionType::Relative,
                overflow: Overflow::clip(),
                ..default()
            },
            Themed(roles::VIRTUAL_GRID_SCROLLBAR),
            SemanticRole::Custom("scrollbar".to_owned()),
            VirtualScrollbar,
            Pickable::default(),
            ChildOf(grid),
        ))
        .id();
    ctx.world.spawn((
        Node {
            position_type: PositionType::Absolute,
            top: percent(0),
            width: percent(100),
            height: percent(100),
            ..default()
        },
        Themed(roles::VIRTUAL_GRID_THUMB),
        VirtualScrollThumb,
        Pickable::default(),
        ChildOf(track),
    ));
    track
}

/// The grid line the scrollbar column starts at: one past the last cell
/// column, and grid lines are 1-based.
fn column_line(cols: u16) -> i16 {
    i16::try_from(cols)
        .unwrap_or(i16::MAX - 1)
        .saturating_add(1)
}

/// Observer: one row per scroll notch.
pub fn on_virtual_grid_scroll(
    scroll: On<Pointer<Scroll>>,
    mut grids: Query<&mut VirtualGridState>,
) {
    let Ok(mut state) = grids.get_mut(scroll.entity) else {
        return;
    };
    // Bevy reports scrolling away from the user as positive `y`; that walks
    // the window towards the start of the source.
    let delta = -scroll.event().y;
    let rows = if delta > 0.0 {
        1
    } else if delta < 0.0 {
        -1
    } else {
        0
    };
    if rows != 0 {
        state.scroll_by(rows);
    }
}

/// `SlottedUiSet::Input`, after the focused-action dispatch: an unclaimed
/// `PagePrev` / `PageNext` while the focus is on a grid or one of its cells
/// pages the window and claims the action (menus M1: controls read actions,
/// not keys). A grid inside a scroll panel defers to the panel, which claims
/// first.
pub fn page_virtual_grids(
    mut events: MessageReader<crate::actions::UiActionEvent>,
    focus: Option<Res<bevy::input_focus::InputFocus>>,
    parents: Query<&ChildOf>,
    mut grids: Query<&mut VirtualGridState>,
    mut claims: ResMut<crate::actions::UiActionClaims>,
) {
    let Some(focused) = focus
        .as_deref()
        .and_then(bevy::input_focus::InputFocus::get)
    else {
        return;
    };
    for event in events.read() {
        let direction: isize = match event.action {
            crate::actions::UiAction::PagePrev => -1,
            crate::actions::UiAction::PageNext => 1,
            _ => continue,
        };
        if event.repeat || claims.is_claimed(event.action) {
            continue;
        }
        let mut current = focused;
        let grid = loop {
            if grids.contains(current) {
                break Some(current);
            }
            match parents.get(current) {
                Ok(child_of) => current = child_of.parent(),
                Err(_) => break None,
            }
        };
        let Some(grid) = grid else {
            continue;
        };
        let Ok(mut state) = grids.get_mut(grid) else {
            continue;
        };
        let page = isize::try_from(state.rows.max(1)).unwrap_or(1);
        state.scroll_by(direction * page);
        claims.claim(event.action);
    }
}

/// `SlottedUiSet::Render`: despawns and respawns the visible cells of every
/// grid whose `first_row`, source length or version changed. A grid over a
/// [pooled](VirtualGridSource::pooled) source instead keeps its cells and
/// rebinds them, every frame, through [`VirtualGridSource::rebind`].
pub fn refresh_virtual_grids(world: &mut World) {
    let grids: Vec<(Entity, VirtualGridState)> = world
        .query::<(Entity, &VirtualGridState)>()
        .iter(world)
        .map(|(e, s)| (e, s.clone()))
        .collect();
    if grids.is_empty() {
        return;
    }
    let sources = world
        .get_resource::<VirtualGridSources>()
        .cloned()
        .unwrap_or_default();

    for (entity, state) in grids {
        let Some(source) = sources.0.get(&state.source).cloned() else {
            if world.get::<UnknownGridSource>(entity).is_none() {
                tracing::warn!(source = %state.source.0, "no virtual grid source registered");
                world.entity_mut(entity).insert(UnknownGridSource);
            }
            continue;
        };
        world.entity_mut(entity).remove::<UnknownGridSource>();

        let pooled = source.pooled();
        let mut next = state.clone();
        next.total = source.len_in(world);
        next.version = source.version();
        if !pooled {
            // A pooled grid's window is its owner's to place: nothing is
            // despawned when it runs past the end, so there is nothing to
            // clamp it for.
            next.first_row = next.first_row.min(next.max_first_row());
        }
        let built = VirtualGridBuilt {
            first_row: next.first_row,
            total: next.total,
            version: next.version,
        };
        // A pooled source rebinds every frame — binding is its own idempotent
        // work, and it is the source, not the window, that decides what a
        // cell shows.
        if !pooled && world.get::<VirtualGridBuilt>(entity) == Some(&built) {
            continue;
        }
        let window = next.window();
        let pool = next.clone();
        if let Some(mut state) = world.get_mut::<VirtualGridState>(entity) {
            *state = next;
        }
        world.entity_mut(entity).insert(built);

        if pooled {
            refresh_pool(world, entity, source.as_ref(), &pool);
            update_scrollbar(world, entity);
            continue;
        }

        // A non-pooled cell is an arbitrary `UiNodeDef`, so reusing one would
        // mean diffing trees: the window is small, and respawning is honest.
        let stale: Vec<Entity> = world
            .query::<(Entity, &VirtualCell)>()
            .iter(world)
            .filter(|(cell, _)| {
                world
                    .get::<ChildOf>(*cell)
                    .is_some_and(|c| c.parent() == entity)
            })
            .map(|(cell, _)| cell)
            .collect();
        for cell in stale {
            world.entity_mut(cell).despawn();
        }

        let (screen, kind, menu) = screen_of(world, entity);
        for index in window {
            let def = source.cell(index);
            // A list wraps each cell in a row of its own (menus M1 4.3), so
            // the source's node keeps its role and the row carries the
            // window's bookkeeping.
            let row = crate::widgets::list::spawn_row(world, entity, index);
            let mut ctx = SpawnCtx {
                world,
                screen,
                kind: kind.clone(),
                menu,
                parent: row.unwrap_or(entity),
            };
            let inner = ctx.spawn_child(&def);
            let cell = row.unwrap_or(inner);
            let tags = world
                .get::<Tags>(cell)
                .cloned()
                .unwrap_or_default()
                .with("cell", &index.to_string());
            world
                .entity_mut(cell)
                .insert((VirtualCell(index), tags, crate::focus_ring::Focusable));
        }
        update_scrollbar(world, entity);
    }
}

/// Keeps a pooled grid's `cols * rows` cells alive and rebinds them to the
/// current window. Cell entities are stable: the pool only grows or shrinks
/// when the grid's shape does, so a screen-tree snapshot lists the same cells
/// — the hidden ones included — however far the window has moved.
fn refresh_pool(
    world: &mut World,
    grid: Entity,
    source: &dyn VirtualGridSource,
    state: &VirtualGridState,
) {
    let cols = usize::from(state.cols).max(1);
    let want = cols * usize::from(state.rows);

    let mut pool: Vec<(usize, Entity)> = world
        .get::<Children>(grid)
        .map(|children| children.iter().collect::<Vec<_>>())
        .unwrap_or_default()
        .into_iter()
        .filter_map(|cell| world.get::<PooledCell>(cell).map(|slot| (slot.0, cell)))
        .collect();
    pool.retain(|(slot, cell)| {
        let keep = *slot < want;
        if !keep {
            world.entity_mut(*cell).despawn();
        }
        keep
    });
    pool.sort_unstable();
    let held: std::collections::HashSet<usize> = pool.iter().map(|(slot, _)| *slot).collect();
    for slot in 0..want {
        if !held.contains(&slot) {
            pool.push((slot, spawn_pooled_cell(world, grid, source, slot)));
        }
    }
    pool.sort_unstable();

    let start = state.first_row * cols;
    for (slot, cell) in pool {
        let index = start + slot;
        let index = (index < state.total).then_some(index);
        let mut cell = cell;
        if !source.rebind(world, cell, index) {
            // The source cannot reuse this one: it gets a fresh cell in the
            // same slot, and one more chance to bind it.
            world.entity_mut(cell).despawn();
            cell = spawn_pooled_cell(world, grid, source, slot);
            source.rebind(world, cell, index);
        }
        match index {
            Some(index) => {
                world.entity_mut(cell).insert(VirtualCell(index));
            }
            None => {
                world.entity_mut(cell).remove::<VirtualCell>();
            }
        }
    }
}

/// One pooled cell, from the source's own spawner or, failing that, from the
/// `UiNodeDef` it hands out for that pool position.
fn spawn_pooled_cell(
    world: &mut World,
    grid: Entity,
    source: &dyn VirtualGridSource,
    slot: usize,
) -> Entity {
    let cell = source.spawn_cell(world, grid, slot).unwrap_or_else(|| {
        let def = source.cell(slot);
        let (screen, kind, menu) = screen_of(world, grid);
        let mut ctx = SpawnCtx {
            world,
            screen,
            kind,
            menu,
            parent: grid,
        };
        ctx.spawn_child(&def)
    });
    world.entity_mut(cell).insert((
        PooledCell(slot),
        ChildOf(grid),
        crate::focus_ring::Focusable,
    ));
    cell
}

/// The window a grid's spawned cells stand for. Compared against the source
/// every frame: equal means the cells are current, so an unchanged grid costs
/// one comparison and an empty source is not mistaken for "never built".
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct VirtualGridBuilt {
    /// First visible row of the spawned cells.
    pub first_row: usize,
    /// `source.len()` they were spawned from.
    pub total: usize,
    /// `source.version()` they were spawned from.
    pub version: u64,
}

/// The screen a grid sits in, for the cells' `SpawnCtx`.
fn screen_of(world: &World, entity: Entity) -> (Entity, crate::def::ScreenKind, Option<Entity>) {
    let mut current = entity;
    loop {
        if let Some(root) = world.get::<ScreenRoot>(current) {
            return (current, root.kind.clone(), root.menu);
        }
        match world.get::<ChildOf>(current) {
            Some(parent) => current = parent.parent(),
            // A grid outside any screen (a HUD layer, a bare test world) is
            // its own screen as far as its cells are concerned.
            None => {
                return (
                    entity,
                    crate::def::ScreenKind::new("slotted:virtual_grid"),
                    None,
                );
            }
        }
    }
}

fn update_scrollbar(world: &mut World, grid: Entity) {
    let Some(state) = world.get::<VirtualGridState>(grid).cloned() else {
        return;
    };
    let total_rows = state.total_rows().max(1);
    let visible = usize::from(state.rows).max(1).min(total_rows);
    #[allow(clippy::cast_precision_loss)]
    let fraction = visible as f32 / total_rows as f32;
    #[allow(clippy::cast_precision_loss)]
    let offset = state.first_row as f32 / total_rows as f32;

    let tracks: Vec<Entity> = world
        .get::<Children>(grid)
        .map(|c| c.iter().collect())
        .unwrap_or_default();
    for track in tracks {
        if world.get::<VirtualScrollbar>(track).is_none() {
            continue;
        }
        let thumbs: Vec<Entity> = world
            .get::<Children>(track)
            .map(|c| c.iter().collect())
            .unwrap_or_default();
        for thumb in thumbs {
            if world.get::<VirtualScrollThumb>(thumb).is_none() {
                continue;
            }
            if let Some(mut node) = world.get_mut::<Node>(thumb) {
                node.height = percent(100.0 * fraction);
                node.top = percent(100.0 * offset);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(total: usize, first_row: usize) -> VirtualGridState {
        VirtualGridState {
            source: DataSourceId(slotted_model::Namespaced::parse("demo:src").expect("id")),
            cols: 9,
            rows: 3,
            first_row,
            total,
            version: 0,
        }
    }

    #[test]
    fn only_the_visible_window_is_ever_spawned() {
        let s = state(10_000, 0);
        assert_eq!(s.window(), 0..27);
        assert_eq!(state(10_000, 5).window(), 45..72);
    }

    #[test]
    fn the_last_page_is_the_end_of_the_source() {
        let s = state(10_000, 0);
        assert_eq!(s.total_rows(), 1112);
        assert_eq!(s.max_first_row(), 1109);
        let mut s = state(10_000, 1109);
        assert!(!s.scroll_by(5), "already at the end");
        assert_eq!(s.first_row, 1109);
    }

    #[test]
    fn scrolling_clamps_at_both_ends() {
        let mut s = state(10_000, 0);
        assert!(!s.scroll_by(-1), "already at the start");
        assert!(s.scroll_by(3));
        assert_eq!(s.first_row, 3);
        assert!(s.scroll_by(-3));
        assert_eq!(s.first_row, 0);
    }

    #[test]
    fn a_short_source_fills_less_than_one_window() {
        let s = state(4, 0);
        assert_eq!(s.window(), 0..4);
        assert_eq!(s.max_first_row(), 0);
    }
}
