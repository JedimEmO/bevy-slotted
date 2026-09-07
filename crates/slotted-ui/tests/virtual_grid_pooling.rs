//! The cell-recycling hook on `VirtualGridSource`, through the harness.
//!
//! Two sources over the same list: one plain, one pooled. What is asserted is
//! the difference the hook is for — a pooled grid keeps its cell entities
//! while the window moves over them, and keeps a full window of them alive
//! past the end of the source, so a screen-tree snapshot lists the same cells
//! from one frame to the next.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use bevy::prelude::*;
use slotted_model::{Actor, Inventory, MenuDef, Namespaced};
use slotted_test::prelude::*;
use slotted_ui::{
    DataSourceId, Layout, LocKey, PooledCell, ScreenDef, Tags, TextRole, UiNodeDef, VirtualCell,
    VirtualGridSource, VirtualGridSources, VirtualGridState,
};

const SCREEN: &str = "pool:test";

/// Columns and rows of the test grid: a six-cell window.
const COLS: u16 = 3;
const ROWS: u16 = 2;

struct Fixture;

impl MenuFixture for Fixture {
    fn def(&self) -> Arc<MenuDef> {
        Arc::new(MenuDef::generic(1))
    }

    fn inventories(&self) -> Vec<Inventory> {
        self.def()
            .inventory_sizes()
            .into_iter()
            .map(Inventory::new)
            .collect()
    }

    fn actor(&self) -> Actor {
        Actor::SURVIVAL
    }
}

fn source_id() -> DataSourceId {
    DataSourceId(Namespaced::parse("pool:cells").expect("id"))
}

/// A cell of the pooled source, so the test can find one without depending on
/// what the grid puts on it.
#[derive(Component, Debug, Clone, Copy)]
struct PoolCell;

/// The plain source: every cell is a text node, respawned when the window
/// moves. What every source did before the hook existed.
#[derive(Debug, Clone, Copy)]
struct Plain(usize);

impl VirtualGridSource for Plain {
    fn len(&self) -> usize {
        self.0
    }
    fn version(&self) -> u64 {
        1
    }
    fn cell(&self, index: usize) -> UiNodeDef {
        UiNodeDef::Text {
            key: LocKey(format!("cell {index}")),
            style: TextRole::Body,
            tags: Tags::new(),
        }
    }
}

/// The pooled source: it spawns its own cells and rebinds them, counting both
/// so the test can tell reuse from a respawn without watching entity ids
/// alone.
#[derive(Debug, Default)]
struct Pooled {
    len: usize,
    spawned: Arc<AtomicUsize>,
    bound: Arc<AtomicUsize>,
}

impl VirtualGridSource for Pooled {
    fn len(&self) -> usize {
        self.len
    }
    fn version(&self) -> u64 {
        1
    }
    fn cell(&self, index: usize) -> UiNodeDef {
        Plain(self.len).cell(index)
    }

    fn pooled(&self) -> bool {
        true
    }

    fn spawn_cell(&self, world: &mut World, grid: Entity, _slot: usize) -> Option<Entity> {
        self.spawned.fetch_add(1, Ordering::Relaxed);
        Some(
            world
                .spawn((
                    Node::default(),
                    Text::new(String::new()),
                    PoolCell,
                    Visibility::Hidden,
                    ChildOf(grid),
                ))
                .id(),
        )
    }

    fn rebind(&self, world: &mut World, cell: Entity, index: Option<usize>) -> bool {
        self.bound.fetch_add(1, Ordering::Relaxed);
        let (text, visibility) = match index {
            Some(index) => (format!("cell {index}"), Visibility::Inherited),
            None => (String::new(), Visibility::Hidden),
        };
        let mut cell = world.entity_mut(cell);
        if let Some(mut node) = cell.get_mut::<Text>() {
            node.0 = text;
        }
        if let Some(mut node) = cell.get_mut::<Visibility>() {
            *node = visibility;
        }
        true
    }
}

fn screen() -> ScreenDef {
    ScreenDef {
        kind: ScreenKind::new(SCREEN),
        initial_focus: None,
        presentation: slotted_ui::Presentation::default(),
        inherits: None,
        remove: vec![],
        root: UiNodeDef::Panel {
            role: slotted_theme::roles::PANEL,
            layout: Layout::default(),
            children: vec![UiNodeDef::VirtualGrid {
                source: source_id(),
                cols: COLS,
                rows: ROWS,
                tags: Tags::new().with(Tags::TEST_ID, "grid"),
            }],
            tags: Tags::new(),
        },
        listring: vec![],
    }
}

fn harness(source: impl VirtualGridSource + 'static) -> (UiHarness, Entity) {
    let mut h = UiHarness::builder()
        .plugins(SlottedPlugins::headless())
        .registries(TestRegistries::basic())
        .theme("glass")
        .build();
    h.world_mut()
        .resource_mut::<VirtualGridSources>()
        .register(source_id(), source);
    h.open_screen(screen(), Fixture);
    h.settle();
    let grid = h.find(&by::tag(Tags::TEST_ID, "grid"));
    (h, grid)
}

