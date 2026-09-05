//! [`LookupCtx`] over the frozen registries.

use std::sync::Arc;

use bevy::prelude::*;
use slotted_model::{ItemId, LookupCtx, Namespaced};
use slotted_registry::FrozenRegistries;

/// The frozen registry set as a resource. Inserted by the game (or the facade)
/// after the data stage; everything in the ecs and ui crates reads through it.
#[derive(Resource, Clone, Debug)]
pub struct Registries(pub Arc<FrozenRegistries>);

impl Registries {
    /// A [`LookupCtx`] view for `apply_click`.
    pub fn lookup(&self) -> RegistryLookup<'_> {
        RegistryLookup(&self.0)
    }
}

impl std::ops::Deref for Registries {
    type Target = FrozenRegistries;
    fn deref(&self) -> &FrozenRegistries {
        &self.0
    }
}

/// Borrowed [`LookupCtx`] implementation over [`FrozenRegistries`].
///
/// Unknown items stack to one and carry no tags, so a stale id never panics.
#[derive(Debug, Clone, Copy)]
pub struct RegistryLookup<'a>(pub &'a FrozenRegistries);

impl LookupCtx for RegistryLookup<'_> {
    fn max_stack(&self, id: ItemId) -> u32 {
        self.0
            .items
            .get(id)
            .map_or(1, |def| def.max_stack_size.max(1))
    }

    fn has_tag(&self, id: ItemId, tag: &Namespaced) -> bool {
        self.0.items_with(tag).contains(&id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use slotted_registry::defs::{ItemDef, TagDef, TagEntry};
    use slotted_registry::registry::Registries as Builder;

    #[test]
    fn lookup_reads_stack_size_and_tags() {
        let id = |s: &str| Namespaced::parse(s).expect("valid id");
        let mut b = Builder::new();
        let mut apple = ItemDef::new(id("t:apple"));
        apple.max_stack_size = 16;
        b.add_item(apple).expect("fresh");
        b.add_item(ItemDef::new(id("t:stone"))).expect("fresh");
        b.add_tag(TagDef {
            name: id("t:food"),
            values: vec![TagEntry::Item(id("t:apple"))],
            replace: false,
        });
        let (frozen, _) = b.freeze().expect("freezes");
        let apple = frozen.item_id(&id("t:apple")).expect("registered");
        let stone = frozen.item_id(&id("t:stone")).expect("registered");
        let ctx = RegistryLookup(&frozen);
        assert_eq!(ctx.max_stack(apple), 16);
        assert_eq!(ctx.max_stack(stone), 64);
        assert_eq!(ctx.max_stack(ItemId(999)), 1);
        assert!(ctx.has_tag(apple, &id("t:food")));
        assert!(!ctx.has_tag(stone, &id("t:food")));
    }
}
