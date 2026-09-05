//! The harness against the headless stack. These pin the three ADR 0002
//! workarounds and the `settle()` contract; they fail loudly if Bevy changes
//! underneath us.
#![allow(clippy::unwrap_used)]

use std::sync::Arc;
use std::time::Duration;

use bevy::input_focus::tab_navigation::{TabGroup, TabIndex};
use bevy::picking::hover::Hovered;
use bevy::picking::pointer::{Location, PointerAction, PointerButton, PointerInput};
use bevy::prelude::*;
use bevy::ui_widgets::Activate;
use pretty_assertions::assert_eq;
use slotted_test::prelude::*;
use slotted_theme::{Tween, TweenTarget};
use slotted_ui::{ScreenDef, Screens, TestId, UiNodeDef};

fn harness() -> UiHarness {
    UiHarness::builder()
        .plugins(SlottedPlugins::headless())
        .resolution(1280.0, 720.0)
        .build()
}

#[derive(Resource, Default)]
struct Activated(Vec<Entity>);

/// A 3x3 grid of 44px buttons at (100, 100), the spike's geometry.
fn spawn_grid(h: &mut UiHarness) -> Vec<Entity> {
    h.app.init_resource::<Activated>();
    h.app
        .add_observer(|on: On<Activate>, mut a: ResMut<Activated>| a.0.push(on.entity));
    let world = h.world_mut();
    let root = world
        .spawn((
            Node {
                width: percent(100),
                height: percent(100),
                ..default()
            },
            TabGroup::new(0),
        ))
        .id();
    let grid = world
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: px(100),
                top: px(100),
                display: Display::Grid,
                grid_template_columns: vec![RepeatedGridTrack::px(3, 44.0)],
                grid_template_rows: vec![RepeatedGridTrack::px(3, 44.0)],
                row_gap: px(4),
                column_gap: px(4),
                ..default()
            },
            ChildOf(root),
        ))
        .id();
    let mut slots = Vec::new();
    for i in 0..9 {
        slots.push(
            world
                .spawn((
                    Node {
                        width: px(44),
                        height: px(44),
                        ..default()
                    },
                    bevy::ui_widgets::Button,
                    Hovered::default(),
                    TabIndex(i),
                    bevy::ui::auto_directional_navigation::AutoDirectionalNavigation::default(),
                    SemanticRole::Slot,
                    TestId::new(format!("slot:{i}")),
                    ChildOf(grid),
                ))
                .id(),
        );
    }
    h.step(2);
    slots
}

/// Workaround 1 (camera `target_info`) and 2 (render assets): without them
/// layout collapses or the first frame panics.
#[test]
fn layout_is_exact_without_a_renderer() {
    let mut h = harness();
    let slots = spawn_grid(&mut h);
    assert_eq!(h.rect_of(slots[0]), Rect::new(100.0, 100.0, 144.0, 144.0));
    assert_eq!(h.rect_of(slots[4]), Rect::new(148.0, 148.0, 192.0, 192.0));
    assert_eq!(h.rect_of(slots[8]), Rect::new(196.0, 196.0, 240.0, 240.0));
}

/// The real picking backend routes a click to exactly one slot and flips
/// `Hovered`, and `Activate` fires from the synthetic click.
#[test]
fn primary_pointer_clicks_and_hovers_through_real_picking() {
    let mut h = harness();
    let slots = spawn_grid(&mut h);
    h.hover(slots[4]);
    assert!(h.world().get::<Hovered>(slots[4]).unwrap().get());
    assert!(!h.world().get::<Hovered>(slots[3]).unwrap().get());
    h.click(slots[4]);
    assert_eq!(h.world().resource::<Activated>().0, vec![slots[4]]);
    // The gutter hits nothing.
    h.click_at(Vec2::new(146.0, 120.0), PointerButton::Primary);
    assert_eq!(h.world().resource::<Activated>().0, vec![slots[4]]);
}

/// Workaround 3, pinned: `update_is_hovered` hard-codes `PointerId::Mouse`,
/// so a custom pointer clicks correctly but never drives `Hovered`. If this
/// test starts failing, upstream generalised the system and the harness can
/// stop caring which id it uses.
#[test]
fn custom_pointer_clicks_but_does_not_update_hovered() {
    let mut h = harness();
    let slots = spawn_grid(&mut h);
    let custom = h.spawn_custom_pointer();
    let id = *h
        .world()
        .get::<bevy::picking::pointer::PointerId>(custom)
        .unwrap();
    let target = bevy::camera::NormalizedRenderTarget::Window(
        bevy::window::WindowRef::Primary
            .normalize(Some(h.window()))
            .unwrap(),
    );
    let pos = h.center_of(slots[0]);
    let loc = || Location {
        target: target.clone(),
        position: pos,
    };
    h.world_mut().write_message(PointerInput::new(
        id,
        loc(),
        PointerAction::Move { delta: pos },
    ));
    h.step(1);
    assert!(
        !h.world().get::<Hovered>(slots[0]).unwrap().get(),
        "PointerId::Custom now drives Hovered; revisit ADR 0002 workaround 3"
    );
    h.world_mut().write_message(PointerInput::new(
        id,
        loc(),
        PointerAction::Press(PointerButton::Primary),
    ));
    h.step(1);
    h.world_mut().write_message(PointerInput::new(
        id,
        loc(),
        PointerAction::Release(PointerButton::Primary),
    ));
    h.step(1);
    assert_eq!(h.world().resource::<Activated>().0, vec![slots[0]]);
}

