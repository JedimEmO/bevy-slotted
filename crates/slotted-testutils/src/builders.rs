//! Small fixtures: a lookup table, a frozen registry and inventory builders.

use std::sync::Arc;

use slotted_model::{ItemId, ItemStack, LookupCtx, MenuDef, Namespaced};
use slotted_registry::FrozenRegistries;
use slotted_registry::defs::{ItemDef, TagDef, TagEntry};
use slotted_registry::registry::Registries as RegistryBuilder;

/// The three items every fixture uses.
///
/// The ids are fixed because [`test_registries`] registers them in this
/// order and the interner is dense from zero; `builders::tests` pins it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TestItems {
    /// `test:stone`, stacks to 64, no tags.
    pub stone: ItemId,
    /// `test:egg`, stacks to 16, tagged `test:food`.
    pub egg: ItemId,
    /// `test:sword`, stacks to 1, tagged `test:tools`.
    pub sword: ItemId,
}

impl Default for TestItems {
    fn default() -> Self {
        Self {
            stone: ItemId(0),
            egg: ItemId(1),
            sword: ItemId(2),
        }
    }
}

/// Parses a namespaced id, panicking on a malformed one.
///
/// # Panics
/// When `text` is not a valid `namespace:path`.
pub fn id(text: &str) -> Namespaced {
    Namespaced::parse(text).expect("valid namespaced id")
}

/// The `test:food` tag, on `test:egg`.
pub fn food_tag() -> Namespaced {
    id("test:food")
}

/// The `test:tools` tag, on `test:sword`.
pub fn tools_tag() -> Namespaced {
    id("test:tools")
}

/// A hand-written [`LookupCtx`] over [`TestItems`], for tests that do not
/// want a registry at all.
///
/// Stack sizes are 64 for `stone`, 16 for `egg` and 1 for everything else,
/// which is the same table [`test_registries`] freezes.
#[derive(Debug, Clone, Copy, Default)]
pub struct TestLookup;

impl LookupCtx for TestLookup {
    fn max_stack(&self, id: ItemId) -> u32 {
        let items = TestItems::default();
        if id == items.stone {
            64
        } else if id == items.egg {
            16
        } else {
            1
        }
    }

    fn has_tag(&self, id: ItemId, tag: &Namespaced) -> bool {
        let items = TestItems::default();
        (id == items.egg && *tag == food_tag()) || (id == items.sword && *tag == tools_tag())
    }
}

/// A frozen registry holding [`TestItems`] and their two tags.
///
/// # Panics
/// Never: the definitions are static and known good.
pub fn test_registries() -> Arc<FrozenRegistries> {
    let mut builder = RegistryBuilder::new();
    for (name, max) in [("test:stone", 64), ("test:egg", 16), ("test:sword", 1)] {
        let mut def = ItemDef::new(id(name));
        def.max_stack_size = max;
        builder.add_item(def).expect("fresh item id");
    }
    builder.add_tag(TagDef {
        name: food_tag(),
        values: vec![TagEntry::Item(id("test:egg"))],
        replace: false,
    });
    builder.add_tag(TagDef {
        name: tools_tag(),
        values: vec![TagEntry::Item(id("test:sword"))],
        replace: false,
    });
    let (frozen, _warnings) = builder.freeze().expect("test registries freeze");
    Arc::new(frozen)
}

/// The ids [`test_registries`] assigned, looked up by name.
///
/// # Panics
/// When the registry does not hold one of the three test items.
pub fn test_items(registries: &FrozenRegistries) -> TestItems {
    let get = |name: &str| registries.item_id(&id(name)).expect("test item registered");
    TestItems {
        stone: get("test:stone"),
        egg: get("test:egg"),
        sword: get("test:sword"),
    }
}

/// A stack of `count` items of kind `item`, as a slot value.
pub fn stack(item: ItemId, count: u32) -> Option<ItemStack> {
    Some(ItemStack::new(item, count))
}

/// An [`Inventory`](slotted_ecs::Inventory) component with `len` empty slots.
pub fn empty_inventory(len: usize) -> slotted_ecs::Inventory {
    slotted_ecs::Inventory::new(len)
}

/// An [`Inventory`](slotted_ecs::Inventory) component with these contents.
///
/// Nothing is marked dirty, so a test starts from a clean mask.
pub fn inventory(slots: impl IntoIterator<Item = Option<ItemStack>>) -> slotted_ecs::Inventory {
    slotted_ecs::Inventory(slotted_model::Inventory::from_slots(slots))
}

/// Spawns one empty [`Inventory`](slotted_ecs::Inventory) entity per handle
/// `def` references, sized as `def` needs, and returns them in handle order.
pub fn spawn_inventories(
    world: &mut bevy::prelude::World,
    def: &MenuDef,
) -> Vec<bevy::prelude::Entity> {
    def.inventory_sizes()
        .into_iter()
        .map(|size| world.spawn(empty_inventory(size)).id())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{TestItems, TestLookup, food_tag, test_items, test_registries, tools_tag};
    use slotted_model::LookupCtx;

    #[test]
    fn the_hand_written_table_agrees_with_the_frozen_registry() {
        let registries = test_registries();
        let items = test_items(&registries);
        assert_eq!(items, TestItems::default());
        let lookup = slotted_ecs::RegistryLookup(&registries);
        for item in [items.stone, items.egg, items.sword] {
            assert_eq!(lookup.max_stack(item), TestLookup.max_stack(item));
            for tag in [food_tag(), tools_tag()] {
                assert_eq!(
                    lookup.has_tag(item, &tag),
                    TestLookup.has_tag(item, &tag),
                    "{item:?} {tag}"
                );
            }
        }
        assert_eq!(TestLookup.max_stack(slotted_model::ItemId(99)), 1);
    }
}
