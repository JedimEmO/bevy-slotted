//! `QUICK_CRAFT`: the three drag kinds and their sequencing rules.
#![allow(clippy::unwrap_used)]

mod common;

use common::{EGG, Fixture, STONE, left, stack};
use pretty_assertions::assert_eq;
use slotted_model::{Actor, ClickAction, ClickError, DragKind, DragStage, SlotIx};

fn start(kind: DragKind) -> ClickAction {
    ClickAction::Drag {
        stage: DragStage::Start,
        kind,
        slot: None,
    }
}

fn add(kind: DragKind, slot: u16) -> ClickAction {
    ClickAction::Drag {
        stage: DragStage::Add,
        kind,
        slot: Some(SlotIx(slot)),
    }
}

fn end(kind: DragKind) -> ClickAction {
    ClickAction::Drag {
        stage: DragStage::End,
        kind,
        slot: None,
    }
}

fn drag(
    f: &mut Fixture,
    kind: DragKind,
    slots: &[u16],
) -> Result<slotted_model::Delta, ClickError> {
    f.click(start(kind))?;
    for &s in slots {
        f.click(add(kind, s))?;
    }
    f.click(end(kind))
}

#[test]
fn left_drag_spreads_evenly_and_keeps_remainder() {
    let mut f = Fixture::chest();
    f.carry(stack(STONE, 10));
    drag(&mut f, DragKind::Left, &[0, 1, 2]).unwrap();
    assert_eq!(f.at_kind(0), Some((STONE, 3)));
    assert_eq!(f.at_kind(1), Some((STONE, 3)));
    assert_eq!(f.at_kind(2), Some((STONE, 3)));
    assert_eq!(f.carried(), Some((STONE, 1)));
    assert_eq!(f.state.state_id, 1);
    assert!(f.state.drag.is_none());
}

#[test]
fn left_drag_tops_up_existing_stacks_to_the_cap() {
    let mut f = Fixture::chest();
    f.put(0, stack(EGG, 15));
    f.carry(stack(EGG, 8));
    drag(&mut f, DragKind::Left, &[0, 1]).unwrap();
    assert_eq!(f.at_kind(0), Some((EGG, 16)));
    assert_eq!(f.at_kind(1), Some((EGG, 4)));
    assert_eq!(f.carried(), Some((EGG, 3)));
}

#[test]
fn left_drag_over_one_slot_places_everything() {
    let mut f = Fixture::chest();
    f.carry(stack(STONE, 10));
    drag(&mut f, DragKind::Left, &[4]).unwrap();
    assert_eq!(f.at_kind(4), Some((STONE, 10)));
    assert_eq!(f.carried(), None);
}

#[test]
fn right_drag_places_one_each() {
    let mut f = Fixture::chest();
    f.carry(stack(STONE, 3));
    drag(&mut f, DragKind::Right, &[0, 1, 2]).unwrap();
    assert_eq!(f.at_kind(0), Some((STONE, 1)));
    assert_eq!(f.at_kind(2), Some((STONE, 1)));
    assert_eq!(f.carried(), None);
}

#[test]
fn cannot_paint_more_slots_than_items() {
    let mut f = Fixture::chest();
    f.carry(stack(STONE, 2));
    f.ok(start(DragKind::Right));
    f.ok(add(DragKind::Right, 0));
    f.ok(add(DragKind::Right, 1));
    // The third slot is skipped, not refused, and the drag survives it.
    assert!(f.ok(add(DragKind::Right, 2)).slots.is_empty());
    f.ok(end(DragKind::Right));
    assert_eq!(f.carried(), None);
    assert_eq!(f.at_kind(1), Some((STONE, 1)));
}

