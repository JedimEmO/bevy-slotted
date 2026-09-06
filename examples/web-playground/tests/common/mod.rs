//! One live playground world, for every integration test that needs one.
//!
//! `docs/design/showcase-contract.md` section 6 asked for `tests_tab.rs`'s
//! `playground_world` to be lifted here once more than one test file wanted
//! it. Three do now.
//!
//! The world is a `UiHarness`: headless, but a real `slotted` world with
//! screens, slots, picking and virtual time. On top of it go the playground's
//! own systems, scheduled the way `build_app` schedules them, so a test walks
//! the same route the browser does -- a request on the bus, drained in
//! `PreUpdate`, applied by the same code -- rather than a shortcut around it.
#![allow(dead_code, clippy::unwrap_used)]

use bevy::prelude::*;
use slotted_test::prelude::*;
use web_playground::bus::{Bus, Request};
use web_playground::showcase::{ActiveScene, CanvasScene, Scene, SceneRegistry, SwitchScene};

/// The mods the playground bundles, on disk.
pub fn mods_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../modded/mods")
}

/// A harness with the bundled mods loaded and the luaur runtime in place.
///
/// `mod_layout_with_base` rather than `mod_layout`: the base namespaces `demo`
/// and `machine` are loaded as mods with synthetic manifests, which is exactly
/// what the playground's `build.rs` bakes into the browser bundle, and without
/// them the Chest, Browser and Machine scenes have no items to put in a chest.
pub fn harness() -> UiHarness {
    let mut harness = UiHarness::builder()
        // The same two plugins `build_app` adds on top of the facade: the
        // furnace simulation with its face widget, and the chest's key
        // bindings and header labels.
        .plugins((
            SlottedPlugins::headless(),
            showcase::machine::MachineDemoPlugin,
            showcase::chest::ChestDemoPlugin,
        ))
        .mods_dir(mods_dir())
        // The size the bundled recording was made at. `tests/showcase.rs`
        // replays it, and a recording carries pointer positions and no
        // locators, so any other size would land the clicks elsewhere.
        .resolution(1600.0, 900.0)
        .theme("glass")
        .build();
    // Before the mods load: the data stage needs a runtime, and this crate's
    // is luaur, the one the browser gets.
    harness
        .world_mut()
        .insert_resource(web_playground::runtime());
    // The same kind of `HudLayoutStorage` adapter `build_app` inserts: memory,
    // standing in for the `localStorage` the page owns. A fresh one per
    // harness rather than the process-wide `hud_store::global`, so two tests
    // in this binary do not fight over one store.
    harness
        .world_mut()
        .insert_resource(slotted_ui::hud_editor::HudLayoutStore::new(
            slotted_ui::hud_editor::MemoryHudLayout::new(),
        ));
    let layout = harness.mod_layout_with_base();
    harness.load_mods(layout);
    harness
}

/// The harness plus the bus and the playground's request plumbing, but no
/// scenes: what `tests_tab.rs` and `restart.rs` want.
pub fn playground_world() -> (UiHarness, Bus) {
    let mut harness = harness();
    let bus = Bus::new();
    let world = harness.world_mut();
    world.insert_resource(bus.clone());
    world.init_resource::<Messages<web_playground::StartTests>>();
    world.get_resource_or_init::<Schedules>().add_systems(
        Update,
        (
            web_playground::begin_tests_for_test,
            web_playground::tests::run_live_tests,
        )
            .chain(),
    );
    (harness, bus)
}

/// The whole showcase: the scene registry, the switch, the scene commands and
/// the two per-frame systems the scenes need, on the default scene.
///
/// The one thing not scheduled is `drain_requests`, which wants an
/// `EditableSource` this harness has no reason to carry; [`switch_to`] and
/// [`command`] take a request off the bus and write the message it would have
/// written, which is the same two lines that system runs.
pub fn showcase_world() -> (UiHarness, Bus) {
    let mut harness = harness();
    let bus = Bus::new();
    let world = harness.world_mut();
    world.insert_resource(bus.clone());
    world.init_resource::<ActiveScene>();
    world.init_resource::<CanvasScene>();
    world.init_resource::<Messages<SwitchScene>>();
    world.init_resource::<Messages<web_playground::SceneCommand>>();
    world.init_resource::<Messages<web_playground::StartTests>>();
    world.insert_resource(SceneRegistry::with_all_scenes());
    world.get_resource_or_init::<Schedules>().add_systems(
        PreUpdate,
        (
            web_playground::showcase::apply_scene_switch,
            web_playground::apply_scene_commands_for_test,
        )
            .chain(),
    );
    world.get_resource_or_init::<Schedules>().add_systems(
        Update,
        (
            web_playground::begin_tests_for_test,
            web_playground::tests::run_live_tests,
        )
            .chain(),
    );
    world.get_resource_or_init::<Schedules>().add_systems(
        PostUpdate,
        (
            web_playground::scenes::multiplayer::pump_link,
            web_playground::scenes::multiplayer::fit_client_screens,
            web_playground::scenes::testing::advance_replay,
            web_playground::publish_replay_status,
        )
            .chain(),
    );
    // What `ShowcasePlugin` does at `Startup`.
    web_playground::showcase::enter_first_scene(harness.world_mut());
    harness.settle();
    (harness, bus)
}

