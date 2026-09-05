//! The item renderer: icon, count, and later durability, cooldown, rarity.

use bevy::prelude::*;
use slotted_icons::{IconRef, Icons};
use slotted_model::ItemStack;

/// The stack a node displays. On slot entities and on the carried-stack node.
/// Set by the slot widget from `SlotChanged`; `Changed<ItemView>` drives
/// [`render_items`].
#[derive(Component, Debug, Clone, Default, PartialEq)]
pub struct ItemView {
    /// What to show. `None` draws an empty slot.
    pub stack: Option<ItemStack>,
}

/// Marker on the icon child of an [`ItemView`] node: an `ImageNode`.
#[derive(Component, Debug, Default, Clone, Copy)]
pub struct ItemIcon;

/// Marker on the count child of an [`ItemView`] node: a `Text` with
/// `Themed(count)`. Hidden when the count is 1 or the slot is empty.
#[derive(Component, Debug, Default, Clone, Copy)]
pub struct ItemCount;

/// `SlottedUiSet::Render`: for each changed [`ItemView`], resolve the icon
/// through [`Icons`], write the `ImageNode` on the [`ItemIcon`] child and the
/// count text on the [`ItemCount`] child, and refresh `SemanticLabel`.
// PHASE2-IMPL: agent B.
pub fn render_items(
    icons: Option<Res<Icons>>,
    changed: Query<(Entity, &ItemView), Changed<ItemView>>,
) {
    let count = changed.iter().count();
    if count > 0 {
        let _ = changed
            .iter()
            .next()
            .map(|(_, v)| v.stack.as_ref().map(|s| icons.as_ref().map(|i| i.icon(s))));
        let _: Option<IconRef> = None;
        tracing::warn!(count, "render_items is not implemented yet");
    }
}
