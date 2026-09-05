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

/// A stack that left a menu and should become an entity in the world.
#[derive(Debug, Clone, PartialEq)]
pub struct DroppedStack {
    /// The menu it left.
    pub menu: Entity,
    /// What was dropped.
    pub stack: ItemStack,
}

/// Stacks that left their menus: clicks outside the window, throws, swaps
/// with nowhere to put the displaced stack, and the carried stack of a menu
/// that was closed.
///
/// The game drains this and spawns item entities. Nothing in this crate reads
/// it back.
///
/// The contract fixes `MenuClosed` at `{ entity, id }`, so the stack a close
/// drops is reported here rather than on the event. See
/// `docs/design/phase2-notes-A.md`.
#[derive(Resource, Debug, Default, Clone, PartialEq)]
pub struct Dropped(pub Vec<DroppedStack>);

impl Dropped {
    /// Records `stacks` as having left `menu`.
    pub fn record(&mut self, menu: Entity, stacks: &[ItemStack]) {
        self.0.extend(stacks.iter().map(|stack| DroppedStack {
            menu,
            stack: stack.clone(),
        }));
    }

    /// Takes everything recorded so far.
    pub fn drain(&mut self) -> Vec<DroppedStack> {
        std::mem::take(&mut self.0)
    }

    /// The stacks dropped by one menu, in order.
    pub fn of(&self, menu: Entity) -> impl Iterator<Item = &ItemStack> {
        self.0
            .iter()
            .filter(move |d| d.menu == menu)
            .map(|d| &d.stack)
    }
}

/// Where the player's own inventories live, so a game can open a container
/// menu without repeating the [`MenuDef`] handle convention.
///
/// The fields are named after the [`MenuDef`] constants they back. A missing
/// entity is filled with a freshly spawned empty [`Inventory`] of the size the
/// definition asks for.
#[derive(Resource, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct PlayerInventories {
    /// Backs [`MenuDef::PLAYER_MAIN`].
    pub main: Option<Entity>,
    /// Backs [`MenuDef::PLAYER_HOTBAR`].
    pub hotbar: Option<Entity>,
    /// Backs [`MenuDef::PLAYER_ARMOR`].
    pub armor: Option<Entity>,
    /// Backs [`MenuDef::PLAYER_OFFHAND`].
    pub offhand: Option<Entity>,
    /// Backs [`MenuDef::CRAFT_GRID`].
    pub craft_grid: Option<Entity>,
}

impl PlayerInventories {
    /// The entity backing handle `index`, under the standard convention.
    pub fn get(&self, index: usize) -> Option<Entity> {
        match index {
            1 => self.main,
            2 => self.hotbar,
            3 => self.armor,
            4 => self.offhand,
            5 => self.craft_grid,
            _ => None,
        }
    }
}

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

/// Spawns a menu over `container` and the player's own inventories, filling
/// the [`InventoryRef`](slotted_model::InventoryRef) list `def` needs.
///
/// Handle 0 is `container`; handles 1 to 5 come from [`PlayerInventories`]
/// under the [`MenuDef`] convention. Any handle the definition uses that
/// neither supplies gets a freshly spawned empty [`Inventory`] of the right
/// size, so a menu always opens with a complete, correctly sized backing set.
pub fn open_container_menu(
    commands: &mut Commands,
    ids: &mut MenuIdAllocator,
    def: Arc<MenuDef>,
    container: Entity,
    player: &PlayerInventories,
    actor: Actor,
) -> Entity {
    let sizes = def.inventory_sizes();
    let inventories: Vec<Entity> = sizes
        .iter()
        .enumerate()
        .map(|(index, size)| {
            let known = if index == 0 {
                Some(container)
            } else {
                player.get(index)
            };
            known.unwrap_or_else(|| commands.spawn(Inventory::new(*size)).id())
        })
        .collect();
    open_menu(commands, ids, def, inventories, actor)
}

/// Drops the carried stack into [`Dropped`], triggers [`MenuClosed`], then
/// despawns the menu entity and its children. Inventory entities are left
/// alone: they belong to the world.
pub fn close_menu(commands: &mut Commands, menu: Entity, id: MenuId) {
    commands.queue(CloseMenu { menu, id });
}

/// The command behind [`close_menu`]. It needs `&mut World` to read the
/// menu's carried stack before the entity goes away.
#[derive(Debug, Clone, Copy)]
pub struct CloseMenu {
    /// The menu entity.
    pub menu: Entity,
    /// Its id with the authority.
    pub id: MenuId,
}

impl Command for CloseMenu {
    type Out = ();

    fn apply(self, world: &mut World) {
        let Ok(entity) = world.get_entity_mut(self.menu) else {
            return;
        };
        let carried = entity
            .get::<OpenMenu>()
            .and_then(|open| open.state.carried.clone());
        if let Some(stack) = carried {
            world
                .get_resource_or_init::<Dropped>()
                .record(self.menu, std::slice::from_ref(&stack));
        }
        // A gesture in progress belonged to this menu. `ClickInterpreter` is
        // a resource shared by every menu, so leaving a paint or a pending
        // double click behind would hand it to whichever menu opens next.
        if let Some(mut interpreter) = world.get_resource_mut::<crate::ClickInterpreter>() {
            interpreter.drag = None;
            interpreter.last_click = None;
        }
        world.trigger(MenuClosed {
            entity: self.menu,
            id: self.id,
        });
        world.flush();
        if let Ok(entity) = world.get_entity_mut(self.menu) {
            entity.despawn();
        }
    }
}