#[test]
fn painting_skips_incompatible_slots() {
    let mut f = Fixture::chest();
    f.put(1, stack(EGG, 1));
    f.put(2, stack(STONE, 64));
    f.carry(stack(STONE, 10));
    f.ok(start(DragKind::Left));
    assert!(f.ok(add(DragKind::Left, 1)).slots.is_empty());
    assert!(f.ok(add(DragKind::Left, 2)).slots.is_empty());
    f.ok(add(DragKind::Left, 0));
    f.ok(add(DragKind::Left, 0)); // duplicate paint is a no-op
    f.ok(end(DragKind::Left));
    assert_eq!(f.at_kind(0), Some((STONE, 10)));
}

#[test]
fn middle_drag_is_creative_only_and_places_full_stacks() {
    let mut f = Fixture::chest();
    f.carry(stack(EGG, 1));
    assert_eq!(
        f.click(start(DragKind::Middle)),
        Err(ClickError::Permission)
    );
    f.click_as(start(DragKind::Middle), Actor::CREATIVE)
        .unwrap();
    f.click_as(add(DragKind::Middle, 0), Actor::CREATIVE)
        .unwrap();
    f.click_as(add(DragKind::Middle, 1), Actor::CREATIVE)
        .unwrap();
    f.click_as(end(DragKind::Middle), Actor::CREATIVE).unwrap();
    assert_eq!(f.at_kind(0), Some((EGG, 16)));
    assert_eq!(f.at_kind(1), Some((EGG, 16)));
    assert_eq!(f.carried(), None);
}

#[test]
fn start_and_add_do_not_bump_state_id() {
    let mut f = Fixture::chest();
    f.carry(stack(STONE, 4));
    let d = f.ok(start(DragKind::Left));
    assert_eq!(d.state_id, 0);
    assert!(d.slots.is_empty());
    f.ok(add(DragKind::Left, 0));
    assert_eq!(f.state.state_id, 0);
    let d = f.ok(end(DragKind::Left));
    assert_eq!(d.state_id, 1);
}

#[test]
fn out_of_order_stages_reset_the_drag() {
    let mut f = Fixture::chest();
    f.carry(stack(STONE, 4));
    assert_eq!(
        f.click(add(DragKind::Left, 0)),
        Err(ClickError::InvalidDragSequence)
    );
    assert_eq!(
        f.click(end(DragKind::Left)),
        Err(ClickError::InvalidDragSequence)
    );
    f.ok(start(DragKind::Left));
    assert_eq!(
        f.click(add(DragKind::Right, 0)),
        Err(ClickError::InvalidDragSequence)
    );
    assert!(f.state.drag.is_none());
    f.ok(start(DragKind::Left));
    f.ok(add(DragKind::Left, 0));
    assert_eq!(
        f.click(end(DragKind::Right)),
        Err(ClickError::InvalidDragSequence)
    );
    assert!(f.state.drag.is_none());
    assert_eq!(f.at(0), None);
}

#[test]
fn start_without_carried_stack_is_invalid() {
    let mut f = Fixture::chest();
    assert_eq!(
        f.click(start(DragKind::Left)),
        Err(ClickError::InvalidDragSequence)
    );
}

#[test]
fn non_drag_click_during_drag_resets_it() {
    let mut f = Fixture::chest();
    f.carry(stack(STONE, 4));
    f.ok(start(DragKind::Left));
    f.ok(add(DragKind::Left, 0));
    assert_eq!(f.click(left(3)), Err(ClickError::InvalidDragSequence));
    assert!(f.state.drag.is_none());
    assert_eq!(f.carried(), Some((STONE, 4)));
    assert_eq!(f.at(0), None);
}

#[test]
fn end_with_nothing_painted_ends_the_drag_and_changes_nothing() {
    let mut f = Fixture::chest();
    f.carry(stack(STONE, 4));
    f.ok(start(DragKind::Left));
    let delta = f.ok(end(DragKind::Left));
    assert!(delta.slots.is_empty());
    assert_eq!(delta.state_id, 0);
    assert_eq!(f.carried(), Some((STONE, 4)));
    assert!(f.state.drag.is_none());
}
