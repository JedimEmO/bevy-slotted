//! Random action sequences never create or destroy items, and `state_id`
//! strictly increases on every mutating success.
#![allow(clippy::unwrap_used)]

mod common;

use common::{DIRT, EGG, Fixture, HELMET, Items, STONE, SWORD, stack};
use proptest::prelude::*;
use slotted_model::{
    Actor, Button, ClickAction, ClickError, DragKind, DragStage, ItemId, ItemStack, LookupCtx,
    MenuDef, SlotBehaviour, SlotIx, ToolbarAction,
};

const ITEMS: [ItemId; 5] = [STONE, EGG, SWORD, HELMET, DIRT];

fn item() -> impl Strategy<Value = ItemId> {
    prop::sample::select(ITEMS.to_vec())
}

fn some_stack() -> impl Strategy<Value = Option<ItemStack>> {
    prop_oneof![
        3 => Just(None),
        5 => item().prop_flat_map(|id| (1..=Items.max_stack(id)).prop_map(move |n| Some(stack(id, n)))),
    ]
}

fn slot(max: u16) -> impl Strategy<Value = SlotIx> {
    prop_oneof![9 => (0..max).prop_map(SlotIx), 1 => Just(SlotIx::OUTSIDE)]
}

fn inventory_ref() -> impl Strategy<Value = slotted_model::InventoryRef> {
    prop::sample::select(vec![
        MenuDef::CONTAINER,
        MenuDef::PLAYER_MAIN,
        MenuDef::PLAYER_HOTBAR,
    ])
}

fn action(slots: u16) -> impl Strategy<Value = ClickAction> {
    let button = prop::sample::select(vec![Button::Left, Button::Right, Button::Middle]);
    let kind = prop::sample::select(vec![DragKind::Left, DragKind::Right, DragKind::Middle]);
    let stage = prop::sample::select(vec![
        DragStage::Start,
        DragStage::Add,
        DragStage::Add,
        DragStage::End,
    ]);
    prop_oneof![
        4 => (slot(slots), button).prop_map(|(slot, button)| ClickAction::Pickup { slot, button }),
        2 => slot(slots).prop_map(|slot| ClickAction::QuickMove { slot }),
        1 => (slot(slots), 0..10u8).prop_map(|(slot, hotbar)| ClickAction::Swap { slot, hotbar: if hotbar == 9 { 40 } else { hotbar } }),
        1 => slot(slots).prop_map(|slot| ClickAction::Clone { slot }),
        1 => (slot(slots), any::<bool>()).prop_map(|(slot, all)| ClickAction::Throw { slot, all }),
        3 => (stage, kind, prop::option::of(slot(slots))).prop_map(|(stage, kind, slot)| ClickAction::Drag { stage, kind, slot }),
        1 => (slot(slots), any::<bool>()).prop_map(|(slot, reverse)| ClickAction::PickupAll { slot, reverse }),
        1 => inventory_ref().prop_map(|inventory| ClickAction::Toolbar(ToolbarAction::Sort { inventory })),
        1 => (inventory_ref(), inventory_ref()).prop_map(|(from, to)| ClickAction::Toolbar(ToolbarAction::QuickStack { from, to })),
        1 => (inventory_ref(), inventory_ref()).prop_map(|(from, to)| ClickAction::Toolbar(ToolbarAction::DepositAll { from, to })),
        1 => (inventory_ref(), inventory_ref()).prop_map(|(from, to)| ClickAction::Toolbar(ToolbarAction::LootAll { from, to })),
        1 => slot(slots).prop_map(|slot| ClickAction::Toolbar(ToolbarAction::ToggleFavorite { slot })),
    ]
}

fn behaviour() -> impl Strategy<Value = SlotBehaviour> {
    prop_oneof![
        12 => Just(SlotBehaviour::Normal),
        1 => Just(SlotBehaviour::Output),
        1 => Just(SlotBehaviour::Ghost),
        1 => Just(SlotBehaviour::Locked),
        1 => Just(SlotBehaviour::Disabled),
    ]
}