/// Asks for `scene` the way the bridge does and settles.
pub fn switch_to(harness: &mut UiHarness, bus: &Bus, scene: Scene) {
    bus.request(Request::SetScene(scene));
    let scene = match bus.take_requests().into_iter().next().unwrap() {
        Request::SetScene(scene) => scene,
        other => panic!("expected SetScene, got {other:?}"),
    };
    harness.world_mut().write_message(SwitchScene(scene));
    harness.settle();
}

/// Sends one scene command the way the bridge does and settles.
///
/// `step(2)` before the settle, because `settle` stops as soon as nothing is
/// animating and nothing is in flight -- which on a quiet frame is
/// immediately, before `apply_scene_commands` has run in `PreUpdate` at all.
pub fn command(harness: &mut UiHarness, command: web_playground::SceneCommand) {
    harness.world_mut().write_message(command);
    harness.step(2);
    harness.settle();
}

/// Which scene the rail is on.
pub fn active(harness: &UiHarness) -> Scene {
    harness.world().resource::<ActiveScene>().0
}

/// Which scene's entities are on the canvas.
pub fn canvas(harness: &UiHarness) -> Scene {
    harness.world().resource::<CanvasScene>().0
}

/// Every console line written since the last drain, oldest first.
pub fn lines(bus: &Bus) -> Vec<(String, String)> {
    bus.drain_console()
        .into_iter()
        .map(|line| (line.who, line.text))
        .collect()
}

/// The kinds of the game screens currently open, sorted.
///
/// The item browser's own panel is a `ScreenRoot` too (`slotted:browser`), and
/// it is not a screen a scene opened: it attaches itself beside one. It is left
/// out here and counted by [`browser_panels`] instead, so a scene's screen
/// count says what the scene opened.
pub fn open_screens(harness: &mut UiHarness) -> Vec<String> {
    let mut kinds: Vec<String> = all_screen_kinds(harness)
        .into_iter()
        .filter(|kind| kind != BROWSER_PANEL)
        .collect();
    kinds.sort();
    kinds
}

/// The browser panel's own screen kind.
pub const BROWSER_PANEL: &str = "slotted:browser";

/// How many browser panels are docked.
pub fn browser_panels(harness: &mut UiHarness) -> usize {
    all_screen_kinds(harness)
        .into_iter()
        .filter(|kind| kind == BROWSER_PANEL)
        .count()
}

fn all_screen_kinds(harness: &mut UiHarness) -> Vec<String> {
    harness
        .world_mut()
        .query::<&slotted_ui::ScreenRoot>()
        .iter(harness.world())
        .map(|root| root.kind.0.to_string())
        .collect()
}

/// Everything in every inventory the world holds, as `(item id, count)` pairs,
/// sorted. What a replay or a network round trip is asserted to land on.
pub fn all_stacks(harness: &mut UiHarness) -> Vec<(String, u32)> {
    let registries = harness
        .world()
        .resource::<slotted_ecs::Registries>()
        .0
        .clone();
    let mut out: Vec<(String, u32)> = harness
        .world_mut()
        .query::<&slotted_ecs::menu::Inventory>()
        .iter(harness.world())
        .flat_map(|inventory| {
            (0..inventory.0.len())
                .filter_map(|i| inventory.0.get(i).cloned())
                .collect::<Vec<_>>()
        })
        .map(|stack| {
            let name = registries
                .items
                .get(stack.id)
                .map_or_else(|| "?".to_owned(), |def| def.name.to_string());
            (name, stack.count)
        })
        .collect();
    out.sort();
    out
}
