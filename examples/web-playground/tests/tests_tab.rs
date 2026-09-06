//! The Tests tab, natively, over the whole path the page walks.
//!
//! The browser's route is `Request::RunTests` -> `StartTests` ->
//! `LiveTestRunner` -> one `TestOp` per frame against the live world -> `ok`
//! and `FAIL` lines on the bus. Contract 3.2 chose the live app over a second
//! headless harness inside the page, so the only way to prove the route works
//! is to give it a live app. `UiHarness` is one: headless, but a real
//! `slotted` world with screens, slots and picking.
//!
//! The mods come from `examples/modded/mods`, which is what
//! `bundle::FILES` is generated from, so the test file the runner loads out of
//! the bundle is the one the mod on disk ships.
#![allow(clippy::unwrap_used)]

use bevy::prelude::*;
use slotted_test::prelude::*;
use web_playground::bus::{Bus, Request};

const CHEST: &str = "copper_chest:chest";

/// A live world with the modded example's three mods loaded and its chest
/// screen open, plus the playground's own two systems on it.
fn playground_world() -> (UiHarness, Bus) {
    let mut harness = UiHarness::builder()
        .plugins(SlottedPlugins::headless())
        .mods_dir(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../modded/mods"))
        .resolution(1600.0, 900.0)
        .theme("glass")
        .build();
    // Before the mods load: the data stage needs a runtime, and this crate's
    // is luaur, the one the browser gets.
    harness
        .world_mut()
        .insert_resource(web_playground::runtime());
    let layout = harness.mod_layout();
    harness.load_mods(layout);

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

/// Everything the runner logged under the `test` source.
fn test_lines(bus: &Bus) -> Vec<String> {
    bus.drain_console()
        .into_iter()
        .filter(|line| line.who == "test")
        .map(|line| line.text)
        .collect()
}

/// Pressing Run tests in the page runs the mod's bundled file against the
/// screen that is open, and every test in it passes.
#[test]
fn the_tests_tab_runs_a_mods_bundled_tests_against_the_live_app() {
    let (mut harness, bus) = playground_world();
    let registries = harness
        .world()
        .resource::<slotted_ecs::Registries>()
        .0
        .clone();
    harness.open_mod_screen(
        CHEST,
        (
            showcase::mods::menu_def(),
            showcase::mods::inventories(&registries),
        ),
    );
    harness.settle();

    // What the Run button sends.
    bus.request(Request::RunTests {
        mod_id: "sorter".to_owned(),
    });
    // `drain_requests` is `plumbing.rs`'s; from here the path is this crate's
    // `begin_tests` and the runner, one op per frame.
    let mod_id = match bus.take_requests().into_iter().next().unwrap() {
        Request::RunTests { mod_id } => mod_id,
        other => panic!("expected RunTests, got {other:?}"),
    };
    harness
        .world_mut()
        .write_message(web_playground::StartTests { mod_id });

    for _ in 0..400 {
        harness.step(1);
        if harness
            .world()
            .get_resource::<web_playground::tests::LiveTestRunner>()
            .is_none()
            && harness
                .world()
                .resource::<Messages<web_playground::StartTests>>()
                .is_empty()
        {
            break;
        }
    }

    let lines = test_lines(&bus);
    assert!(
        lines.iter().any(|l| l.contains("2 passed, 0 failed")),
        "the run finished green; lines: {lines:#?}"
    );
    assert!(
        !lines.iter().any(|l| l.starts_with("FAIL")),
        "no test failed; lines: {lines:#?}"
    );
}

/// Run tests on a mod that bundles none says so and leaves no runner behind.
#[test]
fn a_mod_without_tests_is_reported_and_starts_no_run() {
    let (mut harness, bus) = playground_world();
    harness
        .world_mut()
        .write_message(web_playground::StartTests {
            mod_id: "appleskin_like".to_owned(),
        });
    harness.step(2);

    let lines = test_lines(&bus);
    assert!(
        lines.iter().any(|l| l.contains("bundles no tests")),
        "the page is told why nothing ran; lines: {lines:#?}"
    );
    assert!(
        harness
            .world()
            .get_resource::<web_playground::tests::LiveTestRunner>()
            .is_none(),
        "no run is left in flight"
    );
}
