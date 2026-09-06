//! [`ContainerStore`]: the inventories the server owns, and who may see them.
//!
//! A menu on the server is a *session*, and a session does not own its items.
//! It binds one [`InventoryId`] per [`InventoryRef`](slotted_model::InventoryRef) its
//! [`MenuDef`](slotted_model::MenuDef) addresses, and the items live here. Two
//! players looking into one chest have two sessions that bind the same
//! container id, so an edit through either lands in one place; each also binds
//! its own player inventories, which are marked private and cannot be bound
//! into anybody else's session at all.
//!
//! That split is the whole point. Sharing an inventory is a decision made once,
//! when a session opens, and after that it is impossible to bind the wrong
//! one: the store refuses.

use std::collections::BTreeMap;

use slotted_model::{Inventory, InventoryId, PropertyId};

use crate::message::PeerId;

/// Who is allowed to bind an inventory into a session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Owner {
    /// Anyone the [`Access`](crate::Access) port allows: a chest, a furnace,
    /// a shared crate.
    Shared,
    /// One player, and nobody else, whatever the access policy says. A
    /// player's main inventory, hotbar, armour and offhand.
    Private(PeerId),
}

impl Owner {
    /// `true` when `peer` may bind an inventory with this owner.
    pub fn allows(self, peer: PeerId) -> bool {
        match self {
            Self::Shared => true,
            Self::Private(owner) => owner == peer,
        }
    }
}

/// One inventory as the server holds it: the items, who may bind it, and the
/// synced properties of whatever it is part of.
#[derive(Debug, Clone)]
pub struct Container {
    /// The items.
    pub inventory: Inventory,
    /// Who may bind it into a session.
    pub owner: Owner,
    /// Current values of the synced properties belonging to this container,
    /// so a session opened later starts at the furnace's real burn time
    /// rather than at the definition's initial value.
    pub properties: BTreeMap<PropertyId, i32>,
}

/// Every inventory the server owns, keyed by [`InventoryId`].
///
/// The store hands out ids and never reuses one, so a stale id names nothing
/// rather than naming somebody else's chest.
#[derive(Debug, Default)]
pub struct ContainerStore {
    containers: BTreeMap<InventoryId, Container>,
    next: u64,
}

impl ContainerStore {
    /// An empty store.
    pub fn new() -> Self {
        Self::default()
    }

    fn allocate(&mut self) -> InventoryId {
        self.next += 1;
        InventoryId(self.next)
    }

    /// Adds a shared inventory: a chest, a machine, anything more than one
    /// player may open at once.
    pub fn add_shared(&mut self, inventory: Inventory) -> InventoryId {
        self.add(inventory, Owner::Shared)
    }

    /// Adds an inventory private to `peer`. It can only ever be bound into
    /// that peer's own sessions.
    pub fn add_private(&mut self, peer: PeerId, inventory: Inventory) -> InventoryId {
        self.add(inventory, Owner::Private(peer))
    }

    /// Adds an inventory with an explicit owner.
    pub fn add(&mut self, inventory: Inventory, owner: Owner) -> InventoryId {
        let id = self.allocate();
        self.containers.insert(
            id,
            Container {
                inventory,
                owner,
                properties: BTreeMap::new(),
            },
        );
        id
    }

    /// Drops an inventory. Sessions still bound to it keep working on the
    /// inventories they can still reach; every click through one of them
    /// fails, so a caller should close them first.
    pub fn remove(&mut self, id: InventoryId) -> Option<Container> {
        self.containers.remove(&id)
    }

    /// The container behind `id`.
    pub fn get(&self, id: InventoryId) -> Option<&Container> {
        self.containers.get(&id)
    }

    /// The container behind `id`, mutably.
    pub fn get_mut(&mut self, id: InventoryId) -> Option<&mut Container> {
        self.containers.get_mut(&id)
    }

