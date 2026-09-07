//! `UiScale` and the display's scale factor.
//!
//! A `Node`'s pixels are UI units: Bevy multiplies them by the render
//! target's scale factor and by `UiScale` on the way to the screen. Two
//! things follow, and both are asserted here. A slot written at the theme's
//! `slot_size` measures that many units whatever the scales are, so its
//! physical rect grows with them. And the item browser, which plans its dock
//! from the window rather than from the node tree, has to convert the window
//! into UI units first: at `UiScale` 2 the same 1280x720 window is a 640x360
//! box as far as the panel is concerned, so the panel re-docks into a strip
//! half as wide and fits fewer cards.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use bevy::prelude::*;
use bevy::ui::ui_transform::UiGlobalTransform;
use slotted_browser::{DefaultScreenHandler, ScreenHandlers};
use slotted_test::locator::by;
use slotted_test::prelude::*;
use slotted_ui::{Layout, ScreenDef, Screens, UiNodeDef};

mod common;

const CHEST: &str = "demo:chest";
const WIDTH: f32 = 1920.0;
const HEIGHT: f32 = 1080.0;

fn bare_chest() -> ScreenDef {
    ScreenDef {
        kind: ScreenKind::new(CHEST),
        initial_focus: None,
        presentation: slotted_ui::Presentation::default(),
        inherits: None,
        remove: vec![],
        root: UiNodeDef::Panel {
            role: slotted_theme::roles::PANEL,
            layout: Layout::default(),
            children: vec![UiNodeDef::SlotGrid {
                inventory: MenuDef::CONTAINER,
                cols: 9,
                rows: 3,
                first: 0,
                tags: slotted_ui::Tags::default(),
            }],
            tags: slotted_ui::Tags::default(),
        },
        listring: vec![],
    }
}

/// The chest harness at a given `UiScale` and display scale factor.
fn harness(ui_scale: f32, scale_factor: f32) -> UiHarness {
    let mut h = UiHarness::builder()
        .plugins(SlottedPlugins::headless())
        .registries(TestRegistries::basic())
        .resolution(WIDTH, HEIGHT)
        .ui_scale(ui_scale)
        .scale_factor(scale_factor)
        .theme("glass")
        .build();
    h.world_mut()
        .resource_mut::<Screens>()
        .register(bare_chest());
    h.world_mut()
        .resource_mut::<ScreenHandlers>()
        .register(ScreenKind::new(CHEST), Arc::new(DefaultScreenHandler));
    h
}

fn open(h: &mut UiHarness) -> Opened {
    let opened = h.open_screen(ScreenKind::new(CHEST), ChestFixture::filled());
    h.browser().wait_for_index();
    h.settle();
    opened
}

/// The first slot's physical rect: what the player's display actually shows.
fn first_slot_size(h: &mut UiHarness) -> Vec2 {
    let entity = *h
        .find_all(&by::role(SemanticRole::Slot))
        .first()
        .expect("the chest has slots");
    h.world().get::<ComputedNode>(entity).unwrap().size()
}

/// The panel's physical rect, from its `ComputedNode` and transform.
fn panel_rect(h: &mut UiHarness) -> Rect {
    let entity = *h
        .find_all(&by::role(SemanticRole::Browser))
        .first()
        .expect("the browser panel is attached");
    let node = h.world().get::<ComputedNode>(entity).unwrap();
    let tf = h.world().get::<UiGlobalTransform>(entity).unwrap();
    Rect::from_center_size(tf.translation, node.size())
}

#[test]
fn ui_scale_two_doubles_every_slot_and_re_docks_the_browser() {
    let mut one = harness(1.0, 1.0);
    let screen = open(&mut one).screen;
    let slot_at_one = first_slot_size(&mut one);
    let panel_at_one = panel_rect(&mut one);
    let cols_at_one = one.browser().layout(screen).unwrap().cols;

    let mut two = harness(2.0, 1.0);
    let screen = open(&mut two).screen;
    let slot_at_two = first_slot_size(&mut two);
    let panel_at_two = panel_rect(&mut two);
    let layout_at_two = two.browser().layout(screen).unwrap();

    assert!(
        (slot_at_two - slot_at_one * 2.0).abs().max_element() < 0.5,
        "a slot is drawn twice as big at UiScale 2: {slot_at_one} then {slot_at_two}"
    );
    assert!(
        panel_at_two.max.x <= WIDTH + 1.0 && panel_at_two.min.x >= -1.0,
        "the panel stays inside the window rather than being scaled twice: \
         {panel_at_two:?} in a {WIDTH} px window"
    );
    assert!(
        panel_at_two.width() > panel_at_one.width(),
        "a UI unit is twice as many pixels, so the panel is drawn wider even \
         though it is fewer units across: {} then {}",
        panel_at_one.width(),
        panel_at_two.width()
    );
    assert!(
        layout_at_two.cols < cols_at_one,
        "half as many UI units of free strip fits fewer card columns: {cols_at_one} then {}",
        layout_at_two.cols
    );
}

