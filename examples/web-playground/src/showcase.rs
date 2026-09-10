//! The scene switch: which of the nine showcase scenes the canvas shows.
//!
//! `docs/design/showcase-contract.md` section 1, amended by
//! `docs/design/showcase-refresh-contract.md`. A scene is a set of entities
//! toggled inside the one app, never a restart. [`SceneRegistry`] holds one
//! [`SceneHandler`] per scene; [`apply_scene_switch`] calls the old one's
//! `leave` and the new one's `enter` in `PreUpdate`, after the bus is drained.
//! The 3D backdrop is spawned once by `scene::ScenePlugin` and shared, so the
//! orbit carries on across a switch and the page visibly did not reload.
//!
//! Every scene is on the canvas. Themes used to be the exception, an entry
//! that swapped the controls column and left whatever was open alone; the
//! refresh gave it a settings screen of its own to repaint, so [`ActiveScene`]
//! is the one current scene and the overlay path went with the second one.
//!
//! Below the switch are the native twins of the refresh's five exports
//! ([`menu_open`], [`settings_ron`], [`restore_settings`], [`talk_again`],
//! [`set_value`]): the validation and the request, over any [`Bus`], so a
//! test drives what `bridge` wraps in one line each.

use std::collections::HashMap;

use bevy::prelude::*;
pub use showcase::{SCENES, Scene, SceneDef};
use slotted::browser::BrowserPhase;
use slotted::ui::Value;

use crate::bus::{Bus, Request, quote};

/// The scene the canvas shows, the rail highlights and the page's controls
/// follow.
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

    /// Whether the item browser docks beside this scene's `demo:chest`.
    ///
    /// True for exactly one scene, Chest. It is asked on every entry rather
    /// than left to the scene's own `enter`, because the answer is a
    /// registration in a resource that outlives the scene that made it: the
    /// old Browser scene used to register the `demo:chest` handler and
    /// nothing took it away again, so the Multiplayer scene, which opens two
    /// `demo:chest` screens of its own, docked a panel over each of them for
    /// anyone who had visited Browser first.
    fn docks_the_item_browser(&self) -> bool {
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
}

/// Registers the real scenes and the switch.
#[derive(Debug, Default, Clone, Copy)]
pub struct ShowcasePlugin;

impl Plugin for ShowcasePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ActiveScene>()
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
                    publish_screen,
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
    if let Some(old) = registry.handlers.get(&active) {
        old.leave(world);
    }
    if let Some(new) = registry.handlers.get(&wanted) {
        crate::scenes::chest::set_browser_attached(world, new.docks_the_item_browser());
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

/// `PostUpdate`: the top of the screen stack onto the bus, for
/// `current_screen`. The topmost entry of any kind, overlays included, so a
/// running dialogue reads as `slotted:dialogue` over the furnace; the empty
/// string when nothing is open.
pub fn publish_screen(bus: Res<Bus>, stack: Option<Res<slotted::ui::ScreenStack>>) {
    let Some(stack) = stack else { return };
    if !stack.is_changed() {
        return;
    }
    bus.set_screen(current_screen_of(&stack));
}

/// The kind on top of `stack`, overlays included, or an empty string.
pub fn current_screen_of(stack: &slotted::ui::ScreenStack) -> String {
    stack
        .top_any()
        .map(|entry| entry.kind.0.to_string())
        .unwrap_or_default()
}

/// `current_screen()`: the kind of the screen on top of the stack, as the
/// world last published it.
pub fn current_screen(bus: &Bus) -> String {
    bus.screen()
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

// ---------------------------------------------------------------------------
// The refresh's exports, natively (docs/design/showcase-refresh-contract.md
// section 5)
// ---------------------------------------------------------------------------

/// One of the Menus scene's screens, by the name the page uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MenuScreen {
    /// `showcase:main`.
    Title,
    /// `slotted:pause`.
    Pause,
    /// `demo:settings`.
    Settings,
}

impl MenuScreen {
    /// The three, in the order the page's buttons show them.
    pub const ALL: [MenuScreen; 3] = [MenuScreen::Title, MenuScreen::Pause, MenuScreen::Settings];

    /// The name the page passes to `menu_open`.
    pub const fn id(self) -> &'static str {
        match self {
            MenuScreen::Title => "title",
            MenuScreen::Pause => "pause",
            MenuScreen::Settings => "settings",
        }
    }

    /// The screen with this name, if there is one.
    pub fn from_id(id: &str) -> Option<MenuScreen> {
        MenuScreen::ALL.into_iter().find(|which| which.id() == id)
    }
}

