//! Adversarial review pass over `apply_click`.
//!
//! Every test here tries to break an invariant the click state machine claims:
//! no item is created or destroyed, no slot ever holds more than its cap, an
//! `Err` mutates nothing, and special slot behaviours are honoured by the bulk
//! actions as well as by the plain clicks.

#![allow(clippy::unwrap_used)]

mod common;

use common::{DIRT, EGG, Fixture, HELMET, Items, STONE, SWORD, left, right, shift, stack};
use pretty_assertions::assert_eq;
use slotted_model::{
    Actor, Button, ClickAction, ClickError, DragKind, DragStage, Inventories, InventoryRef, ItemId,
    ItemStack, MenuDef, MenuState, Predicate, SlotBehaviour, SlotDef, SlotIx, SlotRange,
    ToolbarAction,
};

// ----- a menu with one of every awkward slot -------------------------------

/// Container slot ids:
/// `0` normal, `1` output, `2` locked, `3` ghost, `4` filter (stone only),
/// `5` normal capped at 4, `6` normal accepting eggs only.
/// `7..11` player main. `11` hotbar 0 (plain), `12` hotbar 1 (capped at 2),
/// `13` hotbar 2 (eggs only), `14` hotbar 3 (disabled).
fn lab() -> Fixture {
    let mut def = MenuDef::new();
    let c = MenuDef::CONTAINER;
    def.slots.push(SlotDef::new(c, 0));
    def.slots
        .push(SlotDef::new(c, 1).behaviour(SlotBehaviour::Output));
    def.slots
        .push(SlotDef::new(c, 2).behaviour(SlotBehaviour::Locked));
    def.slots
        .push(SlotDef::new(c, 3).behaviour(SlotBehaviour::Ghost));
    def.slots.push(
        SlotDef::new(c, 4)
            .behaviour(SlotBehaviour::Filter)
            .accepts(Predicate::ItemIs(STONE)),
    );
    def.slots.push(SlotDef::new(c, 5).max_stack(4));
    def.slots
        .push(SlotDef::new(c, 6).accepts(Predicate::ItemIs(EGG)));
    def.add_slots(MenuDef::PLAYER_MAIN, 4, SlotBehaviour::Normal);
    def.slots.push(SlotDef::new(MenuDef::PLAYER_HOTBAR, 0));
    def.slots
        .push(SlotDef::new(MenuDef::PLAYER_HOTBAR, 1).max_stack(2));
    def.slots
        .push(SlotDef::new(MenuDef::PLAYER_HOTBAR, 2).accepts(Predicate::ItemIs(EGG)));
    def.slots
        .push(SlotDef::new(MenuDef::PLAYER_HOTBAR, 3).behaviour(SlotBehaviour::Disabled));
    def.hotbar = vec![SlotIx(11), SlotIx(12), SlotIx(13)];
    def.quick_move = slotted_model::RoutingTable::new()
        .route(
            SlotRange::new(0, 7),
            [MenuDef::PLAYER_MAIN, MenuDef::PLAYER_HOTBAR],
        )
        .route(SlotRange::new(7, 14), [MenuDef::CONTAINER]);
    Fixture::new(def)
}

fn swap(slot: u16, hotbar: u8) -> ClickAction {
    ClickAction::Swap {
        slot: SlotIx(slot),
        hotbar,
    }
}

fn throw(slot: u16, all: bool) -> ClickAction {
    ClickAction::Throw {
        slot: SlotIx(slot),
        all,
    }
}

fn pickup_all(slot: u16) -> ClickAction {
    ClickAction::PickupAll {
        slot: SlotIx(slot),
        reverse: false,
    }
}

fn drag_start(kind: DragKind) -> ClickAction {
    ClickAction::Drag {
        stage: DragStage::Start,
        kind,
        slot: None,
    }
}

fn drag_add(kind: DragKind, slot: u16) -> ClickAction {
    ClickAction::Drag {
        stage: DragStage::Add,
        kind,
        slot: Some(SlotIx(slot)),
    }
}

fn drag_end(kind: DragKind) -> ClickAction {
    ClickAction::Drag {
        stage: DragStage::End,
        kind,
        slot: None,
    }
}

// ----- right click ---------------------------------------------------------

