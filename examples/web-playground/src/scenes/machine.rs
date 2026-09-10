//! Scene 2: the furnace.
//!
//! `docs/design/showcase-contract.md` section 3.2. The screen, the property
//! ids and the simulation are [`showcase::machine`]'s, the same ones
//! `examples/machine` runs in a window. The Sort button on it belongs to the
//! `sorter` mod, which injects into the wildcard `slotted:any` and has never
//! seen this tree.
//!
//! [`open`] and [`close`] are shared with the Dialogue scene, which is the
//! smith's conversation over this furnace, still cooking.

use bevy::prelude::*;
use slotted::prelude::*;
use slotted_model::Actor;

use crate::bus::Bus;
use crate::scenes;
use crate::showcase::SceneHandler;

/// Scene 2.
pub struct MachineScene;

impl SceneHandler for MachineScene {
    fn enter(&self, world: &mut World) {
        open(world);
    }

    fn leave(&self, world: &mut World) {
        close(world);
    }
}

/// Opens the furnace with the simulation running, or says on the console
/// why it could not.
///
/// Returns whether it did: the Dialogue scene starts a conversation over the
/// furnace and has nothing to say over an empty canvas.
pub fn open(world: &mut World) -> bool {
    let Some(registries) = scenes::registries(world) else {
        world.resource::<Bus>().log(
            "error",
            "showcase",
            "the machine needs the mods to load first",
        );
        return false;
    };
    // The simulation reads these two every frame and the previous visit may
    // have left the redstone signal on.
    world.insert_resource(showcase::machine::Redstone(false));
    world.insert_resource(showcase::machine::MachineSim { paused: false });

    let def = scenes::register_screen(world, showcase::machine::screen());
    let entities: Vec<Entity> = showcase::machine::inventories(&registries)
        .into_iter()
        .map(|inventory| world.spawn(slotted::ecs::menu::Inventory(inventory)).id())
        .collect();
    let mut ids = world
        .remove_resource::<slotted::ecs::MenuIdAllocator>()
        .unwrap_or_default();
    {
        let mut commands = world.commands();
        let menu = open_menu(
            &mut commands,
            &mut ids,
            showcase::machine::menu_def(),
            entities,
            Actor::SURVIVAL,
        );
        push_screen(&mut commands, def, Some(menu));
    }
    world.insert_resource(ids);
    world.flush();
    true
}

/// The teardown, plus the one resource the simulation would otherwise keep.
pub fn close(world: &mut World) {
    scenes::teardown(world);
    // Without this the simulation keeps writing properties into a menu that
    // is no longer there, and `track_machine_menu` would only replace it
    // the next time a furnace opens.
    world.remove_resource::<showcase::machine::MachineMenu>();
}
