//! What `open_screen` needs from a test.

use std::sync::Arc;

use bevy::prelude::{Entity, World};
use slotted_model::{Actor, MenuDef};
use slotted_ui::{ScreenDef, ScreenKind, Screens};

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

/// Where `open_screen` gets its [`ScreenDef`] from: a [`ScreenKind`] already
/// in [`Screens`], or a def handed over directly.
///
/// A def passed by value or reference is registered on the way through, so
/// `by::screen(kind)` and a later `open_screen(kind, ..)` both find it.
pub trait ScreenSource {
    /// Resolve, registering the def if it is not in [`Screens`] yet.
    fn screen_def(self, world: &mut World) -> Arc<ScreenDef>;
}

impl ScreenSource for ScreenKind {
    fn screen_def(self, world: &mut World) -> Arc<ScreenDef> {
        (&self).screen_def(world)
    }
}

impl ScreenSource for &ScreenKind {
    fn screen_def(self, world: &mut World) -> Arc<ScreenDef> {
        let screens = world.resource::<Screens>();
        if let Some(def) = screens.get(self) {
            return def.clone();
        }
        let mut known: Vec<String> = screens.kinds().map(|k| k.0.to_string()).collect();
        known.sort();
        let known = if known.is_empty() {
            "none are registered".to_owned()
        } else {
            known.join(", ")
        };
        panic!(
            "screen {} is not registered in Screens; known: {known}",
            self.0
        )
    }
}

impl ScreenSource for ScreenDef {
    fn screen_def(self, world: &mut World) -> Arc<ScreenDef> {
        world.resource_mut::<Screens>().register(self)
    }
}

impl ScreenSource for &ScreenDef {
    fn screen_def(self, world: &mut World) -> Arc<ScreenDef> {
        self.clone().screen_def(world)
    }
}

impl ScreenSource for Arc<ScreenDef> {
    fn screen_def(self, world: &mut World) -> Arc<ScreenDef> {
        world.resource_mut::<Screens>().register((*self).clone());
        self
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