#[test]
fn right_click_on_a_single_item_takes_the_whole_stack() {
    let mut f = Fixture::chest();
    f.put(0, stack(STONE, 1));
    f.ok(right(0));
    assert_eq!(f.carried(), Some((STONE, 1)));
    assert_eq!(f.at_kind(0), None);
    assert_eq!(f.total(STONE), 1);
}

#[test]
fn right_click_placing_onto_a_full_stack_does_nothing() {
    let mut f = Fixture::chest();
    f.put(0, stack(STONE, 64));
    f.carry(stack(STONE, 10));
    assert_eq!(f.click(right(0)), Err(ClickError::NothingToDo));
    assert_eq!(f.at_kind(0), Some((STONE, 64)));
    assert_eq!(f.carried(), Some((STONE, 10)));
    assert_eq!(f.state.state_id, 0);
}

#[test]
fn right_click_placing_the_last_carried_item_clears_the_cursor() {
    let mut f = Fixture::chest();
    f.carry(stack(STONE, 1));
    f.ok(right(0));
    assert_eq!(f.carried(), None);
    assert_eq!(f.at_kind(0), Some((STONE, 1)));
    assert_eq!(f.total(STONE), 1);
}

#[test]
fn right_click_onto_a_nearly_full_same_kind_stack_adds_one() {
    let mut f = Fixture::chest();
    f.put(0, stack(STONE, 63));
    f.carry(stack(STONE, 5));
    f.ok(right(0));
    assert_eq!(f.at_kind(0), Some((STONE, 64)));
    assert_eq!(f.carried(), Some((STONE, 4)));
    assert_eq!(f.total(STONE), 68);
}

// ----- left click partial merge -------------------------------------------

#[test]
fn left_click_partial_merge_keeps_the_remainder_in_hand() {
    let mut f = Fixture::chest();
    f.put(0, stack(STONE, 60));
    f.carry(stack(STONE, 10));
    f.ok(left(0));
    assert_eq!(f.at_kind(0), Some((STONE, 64)));
    assert_eq!(f.carried(), Some((STONE, 6)));
    assert_eq!(f.total(STONE), 70);
}

#[test]
fn left_click_into_a_capped_slot_places_only_the_cap() {
    let mut f = lab();
    f.carry(stack(STONE, 10));
    f.ok(left(5));
    assert_eq!(f.at_kind(5), Some((STONE, 4)));
    assert_eq!(f.carried(), Some((STONE, 6)));
}

// ----- swap ----------------------------------------------------------------

#[test]
fn swap_into_an_empty_hotbar_slot_moves_the_stack() {
    let mut f = Fixture::chest();
    f.put(0, stack(STONE, 12));
    f.ok(swap(0, 0));
    assert_eq!(f.at_kind(0), None);
    assert_eq!(f.at_kind(54), Some((STONE, 12)));
    assert_eq!(f.total(STONE), 12);
}

#[test]
fn swap_of_two_same_kind_stacks_exchanges_them() {
    let mut f = Fixture::chest();
    f.put(0, stack(STONE, 12));
    f.put(54, stack(STONE, 5));
    f.ok(swap(0, 0));
    assert_eq!(f.at_kind(0), Some((STONE, 5)));
    assert_eq!(f.at_kind(54), Some((STONE, 12)));
    assert_eq!(f.total(STONE), 17);
}

#[test]
fn swap_of_a_hotbar_slot_with_itself_does_nothing() {
    let mut f = Fixture::chest();
    f.put(54, stack(STONE, 5));
    assert_eq!(f.click(swap(54, 0)), Err(ClickError::NothingToDo));
    assert_eq!(f.at_kind(54), Some((STONE, 5)));
    assert_eq!(f.state.state_id, 0);
}

#[test]
fn swap_between_two_hotbar_slots_works() {
    let mut f = Fixture::chest();
    f.put(54, stack(STONE, 5));
    f.put(55, stack(EGG, 3));
    f.ok(swap(54, 1));
    assert_eq!(f.at_kind(54), Some((EGG, 3)));
    assert_eq!(f.at_kind(55), Some((STONE, 5)));
}

#[test]
fn swap_never_stores_more_than_the_hotbar_slot_cap() {
    let mut f = lab();
    f.put(0, stack(STONE, 10));
    f.put(12, stack(STONE, 1));
    f.click(swap(0, 1)).ok();
    let held = f.at(12).map_or(0, |s| s.count);
    assert!(held <= 2, "hotbar slot 12 caps at 2 but holds {held}");
    assert_eq!(f.total(STONE), 11);
}

