//! `PICKUP_ALL`: double click gathering.
#![allow(clippy::unwrap_used)]

mod common;

use common::{EGG, Fixture, STONE, stack};
use pretty_assertions::assert_eq;
use slotted_model::{ClickAction, ClickError, SlotBehaviour, SlotIx};

fn all(slot: u16, reverse: bool) -> ClickAction {
    ClickAction::PickupAll {
        slot: SlotIx(slot),
        reverse,
    }
}

#[test]
fn gathers_partial_stacks_before_full_ones_up_to_max() {
    let mut f = Fixture::chest();
    f.put(0, stack(STONE, 64));
    f.put(1, stack(STONE, 10));
    f.put(30, stack(STONE, 20));
    f.put(2, stack(EGG, 3));
    f.carry(stack(STONE, 5));
    f.ok(all(3, false));
    assert_eq!(f.carried(), Some((STONE, 64)));
    assert_eq!(f.at(1), None);
    assert_eq!(f.at(30), None);
    // Full stack in slot 0 gave up the remaining 29.
    assert_eq!(f.at_kind(0), Some((STONE, 35)));
    assert_eq!(f.at_kind(2), Some((EGG, 3)));
}

#[test]
fn reverse_scans_from_the_last_slot() {
    let mut f = Fixture::chest();
    f.put(0, stack(STONE, 40));
    f.put(62, stack(STONE, 40));
    f.carry(stack(STONE, 4));
    f.ok(all(5, true));
    assert_eq!(f.carried(), Some((STONE, 64)));
    assert_eq!(f.at(62), None);
    assert_eq!(f.at_kind(0), Some((STONE, 20)));
    let mut g = Fixture::chest();
    g.put(0, stack(STONE, 40));
    g.put(62, stack(STONE, 40));
    g.carry(stack(STONE, 4));
    g.ok(all(5, false));
    assert_eq!(g.at(0), None);
    assert_eq!(g.at_kind(62), Some((STONE, 20)));
}

#[test]
fn needs_carried_stack_and_an_empty_or_unpickable_slot() {
    let mut f = Fixture::chest();
    f.put(0, stack(STONE, 3));
    f.put(1, stack(STONE, 3));
    assert_eq!(f.click(all(2, false)), Err(ClickError::NothingToDo));
    f.carry(stack(STONE, 1));
    // Clicking a pickable non-empty slot is a plain pickup, not a gather.
    assert_eq!(f.click(all(0, false)), Err(ClickError::NothingToDo));
    f.ok(all(2, false));
    assert_eq!(f.carried(), Some((STONE, 7)));
    assert_eq!(f.click(all(2, false)), Err(ClickError::NothingToDo));
}

#[test]
fn skips_output_locked_and_ghost_slots() {
    let mut f = Fixture::chest();
    f.def.slots[0].behaviour = SlotBehaviour::Output;
    f.def.slots[1].behaviour = SlotBehaviour::Locked;
    f.def.slots[2].behaviour = SlotBehaviour::Ghost;
    for s in 0..4 {
        f.put(s, stack(STONE, 2));
    }
    f.carry(stack(STONE, 1));
    f.ok(all(10, false));
    assert_eq!(f.carried(), Some((STONE, 3)));
    assert_eq!(f.at_kind(0), Some((STONE, 2)));
    assert_eq!(f.at_kind(1), Some((STONE, 2)));
    assert_eq!(f.at_kind(2), Some((STONE, 2)));
    assert_eq!(f.at(3), None);
}

#[test]
fn stops_at_the_item_max() {
    let mut f = Fixture::chest();
    f.put(0, stack(EGG, 10));
    f.put(1, stack(EGG, 10));
    f.carry(stack(EGG, 1));
    f.ok(all(5, false));
    assert_eq!(f.carried(), Some((EGG, 16)));
    assert_eq!(f.total(EGG), 21);
}
