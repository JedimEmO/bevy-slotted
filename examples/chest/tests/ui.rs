//! What a consumer's own UI test suite looks like.
//!
//! These tests open `assets/screens/demo_chest.screen.ron` and the content of
//! `assets/data/demo/` — the same two files `cargo run -p chest` reads — with
//! no window, no GPU and no wall clock, then drive them through the public
//! harness. Nothing here reaches into the example's internals: it is all
//! `slotted_test` plus the three functions `chest` exposes for its own
//! `main.rs`.
//!
//! The theme is found because `examples/chest/assets` is a symlink to the
//! workspace `assets/`, which is where Bevy's `AssetPlugin` looks when it
//! resolves against `CARGO_MANIFEST_DIR`.
#![allow(clippy::unwrap_used)]

use bevy::prelude::*;
use chest::{ChestBinding, ChestDemoPlugin};
use pretty_assertions::assert_eq;
use slotted_model::{InventoryRef, ToolbarAction};
use slotted_test::prelude::*;

/// The demo, headless: the RON screen over the RON content, with the demo's
/// own key bindings attached so `Esc` and `E` behave as they do on screen.
fn open_demo_chest() -> (UiHarness, Opened) {
    let registries = chest::load_registries();
    let mut harness = UiHarness::builder()
        .plugins(SlottedPlugins::headless())
        .plugins(ChestDemoPlugin)
        .registries(registries.clone())
        .resolution(1600.0, 900.0)
        .theme("glass")
        .build();
    let opened = harness.open_screen(
        chest::demo_screen(),
        (chest::menu_def(), chest::inventories(&registries)),
    );
    harness.settle();
    (harness, opened)
}

fn chest_slot(harness: &UiHarness, n: usize) -> Entity {
    harness.find(&by::role(SemanticRole::Slot).tag("region", "chest").index(n))
}

fn item(harness: &UiHarness, name: &str) -> ItemId {
    let id = slotted_model::Namespaced::parse(name).unwrap();
    harness
        .world()
        .resource::<Registries>()
        .item_id(&id)
        .unwrap_or_else(|| panic!("{name} is not registered"))
}

/// The whole screen, as the harness sees it. If the RON file or a widget's
/// spawned shape changes, this is the test that says so.
#[test]
fn the_screen_tree_matches_the_ron_file() {
    let (harness, _) = open_demo_chest();

    // The three grids of the screen file: 27 + 27 + 9.
    assert_eq!(harness.find_all(&by::role(SemanticRole::Slot)).len(), 63);
    assert_eq!(harness.find_all(&by::role(SemanticRole::Hotbar)).len(), 1);
    assert!(harness.try_find(&by::anchor("title_end")).is_some());
    assert_eq!(
        harness
            .text_of(harness.find(&by::test_id("title")))
            .as_deref(),
        Some("Copper Chest")
    );

    // Nine of the chest's twenty-seven slots start filled.
    assert_eq!(
        harness
            .text_of(harness.find(&by::test_id("capacity")))
            .as_deref(),
        Some("9 / 27 slots")
    );

    assert_tree_snapshot!(harness.screen_tree());
    harness.assert_conserved();
}

/// The content files reached the widgets: the stack the data stage built is
/// the stack the slot renders.
#[test]
fn the_data_stage_content_reaches_the_slots() {
    let (harness, _) = open_demo_chest();

    let first = chest_slot(&harness, 0);
    let stack = harness.stack_at(first).expect("chest slot 0 is filled");
    assert_eq!(stack.id, item(&harness, "minecraft:cobblestone"));
    assert_eq!(stack.count, 64);
    assert_eq!(harness.displayed_stack(first), Some(stack));

    // The ender pearl caps at sixteen, which is what its item file says.
    let pearls = harness.stack_at(chest_slot(&harness, 20)).unwrap();
    assert_eq!(pearls.count, 16);
    assert_eq!(
        harness
            .world()
            .resource::<Registries>()
            .items
            .get(pearls.id)
            .unwrap()
            .max_stack_size,
        16
    );

    // An empty slot is empty, not merely unrendered.
    assert_eq!(harness.stack_at(chest_slot(&harness, 3)), None);
    harness.assert_conserved();
}