#[test]
fn swap_never_stores_a_rejected_kind_in_a_filtered_hotbar_slot() {
    let mut f = lab();
    f.put(0, stack(STONE, 5));
    f.put(13, stack(EGG, 1));
    f.click(swap(0, 2)).ok();
    assert!(
        f.at(13).is_none_or(|s| s.id == EGG),
        "slot 13 accepts eggs only, holds {:?}",
        f.at_kind(13)
    );
    assert_eq!(f.total(STONE), 5);
    assert_eq!(f.total(EGG), 1);
}

#[test]
fn swap_with_a_locked_or_ghost_slot_is_refused() {
    let mut f = lab();
    f.put(2, stack(STONE, 1));
    f.put(3, stack(STONE, 1));
    assert_eq!(f.click(swap(2, 0)), Err(ClickError::SlotLocked));
    assert_eq!(f.click(swap(3, 0)), Err(ClickError::NotAllowed));
    assert_eq!(f.state.state_id, 0);
}

// ----- quick move ----------------------------------------------------------

#[test]
fn quick_move_tops_up_a_partial_stack_then_spills_into_an_empty_slot() {
    let mut f = Fixture::chest();
    // Eggs cap at 16. Main inventory slot 27 holds 15 of them.
    f.put(27, stack(EGG, 15));
    f.put(28, stack(STONE, 1));
    f.put(0, stack(EGG, 5));
    // Container slot 0 routes into main then hotbar, filling from the end.
    f.ok(shift(0));
    assert_eq!(f.at_kind(0), None);
    assert_eq!(f.at_kind(27), Some((EGG, 16)));
    assert_eq!(f.total(EGG), 20);
    let spill: u32 = (29..63).filter_map(|s| f.at_kind(s)).map(|(_, n)| n).sum();
    assert_eq!(spill, 4);
}

#[test]
fn quick_move_with_every_target_full_changes_nothing() {
    let mut f = Fixture::chest();
    for s in 27..63 {
        f.put(s, stack(DIRT, 64));
    }
    f.put(0, stack(EGG, 5));
    assert_eq!(f.click(shift(0)), Err(ClickError::NothingToDo));
    assert_eq!(f.at_kind(0), Some((EGG, 5)));
    assert_eq!(f.state.state_id, 0);
}

#[test]
fn quick_move_respects_the_target_slot_cap() {
    let mut f = lab();
    f.put(7, stack(STONE, 20));
    // Main routes into the container: slot 0 is plain, 5 caps at 4, 6 is
    // eggs only, and the rest cannot store items at all.
    f.ok(shift(7));
    assert_eq!(f.at_kind(0), Some((STONE, 20)));
    assert_eq!(f.at_kind(7), None);
    assert_eq!(f.total(STONE), 20);
}

// ----- drag ----------------------------------------------------------------

#[test]
fn left_drag_respects_caps_and_keeps_the_remainder() {
    let mut f = Fixture::chest();
    f.put(1, stack(STONE, 63));
    f.carry(stack(STONE, 7));
    f.ok(drag_start(DragKind::Left));
    for s in [0, 1, 2] {
        f.ok(drag_add(DragKind::Left, s));
    }
    f.ok(drag_end(DragKind::Left));
    assert_eq!(f.at_kind(0), Some((STONE, 2)));
    assert_eq!(f.at_kind(1), Some((STONE, 64)));
    assert_eq!(f.at_kind(2), Some((STONE, 2)));
    assert_eq!(f.carried(), Some((STONE, 2)));
    assert_eq!(f.total(STONE), 70);
}

#[test]
fn right_drag_skips_an_output_slot_and_keeps_the_drag_alive() {
    let mut f = lab();
    f.put(1, stack(STONE, 3));
    f.carry(stack(STONE, 4));
    f.ok(drag_start(DragKind::Right));
    f.ok(drag_add(DragKind::Right, 0));
    let skipped = f.ok(drag_add(DragKind::Right, 1));
    assert!(
        skipped.slots.is_empty(),
        "an output slot is skipped, not painted"
    );
    assert_eq!(skipped.state_id, 0);
    assert!(f.state.drag.is_some(), "the drag survives a skipped slot");
    f.ok(drag_add(DragKind::Right, 5));
    f.ok(drag_end(DragKind::Right));
    assert_eq!(f.at_kind(0), Some((STONE, 1)));
    assert_eq!(f.at_kind(1), Some((STONE, 3)), "output slot untouched");
    assert_eq!(f.at_kind(5), Some((STONE, 1)));
    assert_eq!(f.carried(), Some((STONE, 2)));
    assert_eq!(f.total(STONE), 7);
}

