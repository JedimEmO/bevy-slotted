//! `PICKUP`: the left and right click rows of the vanilla table.
#![allow(clippy::unwrap_used)]

mod common;

use common::{DIRT, EGG, Fixture, Items, STONE, SWORD, left, right, shift, stack};
use pretty_assertions::assert_eq;
use slotted_model::{
    Button, ClickAction, ClickError, Inventories, InventoryRef, MenuDef, MenuState, SlotBehaviour,
    SlotDef, SlotIx,
};

#[test]
fn left_click_picks_up_whole_stack() {
    let mut f = Fixture::chest();
    f.put(3, stack(STONE, 20));
    let d = f.ok(left(3));
    assert_eq!(f.carried(), Some((STONE, 20)));
    assert_eq!(f.at(3), None);
    assert_eq!(d.slots, vec![(SlotIx(3), None)]);
    assert_eq!(d.carried.as_ref().map(|s| s.count), Some(20));
    assert_eq!(d.state_id, 1);
    assert_eq!(f.state.state_id, 1);
}

#[test]
fn left_click_places_whole_stack_in_empty_slot() {
    let mut f = Fixture::chest();
    f.carry(stack(STONE, 20));
    f.ok(left(5));
    assert_eq!(f.carried(), None);
    assert_eq!(f.at_kind(5), Some((STONE, 20)));
}

#[test]
fn left_click_merges_up_to_cap_and_keeps_rest() {
    let mut f = Fixture::chest();
    f.put(0, stack(STONE, 60));
    f.carry(stack(STONE, 10));
    f.ok(left(0));
    assert_eq!(f.at_kind(0), Some((STONE, 64)));
    assert_eq!(f.carried(), Some((STONE, 6)));
    // Full slot: nothing happens and state id does not advance.
    assert_eq!(f.click(left(0)), Err(ClickError::NothingToDo));
    assert_eq!(f.state.state_id, 1);
}

#[test]
fn left_click_swaps_different_kinds() {
    let mut f = Fixture::chest();
    f.put(0, stack(STONE, 7));
    f.carry(stack(EGG, 3));
    f.ok(left(0));
    assert_eq!(f.at_kind(0), Some((EGG, 3)));
    assert_eq!(f.carried(), Some((STONE, 7)));
}

#[test]
fn swap_refused_when_carried_exceeds_slot_cap() {
    let mut f = Fixture::chest();
    f.def.slots[0] = SlotDef::new(MenuDef::CONTAINER, 0).max_stack(4);
    f.put(0, stack(EGG, 2));
    f.carry(stack(STONE, 10));
    assert_eq!(f.click(left(0)), Err(ClickError::NothingToDo));
    assert_eq!(f.carried(), Some((STONE, 10)));
}

#[test]
fn right_click_picks_up_ceil_half() {
    let mut f = Fixture::chest();
    f.put(0, stack(STONE, 7));
    f.ok(right(0));
    assert_eq!(f.carried(), Some((STONE, 4)));
    assert_eq!(f.at_kind(0), Some((STONE, 3)));
    f.state.carried = None;
    f.ok(right(0));
    assert_eq!(f.carried(), Some((STONE, 2)));
    assert_eq!(f.at_kind(0), Some((STONE, 1)));
    f.state.carried = None;
    f.ok(right(0));
    assert_eq!(f.carried(), Some((STONE, 1)));
    assert_eq!(f.at(0), None);
}

#[test]
fn right_click_places_one() {
    let mut f = Fixture::chest();
    f.carry(stack(STONE, 5));
    f.ok(right(1));
    assert_eq!(f.at_kind(1), Some((STONE, 1)));
    assert_eq!(f.carried(), Some((STONE, 4)));
    f.ok(right(1));
    assert_eq!(f.at_kind(1), Some((STONE, 2)));
    assert_eq!(f.carried(), Some((STONE, 3)));
}

#[test]
fn right_click_on_different_kind_swaps() {
    let mut f = Fixture::chest();
    f.put(0, stack(DIRT, 1));
    f.carry(stack(STONE, 5));
    f.ok(right(0));
    assert_eq!(f.at_kind(0), Some((STONE, 5)));
    assert_eq!(f.carried(), Some((DIRT, 1)));
}

#[test]
fn right_click_on_full_stack_does_nothing() {
    let mut f = Fixture::chest();
    f.put(0, stack(EGG, 16));
    f.carry(stack(EGG, 5));
    assert_eq!(f.click(right(0)), Err(ClickError::NothingToDo));
}

