//! Scene 1: the moodboard chest with the item browser docked beside it.
//!
//! `docs/design/showcase-contract.md` section 3.1, amended by
//! `docs/design/showcase-refresh-contract.md` section 4.1: the Browser scene
//! folded into this one. It opens `demo:chest` over [`showcase::chest`]'s
//! inventories, which is the same chest `examples/chest` shows in a window
//! and the same one its headless tests drive. The browser attaches to screen
//! kinds a [`slotted::browser::ScreenHandler`] claims; this scene makes the
//! claim and every other scene that opens a `demo:chest` takes it away.

use std::sync::Arc;

use bevy::prelude::*;
use slotted::browser::{DefaultScreenHandler, ScreenHandlers};
use slotted::prelude::*;

use crate::bus::Bus;
use crate::scenes;
use crate::showcase::SceneHandler;

/// Scene 1: the chest with the item and recipe browser docked beside it.
pub struct ChestScene;

impl SceneHandler for ChestScene {
    fn enter(&self, world: &mut World) {
        open(world);
    }

    fn docks_the_item_browser(&self) -> bool {
        true
    }

    fn leave(&self, world: &mut World) {
        scenes::teardown(world);
    }
}

/// Whether `demo:chest` gets a browser panel.
///
/// The contract called this "the attach policy set to deny for that scene".
/// There is no deny in [`slotted::browser::AttachPolicy`]; what
/// there is, and what the policy already means, is `Registered`: a screen kind
/// with no handler gets no panel. Adding a third policy variant to say the same
/// thing would be a library change the contract does not allow and does not
/// need.
///
/// It is called from [`apply_scene_switch`](crate::showcase::apply_scene_switch)
/// for every scene, from that scene's
/// [`docks_the_item_browser`](crate::showcase::SceneHandler::docks_the_item_browser),
/// rather than from the one `enter` that cares. A registration outlives the
/// scene that made it, and other scenes open a `demo:chest` without wanting a
/// panel on it.
pub fn set_browser_attached(world: &mut World, attached: bool) {
    let Some(mut handlers) = world.get_resource_mut::<ScreenHandlers>() else {
        return;
    };
    let kind = ScreenKind::new(showcase::chest::CHEST);
    if attached {
        handlers.register(kind, Arc::new(DefaultScreenHandler));
    } else {
        handlers.remove(&kind);
    }
}

/// Spawns the three inventories, opens the menu and spawns the screen.
pub fn open(world: &mut World) {
    let Some(registries) = scenes::registries(world) else {
        world.resource::<Bus>().log(
            "error",
            "showcase",
            "the chest needs the mods to load first",
        );
        return;
    };
    let def = scenes::register_screen(world, showcase::chest::screen());
    let entities: Vec<Entity> = showcase::chest::inventories(&registries)
        .into_iter()
        .map(|inventory| world.spawn(slotted::ecs::menu::Inventory(inventory)).id())
        .collect();
    showcase::chest::open_over(world, def, entities, false);
}

/// Puts `query` into the item browser's search field.
///
/// It writes the same `SearchChanged` message a category chip writes, which is
/// the browser's own path in: `apply_search` re-evaluates the query and
/// `diff_search_field` writes the text back into the field, so the field shows
/// what was searched and a visitor can carry on typing from it. Nothing here
/// touches the text editor directly.
///
/// Outside the Chest scene this does nothing, deliberately. The page keeps
/// the last scene in its URL and offers the search chips as a control block, so
/// a chip pressed a frame after a switch is an ordinary thing to happen and not
/// worth an error line. There is no browser panel to search when no handler
/// claimed the open screen, and writing the message anyway would leave a query
/// waiting to surprise whoever opens the Chest scene next.
pub fn search(world: &mut World, query: &str) {
    let attached = world
        .get_resource::<ScreenHandlers>()
        .is_some_and(|handlers| handlers.contains(&ScreenKind::new(showcase::chest::CHEST)));
    if !attached {
        world.resource::<Bus>().log(
            "info",
            "showcase",
            format!("no browser is docked; the search for `{query}` went nowhere"),
        );
        return;
    }
    world.write_message(slotted::browser::SearchChanged {
        text: query.to_owned(),
    });
}