/// `menu_open(which)`: pushes the named menu screen. Menus scene only; the
/// world says so on the console when another is open.
///
/// # Errors
///
/// A name that is not `title`, `pause` or `settings`.
pub fn menu_open(bus: &Bus, which: &str) -> Result<(), String> {
    let which = MenuScreen::from_id(which).ok_or_else(|| {
        format!(
            "`{which}` is not a menu screen; the page has {}",
            MenuScreen::ALL
                .iter()
                .map(|w| w.id())
                .collect::<Vec<_>>()
                .join(", ")
        )
    })?;
    bus.request(Request::MenuOpen(which));
    Ok(())
}

/// `settings_ron()`: the saved settings as RON, empty when nothing was
/// saved. Read straight off the bus, where the `PageSettings` store put it.
pub fn settings_ron(bus: &Bus) -> String {
    bus.settings()
}

/// `restore_settings(ron)`: puts a [`settings_ron`] value back, or resets
/// the saved settings when `ron` is empty.
///
/// Two things happen, and the order matters. The bus slot is written here,
/// synchronously, so a `MenuPlugin` that has not loaded the store yet (the
/// page calls this before the first frame, and `apply_settings` runs in
/// `PostStartup`) finds the values when it does. Then a request goes on the
/// queue, so a world that already applied its settings re-seeds them from
/// the text; the two are idempotent, and whichever runs second is a no-op.
///
/// # Errors
///
/// Text that is neither empty nor saved settings. Nothing is written then,
/// so a stale key in `localStorage` cannot empty a store that was fine.
pub fn restore_settings(bus: &Bus, ron: &str) -> Result<(), String> {
    if ron.trim().is_empty() {
        bus.set_settings("");
    } else {
        crate::settings_store::from_ron(ron)?;
        bus.set_settings(ron);
    }
    bus.request(Request::RestoreSettings {
        ron: ron.to_owned(),
    });
    Ok(())
}

/// `talk_again()`: starts the smith's conversation again. Dialogue scene
/// only; the world says so on the console when another is open, and a
/// no-op with a line while one is running.
pub fn talk_again(bus: &Bus) {
    bus.request(Request::TalkAgain);
}

/// `set_value(key, json)`: writes one declared settings key into the value
/// store. `json` is a JSON bool, number or string; it is made the kind the
/// key's default has, so a page can say `1` for a slider and `"1280x720"`
/// for a select.
///
/// # Errors
///
/// A key `showcase::settings::spec` does not declare, or a value that is
/// not a JSON bool, number or string of the key's kind.
pub fn set_value(bus: &Bus, key: &str, json: &str) -> Result<(), String> {
    let defaults = showcase::settings::spec().defaults();
    let Some(default) = defaults.get(key) else {
        return Err(format!(
            "`{key}` is not a settings key; the spec declares {}",
            defaults.keys().cloned().collect::<Vec<_>>().join(", ")
        ));
    };
    let value = parse_json_value(json, default)?;
    bus.request(Request::SetValue {
        key: key.to_owned(),
        value,
    });
    Ok(())
}

/// A JSON bool, number or string as the `Value` kind `like` has.
///
/// Small enough not to be worth `serde_json` in the wasm module: three
/// scalars, and a string with the escapes a page would write.
fn parse_json_value(json: &str, like: &Value) -> Result<Value, String> {
    let text = json.trim();
    let wrong = |what: &str| {
        format!(
            "`{json}` is a JSON {what}, and the key holds a {}",
            match like {
                Value::Bool(_) => "bool",
                Value::Int(_) => "whole number",
                Value::Float(_) => "number",
                Value::Text(_) => "string",
            }
        )
    };
    match (text, like) {
        ("true", Value::Bool(_)) => Ok(Value::Bool(true)),
        ("false", Value::Bool(_)) => Ok(Value::Bool(false)),
        ("true" | "false", _) => Err(wrong("bool")),
        (quoted, _) if quoted.starts_with('"') => {
            let inner = quoted
                .strip_suffix('"')
                .and_then(|q| q.strip_prefix('"'))
                .ok_or_else(|| format!("`{json}` is not a JSON string"))?;
            let unescaped = unescape_json(inner)?;
            match like {
                Value::Text(_) => Ok(Value::Text(unescaped)),
                _ => Err(wrong("string")),
            }
        }
        (number, Value::Int(_)) => number
            .parse::<i64>()
            .map(Value::Int)
            .map_err(|_| format!("`{json}` is not a JSON whole number")),
        (number, Value::Float(_)) => number
            .parse::<f64>()
            .map(Value::Float)
            .map_err(|_| format!("`{json}` is not a JSON number")),
        (number, _) if number.parse::<f64>().is_ok() => Err(wrong("number")),
        _ => Err(format!("`{json}` is not a JSON bool, number or string")),
    }
}