#[test]
fn painting_the_same_slot_twice_does_not_double_count_it() {
    let mut f = Fixture::chest();
    f.carry(stack(STONE, 6));
    f.ok(drag_start(DragKind::Left));
    f.ok(drag_add(DragKind::Left, 0));
    f.ok(drag_add(DragKind::Left, 0));
    f.ok(drag_add(DragKind::Left, 1));
    assert_eq!(f.state.drag.as_ref().unwrap().slots.len(), 2);
    f.ok(drag_end(DragKind::Left));
    assert_eq!(f.at_kind(0), Some((STONE, 3)));
    assert_eq!(f.at_kind(1), Some((STONE, 3)));
    assert_eq!(f.carried(), None);
    assert_eq!(f.total(STONE), 6);
}

#[test]
fn a_drag_over_no_slots_at_all_ends_quietly() {
    let mut f = Fixture::chest();
    f.carry(stack(STONE, 6));
    f.ok(drag_start(DragKind::Left));
    let delta = f.ok(drag_end(DragKind::Left));
    assert!(delta.slots.is_empty());
    assert_eq!(f.carried(), Some((STONE, 6)));
    assert!(f.state.drag.is_none());
    assert_eq!(f.state.state_id, 0);
}

/// Painting an unpaintable slot is a no-op the drag walks straight past.
#[test]
fn painting_locked_disabled_and_filtered_slots_is_silently_skipped() {
    let mut f = lab();
    f.put(7, stack(STONE, 64));
    f.carry(stack(STONE, 4));
    f.ok(drag_start(DragKind::Left));
    for slot in [1, 2, 6, 7, 14] {
        let delta = f.ok(drag_add(DragKind::Left, slot));
        assert!(delta.slots.is_empty(), "slot {slot} should be skipped");
    }
    assert_eq!(f.state.drag.as_ref().unwrap().slots.len(), 0);
    assert_eq!(
        f.click(drag_add(DragKind::Left, 900)),
        Err(ClickError::NoSuchSlot),
        "a slot that does not exist is still an error"
    );
    f.ok(drag_add(DragKind::Left, 0));
    f.ok(drag_end(DragKind::Left));
    assert_eq!(f.at_kind(0), Some((STONE, 4)));
}

#[test]
fn a_middle_drag_needs_creative() {
    let mut f = Fixture::chest();
    f.carry(stack(STONE, 6));
    assert_eq!(
        f.click(drag_start(DragKind::Middle)),
        Err(ClickError::Permission)
    );
    assert!(f.state.drag.is_none());
}

// ----- pickup all ----------------------------------------------------------

#[test]
fn pickup_all_with_an_empty_cursor_does_nothing() {
    let mut f = Fixture::chest();
    f.put(1, stack(STONE, 5));
    assert_eq!(f.click(pickup_all(0)), Err(ClickError::NothingToDo));
    assert_eq!(f.state.state_id, 0);
}

#[test]
fn pickup_all_never_pulls_from_output_or_locked_slots() {
    let mut f = lab();
    f.put(1, stack(STONE, 9));
    f.put(2, stack(STONE, 9));
    f.put(7, stack(STONE, 9));
    f.carry(stack(STONE, 1));
    f.ok(pickup_all(0));
    assert_eq!(f.at_kind(1), Some((STONE, 9)), "output slot untouched");
    assert_eq!(f.at_kind(2), Some((STONE, 9)), "locked slot untouched");
    assert_eq!(f.at_kind(7), None);
    assert_eq!(f.carried(), Some((STONE, 10)));
    assert_eq!(f.total(STONE), 28);
}

#[test]
fn pickup_all_stops_at_the_item_maximum() {
    let mut f = Fixture::chest();
    for s in 0..4 {
        f.put(s, stack(STONE, 30));
    }
    f.carry(stack(STONE, 1));
    f.ok(pickup_all(20));
    assert_eq!(f.carried(), Some((STONE, 64)));
    assert_eq!(f.total(STONE), 121);
    let left_behind: u32 = (0..4).filter_map(|s| f.at_kind(s)).map(|(_, n)| n).sum();
    assert_eq!(left_behind, 57);
}

