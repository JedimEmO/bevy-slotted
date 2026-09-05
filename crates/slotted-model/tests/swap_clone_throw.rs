//! `SWAP`, `CLONE` and `THROW`.
#![allow(clippy::unwrap_used)]

mod common;

use common::{EGG, Fixture, STONE, SWORD, stack};
use pretty_assertions::assert_eq;
use slotted_model::{Actor, ClickAction, ClickError, MenuDef, SlotDef, SlotIx};

fn swap(slot: u16, hotbar: u8) -> ClickAction {
    ClickAction::Swap {
        slot: SlotIx(slot),
        hotbar,
    }
}

#[test]
fn swap_exchanges_slot_with_hotbar_key() {
    let mut f = Fixture::chest();
    f.put(0, stack(STONE, 5));
    f.put(56, stack(EGG, 2)); // hotbar key 3
    f.ok(swap(0, 2));
    assert_eq!(f.at_kind(0), Some((EGG, 2)));
    assert_eq!(f.at_kind(56), Some((STONE, 5)));
}

#[test]
fn swap_moves_into_empty_hotbar_or_empty_slot() {
    let mut f = Fixture::chest();
    f.put(0, stack(STONE, 5));
    f.ok(swap(0, 0));
    assert_eq!(f.at(0), None);
    assert_eq!(f.at_kind(54), Some((STONE, 5)));
    f.ok(swap(1, 0));
    assert_eq!(f.at(54), None);
    assert_eq!(f.at_kind(1), Some((STONE, 5)));
    assert_eq!(f.click(swap(2, 0)), Err(ClickError::NothingToDo));
}

#[test]
fn swap_with_offhand_in_player_menu() {
    let mut f = Fixture::player();
    f.put(9, stack(SWORD, 1));
    f.ok(swap(9, 40));
    assert_eq!(f.at_kind(45), Some((SWORD, 1)));
    assert_eq!(f.at(9), None);
    // Chest menus have no offhand slot.
    let mut c = Fixture::chest();
    c.put(0, stack(STONE, 1));
    assert_eq!(c.click(swap(0, 40)), Err(ClickError::NoSuchSlot));
    assert_eq!(c.click(swap(0, 9)), Err(ClickError::NoSuchSlot));
}

#[test]
fn swap_respects_slot_filters_and_caps() {
    let mut f = Fixture::player();
    f.put(36, stack(STONE, 1));
    assert_eq!(f.click(swap(5, 0)), Err(ClickError::NotAllowed));
    // Capped slot: only `cap` moves in, remainder stays in the hotbar.
    let mut c = Fixture::chest();
    c.def.slots[0] = SlotDef::new(MenuDef::CONTAINER, 0).max_stack(4);
    c.put(54, stack(STONE, 10));
    c.ok(swap(0, 0));
    assert_eq!(c.at_kind(0), Some((STONE, 4)));
    assert_eq!(c.at_kind(54), Some((STONE, 6)));
}

#[test]
fn swap_into_capped_slot_rehomes_displaced_stack() {
    let mut c = Fixture::chest();
    c.def.slots[0] = SlotDef::new(MenuDef::CONTAINER, 0).max_stack(4);
    c.put(0, stack(EGG, 3));
    c.put(54, stack(STONE, 10));
    let d = c.ok(swap(0, 0));
    assert_eq!(c.at_kind(0), Some((STONE, 4)));
    assert_eq!(c.at_kind(54), Some((STONE, 6)));
    assert_eq!(c.at_kind(55), Some((EGG, 3)));
    assert!(d.dropped.is_empty());
    assert_eq!(c.total(EGG), 3);
}

#[test]
fn swap_out_of_output_reports_it() {
    let mut f = Fixture::player();
    f.put(0, stack(STONE, 4));
    let d = f.ok(swap(0, 1));
    assert_eq!(d.taken_from_output, Some(SlotIx(0)));
    assert_eq!(f.at_kind(37), Some((STONE, 4)));
    // Nothing can be swapped *into* an output slot.
    f.put(38, stack(EGG, 1));
    assert_eq!(f.click(swap(0, 2)), Err(ClickError::NotAllowed));
}

#[test]
fn swap_with_itself_is_nothing() {
    let mut f = Fixture::chest();
    f.put(54, stack(STONE, 1));
    assert_eq!(f.click(swap(54, 0)), Err(ClickError::NothingToDo));
}

#[test]
fn clone_needs_creative_and_empty_hand() {
    let mut f = Fixture::chest();
    f.put(0, stack(EGG, 3));
    let clone = ClickAction::Clone { slot: SlotIx(0) };
    assert_eq!(f.click(clone), Err(ClickError::Permission));
    f.click_as(clone, Actor::CREATIVE).unwrap();
    assert_eq!(f.carried(), Some((EGG, 16)));
    assert_eq!(f.at_kind(0), Some((EGG, 3)));
    assert_eq!(
        f.click_as(clone, Actor::CREATIVE),
        Err(ClickError::NothingToDo)
    );
    f.state.carried = None;
    assert_eq!(
        f.click_as(ClickAction::Clone { slot: SlotIx(1) }, Actor::CREATIVE),
        Err(ClickError::NothingToDo)
    );
}

#[test]
fn throw_drops_one_or_all() {
    let mut f = Fixture::chest();
    f.put(0, stack(STONE, 5));
    let d = f.ok(ClickAction::Throw {
        slot: SlotIx(0),
        all: false,
    });
    assert_eq!(d.dropped, vec![stack(STONE, 1)]);
    assert_eq!(f.at_kind(0), Some((STONE, 4)));
    let d = f.ok(ClickAction::Throw {
        slot: SlotIx(0),
        all: true,
    });
    assert_eq!(d.dropped, vec![stack(STONE, 4)]);
    assert_eq!(f.at(0), None);
    assert_eq!(
        f.click(ClickAction::Throw {
            slot: SlotIx(0),
            all: true
        }),
        Err(ClickError::NothingToDo)
    );
}

#[test]
fn throw_needs_empty_hand_and_a_real_slot() {
    let mut f = Fixture::chest();
    f.put(0, stack(STONE, 5));
    f.carry(stack(EGG, 1));
    assert_eq!(
        f.click(ClickAction::Throw {
            slot: SlotIx(0),
            all: false
        }),
        Err(ClickError::NothingToDo)
    );
    f.state.carried = None;
    assert_eq!(
        f.click(ClickAction::Throw {
            slot: SlotIx::OUTSIDE,
            all: false
        }),
        Err(ClickError::NoSuchSlot)
    );
}

#[test]
fn throw_from_output_reports_it() {
    let mut f = Fixture::player();
    f.put(0, stack(STONE, 4));
    let d = f.ok(ClickAction::Throw {
        slot: SlotIx(0),
        all: true,
    });
    assert_eq!(d.taken_from_output, Some(SlotIx(0)));
    assert_eq!(d.dropped, vec![stack(STONE, 4)]);
}
