//! Sweep gestures and phantom previews, the two things the hands-on session
//! asked for after the drag-arming fix landed.
//!
//! 1. Shift held while the left button drags across slots quick-moves every
//!    slot it touches, once each (Mouse Tweaks, research section 6).
//! 2. A paint in progress shows what it is about to do: a dimmed phantom of
//!    the incoming item in every painted slot, and the count the cursor has
//!    left. Ghost, filter, output and locked slots say what they are for.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use bevy::picking::pointer::PointerButton;
use bevy::prelude::*;
use slotted_model::{InventoryRef, MenuDef, Predicate, SlotBehaviour, SlotIx};
use slotted_test::prelude::*;
use slotted_theme::Motion;
use slotted_ui::def::{Layout, Tags, UiNodeDef};
use slotted_ui::{ScreenDef, Screens, SemanticRole, SlotHint, SlotPhantom};

const CHEST: &str = "sweep:chest";
const SPECIAL: &str = "sweep:special";
const WIDTH: f32 = 1280.0;
const HEIGHT: f32 = 720.0;

// ---------------------------------------------------------------------------
// Screens
// ---------------------------------------------------------------------------

fn grid(inventory: InventoryRef, rows: u16, first: u16, region: &str) -> UiNodeDef {
    UiNodeDef::SlotGrid {
        inventory,
        cols: 9,
        rows,
        first,
        tags: Tags::new().with("region", region),
    }
}

fn chest_screen() -> ScreenDef {
    ScreenDef {
        kind: ScreenKind::new(CHEST),
        inherits: None,
        remove: vec![],
        root: UiNodeDef::Panel {
            role: slotted_theme::roles::PANEL,
            layout: Layout {
                gap: 6.0,
                padding: 8.0,
                ..Layout::default()
            },
            children: vec![
                grid(MenuDef::CONTAINER, 3, 0, "chest"),
                grid(MenuDef::PLAYER_MAIN, 3, 27, "player"),
            ],
            tags: Tags::new().with("test_id", "chest_panel"),
        },
        listring: vec![MenuDef::CONTAINER, MenuDef::PLAYER_MAIN],
    }
}

// ---------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------

fn harness() -> UiHarness {
    let mut h = UiHarness::builder()
        .plugins(SlottedPlugins::headless())
        .registries(TestRegistries::basic())
        .resolution(WIDTH, HEIGHT)
        .theme("glass")
        .build();
    {
        let mut screens = h.world_mut().resource_mut::<Screens>();
        screens.register(chest_screen());
        screens.register(special_screen());
    }
    h.world_mut().insert_resource(Motion::default());
    h
}

fn open_chest(h: &mut UiHarness, fixture: ChestFixture) -> Opened {
    let opened = h.open_screen(ScreenKind::new(CHEST), fixture);
    h.settle();
    opened
}

fn slot(h: &UiHarness, region: &str, index: usize) -> Entity {
    h.find(
        &by::role(SemanticRole::Slot)
            .tag("region", region)
            .index(index),
    )
}

fn filled_chest(count: usize) -> ChestFixture {
    let mut fixture = ChestFixture::empty();
    fixture.chest = (0..count)
        .map(|i| {
            (
                i,
                "minecraft:cobblestone".to_owned(),
                4 + u32::try_from(i).unwrap_or(u32::MAX),
            )
        })
        .collect();
    fixture
}

/// Every one of `item` the menu holds, cursor included. Read out of the
/// inventories rather than the grids: a quick-move may well land in the
/// hotbar, which this screen does not draw.
fn total(h: &UiHarness, opened: &Opened, item: &str) -> u32 {
    let id = TestRegistries::item(item);
    let menu = h.world().get::<slotted_ecs::OpenMenu>(opened.menu).unwrap();
    let mut n: u32 = menu
        .inventories
        .iter()
        .filter_map(|e| h.world().get::<slotted_ecs::Inventory>(*e))
        .flat_map(|inv| (0..inv.len()).filter_map(|i| inv.get(i)))
        .filter(|s| s.id == id)
        .map(|s| s.count)
        .sum();
    if let Some(c) = h.carried(opened.menu).filter(|s| s.id == id) {
        n += c.count;
    }
    n
}

// ---------------------------------------------------------------------------
// 1. Sweeps
// ---------------------------------------------------------------------------