/// The cells of `grid`, in pool order for a pooled grid and in index order for
/// a plain one, with the index each currently shows.
fn cells(h: &mut UiHarness, grid: Entity) -> Vec<(Entity, Option<usize>)> {
    let children: Vec<Entity> = h
        .world()
        .get::<Children>(grid)
        .map(|c| c.iter().collect())
        .unwrap_or_default();
    let mut out: Vec<(usize, Entity, Option<usize>)> = children
        .into_iter()
        .filter_map(|cell| {
            let index = h.world().get::<VirtualCell>(cell).map(|c| c.0);
            let order = h
                .world()
                .get::<PooledCell>(cell)
                .map(|slot| slot.0)
                .or(index)?;
            Some((order, cell, index))
        })
        .collect();
    out.sort_unstable_by_key(|(order, _, _)| *order);
    out.into_iter()
        .map(|(_, cell, index)| (cell, index))
        .collect()
}

fn move_window(h: &mut UiHarness, grid: Entity, first_row: usize) {
    h.world_mut()
        .get_mut::<VirtualGridState>(grid)
        .expect("a VirtualGridState")
        .first_row = first_row;
    h.settle();
}

#[test]
fn a_pooled_source_keeps_its_cell_entities_across_a_window_move() {
    let spawned = Arc::new(AtomicUsize::new(0));
    let (mut h, grid) = harness(Pooled {
        len: 100,
        spawned: spawned.clone(),
        bound: Arc::new(AtomicUsize::new(0)),
    });

    let before = cells(&mut h, grid);
    assert_eq!(before.len(), usize::from(COLS * ROWS), "one full window");
    assert_eq!(
        before.iter().map(|(_, i)| *i).collect::<Vec<_>>(),
        (0..6).map(Some).collect::<Vec<_>>(),
    );
    let spawns = spawned.load(Ordering::Relaxed);

    move_window(&mut h, grid, 4);

    let after = cells(&mut h, grid);
    assert_eq!(
        after.iter().map(|(e, _)| *e).collect::<Vec<_>>(),
        before.iter().map(|(e, _)| *e).collect::<Vec<_>>(),
        "the pool kept its entities"
    );
    assert_eq!(
        after.iter().map(|(_, i)| *i).collect::<Vec<_>>(),
        (12..18).map(Some).collect::<Vec<_>>(),
        "rebound to the new window"
    );
    assert_eq!(
        spawned.load(Ordering::Relaxed),
        spawns,
        "nothing was respawned to move the window"
    );
}

#[test]
fn a_plain_source_still_respawns_its_cells_across_a_window_move() {
    let (mut h, grid) = harness(Plain(100));

    let before = cells(&mut h, grid);
    assert_eq!(before.len(), usize::from(COLS * ROWS));
    move_window(&mut h, grid, 4);
    let after = cells(&mut h, grid);

    assert_eq!(
        after.iter().map(|(_, i)| *i).collect::<Vec<_>>(),
        (12..18).map(Some).collect::<Vec<_>>(),
    );
    for (cell, _) in &after {
        assert!(
            !before.iter().any(|(old, _)| old == cell),
            "a plain source respawns its cells"
        );
    }
}

#[test]
fn a_pooled_grid_keeps_a_full_window_past_the_end_of_its_source() {
    let bound = Arc::new(AtomicUsize::new(0));
    let (mut h, grid) = harness(Pooled {
        len: 4,
        spawned: Arc::new(AtomicUsize::new(0)),
        bound: bound.clone(),
    });

    let cells_now = cells(&mut h, grid);
    assert_eq!(cells_now.len(), usize::from(COLS * ROWS), "a full window");
    assert_eq!(
        cells_now.iter().map(|(_, i)| *i).collect::<Vec<_>>(),
        vec![Some(0), Some(1), Some(2), Some(3), None, None],
        "the two past the end are bound to nothing, not despawned"
    );
    for (cell, index) in &cells_now {
        assert_eq!(h.is_visible(*cell), index.is_some());
    }

    // Over-scrolled: the window is the owner's to place, and a pooled grid
    // binds nothing rather than clamping itself back into the source.
    move_window(&mut h, grid, 9);
    let over = cells(&mut h, grid);
    assert_eq!(
        over.iter().map(|(e, _)| *e).collect::<Vec<_>>(),
        cells_now.iter().map(|(e, _)| *e).collect::<Vec<_>>(),
        "same entities"
    );
    assert!(over.iter().all(|(_, index)| index.is_none()));
    assert!(bound.load(Ordering::Relaxed) > 0, "rebind was called");
}
