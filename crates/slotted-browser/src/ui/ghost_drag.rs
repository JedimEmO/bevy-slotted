//! Ghost drag from a card to a Ghost or Filter slot. Package B.

use bevy::prelude::*;

/// On the ghost node under the carried layer while a card is dragged.
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub struct GhostDrag(pub crate::ingredient::Ingredient);

/// Observer of `Pointer<DragStart>` on a card: spawns the ghost.
pub fn on_card_drag_start(_drag: On<Pointer<DragStart>>, _commands: Commands) {
    // PHASE3-IMPL: B
}

/// Observer of `Pointer<DragDrop>` on a `SlotRef`: asks the screen handler's
/// `ghost_drop` and triggers the returned actions.
pub fn on_ghost_drop(_drop: On<Pointer<DragDrop>>, _commands: Commands) {
    // PHASE3-IMPL: B
}
