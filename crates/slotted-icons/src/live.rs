//! `LiveIcons`: a real, slowly turning 3D item behind a `ViewportNode`.
//!
//! Restricted to the hovered item and detail panes; ADR 0003 measured about
//! 0.8 ms per camera, so one per slot is out. [`LiveIcons`] is therefore not
//! a replacement for [`AtlasIcons`](crate::AtlasIcons): it says
//! [`IconRef::Live`] for items it can show live and defers to the atlas for
//! everything else, which is what a slot grid keeps drawing.

use std::sync::Arc;

use slotted_model::ItemStack;

use crate::source::{IconRef, IconSource};

/// Live viewport icons, over a fallback source.
///
/// `slotted-ui` reads [`IconRef::Live`] as "spawn a viewport for this item";
/// anything that cannot host a viewport falls through to `fallback`, so a
/// slot grid under a live tooltip still draws from the atlas.
#[derive(Clone)]
pub struct LiveIcons {
    /// Where non-live requests go. Normally the baked atlas.
    pub fallback: Arc<dyn IconSource>,
}

impl LiveIcons {
    /// Live icons over `fallback`.
    pub fn new(fallback: impl IconSource + 'static) -> Self {
        Self {
            fallback: Arc::new(fallback),
        }
    }

    /// Live icons over an existing shared source.
    #[must_use]
    pub fn over(fallback: Arc<dyn IconSource>) -> Self {
        Self { fallback }
    }
}

impl std::fmt::Debug for LiveIcons {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LiveIcons").finish_non_exhaustive()
    }
}

impl IconSource for LiveIcons {
    fn icon(&self, stack: &ItemStack) -> IconRef {
        // Every known item can be shown live: the viewport builds its subject
        // from the item id, the same way the bake builds a cell from it. An
        // item the fallback does not know is not one we can show either.
        match self.fallback.icon(stack) {
            IconRef::Missing => IconRef::Missing,
            _ => IconRef::Live(stack.id),
        }
    }

    fn live(&self) -> Option<&dyn IconSource> {
        Some(&*self.fallback)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::atlas::AtlasIcons;
    use slotted_model::ItemId;

    #[test]
    fn a_known_item_goes_live_and_an_unknown_one_does_not() {
        let mut atlas = AtlasIcons::default();
        atlas.index.insert(ItemId(1), 0);
        let live = LiveIcons::new(atlas);
        assert_eq!(
            live.icon(&ItemStack::new(ItemId(1), 1)),
            IconRef::Live(ItemId(1))
        );
        assert_eq!(live.icon(&ItemStack::new(ItemId(9), 1)), IconRef::Missing);
    }

    #[test]
    fn the_fallback_is_reachable_for_the_slots_under_a_tooltip() {
        let mut atlas = AtlasIcons::default();
        atlas.index.insert(ItemId(1), 3);
        let live = LiveIcons::new(atlas);
        let fallback = live.live().expect("a fallback");
        assert!(matches!(
            fallback.icon(&ItemStack::new(ItemId(1), 1)),
            IconRef::Atlas { index: 3, .. }
        ));
    }
}
