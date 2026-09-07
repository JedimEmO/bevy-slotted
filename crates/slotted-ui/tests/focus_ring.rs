//! The focus ring and the nav graph (menus contract 2.3, 2.4, 2.6): the ring
//! hides on the mouse and shows on the first key, follows focus with a tween
//! or a snap, explicit `nav.*` links beat the auto navigator, an unresolved
//! link falls through, and `initial_focus` lands where a screen says.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use bevy::prelude::*;
use slotted_model::{InventoryRef, MenuDef};
use slotted_test::prelude::*;
use slotted_theme::{Motion, Theme, Tween, roles};
use slotted_ui::def::{Layout, Tags, UiNodeDef};
use slotted_ui::{
    FocusRing, FocusRingState, Focusable, InputMode, Presentation, PresentationMode, ScreenDef,
    ScreenRoot, SemanticRole,
};

const CHEST: &str = "focus:chest";

fn grid(inventory: InventoryRef, first: u16, tags: Tags) -> UiNodeDef {
    UiNodeDef::SlotGrid {
        inventory,
        cols: 9,
        rows: 3,
        first,
        tags,
    }
}

/// A chest grid over a player grid over a button. The chest grid links
/// `Down` straight to the button, past the player grid, and `Left` to an id
/// that exists nowhere.
fn chest_screen(initial_focus: Option<&str>, mode: PresentationMode) -> ScreenDef {
    ScreenDef {
        kind: ScreenKind::new(CHEST),
        initial_focus: initial_focus.map(str::to_owned),
        presentation: Presentation {
            mode,
            ..Presentation::default()
        },
        inherits: None,
        remove: vec![],
        root: UiNodeDef::Panel {
            role: roles::PANEL,
            layout: Layout {
                gap: 6.0,
                padding: 8.0.into(),
                ..Layout::default()
            },
            children: vec![
                grid(
                    MenuDef::CONTAINER,
                    0,
                    Tags::new()
                        .with("region", "chest")
                        .with("test_id", "chest_grid")
                        .with(Tags::NAV_DOWN, "done")
                        .with(Tags::NAV_LEFT, "nowhere"),
                ),
                grid(
                    MenuDef::PLAYER_MAIN,
                    27,
                    Tags::new()
                        .with("region", "player")
                        .with("test_id", "player_grid"),
                ),
                UiNodeDef::Button {
                    widget: Some(slotted_ui::widgets::kinds::button()),
                    opts: slotted_ui::ButtonOpts::default(),
                    tags: Tags::new().with("test_id", "done"),
                },
            ],
            tags: Tags::new().with("test_id", "chest_panel"),
        },
        listring: vec![MenuDef::CONTAINER, MenuDef::PLAYER_MAIN],
    }
}

fn harness(motion: Motion) -> UiHarness {
    UiHarness::builder()
        .plugins(SlottedPlugins::headless())
        .registries(TestRegistries::basic())
        .theme("glass")
        .motion(motion)
        .build()
}

fn open(h: &mut UiHarness, screen: ScreenDef) -> Entity {
    let opened = h.open_screen(screen, ChestFixture::filled());
    h.settle();
    opened.screen
}

fn slot(h: &UiHarness, region: &str, index: usize) -> Entity {
    h.find(
        &by::role(SemanticRole::Slot)
            .tag("region", region)
            .index(index),
    )
}

fn ring(h: &mut UiHarness) -> Entity {
    h.world_mut()
        .query_filtered::<Entity, With<FocusRing>>()
        .single(h.world())
        .expect("one focus ring")
}

fn ring_state(h: &mut UiHarness) -> FocusRingState {
    let ring = ring(h);
    *h.world().get::<FocusRingState>(ring).unwrap()
}

fn xs(h: &UiHarness) -> f32 {
    slotted_ui::active_tokens(h.world()).spacing.xs
}

fn close(a: Rect, b: Rect) -> bool {
    (a.min - b.min).abs().max_element() < 0.5 && (a.max - b.max).abs().max_element() < 0.5
}

fn grown(rect: Rect, by: f32) -> Rect {
    Rect {
        min: rect.min - Vec2::splat(by),
        max: rect.max + Vec2::splat(by),
    }
}

// ---------------------------------------------------------------------------
// The ring
// ---------------------------------------------------------------------------

