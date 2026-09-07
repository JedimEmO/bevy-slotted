//! The consumer path, end to end: a `ScreenDef` goes in, a driveable chest
//! screen comes out.
//!
//! This is the test the whole crate exists for, and it is the one that needs
//! all three Phase 2 packages at once: `slotted-ecs` to interpret clicks and
//! apply them to the model, `slotted-ui` to spawn slots and tooltips, and the
//! harness to drive them. Each one is written against
//! `docs/design/phase2-contract.md`, and each ends with `assert_conserved()`:
//! whatever a gesture does, the items that existed when the screen opened must
//! still exist.
#![allow(clippy::unwrap_used)]

use std::time::Duration;

use bevy::prelude::*;
use pretty_assertions::assert_eq;
use slotted_ecs::Modifiers;
use slotted_model::{InventoryRef, ToolbarAction};
use slotted_test::prelude::*;
use slotted_ui::{Layout, ScreenDef, Screens, Tags, TooltipTier, UiNodeDef, WidgetKind};

const CHEST: &str = "demo:chest";
const HOTBAR_FIRST: u16 = 27 + 27;

/// The screen the moodboard mocks up: a title, the chest grid, the player's
/// main inventory and hotbar, and the action rail down the side.
fn chest_screen() -> ScreenDef {
    let grid = |inventory: InventoryRef, rows: u16, first: u16, region: &str| UiNodeDef::SlotGrid {
        inventory,
        cols: 9,
        rows,
        first,
        tags: Tags::new().with("region", region),
    };
    ScreenDef {
        kind: ScreenKind::new(CHEST),
        initial_focus: None,
        presentation: slotted_ui::Presentation::default(),
        inherits: None,
        remove: vec![],
        root: UiNodeDef::Panel {
            role: slotted_theme::roles::PANEL,
            layout: Layout {
                gap: 1.0,
                padding: 1.0.into(),
                ..Default::default()
            },
            children: vec![
                UiNodeDef::Text {
                    key: slotted_ui::LocKey("chest.title".to_owned()),
                    style: slotted_ui::TextRole::Title,
                    tags: Tags::new().with("test_id", "title"),
                },
                grid(MenuDef::CONTAINER, 3, 0, "chest"),
                grid(MenuDef::PLAYER_MAIN, 3, 27, "player"),
                grid(MenuDef::PLAYER_HOTBAR, 1, HOTBAR_FIRST, "hotbar"),
                UiNodeDef::Custom {
                    kind: WidgetKind::new("slotted:action_rail"),
                    params: slotted_registry::Value::Unit,
                    children: vec![],
                    tags: Tags::new().with("test_id", "rail"),
                },
                UiNodeDef::Anchor {
                    id: slotted_ui::AnchorId::new("title_end"),
                },
            ],
            tags: Tags::new().with("test_id", "chest_panel"),
        },
        listring: vec![MenuDef::CONTAINER, MenuDef::PLAYER_MAIN],
    }
}

/// A harness with the chest screen registered, the fixture registries in
/// place and the glass theme loaded, showing a filled chest.
fn open_chest() -> (UiHarness, Opened) {
    let mut h = UiHarness::builder()
        .plugins(SlottedPlugins::headless())
        .registries(TestRegistries::basic())
        .resolution(1280.0, 720.0)
        .theme("glass")
        .build();
    h.world_mut()
        .resource_mut::<Screens>()
        .register(chest_screen());
    let opened = h.open_screen(ScreenKind::new(CHEST), ChestFixture::filled());
    h.settle();
    (h, opened)
}

fn chest_slot(h: &UiHarness, n: usize) -> Entity {
    h.find(&by::role(SemanticRole::Slot).tag("region", "chest").index(n))
}

