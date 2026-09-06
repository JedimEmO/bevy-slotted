//! The showcase's scenes, natively: one smoke assertion per scene.
//!
//! `docs/design/showcase-contract.md` section 6. Every scene must open in a
//! headless `UiHarness` and prove one fact about itself, so the page's rail
//! never points at something only a browser can check. The per-scene tests
//! are ignored until package A lands each scene; the two at the bottom run
//! today and keep the table, the registry and the bridge JSON in step.
#![allow(clippy::unwrap_used)]

use bevy::prelude::*;
use slotted_test::prelude::*;
use web_playground::bus::{Bus, Request};
use web_playground::showcase::{ActiveScene, Scene, SceneRegistry, ShowcasePlugin, SwitchScene};

/// A live world with the bundled mods loaded and the showcase switch on it.
///
/// The same shape as `tests_tab.rs`'s `playground_world`; the integration
/// step lifts both into `tests/common/mod.rs`.
fn showcase_world() -> (UiHarness, Bus) {
    let mut harness = UiHarness::builder()
        .plugins(SlottedPlugins::headless())
        .mods_dir(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../modded/mods"))
        .resolution(1600.0, 900.0)
        .theme("glass")
        .build();
    harness
        .world_mut()
        .insert_resource(web_playground::runtime());
    let layout = harness.mod_layout();
    harness.load_mods(layout);

    let bus = Bus::new();
    let world = harness.world_mut();
    world.insert_resource(bus.clone());
    world.init_resource::<ActiveScene>();
    world.init_resource::<Messages<SwitchScene>>();
    world
        .get_resource_or_init::<Schedules>()
        .add_systems(PreUpdate, web_playground::showcase::apply_scene_switch);
    // The registry the plugin builds, without the rest of the plugin: the
    // harness has its own schedule and the plugin's `after(drain_requests)`
    // names a system that is not in it.
    let mut app = App::new();
    app.add_plugins(ShowcasePlugin);
    let registry = app.world_mut().remove_resource::<SceneRegistry>().unwrap();
    world.insert_resource(registry);
    (harness, bus)
}

/// Asks for `scene` the way the bridge does and runs a frame.
fn switch_to(harness: &mut UiHarness, bus: &Bus, scene: Scene) {
    bus.request(Request::SetScene(scene));
    let scene = match bus.take_requests().into_iter().next().unwrap() {
        Request::SetScene(scene) => scene,
        other => panic!("expected SetScene, got {other:?}"),
    };
    harness.world_mut().write_message(SwitchScene(scene));
    harness.step(1);
}

fn active(harness: &UiHarness) -> Scene {
    harness.world().resource::<ActiveScene>().0
}

// ---------------------------------------------------------------------------
// One per scene, in rail order. Each opens the scene and proves the fact the
// contract's table names for it.
// ---------------------------------------------------------------------------

#[test]
#[ignore = "showcase: enabled at integration"]
fn chest_scene_opens_and_a_split_conserves() {
    // 27 container slots; pick up then right-click splits; `assert_conserved`.
    let (mut harness, bus) = showcase_world();
    switch_to(&mut harness, &bus, Scene::Chest);
    assert_eq!(active(&harness), Scene::Chest);
    assert_eq!(harness.find_all(&by::role(SemanticRole::Slot)).len(), 63);
}

#[test]
#[ignore = "showcase: enabled at integration"]
fn browser_scene_docks_and_search_narrows() {
    // `search("#c:ingots")` leaves fewer cards than an empty search.
    let (mut harness, bus) = showcase_world();
    switch_to(&mut harness, &bus, Scene::Browser);
    assert_eq!(active(&harness), Scene::Browser);
}

#[test]
#[ignore = "showcase: enabled at integration"]
fn machine_scene_cooks_and_carries_the_injected_sort() {
    // After three seconds of virtual time `props::COOK` is above zero, and a
    // button tagged `sorter_action = sort` exists on the furnace.
    let (mut harness, bus) = showcase_world();
    switch_to(&mut harness, &bus, Scene::Machine);
    assert_eq!(active(&harness), Scene::Machine);
}

#[test]
#[ignore = "showcase: enabled at integration"]
fn themes_scene_repaints_the_open_screen() {
    // `Request::SetTheme("neon")` changes the panel role's colour in place.
    let (mut harness, bus) = showcase_world();
    switch_to(&mut harness, &bus, Scene::Themes);
    assert_eq!(active(&harness), Scene::Themes);
}

#[test]
fn mods_scene_is_the_playground_and_is_the_default() {
    // Today's playground: the copper chest and its three mods. The scene is
    // real already, so this one runs.
    let (harness, _bus) = showcase_world();
    assert_eq!(active(&harness), Scene::Mods);
    assert!(harness.world().resource::<SceneRegistry>().has(Scene::Mods));
}

#[test]
#[ignore = "showcase: enabled at integration"]
fn hud_scene_spawns_both_layers_and_the_editor_moves_one() {
    // `hud_layer("slotted:hotbar")` and `hud_layer("hud_clock:clock")` exist;
    // with `HudEditMode(true)` a drag changes the hotbar's anchor.
    let (mut harness, bus) = showcase_world();
    switch_to(&mut harness, &bus, Scene::Hud);
    assert_eq!(active(&harness), Scene::Hud);
    assert!(harness.hud_layer("slotted:hotbar").is_some());
}

#[test]
#[ignore = "showcase: enabled at integration"]
fn multiplayer_scene_shares_one_chest_over_the_loopback() {
    // A click on peer 1's screen reaches peer 2's inventory after the link
    // advances; with `Conditions::lossy(100)` for one frame the ack is
    // retransmitted and still lands.
    let (mut harness, bus) = showcase_world();
    switch_to(&mut harness, &bus, Scene::Multiplayer);
    assert_eq!(active(&harness), Scene::Multiplayer);
}

#[test]
#[ignore = "showcase: enabled at integration"]
fn testing_scene_runs_lua_tests_and_replays_the_recording() {
    // `run_tests("sorter")` logs an `ok` line; the bundled recording replays
    // to the inventory the fixture names.
    let (mut harness, bus) = showcase_world();
    switch_to(&mut harness, &bus, Scene::Testing);
    assert_eq!(active(&harness), Scene::Testing);
}

// ---------------------------------------------------------------------------
// The whole set
// ---------------------------------------------------------------------------

#[test]
#[ignore = "showcase: enabled at integration"]
fn every_scene_enters_and_leaves_cleanly() {
    // Cycle all eight in rail order and back to the first; after each switch
    // no `ScreenRoot` and no `OpenMenu` from the previous scene survives.
    let (mut harness, bus) = showcase_world();
    for scene in Scene::ALL.into_iter().chain([Scene::Chest]) {
        switch_to(&mut harness, &bus, scene);
        assert_eq!(active(&harness), scene);
    }
}

/// A stub scene is refused with a `not yet` line and the scene stays. When
/// every scene is real this test has nothing to refuse and is deleted.
#[test]
fn a_stub_scene_is_refused_and_the_current_one_stays() {
    let (mut harness, bus) = showcase_world();
    let stub = Scene::ALL
        .into_iter()
        .find(|scene| !scene.ready())
        .expect("at least one scene is still a stub");
    switch_to(&mut harness, &bus, stub);
    assert_eq!(active(&harness), Scene::Mods);
    let refused = bus
        .drain_console()
        .into_iter()
        .any(|line| line.who == "showcase" && line.text.starts_with("not yet"));
    assert!(refused, "the refusal reaches the console");
}

/// `Scene::ready` (what the page greys out) and the registry (what the world
/// can switch to) name the same scenes, and the bridge JSON lists all eight
/// in rail order with the copy the contract fixed.
#[test]
fn list_scenes_matches_the_page_and_the_registry() {
    let (harness, _bus) = showcase_world();
    let registry = harness.world().resource::<SceneRegistry>();
    let ready: Vec<Scene> = Scene::ALL.into_iter().filter(|s| s.ready()).collect();
    assert_eq!(registry.registered(), ready);

    let json = web_playground::showcase::scenes_json();
    let mut at = 0;
    for scene in Scene::ALL {
        let needle = format!(
            "\"id\":\"{}\",\"title\":\"{}\"",
            scene.id(),
            scene.def().title
        );
        let found = json[at..]
            .find(&needle)
            .unwrap_or_else(|| panic!("{} is listed in order", scene.id()));
        at += found;
        assert_eq!(scene.def().tries.len(), 3);
        assert!(!scene.def().caption.is_empty());
    }
}
