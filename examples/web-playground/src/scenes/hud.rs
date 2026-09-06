//! Scene 6: HUD layers and the position editor.
//!
//! `docs/design/showcase-contract.md` section 3.5. This scene opens no screen,
//! which is the point: the HUD is what a player looks at when no screen is up,
//! and most of the built-in layers hide themselves while one is
//! (`hide_with_screen`).
//!
//! Three things are on show. The nine built-in layers the plugin registers,
//! of which the hotbar is the one that draws something, bound here to a menu
//! that has no screen. A tenth layer registered by a mod, `hud_clock:clock`,
//! whose `data.lua` declared the tree and whose `control.lua` writes the time
//! into it on every `hud_tick` -- no Rust in this crate knows it exists. And
//! the editor: `hud_edit(true)` makes every layer draggable, and where the
//! visitor leaves them goes through the new [`HudLayoutStorage`] port to the
//! page, which keeps it in `localStorage`.
//!
//! [`HudLayoutStorage`]: slotted::ui::hud_editor::HudLayoutStorage

use bevy::prelude::*;
use slotted::prelude::*;
use slotted::ui::hud::{HudHotbar, HudMenu};
use slotted::ui::hud_editor::{HudEditMode, HudLayoutStore};
use slotted_model::Actor;

use crate::bus::Bus;
use crate::scenes;
use crate::showcase::SceneHandler;

/// Scene 6.
pub struct HudScene;

impl SceneHandler for HudScene {
    fn enter(&self, world: &mut World) {
        let Some(registries) = scenes::registries(world) else {
            world.resource::<Bus>().log(
                "error",
                "showcase",
                "the HUD needs the mods to load first",
            );
            return;
        };

        // A menu with inventories and no screen: the hotbar layer draws the
        // player's row out of it, which is what a HUD hotbar is.
        let entities: Vec<Entity> = showcase::chest::inventories(&registries)
            .into_iter()
            .map(|inventory| world.spawn(slotted::ecs::menu::Inventory(inventory)).id())
            .collect();
        let mut ids = world
            .remove_resource::<slotted::ecs::MenuIdAllocator>()
            .unwrap_or_default();
        let menu = {
            let mut commands = world.commands();
            open_menu(
                &mut commands,
                &mut ids,
                showcase::chest::menu_def(),
                entities,
                Actor::SURVIVAL,
            )
        };
        world.insert_resource(ids);
        world.flush();

        world.insert_resource(HudMenu(Some(menu)));
        world.insert_resource(HudHotbar {
            first: showcase::chest::HOTBAR_FIRST,
        });
        // Whatever the page handed back from `localStorage` at boot, or a drag
        // from an earlier visit to this scene.
        apply_stored_layout(world);
    }

    fn leave(&self, world: &mut World) {
        // Leaving the scene leaves edit mode: a visitor who switches away
        // mid-drag should not come back to a canvas where every click moves a
        // layer.
        world.insert_resource(HudEditMode(false));
        world.insert_resource(HudMenu(None));
        scenes::teardown(world);
    }
}

/// Copies the persisted layout into [`HudLayout`](slotted::ui::hud::HudLayout),
/// if there is one.
///
/// `load_hud_layout` does this at `Startup`, which in a browser tab is before
/// the page has had a chance to hand its `localStorage` value back. Doing it
/// again on `enter` is what makes "reload the page and find them where you
/// left them" true.
pub fn apply_stored_layout(world: &mut World) {
    let Some(stored) = world
        .get_resource::<HudLayoutStore>()
        .and_then(|store| store.0.load())
    else {
        return;
    };
    world.insert_resource(stored);
}

/// Forgets the stored layout and puts every layer back where its definition
/// asked for it: the HUD scene's Reset button.
///
/// Both halves are needed. Clearing [`HudLayout`](slotted::ui::hud::HudLayout)
/// is what moves the layers,
/// because `sync_hud_layers` reads a def's own anchor whenever the layout has
/// no override for it. Clearing the store is what stops the next `enter` from
/// putting the old positions straight back, and it is what the page reads
/// through `hud_layout()` a second later to empty its `localStorage` key.
pub fn reset_layout(world: &mut World) {
    if let Some(store) = world.get_resource::<HudLayoutStore>() {
        store.0.save(&slotted::ui::hud::HudLayout::default());
    }
    world.insert_resource(slotted::ui::hud::HudLayout::default());
    world.resource::<Bus>().log(
        "info",
        "showcase",
        "the HUD layout is back to the layers' own anchors",
    );
}

/// The RON the page keeps in `localStorage`, or an empty string when the
/// visitor has not moved anything.
///
/// It reads the world's [`HudLayoutStore`] rather than
/// [`crate::hud_store::global`] directly. In the running app those are the same
/// object -- `build_app` inserts the global as the adapter -- but a test builds
/// its own world, and two tests sharing one process-wide store would fight over
/// it. The `wasm-bindgen` export has no world to read and uses the global; that
/// is the one place it is named.
///
/// # Errors
///
/// A layout that will not serialise, which for a map of names to numbers means
/// something is very wrong.
pub fn layout_ron(world: &World) -> Result<String, String> {
    let Some(store) = world.get_resource::<HudLayoutStore>() else {
        return Ok(String::new());
    };
    match store.0.load() {
        // A layout with no overrides is nothing worth keeping, and the page
        // reads the empty string as "clear the key".
        Some(layout) if layout.anchors.is_empty() => Ok(String::new()),
        Some(layout) => ron::ser::to_string(&layout)
            .map_err(|e| format!("the HUD layout did not serialise: {e}")),
        None => Ok(String::new()),
    }
}

/// Puts a [`layout_ron`] value back into the store and into the live layout.
///
/// An empty `text` is [`reset_layout`], not a parse failure. The page asks for
/// both through the one `restore_hud_layout` export -- it hands back whatever
/// `localStorage` held, which is the empty string when the visitor has never
/// moved a layer or has just pressed Reset -- and the two meanings agree:
/// "nothing stored" and "put them back where they belong" are the same
/// instruction. Deciding it here rather than in the caller keeps it beside the
/// parse it is an alternative to.
///
/// # Errors
///
/// `text` is neither empty nor a `slotted_ui::HudLayout`.
pub fn restore_layout_ron(world: &mut World, text: &str) -> Result<(), String> {
    if text.trim().is_empty() {
        reset_layout(world);
        return Ok(());
    }
    let layout: slotted::ui::hud::HudLayout =
        ron::from_str(text).map_err(|e| format!("the stored HUD layout is not readable: {e}"))?;
    if let Some(store) = world.get_resource::<HudLayoutStore>() {
        store.0.save(&layout);
    }
    world.insert_resource(layout);
    Ok(())
}