#[test]
fn the_ring_is_hidden_on_the_mouse_and_shows_on_the_first_key_press() {
    let mut h = harness(Motion::default());
    open(&mut h, chest_screen(None, PresentationMode::Page));
    let first = slot(&h, "chest", 0);

    // Focus landed, but the player has not touched a key: no ring.
    assert_eq!(h.focused(), Some(first));
    assert_eq!(*h.world().resource::<InputMode>(), InputMode::Pointer);
    let state = ring_state(&mut h);
    assert_eq!(state.target, Some(first));
    assert!(!state.visible);
    let ring = ring(&mut h);
    assert_eq!(h.world().get::<Visibility>(ring), Some(&Visibility::Hidden));

    // One key: the ring shows around the newly focused slot.
    h.key(KeyCode::ArrowRight);
    h.settle();
    let second = slot(&h, "chest", 1);
    assert_eq!(h.focused(), Some(second));
    let state = ring_state(&mut h);
    assert_eq!(
        state,
        FocusRingState {
            target: Some(second),
            visible: true,
        }
    );
    assert_eq!(
        h.world().get::<Visibility>(ring),
        Some(&Visibility::Inherited)
    );
    let want = grown(h.rect_of(second), xs(&h));
    let got = h.rect_of(ring);
    assert!(close(got, want), "ring {got:?} around slot {want:?}");

    // Back on the mouse: hidden again, target kept.
    h.pointer_move_to(Vec2::new(300.0, 300.0));
    h.step(1);
    let state = ring_state(&mut h);
    assert_eq!(state.target, Some(second));
    assert!(!state.visible);
}

#[test]
fn the_ring_slides_between_targets_and_settles_on_the_new_one() {
    let mut h = harness(Motion::default());
    open(&mut h, chest_screen(None, PresentationMode::Page));
    h.key(KeyCode::ArrowRight);
    h.settle();
    let ring = ring(&mut h);

    // The next move starts a tween on the ring.
    h.key(KeyCode::ArrowRight);
    assert!(
        h.world().get::<Tween>(ring).is_some(),
        "a move slides rather than snaps"
    );
    let target = slot(&h, "chest", 2);
    assert_eq!(h.focused(), Some(target));
    h.settle();
    assert!(h.world().get::<Tween>(ring).is_none());
    let want = grown(h.rect_of(target), xs(&h));
    let got = h.rect_of(ring);
    assert!(close(got, want), "ring {got:?} around slot {want:?}");
}

#[test]
fn the_ring_snaps_under_reduced_motion() {
    let mut h = harness(Motion::REDUCED);
    open(&mut h, chest_screen(None, PresentationMode::Page));
    h.key(KeyCode::ArrowRight);
    h.settle();
    let ring = ring(&mut h);

    h.key(KeyCode::ArrowRight);
    assert!(
        h.world().get::<Tween>(ring).is_none(),
        "reduced motion snaps"
    );
    let target = slot(&h, "chest", 2);
    let want = grown(h.rect_of(target), xs(&h));
    let node = h.world().get::<Node>(ring).unwrap();
    assert_eq!(node.left, Val::Px(want.min.x));
    assert_eq!(node.top, Val::Px(want.min.y));
}

#[test]
fn only_focusable_nodes_get_the_ring() {
    let mut h = harness(Motion::default());
    open(&mut h, chest_screen(None, PresentationMode::Page));
    let panel = h.find(&by::test_id("chest_panel"));
    assert!(h.world().get::<Focusable>(panel).is_none());
    h.key(KeyCode::ArrowRight);
    h.set_focus(Some(panel));
    let state = ring_state(&mut h);
    assert_eq!(state.target, None);
    assert!(!state.visible);
}

// ---------------------------------------------------------------------------
// The nav graph
// ---------------------------------------------------------------------------

#[test]
fn a_nav_link_beats_the_auto_navigator() {
    let mut h = harness(Motion::REDUCED);
    open(&mut h, chest_screen(None, PresentationMode::Page));
    let done = h.find(&by::test_id("done"));
    assert!(h.world().get::<Focusable>(done).is_some());

    // Down from the top-left chest slot: the navigator would pick the slot
    // under it; the grid's `nav.down` says the button.
    h.set_focus(Some(slot(&h, "chest", 0)));
    h.key(KeyCode::ArrowDown);
    assert_eq!(h.focused(), Some(done));

    // The link is on the grid, so every slot in it inherits it.
    h.set_focus(Some(slot(&h, "chest", 14)));
    h.key(KeyCode::ArrowDown);
    assert_eq!(h.focused(), Some(done));

    // The player grid has no link: the navigator does its usual thing.
    h.set_focus(Some(slot(&h, "player", 0)));
    h.key(KeyCode::ArrowRight);
    assert_eq!(h.focused(), Some(slot(&h, "player", 1)));
}

