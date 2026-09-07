//! The chest on a gamepad, headless (menus contract 4.4): the screen opens
//! through the stack, the d-pad walks the slots with the focus ring
//! following, South picks a stack up and puts it down, East pops the screen
//! and the menu closes with every item accounted for.
//!
//! The screen is the real `assets/screens/demo_chest.screen.ron`, so what
//! this proves is what the windowed example and the playground open, not a
//! def written for the test.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use bevy::input::gamepad::GamepadButton;
use bevy::prelude::*;
use pretty_assertions::assert_eq;
use slotted_test::prelude::*;
use slotted_ui::{
    FocusRing, InputMode, Layout, LocKey, Presentation, PresentationMode, ScreenDef, Screens, Tags,
    TextRole, UiAction, UiNodeDef, WidgetKind,
};

const CHEST: &str = "demo:chest";
const CHEST_RON: &str = include_str!("../../../assets/screens/demo_chest.screen.ron");

/// The chest example's screen file, registered, over the filled fixture.
fn open_chest() -> (UiHarness, Opened) {
    let mut h = UiHarness::builder()
        .plugins(SlottedPlugins::headless())
        .registries(TestRegistries::basic())
        .resolution(1280.0, 720.0)
        .theme("glass")
        .build();
    h.world_mut()
        .resource_mut::<Screens>()
        .register(ScreenDef::from_ron(CHEST_RON).expect("the chest screen file parses"));
    let opened = h.open_screen(ScreenKind::new(CHEST), ChestFixture::filled());
    h.settle();
    (h, opened)
}

fn chest_slot(h: &UiHarness, n: usize) -> Entity {
    h.find(&by::role(SemanticRole::Slot).tag("region", "chest").index(n))
}

/// The ring's rect in logical px: the `FocusRing` entity's laid-out node.
fn ring_rect(h: &mut UiHarness) -> Rect {
    let ring = h
        .world_mut()
        .query_filtered::<Entity, With<FocusRing>>()
        .single(h.world())
        .expect("the harness app spawns one focus ring");
    h.rect_of(ring)
}

fn assert_ring_around(h: &mut UiHarness, slot: Entity) {
    let ring = h.focus_ring();
    assert!(ring.visible, "the ring shows in gamepad mode");
    assert_eq!(ring.target, Some(slot), "the ring sits on the focused slot");
    let slot_rect = h.rect_of(slot);
    let ring_rect = ring_rect(h);
    assert!(
        ring_rect.min.x < slot_rect.min.x
            && ring_rect.min.y < slot_rect.min.y
            && ring_rect.max.x > slot_rect.max.x
            && ring_rect.max.y > slot_rect.max.y,
        "the ring {ring_rect:?} is drawn around the slot {slot_rect:?}"
    );
}

#[test]
fn the_chest_opens_as_a_stack_entry_with_focus_on_its_first_slot() {
    let (mut h, opened) = open_chest();
    assert_eq!(h.stack(), vec![ScreenKind::new(CHEST)]);
    assert_eq!(
        h.focused(),
        Some(chest_slot(&h, 0)),
        "`initial_focus: chest_grid` lands on the grid's first slot"
    );
    assert_eq!(
        h.world()
            .resource::<slotted_ui::ScreenStack>()
            .top()
            .map(|e| (e.root, e.menu)),
        Some((opened.screen, Some(opened.menu)))
    );
    assert_eq!(h.input_mode(), InputMode::Pointer);
    assert!(
        !h.focus_ring().visible,
        "no ring until the player leaves the mouse"
    );
}

#[test]
fn the_dpad_walks_the_slots_and_the_ring_follows() {
    let (mut h, _) = open_chest();
    h.set_input_mode(InputMode::Gamepad);
    h.settle();
    assert_eq!(h.input_mode(), InputMode::Gamepad);
    let first = chest_slot(&h, 0);
    assert_ring_around(&mut h, first);

    // Along the first row.
    for n in 1..9 {
        h.gamepad(GamepadButton::DPadRight);
        h.settle();
        let slot = chest_slot(&h, n);
        assert_eq!(h.focused(), Some(slot), "DPadRight {n} times");
        assert_ring_around(&mut h, slot);
    }
    // Down a row from the end of the first.
    h.gamepad(GamepadButton::DPadDown);
    h.settle();
    let below = chest_slot(&h, 17);
    assert_eq!(h.focused(), Some(below));
    assert_ring_around(&mut h, below);
    // And back left along the second row.
    h.gamepad(GamepadButton::DPadLeft);
    h.settle();
    assert_eq!(h.focused(), Some(chest_slot(&h, 16)));

    assert_eq!(
        h.input_mode(),
        InputMode::Gamepad,
        "d-pad presses keep the mode"
    );
    h.assert_conserved();
}

#[test]
fn a_real_button_press_switches_the_mode_and_shows_the_ring() {
    let (mut h, _) = open_chest();
    assert!(!h.focus_ring().visible);
    h.gamepad(GamepadButton::DPadRight);
    h.settle();
    assert_eq!(h.input_mode(), InputMode::Gamepad);
    let second = chest_slot(&h, 1);
    assert_eq!(h.focused(), Some(second));
    assert_ring_around(&mut h, second);
}