/// Shift held, press on the first slot, drag over four more: five quick-moves,
/// one per slot, and not one item lost on the way.
#[test]
fn a_shift_sweep_quick_moves_every_slot_it_crosses() {
    let mut h = harness();
    let opened = open_chest(&mut h, filled_chest(5));
    let before = total(&h, &opened, "minecraft:cobblestone");
    let swept: Vec<Entity> = (0..5).map(|i| slot(&h, "chest", i)).collect();

    h.hold(KeyCode::ShiftLeft);
    h.pointer_move_to(h.center_of(swept[0]));
    h.pointer_press(PointerButton::Primary);
    for s in &swept[1..] {
        let pos = h.center_of(*s);
        h.pointer_move_to(pos);
    }
    h.pointer_release(PointerButton::Primary);
    h.release(KeyCode::ShiftLeft);
    h.settle();

    for (i, s) in swept.iter().enumerate() {
        assert!(
            h.stack_at(*s).is_none(),
            "chest slot {i} was not swept: {:?}",
            h.stack_at(*s)
        );
    }
    assert_eq!(
        total(&h, &opened, "minecraft:cobblestone"),
        before,
        "the sweep created or destroyed items"
    );
    assert!(h.carried(opened.menu).is_none(), "a sweep carries nothing");
    h.assert_conserved();
}

/// Crossing a slot twice moves it once. Without this the pointer wandering
/// back over an emptied slot would drag the stack straight back.
#[test]
fn revisiting_a_slot_mid_sweep_does_not_move_it_twice() {
    let mut h = harness();
    let opened = open_chest(&mut h, filled_chest(2));
    let a = slot(&h, "chest", 0);
    let b = slot(&h, "chest", 1);
    let before = total(&h, &opened, "minecraft:cobblestone");

    h.hold(KeyCode::ShiftLeft);
    h.pointer_move_to(h.center_of(a));
    h.pointer_press(PointerButton::Primary);
    for target in [b, a, b, a] {
        let pos = h.center_of(target);
        h.pointer_move_to(pos);
    }
    h.pointer_release(PointerButton::Primary);
    h.release(KeyCode::ShiftLeft);
    h.settle();

    // Both stacks left the chest, and neither was carried back into it.
    assert!(h.stack_at(a).is_none() && h.stack_at(b).is_none());
    assert_eq!(
        total(&h, &opened, "minecraft:cobblestone"),
        before,
        "a revisit moved a stack twice"
    );
}

/// A sweep over nothing does nothing. It is a gesture players make constantly
/// by accident.
#[test]
fn a_sweep_over_empty_slots_is_a_no_op() {
    let mut h = harness();
    let opened = open_chest(&mut h, ChestFixture::empty());
    let swept: Vec<Entity> = (0..4).map(|i| slot(&h, "chest", i)).collect();

    h.hold(KeyCode::ShiftLeft);
    h.pointer_move_to(h.center_of(swept[0]));
    h.pointer_press(PointerButton::Primary);
    for s in &swept[1..] {
        let pos = h.center_of(*s);
        h.pointer_move_to(pos);
    }
    h.pointer_release(PointerButton::Primary);
    h.release(KeyCode::ShiftLeft);
    h.settle();

    for s in &swept {
        assert!(h.stack_at(*s).is_none());
    }
    assert!(h.carried(opened.menu).is_none());
    h.assert_conserved();
}

/// The sweep ends with the button, and the next drag is the paint it always
/// was: nothing about the shift gesture leaks into the one after it.
#[test]
fn a_plain_drag_after_a_sweep_is_a_normal_paint() {
    let mut h = harness();
    let mut fixture = filled_chest(2);
    fixture.main = vec![(0, "minecraft:dirt".to_owned(), 8)];
    let opened = open_chest(&mut h, fixture);

    let a = slot(&h, "chest", 0);
    let b = slot(&h, "chest", 1);
    h.hold(KeyCode::ShiftLeft);
    h.pointer_move_to(h.center_of(a));
    h.pointer_press(PointerButton::Primary);
    let pos = h.center_of(b);
    h.pointer_move_to(pos);
    h.pointer_release(PointerButton::Primary);
    h.release(KeyCode::ShiftLeft);
    h.settle();
    assert!(h.stack_at(a).is_none() && h.stack_at(b).is_none());

    // Now a plain left drag with a carried stack: an even split, not a sweep.
    let carrier = slot(&h, "player", 0);
    h.click(carrier);
    h.settle();
    assert!(h.carried(opened.menu).is_some());
    h.drag_paint(&[a, b]);
    h.settle();

    assert_eq!(
        h.stack_at(a).map(|s| s.count),
        Some(4),
        "the drag after a sweep did not paint"
    );
    assert_eq!(h.stack_at(b).map(|s| s.count), Some(4));
}

// ---------------------------------------------------------------------------
// 2. Phantom previews
// ---------------------------------------------------------------------------

fn phantoms(h: &UiHarness) -> Vec<(Entity, u32)> {
    let mut out: Vec<(Entity, u32)> = h
        .find_all(&by::role(SemanticRole::Slot))
        .into_iter()
        .filter_map(|e| h.world().get::<SlotPhantom>(e).map(|p| (e, p.delta)))
        .collect();
    out.sort_by_key(|(e, _)| *e);
    out
}

