//! The scene switch: which of the eight showcase scenes the canvas shows.
//!
//! `docs/design/showcase-contract.md` section 1. A scene is a set of entities
//! toggled inside the one app, never a restart. [`SceneRegistry`] holds one
//! [`SceneHandler`] per scene that is real; [`apply_scene_switch`] calls the
//! old one's `leave` and the new one's `enter` in `PreUpdate`, after the bus
//! is drained. Until package A lands the other seven, only [`Scene::Mods`] is
//! registered and it is what `scene::ScenePlugin` already opens at `Startup`,
//! so its hooks do nothing.

use std::collections::HashMap;

use bevy::prelude::*;
pub use showcase::{SCENES, Scene, SceneDef};

use crate::bus::{Bus, quote};

/// The scene the canvas is showing.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActiveScene(pub Scene);

impl Default for ActiveScene {
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
}

/// The scenes that are real. A scene missing here is a stub the page greys
/// out; `Scene::ready` and this map must agree, which `tests/showcase.rs`
/// checks.
#[derive(Resource, Default)]
pub struct SceneRegistry {
    handlers: HashMap<Scene, Box<dyn SceneHandler>>,
}

impl SceneRegistry {
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
}

/// The scene the playground has always shown: `scene::ScenePlugin` opens the
/// copper chest at `Startup`, so there is nothing to enter or leave until a
/// second scene exists and the chest has to come and go with this one.
struct ModsScene;

impl SceneHandler for ModsScene {
    fn enter(&self, _world: &mut World) {}
    fn leave(&self, _world: &mut World) {}
}

/// Registers the real scenes and the switch.
#[derive(Debug, Default, Clone, Copy)]
pub struct ShowcasePlugin;

impl Plugin for ShowcasePlugin {
    fn build(&self, app: &mut App) {
        let mut registry = SceneRegistry::default();
        registry.register(Scene::Mods, ModsScene);
        app.init_resource::<ActiveScene>()
            .insert_resource(registry)
            .add_message::<SwitchScene>()
            .add_systems(PreUpdate, apply_scene_switch.after(crate::drain_requests))
            .add_systems(PostUpdate, publish_scene);
    }
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
    let current = world.resource::<ActiveScene>().0;
    let bus = world.resource::<Bus>().clone();
    if wanted == current {
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
    if let Some(old) = registry.handlers.get(&current) {
        old.leave(world);
    }
    if let Some(new) = registry.handlers.get(&wanted) {
        new.enter(world);
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
/// [{"id":"chest","title":"Chest","caption":"..","tries":["..","..",".."],"ready":false}]
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
}