/// The shape of the thing, before any interaction: the def's three grids
/// became 27 + 27 + 9 slots and the fixture's stacks reached them.
#[test]
fn the_screen_spawns_the_grids_the_def_asks_for() {
    let (h, opened) = open_chest();

    assert_eq!(h.find_all(&by::role(SemanticRole::Slot)).len(), 27 + 27 + 9);
    assert_eq!(
        h.find_all(&by::tag("region", "chest")).len(),
        28,
        "grid too"
    );
    assert_eq!(h.find(&by::screen(ScreenKind::new(CHEST))), opened.screen);
    assert!(h.try_find(&by::test_id("chest_panel")).is_some());
    assert!(h.try_find(&by::anchor("title_end")).is_some());

    // The model reached the widgets: slot 0 holds the fixture's 64 cobble.
    let first = chest_slot(&h, 0);
    let stack = h.stack_at(first).expect("the fixture filled chest slot 0");
    assert_eq!(stack.id, TestRegistries::item("minecraft:cobblestone"));
    assert_eq!(stack.count, 64);
    assert_eq!(h.displayed_stack(first), Some(stack));
    assert!(h.is_visible(first));

    // And an empty slot really is empty, not merely unrendered.
    assert_eq!(h.stack_at(chest_slot(&h, 3)), None);
    h.assert_conserved();
}

/// The headline promise from `docs/PLAN.md` 4.12, through real picking.
#[test]
fn shift_click_moves_a_stack_into_the_player_inventory() {
    let (mut h, _) = open_chest();
    let src = chest_slot(&h, 0);
    assert_eq!(h.stack_at(src).map(|s| s.count), Some(64));

    h.shift_click(src);
    h.settle();

    assert_eq!(h.stack_at(src), None, "the chest slot emptied");
    let dst = h.find(
        &by::role(SemanticRole::Slot)
            .tag("region", "player")
            .with_item("minecraft:cobblestone")
            .index(0),
    );
    // The player already held 9 cobble in main slot 14; quick-move fills that
    // partial stack to 64 first. `MenuDef::generic` routes a container slot
    // into the player in reverse, so the remaining 9 land in the last free
    // hotbar slot, not in main. Both regions have to be counted.
    let moved: u32 = ["player", "hotbar"]
        .into_iter()
        .flat_map(|region| {
            h.find_all(
                &by::role(SemanticRole::Slot)
                    .tag("region", region)
                    .with_item("minecraft:cobblestone"),
            )
        })
        .filter_map(|e| h.stack_at(e))
        .map(|s| s.count)
        .sum();
    assert_eq!(moved, 64 + 9, "nothing was created or destroyed");
    assert_eq!(
        h.stack_at(dst).map(|s| s.count),
        Some(64),
        "the partial stack filled first"
    );
    assert_eq!(h.carried(h.find(&by::screen(ScreenKind::new(CHEST)))), None);
    h.assert_conserved();
}

/// Right click picks up half. The 16-cap ender pearls are the interesting
/// case: 16 splits to 8, not to a stack the slot cannot hold.
#[test]
fn right_click_splits_a_stack_onto_the_cursor() {
    let (mut h, opened) = open_chest();
    let pearls = chest_slot(&h, 20);
    assert_eq!(h.stack_at(pearls).map(|s| s.count), Some(16));

    h.right_click(pearls);
    h.settle();

    assert_eq!(h.stack_at(pearls).map(|s| s.count), Some(8));
    let carried = h.carried(opened.menu).expect("half the pearls are carried");
    assert_eq!(carried.count, 8);
    assert_eq!(carried.id, TestRegistries::item("minecraft:ender_pearl"));

    // Put them back down: right-clicking the same slot places one.
    h.right_click(pearls);
    h.settle();
    assert_eq!(h.stack_at(pearls).map(|s| s.count), Some(9));
    assert_eq!(h.carried(opened.menu).map(|s| s.count), Some(7));
    h.assert_conserved();
}

/// A number key swaps the hovered slot with that hotbar slot.
#[test]
fn number_keys_swap_with_the_hotbar() {
    let (mut h, _) = open_chest();
    let chest = chest_slot(&h, 0);
    let hotbar_0 = h.find(
        &by::role(SemanticRole::Slot)
            .tag("region", "hotbar")
            .index(0),
    );

    let in_chest = h.stack_at(chest).expect("64 cobble");
    let in_hotbar = h.stack_at(hotbar_0).expect("the iron pickaxe");
    assert_eq!(in_hotbar.id, TestRegistries::item("minecraft:iron_pickaxe"));

    h.hover(chest);
    h.key(KeyCode::Digit1);
    h.settle();

    assert_eq!(h.stack_at(chest), Some(in_hotbar));
    assert_eq!(h.stack_at(hotbar_0), Some(in_chest));
    h.assert_conserved();
}

