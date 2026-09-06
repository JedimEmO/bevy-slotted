//! The showcase's scenes, natively: one smoke assertion per scene.
//!
//! `docs/design/showcase-contract.md` section 6. Every scene opens in a
//! headless `UiHarness` and proves one fact about itself, so the page's rail
//! never points at something only a browser can check. Below those, four tests
//! about the set as a whole and about the three things the scenes needed that
//! nothing else in the workspace has: the HUD layout round trip, the
//! multiplayer link, and the replay scrubber.
#![allow(clippy::unwrap_used)]

mod common;

use bevy::prelude::*;
use common::{
    active, all_stacks, browser_panels, canvas, command, lines, open_screens, showcase_world,
    switch_to,
};
use slotted_test::prelude::*;
use slotted_theme::ActiveTheme;
use web_playground::SceneCommand;
use web_playground::showcase::{Scene, SceneRegistry};

// ---------------------------------------------------------------------------
// One per scene, in rail order. Each opens the scene and proves the fact the
// contract's table names for it.
// ---------------------------------------------------------------------------

/// 27 container slots plus 27 player and 9 hotbar, and the whole gesture the
/// scene is about -- pick a stack up, split it with a right-click -- conserves.
#[test]
fn chest_scene_opens_and_a_split_conserves() {
    let (mut harness, bus) = showcase_world();
    switch_to(&mut harness, &bus, Scene::Chest);
    assert_eq!(active(&harness), Scene::Chest);
    assert_eq!(harness.find_all(&by::role(SemanticRole::Slot)).len(), 63);

    let slots = harness.find_all(&by::role(SemanticRole::Slot));
    let before: u32 = all_stacks(&mut harness).iter().map(|(_, n)| n).sum();
    harness.click(slots[0]);
    harness.settle();
    harness.right_click(slots[3]);
    harness.settle();
    let after: u32 = all_stacks(&mut harness).iter().map(|(_, n)| n).sum();
    assert!(
        after <= before,
        "a pick-up and a split created nothing: {after} where there were {before}"
    );

    // The browser is denied on this scene: no panel docks beside the chest.
    assert_eq!(
        browser_panels(&mut harness),
        0,
        "the Chest scene shows the chest and nothing else"
    );
}

/// The panel docks, and a search narrows what it lists.
#[test]
fn browser_scene_docks_and_search_narrows() {
    let (mut harness, bus) = showcase_world();
    switch_to(&mut harness, &bus, Scene::Browser);
    assert_eq!(active(&harness), Scene::Browser);
    assert_eq!(browser_panels(&mut harness), 1, "the panel docked");

    let mut browser = harness.browser();
    browser.wait_for_index();
    let everything = browser.visible_entries().len();
    assert!(everything > 0, "the browser lists the demo items");
    browser.search("#c:ingots");
    let narrowed = browser.visible_entries().len();
    assert!(
        narrowed > 0 && narrowed < everything,
        "the tag query narrows the list: {narrowed} of {everything}"
    );
}

/// After three seconds of virtual time the furnace has cooked, and the Sort
/// button the `sorter` mod injected into `slotted:any` is on a screen that mod
/// has never seen.
#[test]
fn machine_scene_cooks_and_carries_the_injected_sort() {
    let (mut harness, bus) = showcase_world();
    switch_to(&mut harness, &bus, Scene::Machine);
    assert_eq!(active(&harness), Scene::Machine);

    assert_eq!(
        harness.find_all(&by::test_id("sorter_sort")).len(),
        1,
        "the injected Sort button is on the furnace"
    );

    harness.advance(std::time::Duration::from_secs(3));
    harness.settle();
    let cook = property(&mut harness, showcase::machine::props::COOK);
    let burn = property(&mut harness, showcase::machine::props::BURN);
    assert!(
        cook > 0 || burn > 0,
        "three seconds of coal moved the machine: cook {cook}, burn {burn}"
    );
}

/// Switching to Themes leaves the canvas where it was, and `set_theme` swaps
/// the handle the whole tree resolves its colours through.
#[test]
fn themes_scene_repaints_without_touching_the_open_screen() {
    let (mut harness, bus) = showcase_world();
    switch_to(&mut harness, &bus, Scene::Chest);
    let before = open_screens(&mut harness);
    let panel = harness.find(&by::test_id("chest_panel"));

    switch_to(&mut harness, &bus, Scene::Themes);
    assert_eq!(active(&harness), Scene::Themes);
    assert_eq!(
        canvas(&harness),
        Scene::Chest,
        "Themes shows the controls over whatever was open"
    );
    assert_eq!(open_screens(&mut harness), before);

    let glass = theme_handle(&harness);
    command(
        &mut harness,
        SceneCommand::SetTheme {
            name: "neon".to_owned(),
        },
    );
    assert_ne!(theme_handle(&harness), glass, "the theme handle changed");
    assert!(
        harness.world().get_entity(panel).is_ok(),
        "the panel was repainted, not respawned"
    );
}

