//! `LiveIcons`: a live `ViewportNode` per requested item (`live` feature).
//!
//! Restricted to the hovered item and detail panes; ADR 0003 measured about
//! 0.8 ms per camera, so one per slot is out.

use slotted_model::ItemStack;

use crate::source::{IconRef, IconSource};

/// Stub adapter. Returns [`IconRef::Missing`] until Phase 6 wires the
/// viewport camera rig.
// PHASE2-IMPL: none. Phase 6.
#[derive(Debug, Default, Clone, Copy)]
pub struct LiveIcons;

impl IconSource for LiveIcons {
    fn icon(&self, _stack: &ItemStack) -> IconRef {
        IconRef::Missing
    }
}