/// The action rail's sort button reaches the model as a `ToolbarAction`.
#[test]
fn the_action_rail_sorts_the_chest() {
    let (mut h, _) = open_chest();
    let rail = h.find(&by::widget_kind(WidgetKind::new("slotted:action_rail")));
    let sort = h.find(
        &by::role(SemanticRole::Button)
            .tag("action", "sort")
            .within(rail),
    );

    // Before: gaps at 3, 6, 7, 8 and content scattered to slot 20.
    assert_eq!(h.stack_at(chest_slot(&h, 3)), None);
    let before: u32 = (0..27)
        .filter_map(|i| h.stack_at(chest_slot(&h, i)))
        .map(|s| s.count)
        .sum();

    h.click(sort);
    h.settle();

    // After: the same items, compacted from slot 0 with no interior gap.
    let counts: Vec<Option<u32>> = (0..27)
        .map(|i| h.stack_at(chest_slot(&h, i)).map(|s| s.count))
        .collect();
    let filled = counts.iter().take_while(|c| c.is_some()).count();
    assert!(counts[filled..].iter().all(Option::is_none), "no gaps left");
    assert_eq!(counts.iter().flatten().sum::<u32>(), before);
    // The two cobble stacks merged: 64 + 23 becomes 64 and 23 no longer.
    let cobble: Vec<u32> = (0..27)
        .filter_map(|i| h.stack_at(chest_slot(&h, i)))
        .filter(|s| s.id == TestRegistries::item("minecraft:cobblestone"))
        .map(|s| s.count)
        .collect();
    assert_eq!(cobble, [64, 23]);

    // The same action by its semantic name reaches the same place.
    h.menu_action(
        h.find(&by::screen(ScreenKind::new(CHEST))),
        ClickAction::Toolbar(ToolbarAction::Sort {
            inventory: MenuDef::CONTAINER,
        }),
    );
    h.settle();
    h.assert_conserved();
}

/// Hovering a slot composes a tooltip, and only after the hover delay.
#[test]
fn a_tooltip_appears_after_the_hover_delay() {
    let (mut h, _) = open_chest();
    assert!(h.tooltip().is_none(), "nothing is hovered yet");

    h.hover(chest_slot(&h, 0));
    h.advance(Duration::from_millis(500));
    h.settle();

    let tooltip = h.tooltip().expect("a hovered slot shows a tooltip");
    assert_eq!(tooltip.tier, TooltipTier::Compact);
    assert!(!tooltip.parts.is_empty(), "name, count and rarity at least");

    // Shift promotes it to the expanded tier without moving the pointer.
    h.request_tooltip(chest_slot(&h, 0), TooltipTier::Expanded);
    h.settle();
    assert_eq!(h.tooltip().map(|t| t.tier), Some(TooltipTier::Expanded));

    // Moving off an item drops it again.
    h.hover(chest_slot(&h, 3));
    h.settle();
    assert!(h.tooltip().is_none(), "the empty slot has nothing to say");
    h.assert_conserved();
}

/// The semantic tree is the crate's stable contract, so it gets a snapshot.
/// A theme swap must not move it; a role, tag or label change must.
#[test]
fn the_chest_screen_tree_is_stable() {
    let (h, _) = open_chest();
    assert_tree_text_snapshot!("chest_screen_tree", h.screen_tree());
    assert_tree_snapshot!("chest_screen_tree_ron", h.screen_tree());
    h.assert_conserved();
}

/// The semantic action path skips layout and picking entirely, and must land
/// on the same model state as the pointer path.
#[test]
fn the_semantic_and_pointer_paths_agree() {
    let (mut h, _) = open_chest();
    let src = chest_slot(&h, 0);
    h.click_slot(src, slotted_model::Button::Left, Modifiers::SHIFT);
    h.settle();
    let by_event = h.stack_at(src);

    let (mut h2, _) = open_chest();
    let src2 = chest_slot(&h2, 0);
    h2.shift_click(src2);
    h2.settle();

    assert_eq!(by_event, h2.stack_at(src2));

    // A pointer press also moves input focus, which the semantic path does
    // not touch: it skips picking by design. Focus the same slot so the
    // comparison is about everything else.
    h.set_focus(Some(src));
    h.settle();
    assert_eq!(h.screen_tree(), h2.screen_tree());
    h.assert_conserved();
    h2.assert_conserved();
}