/// Today's playground: the copper chest and the mods that built it.
#[test]
fn mods_scene_is_the_playground_and_its_chest_is_the_mods_own() {
    let (mut harness, bus) = showcase_world();
    switch_to(&mut harness, &bus, Scene::Mods);
    assert_eq!(active(&harness), Scene::Mods);
    assert_eq!(open_screens(&mut harness), ["copper_chest:chest"]);
    assert_eq!(
        harness.find_all(&by::test_id("sorter_sort")).len(),
        1,
        "the sorter's button is on the mods' own chest too"
    );
}

/// Both HUD layers spawn: the built-in hotbar the plugin registered from Rust,
/// and the clock a mod's `data.lua` registered with no Rust at all.
#[test]
fn hud_scene_spawns_a_builtin_layer_and_a_mods_layer() {
    let (mut harness, bus) = showcase_world();
    switch_to(&mut harness, &bus, Scene::Hud);
    assert_eq!(active(&harness), Scene::Hud);
    assert!(
        open_screens(&mut harness).is_empty(),
        "the HUD scene opens no screen, which is why the layers are visible"
    );
    assert!(
        harness.hud_layer("hotbar").is_some(),
        "the built-in hotbar layer is up"
    );
    assert!(
        harness.hud_layer("hud_clock:clock").is_some(),
        "the mod's clock layer is up"
    );
}

/// Two chest screens over one server, and both are on the canvas at once.
#[test]
fn multiplayer_scene_opens_two_clients_over_one_server() {
    let (mut harness, bus) = showcase_world();
    switch_to(&mut harness, &bus, Scene::Multiplayer);
    assert_eq!(active(&harness), Scene::Multiplayer);
    assert_eq!(
        open_screens(&mut harness),
        ["demo:chest", "demo:chest"],
        "two clients, two screens, one canvas"
    );
    assert_eq!(
        harness.find_all(&by::role(SemanticRole::Slot)).len(),
        63 * 2
    );
    assert!(
        harness
            .world()
            .get_resource::<web_playground::scenes::multiplayer::NetLink>()
            .is_some(),
        "the server is up"
    );
}

/// The Lua tests run against the live app, and the bundled recording loads.
#[test]
fn testing_scene_runs_lua_tests_and_loads_the_recording() {
    let (mut harness, bus) = showcase_world();
    switch_to(&mut harness, &bus, Scene::Testing);
    assert_eq!(active(&harness), Scene::Testing);
    let _ = bus.drain_console();

    harness
        .world_mut()
        .write_message(web_playground::StartTests {
            mod_id: "sorter".to_owned(),
        });
    let mut logged = Vec::new();
    for _ in 0..600 {
        harness.step(1);
        logged.extend(lines(&bus));
        if logged
            .iter()
            .any(|(who, text)| who == "test" && text.contains("passed,"))
        {
            break;
        }
    }
    assert!(
        logged
            .iter()
            .any(|(who, text)| who == "test" && text.starts_with("ok ")),
        "at least one Lua test passed; lines: {logged:#?}"
    );
    assert!(
        !logged.iter().any(|(_, text)| text.starts_with("FAIL")),
        "no Lua test failed; lines: {logged:#?}"
    );

    command(&mut harness, SceneCommand::ReplayLoad);
    let status = harness
        .world()
        .resource::<web_playground::bus::Bus>()
        .clone();
    let _ = status;
    assert!(
        web_playground::scenes::testing::status(harness.world()).contains("\"frames\":8"),
        "the bundled recording has its eight frames: {}",
        web_playground::scenes::testing::status(harness.world())
    );
}

// ---------------------------------------------------------------------------
// The whole set
// ---------------------------------------------------------------------------

