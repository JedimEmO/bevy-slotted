//! The scene switch: which of the eight showcase scenes the canvas shows.
//!
//! `docs/design/showcase-contract.md` section 1. A scene is a set of entities
//! toggled inside the one app, never a restart. [`SceneRegistry`] holds one
//! [`SceneHandler`] per scene; [`apply_scene_switch`] calls the old one's
//! `leave` and the new one's `enter` in `PreUpdate`, after the bus is drained.
//! The 3D backdrop is spawned once by `scene::ScenePlugin` and shared, so the
//! orbit carries on across a switch and the page visibly did not reload.
//!
//! There are two "current scenes" because one entry in the rail is not a scene
//! on the canvas. [`ActiveScene`] is what the rail highlights; [`CanvasScene`]
//! is what is actually spawned. Themes is the difference: selecting it swaps
//! the controls column and leaves whatever screen was open exactly where it
//! was, which is the only way "one screen tree, three skins" is worth looking
//! at. Every other scene moves both.

use std::collections::HashMap;

use bevy::prelude::*;
pub use showcase::{SCENES, Scene, SceneDef};
use slotted::browser::BrowserPhase;

use crate::bus::{Bus, quote};

/// The scene the rail highlights, and the one the page's controls follow.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActiveScene(pub Scene);

impl Default for ActiveScene {
    fn default() -> Self {
        Self(Scene::DEFAULT)
    }
}

/// The scene whose entities are on the canvas.
///
/// The same as [`ActiveScene`] except while Themes is selected, when it is
/// whatever was open before.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub struct CanvasScene(pub Scene);

impl Default for CanvasScene {
    fn default() -> Self {
        Self(Scene::DEFAULT)
    }
}

/// The world asked to show another scene. Written by `drain_requests` and
/// by a native test; read by [`apply_scene_switch`].
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct SwitchScene(pub Scene);

/// What one scene spawns and despawns. Every method takes the whole world
/// because a scene opens menus, screens and resources together.
pub trait SceneHandler: Send + Sync + 'static {
    /// Spawn the scene's entities. The backdrop is already there.
    fn enter(&self, world: &mut World);

    /// Despawn everything `enter` made. Nothing of the scene may survive.
    fn leave(&self, world: &mut World);

    /// Whether the item browser docks beside this scene's `demo:chest`.
    ///
    /// True for exactly one scene, Browser. It is asked on every entry rather
    /// than left to the scene's own `enter`, because the answer is a
    /// registration in a resource that outlives the scene that made it: the
    /// Browser scene used to register the `demo:chest` handler and nothing
    /// took it away again, so the Multiplayer scene, which opens two
    /// `demo:chest` screens of its own, docked a panel over each of them for
    /// anyone who had visited Browser first.
    fn docks_the_item_browser(&self) -> bool {
        false
    }

    /// Whether selecting this scene leaves the canvas alone.
    ///
    /// True for exactly one scene, Themes, and the reason is in this module's
    /// documentation. A handler that says true has its `enter` and `leave`
    /// called for the rail's sake and must not spawn anything.
    fn overlays_current(&self) -> bool {
        false
    }
}

/// The scenes that are real. A scene missing here is a stub the page greys
/// out; `Scene::ready` and this map must agree, which `tests/showcase.rs`
/// checks.
#[derive(Resource, Default)]
pub struct SceneRegistry {
    handlers: HashMap<Scene, Box<dyn SceneHandler>>,
}

impl SceneRegistry {
    /// Every scene the playground can switch to.
    pub fn with_all_scenes() -> Self {
        let mut registry = Self::default();
        crate::scenes::register_all(&mut registry);
        registry
    }

    /// Registers `handler` for `scene`, replacing any earlier one.
    pub fn register(&mut self, scene: Scene, handler: impl SceneHandler) {
        self.handlers.insert(scene, Box::new(handler));
    }

    /// Whether `scene` has a handler.
    pub fn has(&self, scene: Scene) -> bool {
        self.handlers.contains_key(&scene)
    }

    /// Every registered scene, in rail order.
    pub fn registered(&self) -> Vec<Scene> {
        Scene::ALL
            .into_iter()
            .filter(|scene| self.has(*scene))
            .collect()
    }

    fn overlays(&self, scene: Scene) -> bool {
        self.handlers
            .get(&scene)
            .is_some_and(|handler| handler.overlays_current())
    }
}

/// Registers the real scenes and the switch.
#[derive(Debug, Default, Clone, Copy)]
pub struct ShowcasePlugin;

impl Plugin for ShowcasePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ActiveScene>()
            .init_resource::<CanvasScene>()
            .insert_resource(SceneRegistry::with_all_scenes())
            .add_message::<SwitchScene>()
            // After the browser's own `Startup` registrations. Both this and
            // `showcase::chest::register_browser_handler` run in `Startup`,
            // and with nothing between them Bevy is free to run them in
            // either order. In the wrong one the Chest scene removes the
            // `demo:chest` handler and the browser plugin puts it straight
            // back, so scene 1 boots with the panel scene 2 is about docked
            // beside it. The native tests never saw it because they switch
            // scenes after startup; a browser tab lands on it every time.
            .add_systems(
                Startup,
                enter_first_scene.after(BrowserPhase::ScreenHandlers),
            )
            .add_systems(PreUpdate, apply_scene_switch.after(crate::drain_requests))
            .add_systems(
                PostUpdate,
                (
                    publish_scene,
                    crate::scenes::multiplayer::pump_link,
                    crate::scenes::multiplayer::fit_client_screens,
                    crate::scenes::testing::advance_replay,
                ),
            );
    }
}

