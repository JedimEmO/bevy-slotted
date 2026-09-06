//! The machine screen under `slotted_test`. Phase 6 contract section 3.3.
#![allow(clippy::unwrap_used)]

use std::time::Duration;

use machine::props;
use slotted_test::prelude::*;

/// Opens the screen with the machine idle: redstone mode 2 needs a signal
/// this world does not have, so nothing drains while a test looks at it.
fn open_idle_furnace() -> (UiHarness, Opened) {
    let (mut harness, opened) = open_furnace();
    set_property(&mut harness, opened.menu, props::REDSTONE_MODE, 2);
    harness.settle();
    (harness, opened)
}

fn open_furnace() -> (UiHarness, Opened) {
    open_furnace_with(machine::MachineSim { paused: false })
}

/// The furnace with its simulation switched off, so every readout on the
/// screen is the value `menu_def` declares and stays there however many
/// frames `settle()` runs. The snapshot test is the caller.
fn open_frozen_furnace() -> (UiHarness, Opened) {
    open_furnace_with(machine::MachineSim { paused: true })
}

fn open_furnace_with(sim: machine::MachineSim) -> (UiHarness, Opened) {
    let registries = machine::load_registries();
    let mut harness = UiHarness::builder()
        .plugins(SlottedPlugins::headless())
        .plugins(machine::MachineDemoPlugin)
        .registries(registries.clone())
        .resolution(1600.0, 900.0)
        .theme("glass")
        .build();
    harness.world_mut().insert_resource(sim);
    let opened = harness.open_screen(
        machine::furnace_screen(),
        (machine::menu_def(), machine::inventories(&registries)),
    );
    harness.settle();
    (harness, opened)
}

/// A host-side property write, the way the simulation makes one.
fn set_property(
    harness: &mut UiHarness,
    menu: bevy::prelude::Entity,
    id: slotted_model::PropertyId,
    value: i32,
) {
    harness.world_mut().trigger(slotted_ecs::SetProperty {
        entity: menu,
        id,
        value,
    });
    harness.step(1);
}

/// The stack a machine slot holds, by `test_id`.
fn stack(harness: &UiHarness, test_id: &str) -> Option<slotted_model::ItemStack> {
    harness.stack_at(harness.find(&by::test_id(test_id)))
}

/// A node's tag value.
fn tag(harness: &UiHarness, entity: bevy::prelude::Entity, key: &str) -> Option<String> {
    harness
        .world()
        .get::<slotted_ui::Tags>(entity)
        .and_then(|t| t.get(key).map(ToOwned::to_owned))
}

/// The screen opens with every Phase 6 node present.
#[test]
fn the_furnace_screen_opens() {
    let (harness, _) = open_furnace();
    assert!(harness.try_find(&by::test_id("tank")).is_some());
    assert!(harness.try_find(&by::test_id("cook")).is_some());
    assert!(harness.try_find(&by::test_id("tab_redstone")).is_some());
}

/// The whole tree, so a change to any Phase 6 widget's shape shows up in a
/// review diff rather than in a screenshot nobody opens.
///
/// The simulation is paused: a running furnace burns fuel and cooks while
/// the harness settles, so the readouts would carry whatever tick the
/// settle happened to stop on. The sim-driven values are asserted in
/// `cooking_moves_one_item_and_conserves_the_rest` and its neighbours.
#[test]
fn the_furnace_screen_matches_its_snapshot() {
    let (harness, _) = open_frozen_furnace();
    assert_tree_text_snapshot!(harness.screen_tree());
}

/// The tank's fill is the tank property over the capacity property, and it
/// follows a host write.
#[test]
fn the_tank_follows_its_property() {
    let (mut harness, opened) = open_idle_furnace();
    let tank = harness.find(&by::test_id("tank"));
    // `menu_def` starts at 3000 of 8000.
    assert!((harness.tank_fill(tank) - 0.375).abs() < 0.01);

    set_property(&mut harness, opened.menu, props::TANK, 6000);
    harness.settle();
    assert!((harness.tank_fill(tank) - 0.75).abs() < 0.01);
    assert_eq!(
        harness.property_of(tank),
        Some((props::TANK, 6000)),
        "the tank reports the property it is bound to"
    );
}

/// The energy bar reads the same pair of properties the screen file names.
#[test]
fn the_energy_bar_follows_its_property() {
    let (mut harness, opened) = open_idle_furnace();
    let energy = harness.find(&by::test_id("energy"));
    let fill = harness.fill_of(energy).unwrap();
    assert!((fill.max - 10_000.0).abs() < f32::EPSILON);

    set_property(&mut harness, opened.menu, props::ENERGY, 2500);
    harness.settle();
    assert!((harness.tank_fill(energy) - 0.25).abs() < 0.01);
}

