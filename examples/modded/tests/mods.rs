//! Phase 4 verification: a mod adds an item, recipe, screen and tooltip part
//! without Rust, and a control script reloads without losing inventory state.

use modded::layout;
use slotted_test::prelude::*;

fn harness() -> UiHarness {
    UiHarness::builder()
        .plugins(SlottedPlugins::headless())
        .theme("glass")
        .build()
}

#[test]
#[ignore = "PHASE4-IMPL: C"]
fn mod_item_appears_in_a_slot() {
    let mut h = harness();
    let _report = h.load_mods(layout(&modded::mods_dir()));
    // open_mod_screen(CHEST_SCREEN, fixture with a copper chest stack);
    // h.find(&by::role(SemanticRole::Slot).with_item("copper_chest:copper_chest"));
    h.assert_conserved();
}

#[test]
#[ignore = "PHASE4-IMPL: C"]
fn mod_recipe_is_listed_in_the_browser() {}

#[test]
#[ignore = "PHASE4-IMPL: C"]
fn tooltip_part_renders_for_foods() {}

#[test]
#[ignore = "PHASE4-IMPL: C"]
fn control_script_reload_keeps_inventory_state() {
    // copy mods/ to a temp dir, load, click a slot, assert the log line,
    // rewrite control.lua, h.reload_mod("copper_chest"), click again, assert
    // the new line, stack_at unchanged, assert_conserved.
}