/// `Startup`, after the mods have loaded: opens [`Scene::DEFAULT`].
///
/// The mods load in `PreStartup` (`SlottedPacksPlugin::initial_load`), so by
/// the time this runs the registries and the mod-registered screens are there,
/// which is what every scene's `enter` needs.
///
/// It is ordered after [`BrowserPhase::ScreenHandlers`] as well, because a
/// scene's `enter` may take a screen handler away and the browser plugin
/// registers them in the same schedule.
pub fn enter_first_scene(world: &mut World) {
    let wanted = world.resource::<ActiveScene>().0;
    let registry = world.remove_resource::<SceneRegistry>().unwrap_or_default();
    if let Some(handler) = registry.handlers.get(&wanted) {
        crate::scenes::chest::set_browser_attached(world, handler.docks_the_item_browser());
        handler.enter(world);
    }
    world.insert_resource(registry);
}

/// `PreUpdate`, exclusive: the last `SwitchScene` of the frame wins. A scene
/// with no handler is refused on the console and the current one stays.
pub fn apply_scene_switch(world: &mut World) {
    let Some(mut messages) = world.get_resource_mut::<Messages<SwitchScene>>() else {
        return;
    };
    let Some(SwitchScene(wanted)) = messages.drain().last() else {
        return;
    };
    let active = world.resource::<ActiveScene>().0;
    let bus = world.resource::<Bus>().clone();
    if wanted == active {
        return;
    }
    if !world.resource::<SceneRegistry>().has(wanted) {
        bus.log(
            "warn",
            "showcase",
            format!("not yet: the {} scene is a stub", wanted.def().title),
        );
        return;
    }
    // The handlers are taken out of the registry for the call: `enter` and
    // `leave` want `&mut World`, and the registry lives in it.
    let registry = world.remove_resource::<SceneRegistry>().unwrap_or_default();
    let overlay = registry.overlays(wanted);
    if !overlay {
        let canvas = world.resource::<CanvasScene>().0;
        // Leaving Themes is leaving nothing: it never entered the canvas, so
        // whatever it was shown over is still there and stays.
        if !registry.overlays(canvas)
            && let Some(old) = registry.handlers.get(&canvas)
        {
            old.leave(world);
        }
        if let Some(new) = registry.handlers.get(&wanted) {
            crate::scenes::chest::set_browser_attached(world, new.docks_the_item_browser());
            new.enter(world);
        }
        world.resource_mut::<CanvasScene>().0 = wanted;
    }
    world.insert_resource(registry);
    world.resource_mut::<ActiveScene>().0 = wanted;
    bus.log(
        "info",
        "showcase",
        format!("showing the {} scene", wanted.def().title),
    );
}

/// `PostUpdate`: the active scene onto the bus, for `current_scene`.
fn publish_scene(bus: Res<Bus>, active: Res<ActiveScene>) {
    if active.is_changed() {
        bus.set_scene(active.0);
    }
}

/// The scene table as the JSON the rail renders.
///
/// ```json
/// [{"id":"chest","title":"Chest","caption":"..","tries":["..","..",".."],"ready":true}]
/// ```
///
/// Here rather than in `bridge` so a native test can assert the shape the
/// page parses; the export is one line over it.
pub fn scenes_json() -> String {
    let scenes: Vec<String> = SCENES
        .iter()
        .map(|def| {
            let tries: Vec<String> = def.tries.iter().map(|t| quote(t)).collect();
            format!(
                "{{\"id\":{},\"title\":{},\"caption\":{},\"tries\":[{}],\"ready\":{}}}",
                quote(def.scene.id()),
                quote(def.title),
                quote(def.caption),
                tries.join(","),
                def.ready
            )
        })
        .collect();
    format!("[{}]", scenes.join(","))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_json_lists_every_scene_in_rail_order() {
        let json = scenes_json();
        let ids: Vec<&str> = Scene::ALL.iter().map(|s| s.id()).collect();
        let mut at = 0;
        for id in ids {
            let needle = format!("\"id\":\"{id}\"");
            let found = json[at..].find(&needle).expect("every scene is listed");
            at += found;
        }
        assert!(json.contains("\"ready\":true"));
    }

    #[test]
    fn every_scene_in_the_table_has_a_handler() {
        let registry = SceneRegistry::with_all_scenes();
        assert_eq!(registry.registered(), Scene::ALL);
    }

    #[test]
    fn themes_is_the_only_scene_that_leaves_the_canvas_alone() {
        let registry = SceneRegistry::with_all_scenes();
        let overlays: Vec<Scene> = Scene::ALL
            .into_iter()
            .filter(|scene| registry.overlays(*scene))
            .collect();
        assert_eq!(overlays, [Scene::Themes]);
    }
}
