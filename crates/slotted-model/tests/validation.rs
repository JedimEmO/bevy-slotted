//! `ValidationLevel`: item conservation as a runtime choice.
#![allow(clippy::unwrap_used)]

mod common;

use common::{Fixture, Items, STONE, left, stack};
use pretty_assertions::assert_eq;
use slotted_model::{
    Actor, ClickAction, ClickError, DragKind, DragStage, InventoryRef, MenuDef, SlotDef, SlotIx,
    ValidationLevel, apply_click_validated,
};

/// Two menu slots over one inventory cell. Nothing forbids this, and a drag
/// across both writes the same cell twice, so items go missing: exactly the
/// class of bug the conservation check exists to catch.
fn aliased() -> Fixture {
    let cell = InventoryRef::new(0);
    let mut def = MenuDef::new();
    def.slots.push(SlotDef::new(cell, 0));
    def.slots.push(SlotDef::new(cell, 0));
    def.slots.push(SlotDef::new(cell, 1));
    Fixture::new(def)
}

fn paint(f: &mut Fixture, level: ValidationLevel) -> Result<(), ClickError> {
    for (stage, slot) in [
        (DragStage::Start, None),
        (DragStage::Add, Some(SlotIx(0))),
        (DragStage::Add, Some(SlotIx(1))),
        (DragStage::End, None),
    ] {
        apply_click_validated(
            &f.def,
            &mut f.inv,
            &mut f.state,
            ClickAction::Drag {
                stage,
                kind: DragKind::Left,
                slot,
            },
            &Actor::SURVIVAL,
            &Items,
            level,
        )?;
    }
    Ok(())
}

#[test]
fn always_refuses_an_action_that_loses_items() {
    let mut f = aliased();
    f.carry(stack(STONE, 4));
    assert_eq!(
        paint(&mut f, ValidationLevel::Always),
        Err(ClickError::Conservation)
    );
}

#[test]
fn off_lets_the_same_action_through() {
    let mut f = aliased();
    f.carry(stack(STONE, 4));
    assert_eq!(paint(&mut f, ValidationLevel::Off), Ok(()));
    assert_eq!(f.total(STONE), 2, "two of the four were written over");
}

#[test]
fn always_passes_an_honest_action() {
    let mut f = Fixture::chest();
    f.carry(stack(STONE, 10));
    let delta = apply_click_validated(
        &f.def,
        &mut f.inv,
        &mut f.state,
        left(0),
        &Actor::SURVIVAL,
        &Items,
        ValidationLevel::Always,
    )
    .unwrap();
    assert_eq!(delta.slots, vec![(SlotIx(0), Some(stack(STONE, 10)))]);
    assert_eq!(f.total(STONE), 10);
}

#[test]
fn creative_duplication_is_never_checked() {
    for level in [
        ValidationLevel::Off,
        ValidationLevel::Debug,
        ValidationLevel::Always,
    ] {
        assert!(!level.checks(ClickAction::Clone { slot: SlotIx(0) }));
        assert!(!level.checks(ClickAction::Drag {
            stage: DragStage::End,
            kind: DragKind::Middle,
            slot: None,
        }));
    }
    let honest = left(0);
    assert!(!ValidationLevel::Off.checks(honest));
    assert!(ValidationLevel::Always.checks(honest));
    assert_eq!(
        ValidationLevel::Debug.checks(honest),
        cfg!(debug_assertions)
    );
    assert_eq!(ValidationLevel::default(), ValidationLevel::Debug);
}