/// Items per id over every real (non-ghost) slot, the cursor and `dropped`.
fn tally(f: &Fixture, dropped: &[ItemStack]) -> Vec<u64> {
    ITEMS
        .iter()
        .map(|id| {
            let kind = stack(*id, 1);
            let ghost: Vec<_> = f
                .def
                .slots
                .iter()
                .filter(|s| s.behaviour.is_ghost())
                .map(|s| (s.source, usize::from(s.index)))
                .collect();
            let ghost = &ghost;
            let in_slots: u64 = f
                .inv
                .iter()
                .flat_map(|(h, inv)| {
                    inv.slots()
                        .iter()
                        .enumerate()
                        .filter(move |(i, _)| !ghost.contains(&(h, *i)))
                        .filter_map(|(_, s)| s.as_ref())
                        .filter(|s| s.same_kind(&kind))
                        .map(|s| u64::from(s.count))
                })
                .sum();
            let carried = f
                .state
                .carried
                .as_ref()
                .filter(|c| c.same_kind(&kind))
                .map_or(0, |c| u64::from(c.count));
            let dropped: u64 = dropped
                .iter()
                .filter(|d| d.same_kind(&kind))
                .map(|d| u64::from(d.count))
                .sum();
            in_slots + carried + dropped
        })
        .collect()
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(400))]

    #[test]
    fn random_sequences_conserve_items_and_advance_state_id(
        contents in prop::collection::vec(some_stack(), 63),
        behaviours in prop::collection::vec(behaviour(), 27),
        carried in some_stack(),
        creative in any::<bool>(),
        actions in prop::collection::vec(action(63), 1..40),
    ) {
        let mut f = Fixture::chest();
        for (i, b) in behaviours.into_iter().enumerate() {
            f.def.slots[i].behaviour = b;
        }
        for (i, c) in contents.into_iter().enumerate() {
            if let Some(c) = c {
                f.put(u16::try_from(i).unwrap(), c);
            }
        }
        f.state.carried = carried;
        let actor = if creative { Actor::CREATIVE } else { Actor::SURVIVAL };
        let mut dropped = Vec::new();
        let mut before = tally(&f, &dropped);
        for action in actions {
            let prev_id = f.state.state_id;
            let is_dup = matches!(action, ClickAction::Clone { .. } | ClickAction::Drag { kind: DragKind::Middle, .. });
            match f.click_as(action, actor) {
                Ok(delta) => {
                    dropped.extend(delta.dropped.iter().cloned());
                    prop_assert_eq!(delta.state_id, f.state.state_id);
                    prop_assert_eq!(delta.carried.as_ref(), f.state.carried.as_ref());
                    let mutating = !delta.slots.is_empty()
                        || !delta.dropped.is_empty()
                        || matches!(action, ClickAction::Clone { .. });
                    if mutating {
                        prop_assert_eq!(f.state.state_id, prev_id + 1, "{:?}", action);
                    } else {
                        prop_assert!(f.state.state_id == prev_id || f.state.state_id == prev_id + 1);
                    }
                    for (ix, content) in &delta.slots {
                        prop_assert_eq!(f.at(ix.0), content.as_ref());
                    }
                    // Every stored stack is non-empty and within its cap.
                    for (_, inv) in f.inv.iter() {
                        for s in inv.slots().iter().flatten() {
                            prop_assert!(s.count >= 1);
                            prop_assert!(s.count <= Items.max_stack(s.id).max(1) || creative);
                        }
                    }
                }
                Err(e) => {
                    prop_assert_eq!(f.state.state_id, prev_id, "{:?} -> {:?}", action, e);
                    if e != ClickError::InvalidDragSequence {
                        // no partial mutation happened
                        prop_assert_eq!(tally(&f, &dropped), before.clone());
                    }
                }
            }
            let after = tally(&f, &dropped);
            if is_dup {
                before = after;
            } else {
                prop_assert_eq!(&after, &before, "{:?}", action);
            }
        }
    }
}