// ----- throw ---------------------------------------------------------------

#[test]
fn throwing_from_an_empty_slot_does_nothing() {
    let mut f = Fixture::chest();
    assert_eq!(f.click(throw(0, true)), Err(ClickError::NothingToDo));
    assert_eq!(f.state.state_id, 0);
}

#[test]
fn throwing_the_only_item_empties_the_slot() {
    let mut f = Fixture::chest();
    f.put(0, stack(SWORD, 1));
    let delta = f.ok(throw(0, false));
    assert_eq!(f.at_kind(0), None);
    assert_eq!(delta.dropped, vec![stack(SWORD, 1)]);
    assert_eq!(f.total(SWORD), 0);
}

#[test]
fn throwing_from_a_locked_slot_is_refused() {
    let mut f = lab();
    f.put(2, stack(STONE, 3));
    assert_eq!(f.click(throw(2, true)), Err(ClickError::SlotLocked));
    assert_eq!(f.at_kind(2), Some((STONE, 3)));
}

// ----- clone ---------------------------------------------------------------

#[test]
fn cloning_without_creative_changes_absolutely_nothing() {
    let mut f = Fixture::chest();
    f.put(0, stack(STONE, 3));
    let before = (f.inv.clone(), f.state.clone());
    assert_eq!(
        f.click(ClickAction::Clone { slot: SlotIx(0) }),
        Err(ClickError::Permission)
    );
    assert_eq!(f.inv, before.0);
    assert_eq!(f.state, before.1);
    assert_eq!(f.state.state_id, 0);
}

#[test]
fn cloning_in_creative_fills_the_cursor_to_the_item_maximum() {
    let mut f = Fixture::chest();
    f.put(0, stack(EGG, 3));
    f.click_as(ClickAction::Clone { slot: SlotIx(0) }, Actor::CREATIVE)
        .unwrap();
    assert_eq!(f.carried(), Some((EGG, 16)));
    assert_eq!(f.at_kind(0), Some((EGG, 3)));
}

// ----- toolbar -------------------------------------------------------------

#[test]
fn quick_stack_ignores_kinds_the_target_does_not_hold() {
    let mut f = Fixture::chest();
    f.put(0, stack(STONE, 10));
    f.put(27, stack(STONE, 5));
    f.put(28, stack(DIRT, 5));
    f.ok(ClickAction::Toolbar(ToolbarAction::QuickStack {
        from: MenuDef::PLAYER_MAIN,
        to: MenuDef::CONTAINER,
    }));
    assert_eq!(f.at_kind(0), Some((STONE, 15)));
    assert_eq!(f.at_kind(27), None);
    assert_eq!(
        f.at_kind(28),
        Some((DIRT, 5)),
        "dirt is absent from the chest"
    );
}

#[test]
fn deposit_all_skips_output_locked_ghost_and_filtered_slots() {
    let mut f = lab();
    f.put(7, stack(STONE, 64));
    f.put(8, stack(STONE, 64));
    f.ok(ClickAction::Toolbar(ToolbarAction::DepositAll {
        from: MenuDef::PLAYER_MAIN,
        to: MenuDef::CONTAINER,
    }));
    assert_eq!(f.at_kind(0), Some((STONE, 64)));
    assert_eq!(f.at_kind(1), None, "output slot never receives");
    assert_eq!(f.at_kind(2), None, "locked slot never receives");
    assert_eq!(f.at_kind(3), None, "ghost slot never receives");
    assert_eq!(f.at_kind(4), None, "filter slot never receives");
    assert_eq!(f.at_kind(5), Some((STONE, 4)), "capped slot takes its cap");
    assert_eq!(f.at_kind(6), None, "eggs only");
    assert_eq!(f.total(STONE), 128);
}