#[test]
fn middle_button_is_not_a_pickup() {
    let mut f = Fixture::chest();
    f.put(0, stack(STONE, 1));
    assert_eq!(
        f.click(ClickAction::Pickup {
            slot: SlotIx(0),
            button: Button::Middle,
        }),
        Err(ClickError::NotAllowed)
    );
}

#[test]
fn click_outside_drops_all_or_one() {
    let mut f = Fixture::chest();
    f.carry(stack(STONE, 5));
    let d = f.ok(right(SlotIx::OUTSIDE.0));
    assert_eq!(d.dropped, vec![stack(STONE, 1)]);
    assert_eq!(f.carried(), Some((STONE, 4)));
    let d = f.ok(left(SlotIx::OUTSIDE.0));
    assert_eq!(d.dropped, vec![stack(STONE, 4)]);
    assert_eq!(f.carried(), None);
    assert_eq!(
        f.click(left(SlotIx::OUTSIDE.0)),
        Err(ClickError::NothingToDo)
    );
}

#[test]
fn empty_slot_empty_hand_is_nothing() {
    let mut f = Fixture::chest();
    assert_eq!(f.click(left(0)), Err(ClickError::NothingToDo));
    assert_eq!(f.click(left(999)), Err(ClickError::NoSuchSlot));
    assert_eq!(f.state.state_id, 0);
}

#[test]
fn unstackable_items_swap_one_for_one() {
    let mut f = Fixture::chest();
    f.put(0, stack(SWORD, 1));
    f.carry(stack(SWORD, 1));
    // Same kind, slot at cap: nothing to merge.
    assert_eq!(f.click(left(0)), Err(ClickError::NothingToDo));
}

#[test]
fn output_slot_pulls_same_kind_onto_cursor() {
    let mut f = Fixture::player();
    f.put(0, stack(STONE, 4));
    f.carry(stack(STONE, 62));
    let d = f.ok(left(0));
    assert_eq!(f.carried(), Some((STONE, 64)));
    assert_eq!(f.at_kind(0), Some((STONE, 2)));
    assert_eq!(d.taken_from_output, Some(SlotIx(0)));
    // A different kind on the cursor cannot go into an output slot.
    f.carry(stack(EGG, 1));
    assert_eq!(f.click(left(0)), Err(ClickError::NotAllowed));
    f.state.carried = None;
    let d = f.ok(right(0));
    assert_eq!(f.carried(), Some((STONE, 1)));
    assert_eq!(d.taken_from_output, Some(SlotIx(0)));
}

#[test]
fn output_slot_refuses_placement() {
    let mut f = Fixture::player();
    f.carry(stack(STONE, 1));
    assert_eq!(f.click(left(0)), Err(ClickError::NotAllowed));
}

#[test]
fn slot_accepts_predicate_gates_placement() {
    let mut f = Fixture::player();
    f.carry(stack(STONE, 1));
    assert_eq!(f.click(left(5)), Err(ClickError::NotAllowed));
    f.carry(stack(common::HELMET, 1));
    f.ok(left(5));
    assert_eq!(f.at_kind(5), Some((common::HELMET, 1)));
}

#[test]
fn mismatched_inventories_are_rejected() {
    let def = MenuDef::chest(3);
    let mut inv = Inventories::new();
    inv.push(slotted_model::Inventory::new(3));
    let mut state = MenuState::new(&def);
    assert_eq!(
        slotted_model::apply_click(
            &def,
            &mut inv,
            &mut state,
            left(0),
            &slotted_model::Actor::SURVIVAL,
            &Items
        ),
        Err(ClickError::MenuMismatch)
    );
}

#[test]
fn per_slot_cap_limits_placement() {
    let mut f = Fixture::chest();
    f.def.slots[0] = SlotDef::new(InventoryRef::new(0), 0).max_stack(8);
    f.carry(stack(STONE, 20));
    f.ok(left(0));
    assert_eq!(f.at_kind(0), Some((STONE, 8)));
    assert_eq!(f.carried(), Some((STONE, 12)));
}

#[test]
fn disabled_slot_does_not_exist() {
    let mut f = Fixture::chest();
    f.def.slots[0].behaviour = SlotBehaviour::Disabled;
    f.put(0, stack(STONE, 1));
    assert_eq!(f.click(left(0)), Err(ClickError::NoSuchSlot));
    assert_eq!(f.click(shift(0)), Err(ClickError::NoSuchSlot));
}
