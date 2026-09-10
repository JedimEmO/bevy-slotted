//! The showcase's scene table and the code the examples share.
//!
//! `docs/design/showcase-contract.md` is the contract. The table is plain data
//! the page's bridge, a native test and a build script can all read. Beside it
//! live the modules the native examples and the web playground share: the
//! chest's menu and contents ([`chest`]), the machine's simulation and face
//! widget ([`machine`]), the copper chest the Lua mods build ([`mods`]), the
//! cube ring and orbit every scene sits in front of ([`backdrop`]), the
//! settings screen's store, rules and guard ([`settings`]), the menu config,
//! the quit flow and the showcase's title screen ([`menus`]), the smith's
//! conversation ([`dialogue`]), and the screen files compiled in so a scene
//! switch never waits on an asset handle ([`screens`]).

pub mod backdrop;
pub mod chest;
pub mod dialogue;
pub mod machine;
pub mod menus;
pub mod mods;
pub mod screens;
pub mod settings;

/// One of the nine scenes, in rail order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Scene {
    /// The moodboard chest with the item and recipe browser docked beside
    /// it: seven click modes, sweeps, phantoms, tooltips, a search grammar.
    Chest,
    /// The furnace: tanks, bars, side tabs, the simulation, the injected Sort.
    Machine,
    /// The title, pause, settings, confirm and toast over the chest.
    Menus,
    /// A conversation with the smith over the furnace.
    Dialogue,
    /// The theme switcher over the settings screen.
    Themes,
    /// The editable Lua mods with hot reload and restart. Today's playground.
    Mods,
    /// HUD layers and the position editor.
    Hud,
    /// Two clients and a server over a lossy loopback link.
    Multiplayer,
    /// The Lua test runner and a recorded-input replay.
    Testing,
}

impl Scene {
    /// Every scene, in the order the rail shows them.
    pub const ALL: [Scene; 9] = [
        Scene::Chest,
        Scene::Machine,
        Scene::Menus,
        Scene::Dialogue,
        Scene::Themes,
        Scene::Mods,
        Scene::Hud,
        Scene::Multiplayer,
        Scene::Testing,
    ];

    /// The scene the page opens on: the chest, which is the one that shows
    /// the interaction model the whole project is about.
    pub const DEFAULT: Scene = Scene::Chest;

    /// The id the page and the bridge use: lowercase, one word.
    pub const fn id(self) -> &'static str {
        match self {
            Scene::Chest => "chest",
            Scene::Machine => "machine",
            Scene::Menus => "menus",
            Scene::Dialogue => "dialogue",
            Scene::Themes => "themes",
            Scene::Mods => "mods",
            Scene::Hud => "hud",
            Scene::Multiplayer => "multiplayer",
            Scene::Testing => "testing",
        }
    }

    /// The scene with this id, if there is one.
    pub fn from_id(id: &str) -> Option<Scene> {
        Scene::ALL.into_iter().find(|scene| scene.id() == id)
    }

    /// The page copy for this scene.
    pub fn def(self) -> &'static SceneDef {
        &SCENES[self as usize]
    }

    /// Whether the scene is implemented. The page greys the others out.
    pub fn ready(self) -> bool {
        self.def().ready
    }
}

/// What the page shows for one scene. Copy is the contract's, verbatim.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SceneDef {
    /// The scene.
    pub scene: Scene,
    /// The rail label.
    pub title: &'static str,
    /// Two sentences under the title.
    pub caption: &'static str,
    /// Exactly three things to try.
    pub tries: [&'static str; 3],
    /// Package A flips this as each scene lands; package B owns the copy.
    pub ready: bool,
}

