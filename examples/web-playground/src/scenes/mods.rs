//! Scene 5: the copper chest the Lua mods build. Today's playground.
//!
//! `docs/design/showcase-contract.md` section 3.4: unchanged behaviour. The
//! editor, the Tests tab, hot reload and the restart-with-snapshot path are
//! all page and bus machinery that never knew which scene was up, so the only
//! thing this scene does is open the screen `copper_chest/data.lua` registered
//! and seed it from [`showcase::mods`].

use std::sync::Arc;

use bevy::prelude::*;
use slotted::browser::{DefaultScreenHandler, ScreenHandlers};
use slotted::prelude::*;
use slotted_model::Actor;

use crate::bus::Bus;
use crate::scenes;
use crate::showcase::SceneHandler;

/// Scene 5.
pub struct ModsScene;

impl SceneHandler for ModsScene {
    fn enter(&self, world: &mut World) {
        let kind = ScreenKind::new(showcase::mods::CHEST_SCREEN);
        if let Some(mut handlers) = world.get_resource_mut::<ScreenHandlers>() {
            handlers.register(kind.clone(), Arc::new(DefaultScreenHandler));
        }
        let bus = world.resource::<Bus>().clone();
        let Some(registries) = scenes::registries(world) else {
            bus.log("error", "showcase", "the mods have not loaded yet");
            return;
        };
        // The screen is the mod's, not this crate's: `data.lua` registered it
        // and a reload can replace it, so it is looked up rather than parsed.
        let Some(def) = scenes::mod_screen(world, showcase::mods::CHEST_SCREEN) else {
            bus.log(
                "error",
                "showcase",
                format!(
                    "{} is not registered: did the bundled data.lua run?",
                    showcase::mods::CHEST_SCREEN
                ),
            );
            return;
        };
        let entities: Vec<Entity> = showcase::mods::inventories(&registries)
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
                showcase::mods::menu_def(),
                entities,
                Actor::SURVIVAL,
            );
            spawn_screen(&mut commands, def, Some(menu));
        }
        world.insert_resource(ids);
        world.flush();
    }

    fn leave(&self, world: &mut World) {
        scenes::teardown(world);
    }
}
