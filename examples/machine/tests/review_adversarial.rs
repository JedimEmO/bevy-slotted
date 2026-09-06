//! The machine example under abuse: the simulation left running, and a mod
//! test written wrong.
//!
//! `ui.rs` proves the screen works. This file asks the two questions a
//! reviewer asks about a simulation that writes slots: does it ever create or
//! destroy an item it should not, and when a mod's test is wrong, does the
//! modder get a message they can act on.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::BTreeMap;
use std::time::Duration;

use bevy::prelude::*;
use machine::props;
use slotted_model::{ItemId, ItemStack};
use slotted_test::prelude::*;

fn open_furnace() -> (UiHarness, Opened) {
    let registries = machine::load_registries();
    let mut harness = UiHarness::builder()
        .plugins(SlottedPlugins::headless())
        .plugins(machine::MachineDemoPlugin)
        .registries(registries.clone())
        .resolution(1600.0, 900.0)
        .theme("glass")
        // A quarter-second frame: the simulation integrates per second
        // through its own carry accumulators, so a coarser frame is the same
        // simulation and a thousand virtual seconds is four thousand frames
        // rather than sixty thousand.
        .frame_delta(Duration::from_millis(250))
        .build();
    let opened = harness.open_screen(
        machine::furnace_screen(),
        (machine::menu_def(), machine::inventories(&registries)),
    );
    harness.settle();
    (harness, opened)
}

fn set_property(harness: &mut UiHarness, menu: Entity, id: slotted_model::PropertyId, value: i32) {
    harness.world_mut().trigger(slotted_ecs::SetProperty {
        entity: menu,
        id,
        value,
    });
    harness.step(1);
}

/// Every item in every inventory in the world, by kind.
///
/// The harness has its own census behind `assert_conserved`, which is all or
/// nothing. This one is per kind, because the machine's rules are per kind:
/// fuel is consumed and everything else is conserved.
fn census(harness: &UiHarness) -> BTreeMap<ItemId, u64> {
    let world = harness.world();
    let mut counts: BTreeMap<ItemId, u64> = BTreeMap::new();
    let mut query = world
        .try_query::<&slotted_ecs::Inventory>()
        .expect("Inventory is a component");
    for inventory in query.iter(world) {
        for stack in inventory.slots().iter().flatten() {
            *counts.entry(stack.id).or_default() += u64::from(stack.count);
        }
    }
    counts
}

/// A host slot write, the way the simulation makes one.
fn set_slot(
    harness: &mut UiHarness,
    menu: Entity,
    slot: slotted_model::SlotIx,
    stack: Option<ItemStack>,
) {
    harness.world_mut().trigger(slotted_ecs::SetSlot {
        entity: menu,
        slot,
        stack,
    });
    harness.step(1);
}

fn stack(harness: &UiHarness, test_id: &str) -> Option<ItemStack> {
    harness.stack_at(harness.find(&by::test_id(test_id)))
}

fn item(harness: &UiHarness, name: &str) -> ItemId {
    harness
        .world()
        .resource::<slotted_ecs::Registries>()
        .0
        .items
        .id_of(&slotted_model::Namespaced::parse(name).expect("a valid id"))
        .expect("the demo pack registers it")
}

