//! `Ghost`, `Filter`, `Locked` and `Output` slot behaviours.
#![allow(clippy::unwrap_used)]

mod common;

use common::{EGG, Fixture, STONE, left, right, shift, stack};
use pretty_assertions::assert_eq;
use slotted_model::{
    ClickAction, ClickError, DragKind, DragStage, MenuDef, Predicate, SlotBehaviour, SlotDef,
    SlotIx,
};

fn ghost_fixture(behaviour: SlotBehaviour) -> Fixture {
    let mut f = Fixture::chest();
    f.def.slots[0].behaviour = behaviour;
    f
}

#[test]
fn ghost_placement_does_not_consume_and_clears_on_empty_click() {
    for b in [SlotBehaviour::Ghost, SlotBehaviour::Filter] {
        let mut f = ghost_fixture(b);
        f.carry(stack(STONE, 10));
        let d = f.ok(left(0));
        assert_eq!(f.at_kind(0), Some((STONE, 1)));
        assert_eq!(f.carried(), Some((STONE, 10)));
        assert_eq!(d.slots, vec![(SlotIx(0), Some(stack(STONE, 1)))]);
        assert_eq!(f.click(right(0)), Err(ClickError::NothingToDo));
        f.carry(stack(EGG, 3));
        f.ok(right(0));
        assert_eq!(f.at_kind(0), Some((EGG, 1)));
        f.state.carried = None;
        f.ok(left(0));
        assert_eq!(f.at(0), None);
        assert_eq!(f.click(left(0)), Err(ClickError::NothingToDo));
    }
}

#[test]
fn ghost_honours_accepts() {
    let mut f = ghost_fixture(SlotBehaviour::Filter);
    f.def.slots[0].accepts = Some(Predicate::ItemIs(EGG));
    f.carry(stack(STONE, 1));
    assert_eq!(f.click(left(0)), Err(ClickError::NotAllowed));
}

#[test]
fn ghost_quick_move_and_throw_clear_without_dropping() {
    let mut f = ghost_fixture(SlotBehaviour::Ghost);
    f.put(0, stack(STONE, 1));
    let d = f.ok(shift(0));
    assert_eq!(f.at(0), None);
    assert!(d.dropped.is_empty());
    assert_eq!(f.total(STONE), 0);
    f.put(0, stack(STONE, 1));
    let d = f.ok(ClickAction::Throw {
        slot: SlotIx(0),
        all: true,
    });
    assert!(d.dropped.is_empty());
    assert_eq!(f.at(0), None);
}

#[test]
fn ghost_slots_can_be_painted_by_a_drag() {
    let mut f = ghost_fixture(SlotBehaviour::Ghost);
    f.carry(stack(STONE, 5));
    for (stage, slot) in [
        (DragStage::Start, None),
        (DragStage::Add, Some(SlotIx(0))),
        (DragStage::Add, Some(SlotIx(1))),
        (DragStage::End, None),
    ] {
        f.ok(ClickAction::Drag {
            stage,
            kind: DragKind::Left,
            slot,
        });
    }
    assert_eq!(f.at_kind(0), Some((STONE, 1)));
    assert_eq!(f.at_kind(1), Some((STONE, 2)));
    assert_eq!(f.carried(), Some((STONE, 3)));
}

#[test]
fn ghost_slots_are_skipped_by_bulk_moves() {
    let mut f = ghost_fixture(SlotBehaviour::Ghost);
    f.put(0, stack(STONE, 1));
    f.put(1, stack(STONE, 2));
    f.ok(ClickAction::Toolbar(
        slotted_model::ToolbarAction::DepositAll {
            from: MenuDef::CONTAINER,
            to: MenuDef::PLAYER_MAIN,
        },
    ));
    assert_eq!(f.at_kind(0), Some((STONE, 1)));
    assert_eq!(f.at(1), None);
    assert_eq!(f.at_kind(27), Some((STONE, 2)));
}

#[test]
fn locked_slot_rejects_everything_but_stays_visible() {
    let mut f = Fixture::chest();
    f.def.slots[0].behaviour = SlotBehaviour::Locked;
    f.put(0, stack(STONE, 3));
    assert_eq!(f.click(left(0)), Err(ClickError::SlotLocked));
    assert_eq!(f.click(right(0)), Err(ClickError::SlotLocked));
    assert_eq!(f.click(shift(0)), Err(ClickError::SlotLocked));
    assert_eq!(
        f.click(ClickAction::Throw {
            slot: SlotIx(0),
            all: false
        }),
        Err(ClickError::SlotLocked)
    );
    assert_eq!(
        f.click(ClickAction::Swap {
            slot: SlotIx(0),
            hotbar: 0
        }),
        Err(ClickError::SlotLocked)
    );
    f.carry(stack(STONE, 3));
    assert_eq!(f.click(left(0)), Err(ClickError::SlotLocked));
    // Quick-moving into a range containing a locked slot skips it.
    f.state.carried = None;
    f.def.slots[1].behaviour = SlotBehaviour::Locked;
    f.put(60, stack(EGG, 2));
    f.ok(shift(60));
    assert_eq!(f.at_kind(2), Some((EGG, 2)));
    assert_eq!(f.at(1), None);
    assert_eq!(f.state.state_id, 1);
}

#[test]
fn output_slot_is_never_a_quick_move_target() {
    let mut f = Fixture::player();
    f.def.quick_move = slotted_model::RoutingTable::new()
        .route(slotted_model::SlotRange::new(9, 45), [MenuDef::CONTAINER]);
    f.put(9, stack(STONE, 1));
    assert_eq!(f.click(shift(9)), Err(ClickError::NothingToDo));
}

#[test]
fn slot_def_builder_round_trips() {
    let sd = SlotDef::new(MenuDef::CONTAINER, 2)
        .behaviour(SlotBehaviour::Filter)
        .max_stack(1)
        .accepts(Predicate::Not(Box::new(Predicate::AnyOf(vec![STONE]))));
    let back: SlotDef = ron::from_str(&ron::to_string(&sd).unwrap()).unwrap();
    assert_eq!(back, sd);
}
