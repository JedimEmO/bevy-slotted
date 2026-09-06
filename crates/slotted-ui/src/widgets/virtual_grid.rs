//! `virtual_grid`: a windowed grid over a [`VirtualGridSource`]. Phase 6
//! contract section 1.5. Cells are respawned when the window moves; the
//! browser's pooled card grid stays separate this phase.

use std::collections::HashMap;
use std::sync::Arc;

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::def::{DataSourceId, Tags, UiNodeDef};
use crate::screen::SpawnCtx;
use crate::semantic::{SemanticRole, WidgetNode};

/// What a virtual grid shows. Cells are `UiNodeDef`s spawned through the
/// ordinary widget path, so a source can hand out slots, cards or anything.
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

/// A spawned cell; `index` into the source. Also tagged `cell=<index>`.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct VirtualCell(pub usize);

/// The scrollbar track child. `SemanticRole::Custom("scrollbar")`.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct VirtualScrollbar;

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

/// Spawns a virtual grid root and its first window of cells. Contract 1.5.
pub fn spawn_virtual_grid(
    ctx: &mut SpawnCtx<'_>,
    params: &VirtualGridParams,
    _tags: &Tags,
) -> Entity {
    // PHASE6-IMPL: A. Display::Grid node `cols` wide plus an 8 px scrollbar
    // column, Themed(VIRTUAL_GRID), SemanticRole::Grid, Pickable (scroll);
    // spawn the first `rows` rows from the source; scrollbar child with thumb.
    ctx.spawn_node((
        Node::default(),
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
    ))
}

/// `SlottedUiSet::Render`: despawns and respawns the visible cells of every
/// grid whose `first_row`, source length or version changed.
pub fn refresh_virtual_grids(world: &mut World) {
    // PHASE6-IMPL: A. Exclusive: cells spawn through `SpawnCtx`.
    let _ = world;
}
