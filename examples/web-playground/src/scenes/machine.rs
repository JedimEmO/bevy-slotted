//! Scene 3: the furnace.
//!
//! `docs/design/showcase-contract.md` section 3.2. The screen, the property
//! ids and the simulation are [`showcase::machine`]'s, the same ones
//! `examples/machine` runs in a window. The Sort button on it belongs to the
//! `sorter` mod, which injects into the wildcard `slotted:any` and has never
//! seen this tree.

use bevy::prelude::*;
use slotted::prelude::*;
use slotted_model::Actor;
use slotted_registry::FrozenRegistries;

use crate::bus::Bus;
use crate::scenes;
use crate::showcase::SceneHandler;

/// Scene 3.
pub struct MachineScene;

impl SceneHandler for MachineScene {
    fn enter(&self, world: &mut World) {
        let Some(registries) = scenes::registries(world) else {
            world.resource::<Bus>().log(
                "error",
                "showcase",
                "the machine needs the mods to load first",
            );
            return;
        };
        // The simulation reads these two every frame and the previous visit may
        // have left the redstone signal on.
        world.insert_resource(showcase::machine::Redstone(false));
        world.insert_resource(showcase::machine::MachineSim { paused: false });
        open(world, &registries);
    }

    fn leave(&self, world: &mut World) {
        scenes::teardown(world);
        // Without this the simulation keeps writing properties into a menu that
        // is no longer there, and `track_machine_menu` would only replace it
        // the next time a furnace opens.
        world.remove_resource::<showcase::machine::MachineMenu>();
    }
}

fn open(world: &mut World, registries: &FrozenRegistries) {
    let def = scenes::register_screen(world, showcase::machine::screen());
    let entities: Vec<Entity> = showcase::machine::inventories(registries)
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
        spawn_screen(&mut commands, def, Some(menu));
    }
    world.insert_resource(ids);
    world.flush();
}
