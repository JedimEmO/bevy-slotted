//! Golden slot layouts for the standard windows.
#![allow(clippy::unwrap_used)]

use pretty_assertions::assert_eq;
use slotted_model::{InventoryRef, MenuDef, SlotBehaviour, SlotIx};

fn layout(def: &MenuDef) -> Vec<(InventoryRef, u16, SlotBehaviour)> {
    def.slots
        .iter()
        .map(|s| (s.source, s.index, s.behaviour))
        .collect()
}

fn run(
    source: InventoryRef,
    count: u16,
    behaviour: SlotBehaviour,
) -> Vec<(InventoryRef, u16, SlotBehaviour)> {
    (0..count).map(|i| (source, i, behaviour)).collect()
}

fn player_part() -> Vec<(InventoryRef, u16, SlotBehaviour)> {
    let mut v = run(MenuDef::PLAYER_MAIN, 27, SlotBehaviour::Normal);
    v.extend(run(MenuDef::PLAYER_HOTBAR, 9, SlotBehaviour::Normal));
    v
}

#[test]
fn chest_3_is_0_26_container_27_53_main_54_62_hotbar() {
    let def = MenuDef::chest(3);
    let mut expected = run(MenuDef::CONTAINER, 27, SlotBehaviour::Normal);
    expected.extend(player_part());
    assert_eq!(layout(&def), expected);
    assert_eq!(def.slots.len(), 63);
    assert_eq!(def.hotbar, (54..63).map(SlotIx).collect::<Vec<_>>());
    assert_eq!(def.offhand, None);
    assert_eq!(def.inventory_sizes(), vec![27, 27, 9]);
}

#[test]
fn chest_6_is_0_53_container_54_80_main_81_89_hotbar() {
    let def = MenuDef::chest(6);
    let mut expected = run(MenuDef::CONTAINER, 54, SlotBehaviour::Normal);
    expected.extend(player_part());
    assert_eq!(layout(&def), expected);
    assert_eq!(def.slots.len(), 90);
    assert_eq!(def.hotbar, (81..90).map(SlotIx).collect::<Vec<_>>());
}

#[test]
fn generic_5_matches_a_hopper() {
    let def = MenuDef::generic(5);
    assert_eq!(def.slots.len(), 41);
    assert_eq!(def.slot_of(MenuDef::PLAYER_MAIN, 0), Some(SlotIx(5)));
    assert_eq!(def.slot_of(MenuDef::PLAYER_HOTBAR, 8), Some(SlotIx(40)));
}

#[test]
fn player_is_0_result_1_4_grid_5_8_armor_9_35_main_36_44_hotbar_45_offhand() {
    let def = MenuDef::player();
    let mut expected = run(MenuDef::CONTAINER, 1, SlotBehaviour::Output);
    expected.extend(run(MenuDef::CRAFT_GRID, 4, SlotBehaviour::Normal));
    expected.extend(run(MenuDef::PLAYER_ARMOR, 4, SlotBehaviour::Normal));
    expected.extend(player_part());
    expected.extend(run(MenuDef::PLAYER_OFFHAND, 1, SlotBehaviour::Normal));
    assert_eq!(layout(&def), expected);
    assert_eq!(def.slots.len(), 46);
    assert_eq!(def.hotbar, (36..45).map(SlotIx).collect::<Vec<_>>());
    assert_eq!(def.offhand, Some(SlotIx(45)));
    for s in 5..9 {
        let sd = def.slot(SlotIx(s)).unwrap();
        assert_eq!(sd.max_stack, Some(1));
        assert!(sd.accepts.is_some());
    }
}

#[test]
fn quick_move_tables_match_vanilla_directions() {
    let chest = MenuDef::chest(3);
    let to_player = chest.quick_move.resolve_rule(SlotIx(10)).unwrap();
    assert_eq!(
        to_player.to,
        vec![MenuDef::PLAYER_MAIN, MenuDef::PLAYER_HOTBAR]
    );
    assert!(to_player.reverse);
    let to_chest = chest.quick_move.resolve_rule(SlotIx(40)).unwrap();
    assert_eq!(to_chest.to, vec![MenuDef::CONTAINER]);
    assert!(!to_chest.reverse);

    let player = MenuDef::player();
    assert_eq!(
        player.quick_move.resolve(SlotIx(0)),
        &[MenuDef::PLAYER_MAIN, MenuDef::PLAYER_HOTBAR]
    );
    assert_eq!(
        player.quick_move.resolve(SlotIx(3)),
        &[MenuDef::PLAYER_MAIN, MenuDef::PLAYER_HOTBAR]
    );
    assert_eq!(
        player.quick_move.resolve(SlotIx(7)),
        &[MenuDef::PLAYER_MAIN, MenuDef::PLAYER_HOTBAR]
    );
    assert_eq!(
        player.quick_move.resolve(SlotIx(20)),
        &[MenuDef::PLAYER_HOTBAR]
    );
    assert_eq!(
        player.quick_move.resolve(SlotIx(40)),
        &[MenuDef::PLAYER_MAIN]
    );
    assert_eq!(
        player.quick_move.resolve(SlotIx(45)),
        &[MenuDef::PLAYER_MAIN, MenuDef::PLAYER_HOTBAR]
    );
}