#[test]
fn south_picks_the_focused_stack_up_and_puts_it_down_again() {
    let (mut h, opened) = open_chest();
    h.set_input_mode(InputMode::Gamepad);
    let first = chest_slot(&h, 0);
    let stack = h.stack_at(first).expect("the fixture fills chest slot 0");
    assert_eq!(stack.count, 64);

    h.gamepad(GamepadButton::South);
    h.settle();
    assert_eq!(h.stack_at(first), None, "the slot emptied");
    assert_eq!(h.carried(opened.menu), Some(stack.clone()), "into the hand");
    h.assert_conserved();

    h.gamepad(GamepadButton::DPadRight);
    h.gamepad(GamepadButton::DPadRight);
    h.gamepad(GamepadButton::DPadRight);
    h.settle();
    let target = chest_slot(&h, 3);
    assert_eq!(h.focused(), Some(target));
    assert_eq!(h.stack_at(target), None, "slot 3 starts empty");
    h.gamepad(GamepadButton::South);
    h.settle();
    assert_eq!(h.stack_at(target), Some(stack), "placed where the ring was");
    assert_eq!(h.carried(opened.menu), None);
    h.assert_conserved();
}

#[test]
fn east_pops_the_screen_and_closes_the_menu_with_items_conserved() {
    let (mut h, opened) = open_chest();
    h.set_input_mode(InputMode::Gamepad);
    // Something in the hand, so the close has a carried stack to account for.
    h.gamepad(GamepadButton::South);
    h.settle();
    assert!(h.carried(opened.menu).is_some());

    h.gamepad(GamepadButton::East);
    h.settle();

    assert!(h.stack().is_empty(), "East popped the chest");
    assert!(
        h.try_find(&by::screen(ScreenKind::new(CHEST))).is_none(),
        "the screen is gone"
    );
    assert!(
        h.world().get_entity(opened.menu).is_err(),
        "and the stack closed its menu"
    );
    assert_eq!(h.focused(), None, "nothing left to focus");
    assert!(!h.focus_ring().visible);
    h.assert_conserved();
}

#[test]
fn the_keyboard_accept_picks_up_the_focused_stack_once() {
    let (mut h, opened) = open_chest();
    let first = chest_slot(&h, 0);
    let stack = h.stack_at(first).unwrap();
    // Enter reaches the slot through `accept_focused` and, through Bevy's
    // `Button`, as an `Activate` nobody on a slot listens to: one click, not
    // two, or the double-click interpreter would collect instead.
    h.action(UiAction::Accept);
    h.settle();
    assert_eq!(h.stack_at(first), None);
    assert_eq!(h.carried(opened.menu), Some(stack));
    assert_eq!(h.input_mode(), InputMode::Keyboard);
    h.assert_conserved();
}

#[test]
fn the_left_stick_walks_like_the_dpad_and_holds_until_released() {
    let (mut h, _) = open_chest();
    h.stick(Vec2::new(1.0, 0.0));
    h.settle();
    assert_eq!(h.input_mode(), InputMode::Gamepad);
    assert_eq!(
        h.focused(),
        Some(chest_slot(&h, 1)),
        "the stick past the deadzone is one `Right`"
    );
    // Held: the repeat fires after the delay, so a few hundred ms later the
    // focus has moved on.
    h.advance(std::time::Duration::from_millis(600));
    let after_hold = h.focused();
    assert_ne!(after_hold, Some(chest_slot(&h, 1)), "a held stick repeats");
    h.stick(Vec2::ZERO);
    h.advance(std::time::Duration::from_millis(600));
    assert_eq!(h.focused(), after_hold, "released, it stops");
    h.assert_conserved();
}

#[test]
fn back_through_the_keyboard_binding_pops_too() {
    let (mut h, opened) = open_chest();
    h.action(UiAction::Back);
    h.settle();
    assert!(h.stack().is_empty());
    assert!(h.world().get_entity(opened.menu).is_err());
    h.assert_conserved();
}

/// A pause menu: no menu entity, two buttons, focus named on the second.
const PAUSE: &str = "test:pause";

fn pause_screen() -> ScreenDef {
    let button = |id: &str| UiNodeDef::Button {
        widget: WidgetKind::new("slotted:button"),
        tags: Tags::new().with(Tags::TEST_ID, id),
    };
    ScreenDef {
        kind: ScreenKind::new(PAUSE),
        initial_focus: Some("quit".to_owned()),
        presentation: Presentation {
            mode: PresentationMode::Modal,
            ..Presentation::default()
        },
        inherits: None,
        remove: vec![],
        listring: vec![],
        root: UiNodeDef::Panel {
            role: slotted_theme::roles::PANEL,
            layout: Layout {
                gap: 1.0,
                padding: 2.0.into(),
                ..Layout::default()
            },
            children: vec![
                UiNodeDef::Text {
                    key: LocKey("pause.title".to_owned()),
                    style: TextRole::Title,
                    tags: Tags::new().with(Tags::TEST_ID, "title"),
                },
                button("resume"),
                button("quit"),
            ],
            tags: Tags::new().with(Tags::TEST_ID, "pause"),
        },
    }
}

#[test]
fn a_menu_less_screen_opens_through_the_stack_and_initial_focus_lands() {
    let (mut h, _) = open_chest();
    let root = h.open(pause_screen());
    h.settle();

    assert_eq!(
        h.stack(),
        vec![ScreenKind::new(CHEST), ScreenKind::new(PAUSE)],
        "pushed over the chest"
    );
    assert_eq!(h.find(&by::screen(ScreenKind::new(PAUSE))), root);
    assert_eq!(
        h.focused(),
        Some(h.find(&by::test_id("quit"))),
        "`initial_focus` names the second button"
    );
    assert!(
        h.is_visible(chest_slot(&h, 0)),
        "a modal keeps the chest visible under it"
    );

    // Back pops the modal and focus returns to the chest.
    h.gamepad(GamepadButton::East);
    h.settle();
    assert_eq!(h.stack(), vec![ScreenKind::new(CHEST)]);
    assert_eq!(h.focused(), Some(chest_slot(&h, 0)));
    h.assert_conserved();
}