#[test]
fn loot_all_honours_accepts_and_caps_on_the_destination() {
    let mut f = lab();
    f.put(0, stack(STONE, 64));
    f.put(6, stack(EGG, 5));
    f.ok(ClickAction::Toolbar(ToolbarAction::LootAll {
        from: MenuDef::CONTAINER,
        to: MenuDef::PLAYER_HOTBAR,
    }));
    assert_eq!(f.at_kind(11), Some((STONE, 64)));
    assert_eq!(f.at_kind(12), Some((EGG, 2)), "capped at two");
    assert_eq!(f.at_kind(13), Some((EGG, 3)), "eggs are accepted here");
    assert_eq!(f.total(STONE), 64);
    assert_eq!(f.total(EGG), 5);
}

#[test]
fn a_bulk_move_onto_itself_is_refused() {
    let mut f = Fixture::chest();
    f.put(0, stack(STONE, 5));
    assert_eq!(
        f.click(ClickAction::Toolbar(ToolbarAction::DepositAll {
            from: MenuDef::CONTAINER,
            to: MenuDef::CONTAINER,
        })),
        Err(ClickError::NotAllowed)
    );
}

// ----- ghost slots ---------------------------------------------------------

#[test]
fn a_ghost_slot_sets_a_hint_without_consuming_and_clears_without_giving() {
    let mut f = lab();
    f.carry(stack(STONE, 5));
    f.ok(left(3));
    assert_eq!(f.at_kind(3), Some((STONE, 1)));
    assert_eq!(f.carried(), Some((STONE, 5)), "the hand is not touched");

    f.state.carried = None;
    f.ok(left(3));
    assert_eq!(f.at_kind(3), None);
    assert_eq!(f.carried(), None, "clearing a hint gives no items");
}

#[test]
fn a_filter_slot_still_honours_its_predicate() {
    let mut f = lab();
    f.carry(stack(EGG, 5));
    assert_eq!(f.click(left(4)), Err(ClickError::NotAllowed));
    f.state.carried = Some(stack(STONE, 5));
    f.ok(left(4));
    assert_eq!(f.at_kind(4), Some((STONE, 1)));
}

/// Documents a real footgun rather than a bug in `apply_click`: a hint lives in
/// a real inventory index, so `Inventories::count_of` sees it even though the
/// conservation check does not.
#[test]
fn a_ghost_hint_is_visible_to_inventory_counting() {
    let mut f = lab();
    f.carry(stack(STONE, 5));
    f.ok(left(3));
    assert_eq!(
        f.total(STONE),
        6,
        "five real plus one phantom: callers must exclude ghost slots themselves"
    );
}

// ----- state id ------------------------------------------------------------

#[test]
fn every_error_leaves_the_state_id_alone_and_every_success_bumps_it() {
    let mut f = lab();
    f.put(2, stack(STONE, 1));
    let errors = [
        left(2),
        right(2),
        shift(2),
        throw(2, true),
        ClickAction::Clone { slot: SlotIx(2) },
        left(0),
        pickup_all(0),
        ClickAction::Pickup {
            slot: SlotIx(0),
            button: Button::Middle,
        },
        swap(0, 99),
        left(200),
        drag_end(DragKind::Left),
        ClickAction::Toolbar(ToolbarAction::Sort {
            inventory: InventoryRef::new(9),
        }),
        ClickAction::Toolbar(ToolbarAction::ToggleFavorite { slot: SlotIx(1) }),
    ];
    for action in errors {
        let before = f.inv.clone();
        assert!(f.click(action).is_err(), "{action:?} should fail");
        assert_eq!(f.state.state_id, 0, "{action:?} bumped the state id");
        assert_eq!(f.inv, before, "{action:?} mutated an inventory");
    }

    let successes = [
        left(0),
        ClickAction::Toolbar(ToolbarAction::ToggleFavorite { slot: SlotIx(0) }),
    ];
    f.carry(stack(STONE, 2));
    for (n, action) in successes.into_iter().enumerate() {
        let delta = f.ok(action);
        let want = u32::try_from(n).unwrap() + 1;
        assert_eq!(f.state.state_id, want);
        assert_eq!(delta.state_id, want);
    }
}

// ----- conservation fuzz ---------------------------------------------------

struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.0 >> 11
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

const KINDS: [ItemId; 5] = [STONE, EGG, SWORD, HELMET, DIRT];

fn totals(inv: &Inventories, state: &MenuState, dropped: &[ItemStack]) -> [u64; 5] {
    let mut out = [0; 5];
    for (i, id) in KINDS.into_iter().enumerate() {
        let kind = ItemStack::new(id, 1);
        out[i] = inv.count_of(&kind)
            + state
                .carried
                .as_ref()
                .filter(|c| c.same_kind(&kind))
                .map_or(0, |c| u64::from(c.count))
            + dropped
                .iter()
                .filter(|d| d.same_kind(&kind))
                .map(|d| u64::from(d.count))
                .sum::<u64>();
    }
    out
}