/// The table, in rail order. `Scene as usize` indexes it.
pub const SCENES: [SceneDef; 9] = [
    SceneDef {
        scene: Scene::Chest,
        title: "Chest",
        caption: "The Minecraft interaction model with a modern skin, and the item and recipe browser docked beside it. Seven click modes, a sweep, a phantom preview, a tooltip, and a search with a grammar.",
        tries: [
            "Left-click a stack, then right-click to split it",
            "Hold right and drag across empty slots",
            "Type #ingots in the browser, then press R over a card",
        ],
        ready: true,
    },
    SceneDef {
        scene: Scene::Machine,
        title: "Machine",
        caption: "A furnace with a tank, an energy bar and two side tabs, driven by menu properties the simulation writes. The Sort button was injected by a mod that has never seen this screen.",
        tries: [
            "Put coal in the fuel slot and watch the arrow",
            "Open the redstone tab",
            "Press Sort",
        ],
        ready: true,
    },
    SceneDef {
        scene: Scene::Menus,
        title: "Menus",
        caption: "A title screen, a pause, settings that persist, a confirm and a toast, and the game is a chest behind them. Every screen sits on one stack, and the keyboard walks all of it.",
        tries: [
            "Press Play, then Esc twice",
            "Change a setting, reload the page, open Settings again",
            "Walk a menu with the arrow keys and Enter",
        ],
        ready: true,
    },
    SceneDef {
        scene: Scene::Dialogue,
        title: "Dialogue",
        caption: "A conversation with the smith at the furnace, from one RON file. Lines type out, a choice can be gated on a value, and the history page is the transcript.",
        tries: [
            "Press Enter to skip the typing, then choose",
            "Turn on \"found the key\" on the right and ask again",
            "Press X for the history",
        ],
        ready: true,
    },
    SceneDef {
        scene: Scene::Themes,
        title: "Themes",
        caption: "One screen tree, three skins. A theme is a RON file of tokens and materials, and swapping it repaints the open settings screen in place: tabs, sliders, selects, toggles and rich text.",
        tries: [
            "Switch to paper",
            "Switch to neon",
            "Open the Chest scene and switch again",
        ],
        ready: true,
    },
    SceneDef {
        scene: Scene::Mods,
        title: "Mods",
        caption: "Four Lua mods, editable here, hot-reloaded into the running game. The chest keeps its contents across a reload, and a crash restarts the runtime.",
        tries: [
            "Edit control.lua and press Run",
            "Open Tests and run them",
            "Type error(\"boom\") and watch the restart",
        ],
        ready: true,
    },
    SceneDef {
        scene: Scene::Hud,
        title: "HUD",
        caption: "Layers anchored to the screen edges, registered from Rust or from a mod, and a position editor a player can use.",
        tries: [
            "Press the edit button and drag the hotbar",
            "Drag the clock",
            "Reload the page and find them where you left them",
        ],
        ready: true,
    },
    SceneDef {
        scene: Scene::Multiplayer,
        title: "Multiplayer",
        caption: "Two clients and a server in this tab, joined by a lossy loopback link. The left client predicts; the server corrects; both share one chest.",
        tries: [
            "Click a stack on the left and watch the right",
            "Raise the loss slider and click again",
            "Read the message log",
        ],
        ready: true,
    },
    SceneDef {
        scene: Scene::Testing,
        title: "Testing",
        caption: "The mods' tests/*.lua run against the live game, one action per frame, and a recorded session replays through the real input path.",
        tries: [
            "Run the sorter's tests",
            "Scrub the recording",
            "Press play",
        ],
        ready: true,
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_is_in_rail_order_and_indexed_by_the_enum() {
        for (index, scene) in Scene::ALL.into_iter().enumerate() {
            assert_eq!(scene as usize, index);
            assert_eq!(SCENES[index].scene, scene);
            assert_eq!(Scene::from_id(scene.id()), Some(scene));
        }
        assert_eq!(Scene::from_id("chests"), None);
    }

    #[test]
    fn every_scene_is_ready() {
        let ready: Vec<Scene> = Scene::ALL.into_iter().filter(|s| s.ready()).collect();
        assert_eq!(ready, Scene::ALL, "every scene is real");
        assert!(Scene::DEFAULT.ready());
    }
}