    /// The items behind `id`.
    pub fn inventory(&self, id: InventoryId) -> Option<&Inventory> {
        self.containers.get(&id).map(|c| &c.inventory)
    }

    /// The items behind `id`, mutably. A caller writing here is responsible
    /// for telling the sessions; [`MenuServer::flush`](crate::MenuServer::flush)
    /// does that off the dirty mask.
    pub fn inventory_mut(&mut self, id: InventoryId) -> Option<&mut Inventory> {
        self.containers.get_mut(&id).map(|c| &mut c.inventory)
    }

    /// Who may bind `id`.
    pub fn owner(&self, id: InventoryId) -> Option<Owner> {
        self.containers.get(&id).map(|c| c.owner)
    }

    /// Every id in the store, ascending.
    pub fn ids(&self) -> impl Iterator<Item = InventoryId> + '_ {
        self.containers.keys().copied()
    }

    /// Every container with its id, ascending.
    pub fn iter(&self) -> impl Iterator<Item = (InventoryId, &Container)> {
        self.containers.iter().map(|(id, c)| (*id, c))
    }

    /// Forgets the recorded changes on every container. The server calls this
    /// after it has told the sessions.
    pub fn clear_changed(&mut self) {
        for container in self.containers.values_mut() {
            container.inventory.clear_changed();
        }
    }

    /// Number of containers.
    pub fn len(&self) -> usize {
        self.containers.len()
    }

    /// `true` when the store holds nothing.
    pub fn is_empty(&self) -> bool {
        self.containers.is_empty()
    }

    /// Drops every inventory private to `peer`. Called when a player leaves.
    pub fn remove_private(&mut self, peer: PeerId) {
        self.containers
            .retain(|_, c| c.owner != Owner::Private(peer));
    }
}

#[allow(clippy::unwrap_used)]
#[cfg(test)]
mod tests {
    use super::*;
    use slotted_model::{ItemId, ItemStack};

    fn stone(count: u32) -> ItemStack {
        ItemStack::new(ItemId(1), count)
    }

    #[test]
    fn ids_are_never_reused() {
        let mut store = ContainerStore::new();
        let first = store.add_shared(Inventory::new(4));
        store.remove(first);
        let second = store.add_shared(Inventory::new(4));
        assert_ne!(first, second, "a stale id must not name somebody else");
        assert!(store.get(first).is_none());
    }

    #[test]
    fn only_the_owner_may_bind_a_private_inventory() {
        let alice = PeerId(1);
        let bob = PeerId(2);
        let mut store = ContainerStore::new();
        let pockets = store.add_private(alice, Inventory::new(4));
        let chest = store.add_shared(Inventory::new(4));

        assert!(store.owner(pockets).unwrap().allows(alice));
        assert!(!store.owner(pockets).unwrap().allows(bob));
        assert!(store.owner(chest).unwrap().allows(bob));
    }

    #[test]
    fn dropping_a_peer_takes_its_pockets_and_leaves_the_chest() {
        let alice = PeerId(1);
        let mut store = ContainerStore::new();
        let chest = store.add_shared(Inventory::new(4));
        store.add_private(alice, Inventory::new(4));
        assert_eq!(store.len(), 2);

        store.remove_private(alice);

        assert_eq!(store.len(), 1);
        assert!(store.get(chest).is_some());
    }

    #[test]
    fn a_write_marks_the_cell_and_clear_changed_forgets_it() {
        let mut store = ContainerStore::new();
        let chest = store.add_shared(Inventory::new(4));
        store.inventory_mut(chest).unwrap().set(2, Some(stone(7)));

        let dirty: Vec<usize> = store.inventory(chest).unwrap().changed().iter().collect();
        assert_eq!(dirty, vec![2]);

        store.clear_changed();
        assert!(!store.inventory(chest).unwrap().changed().any());
        assert_eq!(store.inventory(chest).unwrap().get(2), Some(&stone(7)));
    }
}
