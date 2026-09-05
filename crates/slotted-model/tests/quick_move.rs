//! `QUICK_MOVE`: shift-click routing for the standard windows.
#![allow(clippy::unwrap_used)]

mod common;

use common::{EGG, Fixture, STONE, shift, stack};
use pretty_assertions::assert_eq;
use slotted_model::{ClickError, InventoryRef, MenuDef, SlotIx};

#[test]
fn chest_to_player_fills_hotbar_from_the_right_first() {
    let mut f = Fixture::chest();
    f.put(0, stack(STONE, 10));
    let d = f.ok(shift(0));
    assert_eq!(f.at(0), None);
    assert_eq!(f.at_kind(62), Some((STONE, 10)));
    assert_eq!(d.slots.len(), 2);
    assert_eq!(d.taken_from_output, None);
}

#[test]
fn chest_to_player_merges_before_filling_empty_slots() {
    let mut f = Fixture::chest();
    f.put(0, stack(STONE, 10));
    f.put(30, stack(STONE, 60)); // main slot, partially full
    f.ok(shift(0));
    assert_eq!(f.at_kind(30), Some((STONE, 64)));
    assert_eq!(f.at_kind(62), Some((STONE, 6)));
}

#[test]
fn chest_to_player_partial_when_player_is_nearly_full() {
    let mut f = Fixture::chest();
    for s in 27..63 {
        f.put(s, stack(EGG, 16));
    }
    f.put(62, stack(STONE, 60));
    f.put(0, stack(STONE, 10));
    f.ok(shift(0));
    assert_eq!(f.at_kind(0), Some((STONE, 6)));
    assert_eq!(f.at_kind(62), Some((STONE, 64)));
    assert_eq!(f.click(shift(0)), Err(ClickError::NothingToDo));
}

#[test]
fn player_to_chest_fills_from_the_front() {
    let mut f = Fixture::chest();
    f.put(60, stack(STONE, 10));
    f.put(4, stack(STONE, 62));
    f.ok(shift(60));
    assert_eq!(f.at(60), None);
    assert_eq!(f.at_kind(4), Some((STONE, 64)));
    assert_eq!(f.at_kind(0), Some((STONE, 8)));
}

#[test]
fn double_chest_layout_routes_the_same_way() {
    let mut f = Fixture::new(MenuDef::chest(6));
    f.put(53, stack(STONE, 3));
    f.ok(shift(53));
    assert_eq!(f.at_kind(89), Some((STONE, 3)));
    f.ok(shift(89));
    assert_eq!(f.at_kind(0), Some((STONE, 3)));
}

#[test]
fn player_hotbar_and_main_swap_with_each_other() {
    let mut f = Fixture::player();
    f.put(36, stack(STONE, 5));
    f.ok(shift(36));
    assert_eq!(f.at(36), None);
    assert_eq!(f.at_kind(9), Some((STONE, 5)));
    f.ok(shift(9));
    assert_eq!(f.at(9), None);
    assert_eq!(f.at_kind(36), Some((STONE, 5)));
}

#[test]
fn player_armor_and_grid_go_to_main_first() {
    let mut f = Fixture::player();
    f.put(5, stack(common::HELMET, 1));
    f.ok(shift(5));
    assert_eq!(f.at_kind(9), Some((common::HELMET, 1)));
    f.put(1, stack(STONE, 3));
    f.ok(shift(1));
    assert_eq!(f.at_kind(10), Some((STONE, 3)));
    f.put(45, stack(EGG, 2));
    f.ok(shift(45));
    assert_eq!(f.at_kind(11), Some((EGG, 2)));
}

#[test]
fn crafting_result_moves_reverse_and_reports_output() {
    let mut f = Fixture::player();
    f.put(0, stack(STONE, 4));
    let d = f.ok(shift(0));
    assert_eq!(f.at(0), None);
    assert_eq!(f.at_kind(44), Some((STONE, 4)));
    assert_eq!(d.taken_from_output, Some(SlotIx(0)));
}

#[test]
fn unstackables_skip_the_merge_pass() {
    let mut f = Fixture::chest();
    f.put(0, stack(common::SWORD, 1));
    f.put(62, stack(common::SWORD, 1));
    f.ok(shift(0));
    assert_eq!(f.at_kind(61), Some((common::SWORD, 1)));
    assert_eq!(f.at_kind(62), Some((common::SWORD, 1)));
}

#[test]
fn listring_is_the_fallback_when_no_rule_matches() {
    let mut f = Fixture::chest();
    f.def.quick_move.rules.clear();
    f.def.listring = vec![MenuDef::CONTAINER, MenuDef::PLAYER_MAIN];
    f.put(0, stack(STONE, 1));
    f.ok(shift(0));
    assert_eq!(f.at_kind(27), Some((STONE, 1)));
    f.ok(shift(27));
    assert_eq!(f.at_kind(0), Some((STONE, 1)));
    // Hotbar is not in the ring, so it has nowhere to go.
    f.put(60, stack(STONE, 1));
    assert_eq!(f.click(shift(60)), Err(ClickError::NothingToDo));
}

#[test]
fn routing_never_targets_the_source_slot() {
    let mut f = Fixture::chest();
    f.def.quick_move.rules.clear();
    f.def.quick_move = f
        .def
        .quick_move
        .clone()
        .route(slotted_model::SlotRange::new(0, 27), [InventoryRef::new(0)]);
    f.put(0, stack(STONE, 10));
    f.ok(shift(0));
    assert_eq!(f.at(0), None);
    assert_eq!(f.at_kind(1), Some((STONE, 10)));
}
