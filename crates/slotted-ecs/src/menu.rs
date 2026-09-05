//! Components for inventories, open menus and slot bindings, and the
//! open/close helpers.

use std::collections::HashMap;
use std::sync::Arc;

use bevy::prelude::*;
use slotted_model::{Actor, ItemStack, MenuDef, MenuId, MenuState, PropertyId, SlotIx};

use crate::events::{MenuClosed, MenuOpened};

/// An inventory living on an entity: a chest, a player, a machine.
///
/// Wraps the model type; `Deref` for reads, `DerefMut` for writes. The
/// prediction systems assemble a `slotted_model::Inventories` from the
/// entities named in [`OpenMenu::inventories`] and write changes back.
#[derive(Component, Debug, Clone, PartialEq, Deref, DerefMut)]
pub struct Inventory(pub slotted_model::Inventory);

impl Inventory {
    /// `len` empty slots.
    pub fn new(len: usize) -> Self {
        Self(slotted_model::Inventory::new(len))
    }
}

/// An open menu. One entity per open container screen, per player.
///
/// `inventories[i]` is the entity whose [`Inventory`] backs
/// `InventoryRef::new(i)` in `def`. Their lengths must match
/// `def.inventory_sizes()` or every action fails with `MenuMismatch`.
#[derive(Component, Debug, Clone)]
pub struct OpenMenu {
    /// The layout: slots, routing, properties.
    pub def: Arc<MenuDef>,
    /// Carried stack, state id, drag, property values.
    pub state: MenuState,
    /// Backing inventory entities in `InventoryRef` order.
    pub inventories: Vec<Entity>,
    /// Id with the authority (vanilla's window id).
    pub id: MenuId,
    /// Permissions of the player driving this menu.
    pub actor: Actor,
}

/// Mirror of `OpenMenu::state.carried`, kept on the menu entity as its own
/// component so the carried-stack layer can query `Changed<Carried>`.
#[derive(Component, Debug, Clone, Default, PartialEq)]
pub struct Carried(pub Option<ItemStack>);

/// One synced property of a menu, on a child entity of the menu. Bars and
/// tanks bind to these by id; `PropertyChanged` targets this entity.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct MenuProperty {
    /// Which property in `MenuDef::properties`.
    pub id: PropertyId,
    /// Current value.
    pub value: i32,
}

/// Marker mirrored onto a slot entity when its inventory slot is favourited.
/// Maintained by `SlottedEcsSet::Reconcile`; the ui crate only reads it.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Favorite;

/// Binds a UI entity to one slot of one open menu.
///
/// This is the only thing the ui crate has to attach for a node to become a
/// slot: `SlotClicked` targets it, `SlotChanged` targets it, `Favorite` is
/// mirrored onto it. Registration into [`SlotEntities`] is automatic.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SlotRef {
    /// The menu entity.
    pub menu: Entity,
    /// Slot index within that menu's `MenuDef`.
    pub slot: SlotIx,
}

/// Reverse index kept on the menu entity: which UI entity draws each slot.
/// Filled by `register_slot_refs` from `Added<SlotRef>`.
#[derive(Component, Debug, Clone, Default)]
pub struct SlotEntities(pub HashMap<SlotIx, Entity>);

/// Hands out [`MenuId`]s for locally opened menus.
#[derive(Resource, Debug, Default)]
pub struct MenuIdAllocator(u32);

impl MenuIdAllocator {
    /// The next unused id.
    pub fn next_id(&mut self) -> MenuId {
        self.0 = self.0.wrapping_add(1);
        MenuId(self.0)
    }
}

/// Spawns a menu entity for `def` over `inventories` and triggers
/// [`MenuOpened`]. Returns the menu entity.
///
/// Property children are spawned with [`MenuProperty`] at the def's initial
/// values. The ui crate observes `MenuOpened` to spawn the screen.
pub fn open_menu(
    commands: &mut Commands,
    ids: &mut MenuIdAllocator,
    def: Arc<MenuDef>,
    inventories: Vec<Entity>,
    actor: Actor,
) -> Entity {
    let id = ids.next_id();
    let state = MenuState::new(&def);
    let properties: Vec<MenuProperty> = def
        .properties
        .iter()
        .map(|p| MenuProperty {
            id: p.id,
            value: p.initial,
        })
        .collect();
    let menu = commands
        .spawn((
            OpenMenu {
                def,
                state,
                inventories,
                id,
                actor,
            },
            Carried::default(),
            SlotEntities::default(),
        ))
        .id();
    for property in properties {
        commands.spawn((property, ChildOf(menu)));
    }
    commands.trigger(MenuOpened { entity: menu, id });
    menu
}

/// Triggers [`MenuClosed`] then despawns the menu entity and its children.
/// Inventory entities are left alone: they belong to the world.
pub fn close_menu(commands: &mut Commands, menu: Entity, id: MenuId) {
    commands.trigger(MenuClosed { entity: menu, id });
    commands.entity(menu).despawn();
}