/// A right-drag paints one item per slot. Three painted slots means three
/// phantoms saying `+1`, and a cursor that has visibly lost three items,
/// before a single item has actually moved.
#[test]
fn a_right_paint_shows_a_phantom_per_slot_and_counts_the_cursor_down() {
    let mut h = harness();
    let mut fixture = ChestFixture::empty();
    fixture.main = vec![(0, "minecraft:cobblestone".to_owned(), 9)];
    let opened = open_chest(&mut h, fixture);

    let carrier = slot(&h, "player", 0);
    h.click(carrier);
    h.settle();
    assert_eq!(h.carried(opened.menu).map(|s| s.count), Some(9));

    let painted: Vec<Entity> = (0..3).map(|i| slot(&h, "chest", i)).collect();
    h.pointer_move_to(h.center_of(painted[0]));
    h.pointer_press(PointerButton::Secondary);
    for s in &painted[1..] {
        let pos = h.center_of(*s);
        h.pointer_move_to(pos);
    }
    h.step(1);

    let shown = phantoms(&h);
    assert_eq!(shown.len(), 3, "one phantom per painted slot: {shown:?}");
    for (_, delta) in &shown {
        assert_eq!(*delta, 1, "a right paint places one item per slot");
    }
    assert_eq!(
        h.world()
            .resource::<slotted_ui::DragGhost>()
            .remaining
            .unwrap_or(0),
        6,
        "the ghost count did not drop by the three items it is about to place"
    );
    // Nothing has actually moved yet.
    for s in &painted {
        assert!(h.stack_at(*s).is_none());
    }

    h.pointer_release(PointerButton::Secondary);
    h.settle();

    assert!(phantoms(&h).is_empty(), "phantoms outlived the paint");
    for s in &painted {
        assert_eq!(h.stack_at(*s).map(|s| s.count), Some(1));
    }
    assert_eq!(h.carried(opened.menu).map(|s| s.count), Some(6));
}

/// The phantoms are not a second guess at the distribution: they are the
/// model's own plan, so an uneven left paint shows exactly what lands.
#[test]
fn the_left_paint_preview_is_the_models_own_plan() {
    let mut h = harness();
    let mut fixture = ChestFixture::empty();
    fixture.main = vec![(0, "minecraft:cobblestone".to_owned(), 7)];
    let opened = open_chest(&mut h, fixture);

    let carrier = slot(&h, "player", 0);
    h.click(carrier);
    h.settle();

    let painted: Vec<Entity> = (0..3).map(|i| slot(&h, "chest", i)).collect();
    h.pointer_move_to(h.center_of(painted[0]));
    h.pointer_press(PointerButton::Primary);
    for s in &painted[1..] {
        let pos = h.center_of(*s);
        h.pointer_move_to(pos);
    }
    h.step(1);

    let previewed: Vec<(SlotIx, u32)> = {
        let menu = h.world().get::<slotted_ecs::OpenMenu>(opened.menu).unwrap();
        let inv: slotted_model::Inventories = menu
            .inventories
            .iter()
            .map(|e| {
                h.world()
                    .get::<slotted_ecs::Inventory>(*e)
                    .unwrap()
                    .0
                    .clone()
            })
            .collect();
        let registries = h.world().resource::<slotted_ecs::Registries>();
        slotted_model::preview_drag(&menu.def, &inv, &menu.state, &registries.lookup())
            .into_iter()
            .map(|(s, p)| (s, p.delta))
            .collect()
    };
    assert_eq!(previewed.len(), 3, "the model plans three slots");

    let shown: Vec<(SlotIx, u32)> = painted
        .iter()
        .map(|e| {
            let p = h
                .world()
                .get::<SlotPhantom>(*e)
                .expect("a painted slot shows a phantom");
            let slot = h.world().get::<slotted_ecs::SlotRef>(*e).unwrap().slot;
            (slot, p.delta)
        })
        .collect();
    assert_eq!(shown, previewed, "the phantoms disagree with the model");

    h.pointer_release(PointerButton::Primary);
    h.settle();
    for (slot_entity, (_, delta)) in painted.iter().zip(previewed) {
        assert_eq!(
            h.stack_at(*slot_entity).map(|s| s.count),
            Some(delta),
            "the paint did not land where the phantom said"
        );
    }
}

// ---------------------------------------------------------------------------
// 3. Hints on special slots
// ---------------------------------------------------------------------------

