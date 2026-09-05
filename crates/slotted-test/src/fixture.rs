//! What `open_screen` needs from a test.

use std::sync::Arc;

use bevy::prelude::Entity;
use slotted_model::{Actor, MenuDef};

/// A menu plus the inventories it opens over. Games implement this on their
/// own fixture types (`ChestFixture::filled()`).
pub trait MenuFixture {
    /// The layout.
    fn def(&self) -> Arc<MenuDef>;
    /// Inventories in `InventoryRef` order, lengths matching
    /// `def().inventory_sizes()`.
    fn inventories(&self) -> Vec<slotted_model::Inventory>;
    /// Permissions of the test player.
    fn actor(&self) -> Actor {
        Actor::SURVIVAL
    }
}

impl MenuFixture for (Arc<MenuDef>, Vec<slotted_model::Inventory>) {
    fn def(&self) -> Arc<MenuDef> {
        self.0.clone()
    }
    fn inventories(&self) -> Vec<slotted_model::Inventory> {
        self.1.clone()
    }
}

impl MenuFixture for MenuDef {
    fn def(&self) -> Arc<MenuDef> {
        Arc::new(self.clone())
    }
    fn inventories(&self) -> Vec<slotted_model::Inventory> {
        self.inventory_sizes()
            .into_iter()
            .map(slotted_model::Inventory::new)
            .collect()
    }
}

/// What `open_screen` returns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Opened {
    /// The `OpenMenu` entity.
    pub menu: Entity,
    /// The screen root entity.
    pub screen: Entity,
    /// Inventory entities in `InventoryRef` order.
    pub inventories: Vec<Entity>,
}