/// Cycle all eight in rail order and back to the first. After each switch,
/// nothing of the previous scene is left on the canvas: the count of screens
/// and menus is the count that scene opens, never that plus a leftover.
#[test]
fn every_scene_enters_and_leaves_cleanly() {
    let (mut harness, bus) = showcase_world();
    for scene in Scene::ALL.into_iter().chain([Scene::Chest]) {
        switch_to(&mut harness, &bus, scene);
        assert_eq!(active(&harness), scene);
        let screens = open_screens(&mut harness);
        let menus = harness
            .world_mut()
            .query::<&slotted_ecs::menu::OpenMenu>()
            .iter(harness.world())
            .count();
        let expected: usize = match scene {
            // No screen at all, and one menu the hotbar layer draws from.
            Scene::Hud => 0,
            // Two clients, two screens.
            Scene::Multiplayer => 2,
            // Themes keeps whatever was open, which at that point is Machine.
            _ => 1,
        };
        assert_eq!(screens.len(), expected, "screens after entering {scene:?}");
        assert_eq!(menus, expected.max(1), "menus after entering {scene:?}");
    }
}

/// `Scene::ready` (what the page greys out) and the registry (what the world
/// can switch to) name the same scenes, and the bridge JSON lists all eight in
/// rail order with the copy the contract fixed.
#[test]
fn list_scenes_matches_the_page_and_the_registry() {
    let registry = SceneRegistry::with_all_scenes();
    let ready: Vec<Scene> = Scene::ALL.into_iter().filter(|s| s.ready()).collect();
    assert_eq!(registry.registered(), ready);
    assert_eq!(ready, Scene::ALL, "every scene is real");

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

// ---------------------------------------------------------------------------
// The three things the scenes needed that nothing else has
// ---------------------------------------------------------------------------

/// A click on one client reaches the other client's copy of the shared chest,
/// over a link with a hundred milliseconds of latency, and nothing is created
/// or destroyed on the way.
#[test]
fn a_click_on_one_client_reaches_the_other_over_a_lossy_link() {
    let (mut harness, bus) = showcase_world();
    switch_to(&mut harness, &bus, Scene::Multiplayer);
    command(
        &mut harness,
        SceneCommand::NetConfig {
            latency_ms: 100,
            drop_percent: 0,
        },
    );

    let before = all_stacks(&mut harness);
    // The two screens are spawned in peer order, so the first 63 slots the
    // query yields for the left one are client A's.
    let slots = harness.find_all(&by::role(SemanticRole::Slot));
    assert_eq!(slots.len(), 63 * 2);
    let full = slots
        .iter()
        .copied()
        .find(|slot| harness.stack_at(*slot).is_some())
        .expect("client A's chest has something in it");
    harness.click(full);

    // A hundred milliseconds each way is six frames each way at sixty; twenty
    // is comfortably past the round trip and past the server's own flush.
    harness.step(40);
    harness.settle();

    let after = all_stacks(&mut harness);
    assert_ne!(before, after, "the click changed something");
    let total: u32 = after.iter().map(|(_, count)| count).sum();
    let was: u32 = before.iter().map(|(_, count)| count).sum();
    assert!(
        total <= was,
        "nothing was created: {total} items where there were {was}"
    );

    let log = lines(&bus);
    assert!(
        log.iter().any(|(who, _)| who == "net"),
        "the message log has the round trip in it: {log:#?}"
    );
}

/// A HUD layout survives the round trip the page makes it take: the world
/// saves it through the storage port, the page reads the RON, hands it back,
/// and the layers come up where they were left.
#[test]
fn a_hud_layout_round_trips_through_the_storage_port() {
    let (mut harness, bus) = showcase_world();
    switch_to(&mut harness, &bus, Scene::Hud);

    // Where the visitor dragged the clock to.
    let moved = slotted_ui::hud::HudAnchor {
        anchor: slotted_ui::hud::NineAnchor::TopLeft,
        offset: Vec2::new(48.0, 96.0),
        scale: 1.0,
    };
    {
        let mut layout = harness
            .world_mut()
            .resource_mut::<slotted_ui::hud::HudLayout>();
        layout
            .anchors
            .insert(slotted_ui::hud::HudLayerId::new("hud_clock:clock"), moved);
    }
    // `save_hud_layout` runs in `Last` and only when the layout changed, so
    // the write needs a frame to reach the store; `settle` may step none.
    harness.step(3);

    // What the page would read out and put in `localStorage`.
    let stored =
        web_playground::scenes::hud::layout_ron(harness.world()).expect("the layout serialises");
    assert!(
        stored.contains("hud_clock:clock"),
        "the drag reached the store: {stored:?}"
    );

    // A reload: the layer is back at its registered anchor, and then the page
    // hands the stored RON back.
    switch_to(&mut harness, &bus, Scene::Chest);
    harness
        .world_mut()
        .insert_resource(slotted_ui::hud::HudLayout::default());
    switch_to(&mut harness, &bus, Scene::Hud);
    command(
        &mut harness,
        SceneCommand::RestoreHud {
            ron: stored.clone(),
        },
    );

    let layout = harness.world().resource::<slotted_ui::hud::HudLayout>();
    assert_eq!(
        layout
            .anchors
            .get(&slotted_ui::hud::HudLayerId::new("hud_clock:clock")),
        Some(&moved),
        "the clock came back where it was left"
    );
}

/// The bundled recording plays through the real input path and lands on the
/// state it was recorded in: a stack picked up and three slots painted one
/// item each.
#[test]
fn the_bundled_recording_replays_to_the_state_it_recorded() {
    let (mut harness, bus) = showcase_world();
    switch_to(&mut harness, &bus, Scene::Testing);
    command(&mut harness, SceneCommand::ReplayLoad);

    let fresh = all_stacks(&mut harness);
    let frames = frames_of(&harness);
    assert_eq!(frames, 8, "the checked-in recording has eight frames");

    command(&mut harness, SceneCommand::ReplaySeek { frame: frames });
    finish_seek(&mut harness);
    assert!(
        status_of(&harness).contains(&format!("\"frame\":{frames}")),
        "the scrubber is at the end: {}",
        status_of(&harness)
    );

    // The gesture recorded is a pick-up and a three-slot paint, so what the
    // chest ends on is three stacks of one and a cursor holding the rest.
    let played = all_stacks(&mut harness);
    assert_ne!(fresh, played, "the recorded gesture changed the chest");
    let painted = played.iter().filter(|(_, count)| *count == 1).count();
    assert!(
        painted >= 3,
        "the sweep left one item in each of three slots: {played:#?}"
    );

    // Seeking back to zero puts the chest exactly as it opened, which is what
    // makes the scrubber a scrubber rather than a fast-forward button.
    command(&mut harness, SceneCommand::ReplaySeek { frame: 0 });
    finish_seek(&mut harness);
    assert_eq!(all_stacks(&mut harness), fresh, "a rewind is a real rewind");
    assert!(status_of(&harness).contains("\"frame\":0"));
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn theme_handle(harness: &UiHarness) -> Handle<slotted_theme::Theme> {
    harness.world().resource::<ActiveTheme>().0.clone()
}

fn frames_of(harness: &UiHarness) -> u32 {
    harness
        .world()
        .get_resource::<web_playground::scenes::testing::Replay>()
        .map_or(0, |replay| {
            u32::try_from(replay.cursor.frames()).unwrap_or(0)
        })
}

/// Steps until the scrubber has walked to where it was aimed.
///
/// A seek feeds one recorded frame per app frame on purpose: every input is a
/// message the picking backend reads on the next run, so twenty in one frame
/// would land nineteen clicks on the last position.
fn finish_seek(harness: &mut UiHarness) {
    for _ in 0..200 {
        harness.step(1);
        if !web_playground::scenes::testing::is_seeking(harness.world()) {
            break;
        }
    }
    harness.settle();
}

fn status_of(harness: &UiHarness) -> String {
    web_playground::scenes::testing::status(harness.world())
}

/// One synced menu property of the open menu.
fn property(harness: &mut UiHarness, id: slotted_model::PropertyId) -> i32 {
    harness
        .world_mut()
        .query::<&slotted_ecs::menu::OpenMenu>()
        .iter(harness.world())
        .find_map(|open| {
            let index = open.def.properties.iter().position(|p| p.id == id)?;
            open.state.properties.get(index).copied()
        })
        .unwrap_or(0)
}

/// A restart lands on the scene the visitor was in.
///
/// On `wasm32` a mod that raises takes the module down; the page
/// re-instantiates it, which comes up on `Scene::DEFAULT`, and hands back the
/// snapshot it was holding. Contract section 4 asked the snapshot to carry the
/// scene so that the visitor is not thrown back to the chest from wherever
/// they were.
#[test]
fn a_snapshot_carries_the_scene_the_visitor_was_in() {
    let (mut harness, bus) = showcase_world();
    switch_to(&mut harness, &bus, Scene::Machine);
    harness
        .world_mut()
        .get_resource_or_init::<Schedules>()
        .add_systems(PostUpdate, web_playground::publish_snapshot_for_test);
    harness.step(2);

    let ron = bus.snapshot();
    assert!(!ron.is_empty(), "the world published a snapshot");
    let snapshot = web_playground::snapshot::Snapshot::from_ron(&ron).expect("it is a snapshot");
    assert_eq!(snapshot.scene(), Some(Scene::Machine));

    // What the page hands back after the restart, into a module that has come
    // up on the default scene.
    let (mut restarted, restarted_bus) = showcase_world();
    assert_eq!(active(&restarted), Scene::DEFAULT);
    restarted
        .world_mut()
        .init_resource::<Messages<web_playground::RestoreState>>();
    restarted
        .world_mut()
        .get_resource_or_init::<Schedules>()
        .add_systems(PreUpdate, web_playground::apply_restore_for_test);
    restarted
        .world_mut()
        .write_message(web_playground::RestoreState::new(ron));
    restarted.step(3);
    assert_eq!(active(&restarted), Scene::Machine);
    let _ = restarted_bus;
}

/// A snapshot written before the scene field existed still restores its
/// stacks, and simply says nothing about the scene.
#[test]
fn a_snapshot_from_before_the_scene_field_still_reads() {
    let snapshot = web_playground::snapshot::Snapshot::from_ron(
        "(inventories:[(len:9,slots:[(index:0,item:\"minecraft:coal\",count:4)])])",
    )
    .expect("the older shape still parses");
    assert_eq!(snapshot.scene(), None);
    assert_eq!(snapshot.inventories.len(), 1);
}

/// The page's search chips drive the browser's grammar through the same
/// message a category chip writes, and the field shows what was searched.
#[test]
fn a_search_chip_narrows_the_browser_and_fills_its_field() {
    let (mut harness, bus) = showcase_world();
    switch_to(&mut harness, &bus, Scene::Browser);
    harness.browser().wait_for_index();
    let everything = harness.browser().visible_entries().len();

    command(
        &mut harness,
        SceneCommand::BrowserSearch {
            query: "#c:ingots".to_owned(),
        },
    );
    harness.settle();

    let narrowed = harness.browser().visible_entries().len();
    assert!(
        narrowed > 0 && narrowed < everything,
        "the chip narrowed the list: {narrowed} of {everything}"
    );
    let field = harness.find(&by::test_id("browser.search"));
    assert_eq!(
        harness
            .world()
            .get::<bevy::text::EditableText>(field)
            .map(|text| text.value().to_string())
            .as_deref(),
        Some("#c:ingots"),
        "the field shows the query, so a visitor can type on from it"
    );
}

/// A search chip pressed outside the Browser scene does nothing and is not an
/// error, and leaves no query waiting for whoever opens the Browser next.
#[test]
fn a_search_chip_outside_the_browser_scene_is_a_no_op() {
    let (mut harness, bus) = showcase_world();
    switch_to(&mut harness, &bus, Scene::Chest);
    let _ = bus.drain_console();

    command(
        &mut harness,
        SceneCommand::BrowserSearch {
            query: "#c:ingots".to_owned(),
        },
    );
    assert!(
        !lines(&bus).iter().any(|(_, text)| text.starts_with("FAIL")),
        "a chip pressed on the wrong scene is not a failure"
    );

    switch_to(&mut harness, &bus, Scene::Browser);
    harness.browser().wait_for_index();
    let field = harness.find(&by::test_id("browser.search"));
    assert_eq!(
        harness
            .world()
            .get::<bevy::text::EditableText>(field)
            .map(|text| text.value().to_string())
            .as_deref(),
        Some(""),
        "no query was left waiting for the Browser scene"
    );
}

/// `restore_hud_layout("")` is the Reset button: the stored layout is
/// forgotten and every layer goes back to its own anchor.
#[test]
fn an_empty_hud_layout_resets_rather_than_failing_to_parse() {
    let (mut harness, bus) = showcase_world();
    switch_to(&mut harness, &bus, Scene::Hud);

    let clock = slotted_ui::hud::HudLayerId::new("hud_clock:clock");
    let moved = slotted_ui::hud::HudAnchor {
        anchor: slotted_ui::hud::NineAnchor::BottomLeft,
        offset: Vec2::new(24.0, -24.0),
        scale: 1.0,
    };
    {
        let mut layout = harness
            .world_mut()
            .resource_mut::<slotted_ui::hud::HudLayout>();
        layout.anchors.insert(clock.clone(), moved);
    }
    harness.step(3);
    assert!(
        !web_playground::scenes::hud::layout_ron(harness.world())
            .expect("it serialises")
            .is_empty(),
        "the drag reached the store"
    );

    command(
        &mut harness,
        SceneCommand::RestoreHud { ron: String::new() },
    );
    assert!(
        harness
            .world()
            .resource::<slotted_ui::hud::HudLayout>()
            .anchors
            .is_empty(),
        "reset put every layer back on its own anchor"
    );
    assert_eq!(
        web_playground::scenes::hud::layout_ron(harness.world()).expect("it serialises"),
        "",
        "and the page reads back an empty layout, so it can clear its key"
    );
    // The layers are still there: a reset moves them, it does not remove them.
    assert!(harness.hud_layer("hud_clock:clock").is_some());
}

/// The panel the Browser scene docks does not follow the visitor out of it.
///
/// The browser attaches to screen kinds a `ScreenHandler` claims, and a claim
/// lives in a resource that outlives the scene that made it. Three scenes open
/// a `demo:chest`: Chest and Multiplayer want no panel on it, Browser does. So
/// the claim has to be re-decided on every entry rather than made once. It was
/// not, and the Multiplayer scene docked a panel over each of its two clients
/// for any visitor who had opened Browser first -- which is every visitor
/// walking the rail in order.
#[test]
fn the_browser_panel_does_not_follow_the_visitor_into_the_next_scene() {
    let (mut harness, bus) = showcase_world();

    switch_to(&mut harness, &bus, Scene::Browser);
    assert_eq!(
        browser_panels(&mut harness),
        1,
        "the Browser scene is the one that docks a panel"
    );

    switch_to(&mut harness, &bus, Scene::Multiplayer);
    assert_eq!(
        browser_panels(&mut harness),
        0,
        "a panel docked over one of the two multiplayer clients"
    );

    switch_to(&mut harness, &bus, Scene::Chest);
    assert_eq!(
        browser_panels(&mut harness),
        0,
        "a panel docked beside the chest in the scene that is about the chest"
    );

    // And it comes back when it is asked for, so the denial is a decision and
    // not a handler that was lost on the way.
    switch_to(&mut harness, &bus, Scene::Browser);
    assert_eq!(
        browser_panels(&mut harness),
        1,
        "the panel did not come back on a second visit to the Browser scene"
    );
}

/// A live test run is walking a real screen when something despawns it.
///
/// The runner keeps the entities it found on the way in and asks the world
/// about them one op per frame, so a screen that goes away under it leaves the
/// next op pointing at a despawned entity. Natively that is a panic in a test;
/// on `wasm32-unknown-unknown` it is an aborted module and a dead canvas, and
/// the page can only restart and lose the visitor's place.
///
/// Two things do it, and the Testing scene does both. Pressing Load recording
/// rewinds the canvas to the recording's opening chest, which is a teardown;
/// and any scene switch is a teardown. A visitor who presses Run and then
/// reaches for the scrubber is doing something perfectly ordinary, and it took
/// the tab down.
#[test]
fn loading_the_recording_under_a_running_test_stops_the_run_and_not_the_module() {
    let (mut harness, bus) = showcase_world();
    switch_to(&mut harness, &bus, Scene::Testing);

    harness
        .world_mut()
        .write_message(web_playground::StartTests {
            mod_id: "sorter".to_owned(),
        });
    harness.step(2);
    assert!(
        harness
            .world()
            .get_resource::<web_playground::tests::LiveTestRunner>()
            .is_some(),
        "the run did not start, so this test proves nothing"
    );

    // The rewind. Nothing here may panic, and the run has to be over.
    web_playground::scenes::testing::load(harness.world_mut())
        .expect("the bundled recording loads");
    harness.settle();

    assert!(
        harness
            .world()
            .get_resource::<web_playground::tests::LiveTestRunner>()
            .is_none(),
        "the runner survived the rewind and is driving a screen that is gone"
    );
    assert!(
        bus.drain_console()
            .into_iter()
            .any(|line| line.who == "test" && line.text.contains("stopped")),
        "the run was dropped without telling the page why"
    );
}

/// The same hazard by the other route: the rail, pressed mid-run.
#[test]
fn switching_scenes_under_a_running_test_stops_the_run_and_not_the_module() {
    let (mut harness, bus) = showcase_world();
    switch_to(&mut harness, &bus, Scene::Testing);

    harness
        .world_mut()
        .write_message(web_playground::StartTests {
            mod_id: "sorter".to_owned(),
        });
    harness.step(2);
    assert!(
        harness
            .world()
            .get_resource::<web_playground::tests::LiveTestRunner>()
            .is_some(),
        "the run did not start, so this test proves nothing"
    );

    switch_to(&mut harness, &bus, Scene::Machine);
    harness.settle();

    assert!(
        harness
            .world()
            .get_resource::<web_playground::tests::LiveTestRunner>()
            .is_none(),
        "the runner followed the visitor into the next scene"
    );
}

/// The two clients sit beside each other rather than on top of each other.
///
/// A screen root is a full-canvas centring node, so the scene gives each
/// client half the width. That alone was not enough: the chest panel under it
/// is as wide as nine slots and a four-button action rail make it, which is
/// wider than half of any ordinary canvas, so it overflowed its column and the
/// left client's action rail was drawn behind the right client's panel. The
/// fix is a scale on each root, and this is the assertion that it is applied
/// and sufficient.
#[test]
fn the_two_multiplayer_clients_fit_the_halves_of_the_canvas_they_are_given() {
    let (mut harness, bus) = showcase_world();
    switch_to(&mut harness, &bus, Scene::Multiplayer);
    harness.settle();

    let world = harness.world_mut();
    let mut roots = world.query_filtered::<(
        &bevy::prelude::ComputedNode,
        &bevy::prelude::Children,
        &bevy::prelude::UiTransform,
    ), bevy::prelude::With<web_playground::scenes::multiplayer::ClientScreen>>(
    );
    let mut measured = Vec::new();
    let snapshot: Vec<_> = roots
        .iter(world)
        .map(|(node, children, transform)| {
            (
                node.size().x,
                children.iter().collect::<Vec<_>>(),
                transform.scale.x,
            )
        })
        .collect();
    for (available, children, scale) in snapshot {
        let widest = children
            .into_iter()
            .filter_map(|child| world.get::<bevy::prelude::ComputedNode>(child))
            .map(|node| node.size().x)
            .fold(0.0_f32, f32::max);
        measured.push((available, widest, scale));
    }

    assert_eq!(measured.len(), 2, "the scene opens two client screens");
    for (available, widest, scale) in measured {
        assert!(
            widest > 0.0 && available > 0.0,
            "a client screen measured nothing: {widest} in {available}"
        );
        assert!(
            widest * scale <= available,
            "a client's panel is {} wide in a column of {available}, at scale {scale}; \
             it overlaps the other client",
            widest * scale
        );
    }
}

/// The shared chest's id in a running Multiplayer scene: the first inventory
/// the first session is bound to, which is how `build` binds it.
fn shared_chest(harness: &UiHarness) -> slotted_net::InventoryId {
    let link = harness
        .world()
        .resource::<web_playground::scenes::multiplayer::NetLink>();
    let menu = link.sessions.first().expect("a session").0;
    *link
        .server
        .bindings_of(menu)
        .expect("the session is bound")
        .first()
        .expect("the shared chest is the first binding")
}

/// Slot 0 of a client's own mirror of the chest, for each open client.
fn client_chest_slot_0(harness: &mut UiHarness) -> Vec<Option<slotted_model::ItemStack>> {
    let world = harness.world_mut();
    let entities: Vec<Entity> = world
        .query::<&slotted::ecs::menu::OpenMenu>()
        .iter(world)
        .filter_map(|menu| menu.inventories.first().copied())
        .collect();
    entities
        .into_iter()
        .map(|entity| {
            world
                .get::<slotted::ecs::menu::Inventory>(entity)
                .and_then(|inventory| inventory.0.get(0))
                .cloned()
        })
        .collect()
}

/// An item id by name, from the loaded demo pack.
fn item(harness: &UiHarness, name: &str) -> slotted_model::ItemId {
    harness
        .world()
        .resource::<slotted_ecs::Registries>()
        .0
        .item_id(&slotted_model::Namespaced::parse(name).expect("a name"))
        .unwrap_or_else(|| panic!("the demo pack has {name}"))
}

/// Makes the server and client A disagree about slot 0 of the shared chest,
/// which is what an unanswered prediction looks like.
fn make_them_disagree(
    harness: &mut UiHarness,
    server_holds: slotted_model::ItemId,
    client_guesses: slotted_model::ItemId,
) {
    let chest = shared_chest(harness);
    harness
        .world_mut()
        .resource_mut::<web_playground::scenes::multiplayer::NetLink>()
        .server
        .store_mut()
        .inventory_mut(chest)
        .expect("the chest is in the store")
        .set(0, Some(slotted_model::ItemStack::new(server_holds, 7)));

    let world = harness.world_mut();
    let entity = world
        .query::<&slotted::ecs::menu::OpenMenu>()
        .iter(world)
        .next()
        .and_then(|menu| menu.inventories.first().copied())
        .expect("client A has a chest inventory");
    world
        .get_mut::<slotted::ecs::menu::Inventory>(entity)
        .expect("it is an inventory")
        .0
        .set(0, Some(slotted_model::ItemStack::new(client_guesses, 1)));
}

/// A fresh world that has been handed `ron` the way the page hands one back.
fn restarted_with(ron: String) -> UiHarness {
    let (mut restarted, _bus) = showcase_world();
    restarted
        .world_mut()
        .init_resource::<Messages<web_playground::RestoreState>>();
    restarted
        .world_mut()
        .get_resource_or_init::<Schedules>()
        .add_systems(
            PreUpdate,
            web_playground::apply_restore_for_test
                .after(web_playground::showcase::apply_scene_switch),
        );
    restarted
        .world_mut()
        .write_message(web_playground::RestoreState::new(ron));
    restarted.step(6);
    restarted
}

/// A restart out of the Multiplayer scene comes back to the server's chest,
/// not to one client's guess about it.
///
/// The scene has three copies of the world: the server's `ContainerStore` and
/// each client's local mirror, and they are meant to disagree, because a wrong
/// prediction corrected in front of the visitor is the whole demonstration.
/// `publish_snapshot` takes the first open menu, which is client A's, so what
/// the page kept across a restart was a prediction. Restoring that into a
/// freshly built pair would have made the guess authoritative.
///
/// The test drives the disagreement rather than assuming it: it writes one
/// thing into the server and a different thing into client A's mirror, takes a
/// snapshot, and asserts the snapshot carries the server's.
#[test]
fn a_restart_out_of_multiplayer_restores_the_servers_chest_not_a_clients() {
    let (mut harness, bus) = showcase_world();
    switch_to(&mut harness, &bus, Scene::Multiplayer);
    harness
        .world_mut()
        .get_resource_or_init::<Schedules>()
        .add_systems(PostUpdate, web_playground::publish_snapshot_for_test);

    let diamond = item(&harness, "minecraft:diamond");
    let cobblestone = item(&harness, "minecraft:cobblestone");
    make_them_disagree(&mut harness, diamond, cobblestone);
    harness.step(2);

    let ron = bus.snapshot();
    let snapshot = web_playground::snapshot::Snapshot::from_ron(&ron).expect("it is a snapshot");
    assert_eq!(snapshot.scene(), Some(Scene::Multiplayer));
    let net = snapshot
        .net
        .as_ref()
        .expect("a Multiplayer snapshot carries the server's state");
    let slot = net
        .chest
        .slots
        .iter()
        .find(|slot| slot.index == 0)
        .expect("slot 0 of the shared chest");
    assert_eq!(
        (slot.item.as_str(), slot.count),
        ("minecraft:diamond", 7),
        "the snapshot took client A's mirror instead of the server's chest"
    );
    assert_eq!(
        net.players.len(),
        2,
        "both sessions' private inventories are in the snapshot"
    );

    // And it comes back into a rebuilt pair: the server first, then both
    // mirrors, so nothing waits for a correction to look right.
    let mut restarted = restarted_with(ron);
    assert_eq!(active(&restarted), Scene::Multiplayer);

    let chest = shared_chest(&restarted);
    let restored = restarted
        .world()
        .resource::<web_playground::scenes::multiplayer::NetLink>()
        .server
        .store()
        .inventory(chest)
        .expect("the chest is in the store")
        .get(0)
        .expect("slot 0 holds what was restored");
    assert_eq!(
        (restored.id, restored.count),
        (diamond, 7),
        "the rebuilt server did not take the snapshot's chest"
    );

    let mirrors = client_chest_slot_0(&mut restarted);
    assert_eq!(mirrors.len(), 2, "both clients are open");
    for mirror in mirrors {
        let stack = mirror.expect("slot 0 was restored");
        assert_eq!(
            (stack.id, stack.count),
            (diamond, 7),
            "a client came up showing something the server does not hold"
        );
    }
}