/// The body of a JSON string literal, escapes resolved.
fn unescape_json(inner: &str) -> Result<String, String> {
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        match chars.next() {
            Some('"') => out.push('"'),
            Some('\\') => out.push('\\'),
            Some('/') => out.push('/'),
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('t') => out.push('\t'),
            Some('u') => {
                let code: String = chars.by_ref().take(4).collect();
                let point = u32::from_str_radix(&code, 16)
                    .ok()
                    .and_then(char::from_u32)
                    .ok_or_else(|| format!("`\\u{code}` is not a character"))?;
                out.push(point);
            }
            other => return Err(format!("`\\{}` is not a JSON escape", other.unwrap_or(' '))),
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_open_accepts_the_three_names_and_nothing_else() {
        let bus = Bus::new();
        for which in MenuScreen::ALL {
            menu_open(&bus, which.id()).expect("a real name");
        }
        assert_eq!(bus.take_requests().len(), 3);
        let error = menu_open(&bus, "about").expect_err("not a screen the page can open");
        assert!(error.contains("title, pause, settings"), "{error}");
        assert!(bus.take_requests().is_empty(), "a refusal queues nothing");
    }

    #[test]
    fn restore_settings_writes_the_bus_first_and_refuses_what_is_not_settings() {
        let bus = Bus::new();
        let ron = "(values:{\"settings.ui_scale\":1.25},bindings:None)";
        restore_settings(&bus, ron).expect("saved settings");
        assert_eq!(settings_ron(&bus), ron, "the store's load finds it at once");
        assert!(matches!(
            bus.take_requests().as_slice(),
            [Request::RestoreSettings { ron: text }] if text == ron
        ));

        restore_settings(&bus, "  ").expect("empty resets");
        assert_eq!(settings_ron(&bus), "");

        bus.set_settings(ron);
        assert!(restore_settings(&bus, "(values:").is_err());
        assert_eq!(settings_ron(&bus), ron, "a refusal leaves the slot alone");
    }

    #[test]
    fn set_value_checks_the_key_and_makes_the_value_the_keys_kind() {
        let bus = Bus::new();
        set_value(&bus, "demo.found_key", "true").expect("a declared toggle");
        set_value(&bus, "settings.ui_scale", "1").expect("a whole number is a float");
        set_value(&bus, "settings.resolution", "\"1280x720\"").expect("a select's option");
        set_value(&bus, "settings.player_name", "\"a \\\"quoted\\\" name\"")
            .expect("a string with escapes");
        let values: Vec<(String, Value)> = bus
            .take_requests()
            .into_iter()
            .map(|r| match r {
                Request::SetValue { key, value } => (key, value),
                other => panic!("{other:?}"),
            })
            .collect();
        assert_eq!(
            values,
            vec![
                ("demo.found_key".to_owned(), Value::Bool(true)),
                ("settings.ui_scale".to_owned(), Value::Float(1.0)),
                (
                    "settings.resolution".to_owned(),
                    Value::Text("1280x720".to_owned())
                ),
                (
                    "settings.player_name".to_owned(),
                    Value::Text("a \"quoted\" name".to_owned())
                ),
            ]
        );

        assert!(
            set_value(&bus, "demo.nothing", "true").is_err(),
            "an undeclared key"
        );
        assert!(
            set_value(&bus, "demo.found_key", "1").is_err(),
            "a number for a bool"
        );
        assert!(
            set_value(&bus, "settings.ui_scale", "\"big\"").is_err(),
            "a string for a slider"
        );
        assert!(
            set_value(&bus, "settings.ui_scale", "{}").is_err(),
            "not a scalar"
        );
        assert!(bus.take_requests().is_empty(), "a refusal queues nothing");
    }

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
}