#[test]
fn a_scale_factor_of_two_doubles_every_slot_and_re_docks_the_browser() {
    let mut one = harness(1.0, 1.0);
    let screen = open(&mut one).screen;
    let slot_at_one = first_slot_size(&mut one);
    let cols_at_one = one.browser().layout(screen).unwrap().cols;

    let mut two = harness(1.0, 2.0);
    let screen = open(&mut two).screen;
    let slot_at_two = first_slot_size(&mut two);
    let layout_at_two = two.browser().layout(screen).unwrap();

    assert!(
        (slot_at_two - slot_at_one * 2.0).abs().max_element() < 0.5,
        "a slot's physical rect follows the display: {slot_at_one} then {slot_at_two}"
    );
    assert_eq!(
        layout_at_two.cols, cols_at_one,
        "the window's logical size is unchanged, so the dock is unchanged"
    );
}

#[test]
fn a_theme_with_a_bigger_slot_gets_bigger_slots() {
    let mut h = harness(1.0, 1.0);
    open(&mut h);
    let before = first_slot_size(&mut h);

    // A theme is an asset, so a size token is changed by installing one: the
    // harness's own theme file may or may not have loaded, and either way a
    // widget reads the token when it spawns.
    let mut tokens = slotted_theme::Tokens::default();
    tokens.sizes.slot_size = 64.0;
    let theme = slotted_theme::Theme {
        name: "big-slots".to_owned(),
        tokens,
        roles: std::collections::HashMap::new(),
    };
    let handle = h
        .world_mut()
        .resource_mut::<Assets<slotted_theme::Theme>>()
        .add(theme);
    h.world_mut()
        .insert_resource(slotted_theme::ActiveTheme(handle));
    // A widget reads its sizes as it spawns, so the new numbers arrive with
    // the next screen rather than by resizing live nodes.
    h.open_screen(ScreenKind::new(CHEST), ChestFixture::filled());
    h.settle();
    // The first screen is still open, so both sizes are on the glass at once:
    // the point is that the screen spawned under the new theme is the one
    // drawn at 64.
    let sizes: Vec<f32> = h
        .find_all(&by::role(SemanticRole::Slot))
        .into_iter()
        .map(|e| h.world().get::<ComputedNode>(e).unwrap().size().x)
        .collect();

    assert!(
        (before.x - 44.0).abs() < 0.5,
        "the default slot is 44 px: {before}"
    );
    assert!(
        sizes.iter().any(|w| (w - 64.0).abs() < 0.5),
        "the token is what a slot measures: {sizes:?}"
    );
}

// ---------------------------------------------------------------------------
// Keyboard shortcuts while a text field has the keyboard
// ---------------------------------------------------------------------------

/// The screen's first slot.
fn first_slot(h: &mut UiHarness) -> Entity {
    *h.find_all(&by::role(SemanticRole::Slot))
        .first()
        .expect("the chest has slots")
}

#[test]
fn digits_do_not_swap_the_hotbar_while_the_search_field_has_the_keyboard() {
    let mut h = harness(1.0, 1.0);
    open(&mut h);
    let slot = first_slot(&mut h);
    let before = h
        .world()
        .get::<slotted_ui::ItemView>(slot)
        .unwrap()
        .stack
        .clone();

    h.browser().focus_search();
    assert!(
        h.world()
            .resource::<bevy::input_focus::InputFocus>()
            .get()
            .is_some(),
        "the search field owns the keyboard"
    );
    h.hover(slot);
    h.key(KeyCode::Digit3);
    h.settle();
    let after = h
        .world()
        .get::<slotted_ui::ItemView>(slot)
        .unwrap()
        .stack
        .clone();
    assert_eq!(before, after, "the digit went to the field, not the hotbar");

    // With the field blurred the same key does swap, so the gate is the focus
    // and not a broken shortcut.
    h.browser().blur_search();
    h.hover(slot);
    h.key(KeyCode::Digit3);
    h.settle();
    let swapped = h
        .world()
        .get::<slotted_ui::ItemView>(slot)
        .unwrap()
        .stack
        .clone();
    assert_ne!(
        before, swapped,
        "a blurred field lets the hotbar swap through"
    );
}

#[test]
fn the_arrow_keys_stay_in_the_search_field_while_it_has_the_keyboard() {
    let mut h = harness(1.0, 1.0);
    open(&mut h);
    h.browser().focus_search();
    let field = h
        .world()
        .resource::<bevy::input_focus::InputFocus>()
        .get()
        .expect("the search field owns the keyboard");

    h.key(KeyCode::ArrowRight);
    h.key(KeyCode::ArrowDown);
    h.settle();

    assert_eq!(
        h.world().resource::<bevy::input_focus::InputFocus>().get(),
        Some(field),
        "directional navigation left the caret alone"
    );
}