/// A screen with one of each special slot, so the hints have somewhere to
/// show. Slot 0 is a filter, 1 an output, 2 locked, 3 an ordinary slot.
fn special_menu() -> MenuDef {
    let mut def = MenuDef::chest(1);
    def.slots[0].behaviour = SlotBehaviour::Filter;
    def.slots[0].accepts = Some(Predicate::ItemIs(slotted_model::ItemId(0)));
    def.slots[1].behaviour = SlotBehaviour::Output;
    def.slots[2].behaviour = SlotBehaviour::Locked;
    def
}

fn special_screen() -> ScreenDef {
    ScreenDef {
        kind: ScreenKind::new(SPECIAL),
        inherits: None,
        remove: vec![],
        root: UiNodeDef::Panel {
            role: slotted_theme::roles::PANEL,
            layout: Layout::default(),
            children: vec![grid(MenuDef::CONTAINER, 1, 0, "special")],
            tags: Tags::new().with("test_id", "special_panel"),
        },
        listring: vec![MenuDef::CONTAINER],
    }
}

/// An empty filter slot says what it takes, both as a dimmed icon and in the
/// tooltip. An empty chest slot says nothing at all.
#[test]
fn a_filter_slot_shows_its_hint_and_explains_itself() {
    let mut h = harness();
    h.open_screen(ScreenKind::new(SPECIAL), special_menu());
    h.settle();

    let filter = slot(&h, "special", 0);
    let output = slot(&h, "special", 1);
    let locked = slot(&h, "special", 2);
    let plain = slot(&h, "special", 3);

    let hint = |e: Entity| h.world().get::<SlotHint>(e).cloned();
    assert!(
        matches!(hint(filter), Some(SlotHint::Accepts { .. })),
        "the filter slot has no hint: {:?}",
        hint(filter)
    );
    assert!(matches!(hint(output), Some(SlotHint::Output)));
    assert!(matches!(hint(locked), Some(SlotHint::Locked)));
    assert!(hint(plain).is_none(), "an ordinary slot needs no hint");

    // And it says so in words, where a player will read it.
    h.request_tooltip(filter, slotted_ui::TooltipTier::Compact);
    h.settle();
    let text = format!("{:?}", h.tooltip().expect("the filter slot has a tooltip"));
    assert!(
        text.to_lowercase().contains("accepts"),
        "the filter tooltip does not say what it accepts: {text}"
    );
}

// ---------------------------------------------------------------------------
// 4. Validity on the carried ghost
// ---------------------------------------------------------------------------

/// Carrying a stack over a slot that will not take it says so before the
/// click: the ring goes to the theme's bad colour over an output slot and to
/// its good colour over an empty one.
#[test]
fn the_carried_ring_reads_bad_over_an_output_slot_and_good_over_an_empty_one() {
    let mut h = harness();
    let mut menu = special_menu();
    menu.slots[3].behaviour = SlotBehaviour::Normal;
    // Something to pick up, in the one ordinary slot.
    let mut inventories: Vec<slotted_model::Inventory> = menu
        .inventory_sizes()
        .into_iter()
        .map(slotted_model::Inventory::new)
        .collect();
    inventories[MenuDef::CONTAINER.index()].set(
        3,
        Some(ItemStack::new(
            TestRegistries::item("minecraft:cobblestone"),
            4,
        )),
    );
    let opened = h.open_screen(
        ScreenKind::new(SPECIAL),
        (std::sync::Arc::new(menu), inventories),
    );
    h.settle();

    let source = slot(&h, "special", 3);
    let output = slot(&h, "special", 1);
    let empty = slot(&h, "special", 4);
    h.click(source);
    h.settle();
    assert!(h.carried(opened.menu).is_some(), "holding a stack");

    h.hover(output);
    h.step(2);
    assert_eq!(
        h.world().resource::<slotted_ui::DragGhost>().validity,
        Some(slotted_ui::Validity::Bad),
        "an output slot takes nothing, and the ghost should say so"
    );

    h.hover(empty);
    h.step(2);
    assert_eq!(
        h.world().resource::<slotted_ui::DragGhost>().validity,
        Some(slotted_ui::Validity::Good),
        "an empty slot takes the stack"
    );
}

/// The five hint glyphs ship with the crate's assets. `HintGlyphs` loads them
/// by path, so a renamed file is a silently blank slot rather than an error.
#[test]
fn the_hint_glyphs_are_where_the_loader_looks_for_them() {
    let assets = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets/icons")
        .canonicalize()
        .expect("the workspace assets directory");
    for name in [
        "hint_output",
        "hint_locked",
        "hint_tag",
        "hint_any",
        "hint_info",
    ] {
        let path = assets.join(format!("{name}.png"));
        let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        assert_eq!(
            &bytes[..8],
            b"\x89PNG\r\n\x1a\n",
            "{} is not a PNG",
            path.display()
        );
    }
}