#[test]
fn keyboard_focus_and_arrow_navigation_work() {
    let mut h = harness();
    let slots = spawn_grid(&mut h);
    h.set_focus(Some(slots[4]));
    h.focus_dir(bevy::math::CompassOctant::East);
    assert_eq!(h.focused(), Some(slots[5]));
    h.focus_dir(bevy::math::CompassOctant::South);
    assert_eq!(h.focused(), Some(slots[8]));
    h.focus_next();
    // Tab wraps within the group.
    assert_eq!(h.focused(), Some(slots[0]));
    h.key(KeyCode::Enter);
    assert_eq!(h.world().resource::<Activated>().0, vec![slots[0]]);
}

#[test]
fn locators_resolve_over_the_semantic_tree() {
    let mut h = harness();
    let slots = spawn_grid(&mut h);
    assert_eq!(h.find_all(&by::role(SemanticRole::Slot)).len(), 9);
    assert_eq!(h.find(&by::test_id("slot:7")), slots[7]);
    assert_eq!(h.find(&by::role(SemanticRole::Slot).index(2)), slots[2]);
    assert_eq!(h.try_find(&by::test_id("nope")), None);
}

#[test]
fn settle_terminates_with_motion_on_and_off() {
    let mut h = harness();
    let node = h.world_mut().spawn(Node::default()).id();
    let settled = h.settle();
    assert!(settled >= 1);

    h.world_mut().entity_mut(node).insert(Tween {
        target: TweenTarget::Alpha { from: 0.0, to: 1.0 },
        duration: Duration::from_millis(100),
        elapsed: Duration::ZERO,
    });
    let frames = h.settle();
    // 100 ms at 16.667 ms per frame is 6 frames, plus one for the frame that
    // notices the tween is gone.
    assert!((6..=8).contains(&frames), "settled in {frames} frames");
    assert!(h.world().get::<Tween>(node).is_none());

    let mut reduced = UiHarness::builder()
        .plugins(SlottedPlugins::headless())
        .motion(slotted_theme::Motion::REDUCED)
        .max_settle_frames(10)
        .build();
    let r = reduced.try_settle();
    assert!(r.is_ok());
}

#[test]
fn settle_gives_up_at_the_frame_cap() {
    let mut h = UiHarness::builder()
        .plugins(SlottedPlugins::headless())
        .max_settle_frames(5)
        .build();
    h.world_mut().spawn(Tween {
        target: TweenTarget::Alpha { from: 0.0, to: 1.0 },
        duration: Duration::from_mins(1),
        elapsed: Duration::ZERO,
    });
    let err = h.try_settle().expect_err("never settles");
    assert_eq!(err.frames, 5);
    assert_eq!(err.motions, 1);
}

#[test]
fn open_screen_spawns_a_screen_root_bound_to_a_menu() {
    let mut h = harness();
    let kind = ScreenKind::new("demo:chest");
    h.world_mut().resource_mut::<Screens>().register(ScreenDef {
        kind: kind.clone(),
        inherits: None,
        root: UiNodeDef::Panel {
            role: slotted_theme::roles::PANEL,
            layout: slotted_ui::Layout::default(),
            children: vec![UiNodeDef::SlotGrid {
                inventory: MenuDef::CONTAINER,
                cols: 9,
                rows: 3,
                first: 0,
                tags: slotted_ui::Tags::new().with("region", "chest"),
            }],
            tags: slotted_ui::Tags::new().with("test_id", "chest_panel"),
        },
        listring: vec![],
    });
    let opened = h.open_screen(kind.clone(), MenuDef::chest(3));
    h.settle();

    assert!(
        h.world()
            .get::<slotted_ecs::OpenMenu>(opened.menu)
            .is_some()
    );
    assert_eq!(opened.inventories.len(), 3);
    let root = h.find(&by::screen(kind));
    assert_eq!(root, opened.screen);
    assert_eq!(
        h.world().get::<slotted_ui::ScreenRoot>(root).unwrap().menu,
        Some(opened.menu)
    );
    let tree = h.screen_tree();
    assert_eq!(tree.roots.len(), 2, "screen root and carried layer");
    assert_eq!(tree.roots[0].role, SemanticRole::Screen);
    assert_eq!(tree.roots[1].role, SemanticRole::Carried);
    // Tags and test ids from the def land on the spawned node even in the
    // skeleton; widget bodies fill in the rest.
    assert!(h.try_find(&by::test_id("chest_panel")).is_some());
    let _ = Arc::new(());
}