/// The example's conservation rule, stated once so the test and the reader
/// agree on it:
///
/// - **Fuel is consumed.** Burning takes one item out of the fuel slot and
///   turns it into `burn_max` units of burn time. That item is gone, and no
///   `Dropped` event stands in for it: the machine is the authority over its
///   own slots and this is what a furnace does.
/// - **Everything else is conserved exactly.** Cooking's result is the input
///   item, so one item leaves the input slot and one identical item arrives in
///   the output slot on the same frame. No recipe table means no
///   transmutation, so nothing may appear or vanish.
///
/// A thousand virtual seconds is long enough for the input to run out, the
/// fuel to run out, the tank and the energy bar to bottom out, and the
/// simulation to keep ticking against empty slots, which is where an
/// off-by-one destroys an item.
#[test]
fn the_simulation_conserves_everything_but_the_fuel_it_burns() {
    let (mut harness, opened) = open_furnace();
    // Mode 2 needs a redstone signal this world does not have, so the machine
    // holds still while the baseline is taken. `open_screen` settles, and the
    // settle runs the simulation: a census taken before this would already be
    // missing whatever those frames cooked.
    set_property(&mut harness, opened.menu, props::REDSTONE_MODE, 2);
    harness.settle();

    let coal = item(&harness, "minecraft:coal");
    let cobblestone = item(&harness, "minecraft:cobblestone");

    // Stock the machine, the way a hopper would. `SetSlot` is the host write
    // the simulation itself uses, so this is not a back door.
    set_slot(
        &mut harness,
        opened.menu,
        machine::slots::INPUT,
        Some(ItemStack::new(cobblestone, 64)),
    );
    set_slot(
        &mut harness,
        opened.menu,
        machine::slots::FUEL,
        Some(ItemStack::new(coal, 64)),
    );
    harness.step(2);

    let before = census(&harness);
    let fuel_before = stack(&harness, "fuel").map_or(0, |s| u64::from(s.count));
    let input_before = stack(&harness, "input").map_or(0, |s| u64::from(s.count));
    assert!(fuel_before > 0, "there is fuel to burn");
    assert!(input_before > 0, "and something to cook");

    // Mode 0 is "ignore the signal": the machine runs flat out from here, for
    // long enough to empty the input, empty the fuel, bottom out the tank and
    // the energy bar, and go on ticking against empty slots.
    set_property(&mut harness, opened.menu, props::REDSTONE_MODE, 0);
    for _ in 0..1000 {
        harness.advance(Duration::from_secs(1));
    }
    harness.settle();

    let after = census(&harness);
    let kinds: std::collections::BTreeSet<ItemId> =
        before.keys().chain(after.keys()).copied().collect();
    let mut wrong = Vec::new();
    for kind in kinds {
        let (was, now) = (
            before.get(&kind).copied().unwrap_or(0),
            after.get(&kind).copied().unwrap_or(0),
        );
        if kind == coal {
            // The one item the machine may destroy, and only downwards.
            if now > was {
                wrong.push(format!("coal was created: {was} -> {now}"));
            }
        } else if was != now {
            wrong.push(format!("{kind:?}: {was} -> {now}"));
        }
    }
    assert!(
        wrong.is_empty(),
        "1000 virtual seconds created or destroyed items:\n  {}",
        wrong.join("\n  ")
    );

    // And the machine really ran, so the assertion above is not passing
    // because nothing happened.
    let fuel_after = stack(&harness, "fuel").map_or(0, |s| u64::from(s.count));
    let input_after = stack(&harness, "input").map_or(0, |s| u64::from(s.count));
    assert!(
        fuel_after < fuel_before,
        "fuel was burnt: {fuel_before} -> {fuel_after}"
    );
    assert!(
        input_after < input_before,
        "and input was cooked: {input_before} -> {input_after}"
    );
}

/// The simulation keeps ticking after everything is empty. Nothing may go
/// negative and no slot may fill itself back up.
#[test]
fn a_machine_that_has_run_dry_stays_dry() {
    let (mut harness, opened) = open_furnace();
    set_property(&mut harness, opened.menu, props::REDSTONE_MODE, 0);
    for _ in 0..1000 {
        harness.advance(Duration::from_secs(1));
    }
    harness.settle();

    for id in ["burn", "cook", "energy", "tank"] {
        let fill = harness
            .fill_of(harness.find(&by::test_id(id)))
            .expect("a FillValue");
        assert!(
            fill.value >= 0.0,
            "{id} did not run past zero: {:?}",
            fill.value
        );
        assert!(fill.fraction().is_finite(), "{id} has a real fraction");
    }
    let dry = census(&harness);
    harness.advance(Duration::from_mins(1));
    harness.settle();
    assert_eq!(
        census(&harness),
        dry,
        "a machine with nothing to do changes nothing"
    );
}

// ---------------------------------------------------------------------------
// Mod tests written wrong
// ---------------------------------------------------------------------------

fn mods_harness() -> UiHarness {
    let mut harness = UiHarness::builder()
        .plugins(SlottedPlugins::headless())
        .resolution(1600.0, 900.0)
        .theme("glass")
        .mods_dir(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("mods"))
        .build();
    harness.register_fixture("chest", ChestFixture::empty());
    // What `test-mods` does with `<mods dir>/../screens/*.screen.ron`: a mod
    // test opens a screen the game owns, and no mod registers that one.
    harness
        .world_mut()
        .resource_mut::<Screens>()
        .register(machine::furnace_screen());
    let layout = harness.mod_layout_with_base();
    harness.load_mods(layout);
    harness
}

