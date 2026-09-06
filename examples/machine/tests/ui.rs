//! The machine screen under `slotted_test`. Phase 6 contract section 3.3.
#![allow(clippy::unwrap_used)]

use slotted_test::prelude::*;

fn open_furnace() -> (UiHarness, Opened) {
    let registries = machine::load_registries();
    let mut harness = UiHarness::builder()
        .plugins(SlottedPlugins::headless())
        .plugins(machine::MachineDemoPlugin)
        .registries(registries.clone())
        .resolution(1600.0, 900.0)
        .theme("glass")
        .build();
    let opened = harness.open_screen(
        machine::furnace_screen(),
        (machine::menu_def(), machine::inventories(&registries)),
    );
    harness.settle();
    (harness, opened)
}

/// The screen opens with every Phase 6 node present, today as placeholders.
#[test]
fn the_furnace_screen_opens() {
    let (harness, _) = open_furnace();
    assert!(harness.try_find(&by::test_id("tank")).is_some());
    assert!(harness.try_find(&by::test_id("cook")).is_some());
    assert!(harness.try_find(&by::test_id("tab_redstone")).is_some());
}

// PHASE6-IMPL: C. Tree snapshot; cooking advances `cook` and moves an item;
// tank fill from the property; redstone icon button cycles by click and
// shift-click and the property follows; side tab opens on click and
// publishes an exclusion zone the browser respects; `run_mod_tests("sorter")`
// all pass; `assert_conserved` after the sim ran.
#[test]
#[ignore = "PHASE6-IMPL: C"]
fn the_sorter_mod_tests_pass() {}