#[test]
fn two_thousand_random_actions_conserve_every_item_kind() {
    use slotted_model::LookupCtx;

    let def = MenuDef::chest(3);
    let mut inv = Inventories::for_menu(&def);
    let mut state = MenuState::new(&def);
    let mut rng = Lcg(0x5EED_1234_ABCD_0001);

    // Seed a third of the slots.
    for slot in 0..63u16 {
        if rng.below(3) == 0 {
            let id = KINDS[usize::try_from(rng.below(5)).unwrap()];
            let max = Items.max_stack(id);
            let count = u32::try_from(rng.below(u64::from(max))).unwrap() + 1;
            let sd = def.slot(SlotIx(slot)).unwrap();
            inv[sd.source].set(usize::from(sd.index), Some(ItemStack::new(id, count)));
        }
    }

    let mut dropped: Vec<ItemStack> = Vec::new();
    let want = totals(&inv, &state, &dropped);

    let mut applied = 0usize;
    while applied < 2000 {
        let s = u16::try_from(rng.below(64)).unwrap();
        let action = match rng.below(10) {
            0 => ClickAction::Pickup {
                slot: SlotIx(s),
                button: Button::Left,
            },
            1 => ClickAction::Pickup {
                slot: SlotIx(s),
                button: Button::Right,
            },
            2 => ClickAction::QuickMove { slot: SlotIx(s) },
            3 => ClickAction::Swap {
                slot: SlotIx(s),
                hotbar: u8::try_from(rng.below(9)).unwrap(),
            },
            4 => ClickAction::Throw {
                slot: SlotIx(s),
                all: rng.below(2) == 0,
            },
            5 => ClickAction::PickupAll {
                slot: SlotIx(s),
                reverse: rng.below(2) == 0,
            },
            6 => ClickAction::Pickup {
                slot: SlotIx::OUTSIDE,
                button: Button::Left,
            },
            7 => ClickAction::Toolbar(ToolbarAction::Sort {
                inventory: MenuDef::CONTAINER,
            }),
            8 => ClickAction::Toolbar(ToolbarAction::QuickStack {
                from: MenuDef::PLAYER_MAIN,
                to: MenuDef::CONTAINER,
            }),
            _ => ClickAction::Toolbar(ToolbarAction::ToggleFavorite { slot: SlotIx(s) }),
        };

        // Every so often run a whole drag instead.
        if rng.below(7) == 0 {
            let kind = if rng.below(2) == 0 {
                DragKind::Left
            } else {
                DragKind::Right
            };
            let mut run = |a: ClickAction, inv: &mut Inventories, state: &mut MenuState| {
                if let Ok(delta) =
                    slotted_model::apply_click(&def, inv, state, a, &Actor::SURVIVAL, &Items)
                {
                    dropped.extend(delta.dropped);
                }
            };
            run(drag_start(kind), &mut inv, &mut state);
            for _ in 0..rng.below(5) {
                let painted = u16::try_from(rng.below(64)).unwrap();
                run(drag_add(kind, painted), &mut inv, &mut state);
            }
            run(drag_end(kind), &mut inv, &mut state);
        } else if let Ok(delta) =
            slotted_model::apply_click(&def, &mut inv, &mut state, action, &Actor::SURVIVAL, &Items)
        {
            dropped.extend(delta.dropped);
        }

        applied += 1;
        assert_eq!(
            totals(&inv, &state, &dropped),
            want,
            "item count changed after {applied} actions ({action:?})"
        );
        for (i, sd) in def.slots.iter().enumerate() {
            if let Some(stored) = inv[sd.source].get(usize::from(sd.index)) {
                assert!(stored.count > 0, "slot {i} holds a zero-count stack");
                let cap = sd.cap(stored.id, &Items);
                assert!(
                    stored.count <= cap,
                    "slot {i} holds {} of cap {cap}",
                    stored.count
                );
            }
        }
        if let Some(c) = state.carried.as_ref() {
            assert!(c.count > 0, "the cursor holds a zero-count stack");
        }
    }
}