/// `expect_stack` on a slot the fixture does not have is the commonest way to
/// get a mod test wrong: a fixture of 27 slots and an index of 99. It fails
/// the test rather than taking the runner down, and the message says the slot
/// and what was expected there, which is what a modder needs to fix it.
#[test]
fn expect_stack_on_a_missing_slot_fails_with_a_message_a_modder_can_use() {
    let mut harness = mods_harness();
    let source = r#"
local t = slotted.test

t.test("a slot that is not there", function()
    t.open_screen("machine:furnace", { slots = { 3, 27, 9 } })
    t.expect_stack({ test_id = "no_such_slot" }, "minecraft:coal", 1)
end)
"#;
    let report = harness.run_lua_tests(
        "sorter",
        std::path::Path::new("tests/missing_slot.lua"),
        source,
    );
    let result = report.results.first().expect("one test ran");
    assert!(!result.passed, "the test failed rather than passing");
    let message = result.message.clone().unwrap_or_default();
    assert!(
        !message.is_empty(),
        "a failed expectation carries a message"
    );
    // Readable means: it names what could not be found, not an index into
    // some internal vector and not a bare `None`.
    assert!(
        message.contains("no_such_slot"),
        "the message names the locator that found nothing: {message}"
    );
    assert!(
        !message.contains("panicked"),
        "a wrong test is a failure, not a panic: {message}"
    );
}

/// The other half of the same mistake: the slot is there, but it does not
/// hold what the test said. The message has to name both stacks or a modder
/// cannot tell an empty slot from a wrong count.
#[test]
fn expect_stack_on_a_slot_holding_something_else_says_what_it_found() {
    let mut harness = mods_harness();
    let source = r#"
local t = slotted.test

t.test("an empty slot is not a coal slot", function()
    t.open_screen("machine:furnace", { slots = { 3, 27, 9 } })
    t.expect_stack({ test_id = "output" }, "minecraft:coal", 1)
end)
"#;
    let report = harness.run_lua_tests("sorter", std::path::Path::new("tests/wrong.lua"), source);
    let result = report.results.first().expect("one test ran");
    assert!(!result.passed);
    let message = result.message.clone().unwrap_or_default();
    assert!(
        message.contains("minecraft:coal"),
        "the message says what was expected: {message}"
    );
    assert!(
        !message.contains("panicked"),
        "a wrong expectation is a failure, not a panic: {message}"
    );
}

/// A wrong locator anywhere else fails the same way: one failed test, a
/// message, and the rest of the file still runs.
#[test]
fn a_bad_locator_fails_one_test_and_leaves_the_others_running() {
    let mut harness = mods_harness();
    let source = r#"
local t = slotted.test

t.test("first, wrong", function()
    t.open_screen("machine:furnace", { slots = { 3, 27, 9 } })
    t.click({ test_id = "nothing_is_called_this" })
end)

t.test("second, right", function()
    t.open_screen("machine:furnace", { slots = { 3, 27, 9 } })
    t.expect(true, "arithmetic still works")
end)
"#;
    let report = harness.run_lua_tests("sorter", std::path::Path::new("tests/two.lua"), source);
    assert_eq!(report.results.len(), 2, "both tests ran");
    assert!(!report.results[0].passed, "the wrong one failed");
    assert!(
        report.results[0]
            .message
            .as_deref()
            .is_some_and(|m| m.contains("nothing_is_called_this")),
        "and named the locator: {:?}",
        report.results[0].message
    );
    assert!(
        report.results[1].passed,
        "the failure did not stop the file: {:?}",
        report.results[1].message
    );
}

/// A mod that ships no `tests/` directory is not a failure. `run_mod_tests`
/// returns no reports, which is what makes `test-mods` print
/// `0 passed, 0 failed in 0 file(s)` and exit 0: its exit code is
/// `failed == 0`, and a mod with nothing to run fails nothing.
#[test]
fn a_mod_without_tests_reports_nothing_and_fails_nothing() {
    let mut harness = mods_harness();
    let reports = harness.run_mod_tests("no_such_mod");
    assert!(
        reports.is_empty(),
        "no tests directory means no reports: {reports:?}"
    );
    let failed: usize = reports
        .iter()
        .flat_map(|r| &r.results)
        .filter(|r| !r.passed)
        .count();
    assert_eq!(failed, 0, "and so nothing failed, which is exit code 0");
}
