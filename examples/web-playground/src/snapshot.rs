//! What survives a restart.
//!
//! On `wasm32` a Lua error raised by a mod aborts the whole module (ADR 0004).
//! The page catches that, re-instantiates the module and asks it to carry on,
//! and the one thing a visitor would be sad to lose is the chest: they had
//! moved stacks around, and a restart that empties the container reads as the
//! playground losing their work rather than as a mod misbehaving.
//!
//! So the world publishes a [`Snapshot`] of the open menu's inventories every
//! so often, the page keeps the last one, and after a restart it hands it back
//! through `restore_state`.
//!
//! Stacks are named, not numbered. An [`ItemId`](slotted_model::ItemId) is a
//! position in the registries the data stage built, and the restarted module
//! rebuilds those from whatever Lua the editor holds *now*, which may register
//! a different set. `copper_chest:copper_ingot` means the same thing before
//! and after; `ItemId(3)` does not. A name the new registries do not have is
//! dropped with a warning, the same call the demo table makes.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use slotted_model::{ItemStack, Namespaced};
use slotted_registry::FrozenRegistries;

/// One occupied slot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Slot {
    /// Position in its inventory.
    pub index: usize,
    /// The item's namespaced id, as text.
    pub item: String,
    /// How many.
    pub count: u32,
}

/// One inventory: its length and the slots that hold something.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct InventorySnapshot {
    /// How many slots it has.
    pub len: usize,
    /// The occupied ones, in index order.
    pub slots: Vec<Slot>,
}

/// The Multiplayer scene's authoritative state: the server's, not a client's.
///
/// The other seven scenes have one copy of everything, so the open menu's
/// inventories are the whole world. Multiplayer has three: the server's
/// [`ContainerStore`](slotted_net::ContainerStore) and each client's local
/// mirror of it, and they are allowed to disagree, because watching a wrong
/// guess corrected is the scene. Snapshotting a client therefore captures a
/// prediction, and restoring it into a rebuilt pair would make the guess
/// authoritative. The server's copy is the one that is true.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetSnapshot {
    /// The chest both players share, as the server holds it.
    pub chest: InventorySnapshot,
    /// One entry per session, in `NetLink::sessions` order: that peer's
    /// private inventories, in the order the server bound them after the
    /// shared chest.
    pub players: Vec<Vec<InventorySnapshot>>,
    /// What each session had on the cursor, in the same order.
    ///
    /// A stack on the cursor is out of every container, so leaving it out of
    /// the snapshot would lose it: a restart mid-drag would come back with the
    /// chest one stack short and no sign of where it went. It is restored the
    /// way [`MenuServer`](slotted_net::MenuServer) itself returns a carried
    /// stack when a session closes, which is what a crashed module is.
    #[serde(default)]
    pub carried: Vec<Option<Carried>>,
}

/// A stack on a cursor, named the way [`Slot`] names one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Carried {
    /// The item's namespaced id, as text.
    pub item: String,
    /// How many.
    pub count: u32,
}

/// The open menu's inventories, in `OpenMenu::inventories` order, and which
/// showcase scene they belong to.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Resource)]
pub struct Snapshot {
    /// One entry per inventory the menu holds.
    pub inventories: Vec<InventorySnapshot>,
    /// The scene the visitor was in, as a `showcase::Scene` id
    /// (docs/design/showcase-contract.md section 4).
    ///
    /// A restart re-instantiates the module, which comes up on
    /// `Scene::DEFAULT`; without this the visitor is thrown back to the chest
    /// from whichever scene the mod raised in. `#[serde(default)]` so a
    /// snapshot the page kept from before this field still restores its
    /// inventories.
    #[serde(default)]
    pub scene: Option<String>,
    /// The Multiplayer scene's server state, when that is the scene.
    ///
    /// `#[serde(default)]` for the same reason `scene` has it: a snapshot the
    /// page kept from before this field still restores its inventories.
    #[serde(default)]
    pub net: Option<NetSnapshot>,
}

impl Snapshot {
    /// Whether anything at all was captured.
    pub fn is_empty(&self) -> bool {
        self.inventories.is_empty()
    }

    /// The snapshot as RON, the same format the rest of the workspace writes.
    ///
    /// # Errors
    ///
    /// A serialiser failure, which for these plain types means an allocation
    /// failure.
    pub fn to_ron(&self) -> Result<String, ron::Error> {
        ron::ser::to_string(self)
    }

    /// Reads what [`Snapshot::to_ron`] wrote.
    ///
    /// # Errors
    ///
    /// [`ron::error::SpannedError`] when the text is not a snapshot, which is
    /// what a hand-edited or stale value from the page looks like.
    pub fn from_ron(text: &str) -> Result<Self, ron::error::SpannedError> {
        ron::from_str(text)
    }

    /// The scene this snapshot was taken in, when it names one this build
    /// knows.
    pub fn scene(&self) -> Option<showcase::Scene> {
        self.scene.as_deref().and_then(showcase::Scene::from_id)
    }