/// Shift-click sends a stack across the listring and back, and nothing is
/// created or destroyed on the way.
#[test]
fn a_shift_click_round_trip_conserves_every_item() {
    let (mut harness, opened) = open_demo_chest();

    let source = chest_slot(&harness, 0);
    let cobblestone = harness.stack_at(source).unwrap();
    assert_eq!(cobblestone.count, 64);

    harness.shift_click(source);
    harness.settle();
    assert_eq!(
        harness.stack_at(source),
        None,
        "the whole stack left the chest"
    );

    // It landed somewhere in the player's inventories.
    let moved: u32 = (0..63)
        .filter_map(|i| harness.stack_at(harness.find(&by::role(SemanticRole::Slot).index(i))))
        .filter(|s| s.id == cobblestone.id)
        .map(|s| s.count)
        .sum();
    assert_eq!(
        moved,
        64 + 23 + 9,
        "the chest's two stacks plus the player's"
    );

    // Send it back from wherever it went.
    let landed = harness.find(
        &by::role(SemanticRole::Slot)
            .tag("region", "player")
            .with_item("minecraft:cobblestone"),
    );
    harness.shift_click(landed);
    harness.settle();

    assert_eq!(opened.inventories.len(), 3);
    harness.assert_conserved();
}

/// The rail button in the screen file is wired to the model's toolbar action.
#[test]
fn the_sort_rail_button_sorts_the_container() {
    let (mut harness, opened) = open_demo_chest();

    let sort = harness.find(&by::tag("action", "sort"));
    assert_eq!(harness.text_of(sort).as_deref(), Some("Sort"));

    // Before: the chest has gaps at 3, 7, 8 and everything after 20.
    assert_eq!(harness.stack_at(chest_slot(&harness, 3)), None);
    let filled_before = (0..27)
        .filter(|i| harness.stack_at(chest_slot(&harness, *i)).is_some())
        .count();

    harness.activate(sort);
    harness.settle();

    // After: the same number of stacks, packed against the front.
    let filled_after = (0..27)
        .filter(|i| harness.stack_at(chest_slot(&harness, *i)).is_some())
        .count();
    assert!(
        filled_after <= filled_before,
        "sorting merges partial stacks, it never adds them"
    );
    for i in 0..filled_after {
        assert!(
            harness.stack_at(chest_slot(&harness, i)).is_some(),
            "slot {i} should be packed"
        );
    }
    assert_eq!(harness.stack_at(chest_slot(&harness, filled_after)), None);

    // The two cobblestone stacks merged into one full stack.
    let cobblestone = item(&harness, "minecraft:cobblestone");
    let total: u32 = (0..27)
        .filter_map(|i| harness.stack_at(chest_slot(&harness, i)))
        .filter(|s| s.id == cobblestone)
        .map(|s| s.count)
        .sum();
    assert_eq!(total, 64 + 23);

    // The rail's action is the one the screen file named.
    let action = harness
        .world()
        .get::<slotted::ui::RailAction>(sort)
        .expect("a rail button carries its action");
    assert_eq!(
        action.0,
        ToolbarAction::Sort {
            inventory: InventoryRef::new(0)
        }
    );

    assert_eq!(
        opened.menu,
        harness
            .world()
            .resource::<ChestBinding>()
            .open
            .unwrap()
            .menu
    );
    harness.assert_conserved();
}

/// `Esc` and `E` through real key events, the same path a player's keyboard
/// takes.
#[test]
fn esc_closes_the_screen_and_e_opens_it_again() {
    let (mut harness, opened) = open_demo_chest();
    assert!(
        harness
            .try_find(&by::screen(ScreenKind::new(chest::CHEST)))
            .is_some()
    );

    harness.key(KeyCode::Escape);
    harness.settle();
    assert!(
        harness
            .try_find(&by::screen(ScreenKind::new(chest::CHEST)))
            .is_none(),
        "Esc despawns the screen"
    );
    assert!(
        harness.world().get_entity(opened.menu).is_err(),
        "and its menu"
    );
    assert!(harness.world().resource::<ChestBinding>().open.is_none());

    harness.key(KeyCode::KeyE);
    harness.settle();
    let reopened = harness.find(&by::screen(ScreenKind::new(chest::CHEST)));
    assert_ne!(reopened, opened.screen, "a fresh screen entity");
    assert_eq!(harness.find_all(&by::role(SemanticRole::Slot)).len(), 63);

    // The same chest, not a new one: the inventory entities outlived the close
    // and the stacks are still where they were.
    let binding = harness.world().resource::<ChestBinding>().clone();
    assert_eq!(binding.inventories, opened.inventories);
    assert_eq!(harness.stack_at(chest_slot(&harness, 0)).unwrap().count, 64);
    assert_eq!(
        harness
            .text_of(harness.find(&by::test_id("capacity")))
            .as_deref(),
        Some("9 / 27 slots")
    );

    harness.assert_conserved();
}