/// Cooking advances the `cook` property and moves one item from the input to
/// the output, and nothing is created or destroyed on the way.
#[test]
fn cooking_moves_one_item_and_conserves_the_rest() {
    let (mut harness, opened) = open_furnace();
    // Fuel already burning, so the run consumes no coal and conservation
    // covers every item in the world.
    set_property(&mut harness, opened.menu, props::BURN, 1600);
    let before = stack(&harness, "input").unwrap();
    assert!(stack(&harness, "output").is_none());

    harness.advance(Duration::from_secs(1));
    let cook = harness.find(&by::test_id("cook"));
    let half = harness.fill_of(cook).unwrap();
    assert!(
        half.value > 0.0,
        "the progress arrow moved while cooking: {half:?}"
    );

    // Two seconds is one item at `COOK_PER_SECOND` against `cook_max` 200.
    harness.advance(Duration::from_millis(1200));
    let after = stack(&harness, "input").unwrap();
    let output = stack(&harness, "output").expect("one item reached the output");
    assert_eq!(after.count, before.count - 1);
    assert_eq!(output.id, before.id);
    harness.assert_conserved();
}

/// The redstone gate: mode `2` runs only while the signal is on.
#[test]
fn the_redstone_gate_stops_the_machine() {
    let (mut harness, opened) = open_furnace();
    set_property(&mut harness, opened.menu, props::BURN, 1600);
    set_property(&mut harness, opened.menu, props::REDSTONE_MODE, 2);
    harness.advance(Duration::from_secs(1));
    let cook = harness.find(&by::test_id("cook"));
    assert!(
        harness.fill_of(cook).unwrap().value < f32::EPSILON,
        "no signal, mode 2: the machine is idle"
    );

    harness.world_mut().insert_resource(machine::Redstone(true));
    harness.advance(Duration::from_millis(500));
    assert!(
        harness.fill_of(cook).unwrap().value > 0.0,
        "the signal came on and cooking started"
    );
}

/// The icon button cycles forward on a click and back on a shift-click, and
/// the property it is bound to follows.
#[test]
fn the_redstone_button_cycles_and_writes_its_property() {
    let (mut harness, _) = open_furnace();
    let tab = harness.find(&by::test_id("tab_redstone"));
    harness.toggle_side_tab(tab);
    harness.settle();

    let button = harness.find(&by::test_id("redstone_mode"));
    assert_eq!(tag(&harness, button, "state"), Some("ignore".to_owned()));

    harness.click(button);
    harness.settle();
    assert_eq!(tag(&harness, button, "state"), Some("low".to_owned()));
    assert_eq!(harness.property_of(button), Some((props::REDSTONE_MODE, 1)));

    harness.shift_click(button);
    harness.settle();
    assert_eq!(tag(&harness, button, "state"), Some("ignore".to_owned()));
    assert_eq!(harness.property_of(button), Some((props::REDSTONE_MODE, 0)));
}

/// The side tab opens on a click of its header and publishes an exclusion
/// zone, which is what keeps the item browser off it.
#[test]
fn a_side_tab_opens_and_excludes_the_browser() {
    let (mut harness, opened) = open_furnace();
    let tab = harness.find(&by::test_id("tab_sides"));
    assert_eq!(harness.side_tab_open(tab), Some(false));
    let before = harness.exclusion_zones(opened.screen).len();

    // The header is the tab root's own child, so scope the lookup to the
    // tab: the tab's label also appears as a heading inside its content.
    let header = harness.find(&by::tag("side_tab", "header").within(tab));
    harness.click(header);
    harness.settle();

    assert_eq!(harness.side_tab_open(tab), Some(true));
    assert!(
        harness.exclusion_zones(opened.screen).len() > before,
        "an open tab publishes an exclusion zone"
    );
}

/// The face pad the example registers as its own widget: six buttons, each
/// bound to its own property.
#[test]
fn the_face_pad_binds_six_properties() {
    let (mut harness, _) = open_furnace();
    let tab = harness.find(&by::test_id("tab_sides"));
    harness.toggle_side_tab(tab);
    harness.settle();

    for (face, name) in ["up", "left", "front", "right", "down", "back"]
        .into_iter()
        .enumerate()
    {
        let button = harness.find(&by::test_id(&format!("face_{name}")));
        let expected = slotted_model::PropertyId(props::FACES.0 + u16::try_from(face).unwrap());
        assert_eq!(
            harness.property_of(button).map(|(id, _)| id),
            Some(expected),
            "face {name} is bound to its own property"
        );
    }
}

/// The `sorter` mod's own `tests/sort.lua`, run through the harness the same
/// way `cargo xtask test-mods` runs it.
#[test]
fn the_sorter_mod_tests_pass() {
    let mut harness = UiHarness::builder()
        .plugins(SlottedPlugins::headless())
        .plugins(machine::MachineDemoPlugin)
        .resolution(1600.0, 900.0)
        .theme("glass")
        .mods_dir(machine::mods_dir())
        .build();
    harness
        .world_mut()
        .resource_mut::<Screens>()
        .register(machine::furnace_screen());
    let layout = harness.mod_layout_with_base();
    harness.load_mods(layout);

    let reports = harness.run_mod_tests("sorter");
    assert!(!reports.is_empty(), "sorter ships tests/sort.lua");
    for report in &reports {
        for result in &report.results {
            assert!(
                result.passed,
                "{}/{}: {}: {}",
                report.mod_id,
                report.file_name(),
                result.name,
                result.message.as_deref().unwrap_or("no message")
            );
        }
    }
}