    /// Captures `inventories`, naming every stack through `registries`.
    pub fn capture<'a>(
        registries: &FrozenRegistries,
        inventories: impl IntoIterator<Item = &'a slotted_model::Inventory>,
    ) -> Self {
        Self {
            scene: None,
            net: None,
            inventories: inventories
                .into_iter()
                .map(|inventory| capture_inventory(registries, inventory))
                .collect(),
        }
    }

    /// Writes this snapshot's `index`-th inventory into `inventory`, clearing
    /// whatever was there. Returns how many stacks were dropped because the
    /// current registries have no such item.
    ///
    /// An inventory whose length no longer matches is still restored as far as
    /// it goes: a mod that shrank the chest from three rows to two should not
    /// cost the visitor the two rows that still fit.
    pub fn apply_to(
        &self,
        index: usize,
        registries: &FrozenRegistries,
        inventory: &mut slotted_model::Inventory,
    ) -> usize {
        let Some(source) = self.inventories.get(index) else {
            return 0;
        };
        apply_inventory(source, registries, inventory)
    }
}

/// One inventory, with every stack named through `registries`.
///
/// The free function beside the method because the Multiplayer scene captures
/// inventories the open menu does not hold: they live in the server's store,
/// and there is no `Snapshot` to hang them off until they are all in hand.
#[must_use]
pub fn capture_inventory(
    registries: &FrozenRegistries,
    inventory: &slotted_model::Inventory,
) -> InventorySnapshot {
    InventorySnapshot {
        len: inventory.len(),
        slots: (0..inventory.len())
            .filter_map(|index| {
                let stack = inventory.get(index)?;
                let name = registries.items.name_of(stack.id)?;
                Some(Slot {
                    index,
                    item: name.to_string(),
                    count: stack.count,
                })
            })
            .collect(),
    }
}

/// Writes `source` into `inventory`, clearing whatever was there, and answers
/// how many stacks were dropped because the current registries have no such
/// item.
///
/// An inventory whose length no longer matches is still restored as far as it
/// goes: a mod that shrank the chest from three rows to two should not cost
/// the visitor the two rows that still fit.
pub fn apply_inventory(
    source: &InventorySnapshot,
    registries: &FrozenRegistries,
    inventory: &mut slotted_model::Inventory,
) -> usize {
    for slot in 0..inventory.len() {
        inventory.set(slot, None);
    }
    let mut dropped = 0;
    for slot in &source.slots {
        if slot.index >= inventory.len() {
            dropped += 1;
            continue;
        }
        let resolved = Namespaced::parse(&slot.item)
            .ok()
            .and_then(|name| registries.item_id(&name));
        let Some(id) = resolved else {
            dropped += 1;
            continue;
        };
        inventory.set(slot.index, Some(ItemStack::new(id, slot.count)));
    }
    dropped
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn registries() -> FrozenRegistries {
        use slotted_registry::{ItemDef, Registries};
        let mut registries = Registries::new();
        for name in ["demo:apple", "demo:ingot"] {
            registries
                .add_item(ItemDef::new(Namespaced::parse(name).unwrap()))
                .unwrap();
        }
        registries.freeze().unwrap().0
    }

    fn stocked(registries: &FrozenRegistries) -> slotted_model::Inventory {
        let mut inventory = slotted_model::Inventory::new(9);
        let apple = registries
            .item_id(&Namespaced::parse("demo:apple").unwrap())
            .unwrap();
        let ingot = registries
            .item_id(&Namespaced::parse("demo:ingot").unwrap())
            .unwrap();
        inventory.set(0, Some(ItemStack::new(apple, 12)));
        inventory.set(4, Some(ItemStack::new(ingot, 64)));
        inventory
    }

    #[test]
    fn a_snapshot_round_trips_through_ron_and_back_into_an_inventory() {
        let registries = registries();
        let before = stocked(&registries);
        let snapshot = Snapshot::capture(&registries, [&before]);

        let text = snapshot.to_ron().unwrap();
        let read = Snapshot::from_ron(&text).unwrap();
        assert_eq!(read, snapshot);

        let mut after = slotted_model::Inventory::new(9);
        assert_eq!(read.apply_to(0, &registries, &mut after), 0);
        for slot in 0..9 {
            assert_eq!(
                after.get(slot).map(|s| (s.id, s.count)),
                before.get(slot).map(|s| (s.id, s.count)),
                "slot {slot} differs"
            );
        }
    }

    #[test]
    fn restoring_clears_what_was_there_first() {
        let registries = registries();
        let empty = Snapshot::capture(&registries, [&slotted_model::Inventory::new(9)]);
        let mut inventory = stocked(&registries);
        assert_eq!(empty.apply_to(0, &registries, &mut inventory), 0);
        assert!((0..9).all(|slot| inventory.get(slot).is_none()));
    }

    #[test]
    fn a_stack_the_new_registries_do_not_know_is_dropped_not_fatal() {
        let registries = registries();
        let snapshot = Snapshot {
            scene: None,
            net: None,
            inventories: vec![InventorySnapshot {
                len: 9,
                slots: vec![
                    Slot {
                        index: 0,
                        item: "demo:apple".to_owned(),
                        count: 3,
                    },
                    Slot {
                        index: 1,
                        item: "gone:thing".to_owned(),
                        count: 3,
                    },
                    // Past the end: the mod shrank the container.
                    Slot {
                        index: 40,
                        item: "demo:ingot".to_owned(),
                        count: 3,
                    },
                ],
            }],
        };
        let mut inventory = slotted_model::Inventory::new(9);
        assert_eq!(snapshot.apply_to(0, &registries, &mut inventory), 2);
        assert_eq!(inventory.get(0).map(|s| s.count), Some(3));
        assert!(inventory.get(1).is_none());
    }

    #[test]
    fn an_unreadable_snapshot_is_an_error_rather_than_a_panic() {
        assert!(Snapshot::from_ron("not a snapshot").is_err());
        assert!(Snapshot::from_ron("").is_err());
    }
}