#[test]
fn an_unresolved_nav_link_falls_through_to_the_navigator() {
    let mut h = harness(Motion::REDUCED);
    open(&mut h, chest_screen(None, PresentationMode::Page));
    // `nav.left` names "nowhere"; the navigator still finds the slot to the
    // left.
    h.set_focus(Some(slot(&h, "chest", 1)));
    h.key(KeyCode::ArrowLeft);
    assert_eq!(h.focused(), Some(slot(&h, "chest", 0)));
    // Pressing again falls through to the navigator again, which finds the
    // `done` button: an M1 button is in the directional graph, and Bevy's
    // navigator takes anything whose centre is in the west half-plane as a
    // neighbour, however far below (docs/FOLLOWUPS.md, Menus M1). The link
    // never blocks a press, which is what this test is about.
    h.key(KeyCode::ArrowLeft);
    assert_eq!(h.focused(), Some(h.find(&by::test_id("done"))));
    // With the button gone from the graph's west, nothing is left of slot 0
    // and focus stays put.
    let done = h.find(&by::test_id("done"));
    h.world_mut()
        .entity_mut(done)
        .remove::<bevy::ui::auto_directional_navigation::AutoDirectionalNavigation>();
    h.set_focus(Some(slot(&h, "chest", 0)));
    h.key(KeyCode::ArrowLeft);
    assert_eq!(h.focused(), Some(slot(&h, "chest", 0)));
}

#[test]
fn initial_focus_lands_where_named() {
    let mut h = harness(Motion::REDUCED);
    let screen = open(
        &mut h,
        chest_screen(Some("player_grid"), PresentationMode::Page),
    );
    let first_player = slot(&h, "player", 0);
    assert_eq!(h.focused(), Some(first_player));
    let root = h.world().get::<ScreenRoot>(screen).unwrap();
    assert_eq!(root.initial_focus, Some(first_player));
}

#[test]
fn initial_focus_defaults_to_the_first_focusable_in_tree_order() {
    let mut h = harness(Motion::REDUCED);
    let screen = open(&mut h, chest_screen(None, PresentationMode::Page));
    let first = slot(&h, "chest", 0);
    assert_eq!(h.focused(), Some(first));
    let root = h.world().get::<ScreenRoot>(screen).unwrap();
    assert_eq!(root.initial_focus, Some(first));
}

#[test]
fn an_overlay_never_takes_focus() {
    let mut h = harness(Motion::REDUCED);
    let screen = open(&mut h, chest_screen(None, PresentationMode::Overlay));
    // Bevy's input-focus plugin parks focus on the window at startup; the
    // overlay must leave it there.
    assert_eq!(h.focused(), Some(h.window()));
    assert_eq!(ring_state(&mut h).target, None);
    let root = h.world().get::<ScreenRoot>(screen).unwrap();
    assert_eq!(root.initial_focus, None);
}

// ---------------------------------------------------------------------------
// Themes
// ---------------------------------------------------------------------------

#[test]
fn every_shipped_theme_defines_the_ring_and_the_scrim() {
    const GLASS: &str = include_str!("../../../assets/themes/glass.theme.ron");
    const PAPER: &str = include_str!("../../../assets/themes/paper.theme.ron");
    const NEON: &str = include_str!("../../../assets/themes/neon.theme.ron");
    assert_eq!(roles::ALL.len(), 90);
    assert!(roles::ALL.contains(&roles::FOCUS_RING));
    assert!(roles::ALL.contains(&roles::SCRIM));
    for (name, text) in [("glass", GLASS), ("paper", PAPER), ("neon", NEON)] {
        let theme = Theme::from_ron(text).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(
            theme.missing_roles(),
            Vec::<slotted_theme::Role>::new(),
            "{name}"
        );
        assert!(theme.roles.contains_key(&roles::FOCUS_RING), "{name}");
        assert!(theme.roles.contains_key(&roles::SCRIM), "{name}");
    }
}
