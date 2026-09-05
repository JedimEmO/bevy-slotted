//! Toolbar actions: sort, quick stack, deposit all, loot all, favorites.
#![allow(clippy::unwrap_used)]

mod common;

use common::{DIRT, EGG, Fixture, STONE, SWORD, stack};
use pretty_assertions::assert_eq;
use slotted_model::{ClickAction, ClickError, MenuDef, SlotBehaviour, SlotIx, ToolbarAction};

fn sort(inventory: slotted_model::InventoryRef) -> ClickAction {
    ClickAction::Toolbar(ToolbarAction::Sort { inventory })
}

#[test]
fn sort_groups_by_id_merges_and_orders_by_count() {
    let mut f = Fixture::chest();
    f.put(0, stack(EGG, 5));
    f.put(3, stack(STONE, 40));
    f.put(7, stack(STONE, 40));
    f.put(9, stack(EGG, 14));
    f.put(12, stack(DIRT, 1));
    let d = f.ok(sort(MenuDef::CONTAINER));
    assert_eq!(f.at_kind(0), Some((STONE, 64)));
    assert_eq!(f.at_kind(1), Some((STONE, 16)));
    assert_eq!(f.at_kind(2), Some((EGG, 16)));
    assert_eq!(f.at_kind(3), Some((EGG, 3)));
    assert_eq!(f.at_kind(4), Some((DIRT, 1)));
    assert_eq!(f.at(5), None);
    assert_eq!(f.total(STONE), 80);
    assert_eq!(f.total(EGG), 19);
    assert!(d.slots.iter().all(|(s, _)| s.0 < 27));
    assert_eq!(
        f.click(sort(MenuDef::CONTAINER)),
        Err(ClickError::NothingToDo)
    );
}

#[test]
fn sort_pins_favorites_in_place() {
    let mut f = Fixture::chest();
    f.put(3, stack(EGG, 5));
    f.put(5, stack(STONE, 3));
    f.put(8, stack(STONE, 2));
    f.inv[MenuDef::CONTAINER].set_favorite(5, true);
    f.ok(sort(MenuDef::CONTAINER));
    // Pinned stone stays put; the rest sorts around it without merging into it.
    assert_eq!(f.at_kind(0), Some((STONE, 2)));
    assert_eq!(f.at_kind(1), Some((EGG, 5)));
    assert_eq!(f.at(3), None);
    assert_eq!(f.at_kind(5), Some((STONE, 3)));
    assert_eq!(f.at(8), None);
}

#[test]
fn sort_leaves_other_inventories_and_special_slots_alone() {
    let mut f = Fixture::chest();
    f.def.slots[0].behaviour = SlotBehaviour::Locked;
    f.put(0, stack(EGG, 5));
    f.put(2, stack(STONE, 3));
    f.put(30, stack(DIRT, 2));
    f.ok(sort(MenuDef::CONTAINER));
    assert_eq!(f.at_kind(0), Some((EGG, 5)));
    assert_eq!(f.at_kind(1), Some((STONE, 3)));
    assert_eq!(f.at_kind(30), Some((DIRT, 2)));
}

#[test]
fn quick_stack_moves_only_kinds_already_present() {
    let mut f = Fixture::chest();
    f.put(0, stack(STONE, 60));
    f.put(30, stack(STONE, 10));
    f.put(31, stack(EGG, 4));
    f.put(60, stack(STONE, 2));
    f.ok(ClickAction::Toolbar(ToolbarAction::QuickStack {
        from: MenuDef::PLAYER_MAIN,
        to: MenuDef::CONTAINER,
    }));
    assert_eq!(f.at_kind(0), Some((STONE, 64)));
    assert_eq!(f.at_kind(1), Some((STONE, 6)));
    assert_eq!(f.at(30), None);
    assert_eq!(f.at_kind(31), Some((EGG, 4)));
    assert_eq!(f.at_kind(60), Some((STONE, 2)));
}

#[test]
fn deposit_all_and_loot_all_move_everything_that_fits() {
    let mut f = Fixture::chest();
    f.put(27, stack(STONE, 10));
    f.put(28, stack(EGG, 4));
    f.put(54, stack(SWORD, 1));
    f.inv[MenuDef::PLAYER_MAIN].set_favorite(1, true);
    f.ok(ClickAction::Toolbar(ToolbarAction::DepositAll {
        from: MenuDef::PLAYER_MAIN,
        to: MenuDef::CONTAINER,
    }));
    assert_eq!(f.at_kind(0), Some((STONE, 10)));
    assert_eq!(f.at(27), None);
    assert_eq!(f.at_kind(28), Some((EGG, 4))); // pinned
    assert_eq!(f.at_kind(54), Some((SWORD, 1))); // hotbar not part of `from`

    f.ok(ClickAction::Toolbar(ToolbarAction::LootAll {
        from: MenuDef::CONTAINER,
        to: MenuDef::PLAYER_HOTBAR,
    }));
    assert_eq!(f.at(0), None);
    assert_eq!(f.at_kind(55), Some((STONE, 10)));
    assert_eq!(
        f.click(ClickAction::Toolbar(ToolbarAction::LootAll {
            from: MenuDef::CONTAINER,
            to: MenuDef::PLAYER_HOTBAR,
        })),
        Err(ClickError::NothingToDo)
    );
    assert_eq!(
        f.click(ClickAction::Toolbar(ToolbarAction::DepositAll {
            from: MenuDef::CONTAINER,
            to: MenuDef::CONTAINER,
        })),
        Err(ClickError::NotAllowed)
    );
}

#[test]
fn bulk_moves_respect_filters_and_outputs_on_the_target() {
    let mut f = Fixture::player();
    f.put(9, stack(STONE, 3));
    f.put(10, stack(common::HELMET, 1));
    // Target the armor inventory: only the helmet fits, and only in slot 5.
    f.ok(ClickAction::Toolbar(ToolbarAction::DepositAll {
        from: MenuDef::PLAYER_MAIN,
        to: MenuDef::PLAYER_ARMOR,
    }));
    assert_eq!(f.at_kind(5), Some((common::HELMET, 1)));
    assert_eq!(f.at_kind(9), Some((STONE, 3)));
    // Output slots never receive.
    assert_eq!(
        f.click(ClickAction::Toolbar(ToolbarAction::DepositAll {
            from: MenuDef::PLAYER_MAIN,
            to: MenuDef::CONTAINER,
        })),
        Err(ClickError::NothingToDo)
    );
}

#[test]
fn toggle_favorite_flips_the_bit_and_reports_the_slot() {
    let mut f = Fixture::chest();
    let toggle = ClickAction::Toolbar(ToolbarAction::ToggleFavorite { slot: SlotIx(30) });
    let d = f.ok(toggle);
    assert!(f.inv[MenuDef::PLAYER_MAIN].is_favorite(3));
    assert_eq!(d.slots, vec![(SlotIx(30), None)]);
    assert_eq!(d.state_id, 1);
    f.ok(toggle);
    assert!(!f.inv[MenuDef::PLAYER_MAIN].is_favorite(3));
    let mut p = Fixture::player();
    assert_eq!(
        p.click(ClickAction::Toolbar(ToolbarAction::ToggleFavorite {
            slot: SlotIx(0)
        })),
        Err(ClickError::NotAllowed)
    );
}
