//! The nine showcase scenes, and the two things they all need.
//!
//! `docs/design/showcase-contract.md` section 2. A scene is a set of entities
//! and resources toggled inside the one app: [`crate::showcase`] owns the
//! switch, this module owns what each scene spawns.
//!
//! The two shared pieces are [`teardown`], which is how a `leave` is written
//! once instead of nine times, and [`registries`], which is the frozen
//! registry set every scene fills its inventories against and which does not
//! exist until the mods have loaded.

pub mod chest;
pub mod dialogue;
pub mod hud;
pub mod machine;
pub mod menus;
pub mod mods;
pub mod multiplayer;
pub mod testing;
pub mod themes;

use std::sync::Arc;

use bevy::prelude::*;
use slotted::prelude::*;
use slotted_registry::FrozenRegistries;

use crate::showcase::{Scene, SceneRegistry};

/// Registers every scene. Called by `ShowcasePlugin`.
pub fn register_all(registry: &mut SceneRegistry) {
    registry.register(Scene::Chest, chest::ChestScene);
    registry.register(Scene::Machine, machine::MachineScene);
    registry.register(Scene::Menus, menus::MenusScene);
    registry.register(Scene::Dialogue, dialogue::DialogueScene);
    registry.register(Scene::Themes, themes::ThemesScene);
    registry.register(Scene::Mods, mods::ModsScene);
    registry.register(Scene::Hud, hud::HudScene);
    registry.register(Scene::Multiplayer, multiplayer::MultiplayerScene);
    registry.register(Scene::Testing, testing::TestingScene);
}

/// The frozen registries, when the mods have finished loading.
///
/// Every scene needs them and none of them can do anything sensible without,
/// so they all fail the same way: a line on the page's console and an empty
/// canvas, rather than a panic, which on wasm is a dead tab.
pub fn registries(world: &World) -> Option<Arc<FrozenRegistries>> {
    world.get_resource::<Registries>().map(|r| r.0.clone())
}

/// Despawns every inventory entity no open menu borrows.
///
/// A scene that opens a chest more than once (Menus: Play, Esc, Play) spawns
/// three inventories each time and closing the chest frees none of them; the
/// [`teardown`] catches the leak at the scene's end, this catches it at the
/// next open, so a long visit does not carry a copy of the chest per Play.
pub fn despawn_orphan_inventories(world: &mut World) {
    let borrowed: std::collections::HashSet<Entity> = world
        .query::<&OpenMenu>()
        .iter(world)
        .flat_map(|menu| menu.inventories.iter().copied())
        .collect();
    let orphans: Vec<Entity> = world
        .query_filtered::<Entity, With<slotted::ecs::menu::Inventory>>()
        .iter(world)
        .filter(|entity| !borrowed.contains(entity))
        .collect();
    for inventory in orphans {
        if let Ok(entity) = world.get_entity_mut(inventory) {
            entity.despawn();
        }
    }
}

/// Despawns every screen and every menu, whichever scene made them.
///
/// This is what `leave` means. It is written as "everything on screen" rather
/// than "the entities I remember spawning" on purpose: a scene's `enter` hands
/// entities to `push_screen`, to `open_menu` and to the browser's own
/// attachment pass, and a list of what to undo would be a second copy of that
/// knowledge that goes stale the first time one of them spawns something new.
/// `tests/showcase.rs` asserts the postcondition directly.
///
/// The screens and menus go through the library's own `close_screen` and
/// `close_menu`, so observers watching a screen close still see it, and the
/// stack drops the entry of every root closed this way. The
/// inventory entities are despawned outright: nothing observes those, and they
/// belong to the scene rather than to the menu that borrowed them.
pub fn teardown(world: &mut World) {
    // Before anything is despawned: a live test run holds entities from the
    // screen about to go, and asks the world about them next frame.
    crate::tests::cancel_live_tests(world);

    let screens: Vec<Entity> = world
        .query_filtered::<Entity, With<ScreenRoot>>()
        .iter(world)
        .collect();
    let menus: Vec<(Entity, slotted_model::MenuId)> = world
        .query::<(Entity, &OpenMenu)>()
        .iter(world)
        .map(|(entity, menu)| (entity, menu.id))
        .collect();
    let inventories: Vec<Entity> = world
        .query_filtered::<Entity, With<slotted::ecs::menu::Inventory>>()
        .iter(world)
        .collect();

    {
        let mut commands = world.commands();
        for screen in screens {
            slotted::ui::close_screen(&mut commands, screen);
        }
        for (menu, id) in menus {
            slotted::ecs::close_menu(&mut commands, menu, id);
        }
    }
    world.flush();
    for inventory in inventories {
        if let Ok(entity) = world.get_entity_mut(inventory) {
            entity.despawn();
        }
    }
    world.flush();
}

/// Registers `def` and hands back the shared copy `push_screen` wants.
///
/// Registering on every `enter` rather than once at startup is deliberate: a
/// mod reload replaces the mod-owned screens, and a scene that cached an `Arc`
/// from the first frame would keep opening the tree the mods had before the
/// visitor edited them.
pub fn register_screen(world: &mut World, def: ScreenDef) -> Arc<ScreenDef> {
    world.resource_mut::<Screens>().register(def)
}

/// The screen a mod registered, by kind, when its `data.lua` ran.
pub fn mod_screen(world: &World, kind: &str) -> Option<Arc<ScreenDef>> {
    world
        .get_resource::<Screens>()?
        .get(&ScreenKind::new(kind))
        .cloned()
}
