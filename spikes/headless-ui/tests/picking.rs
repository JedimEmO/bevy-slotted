use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use bevy::ui::Pressed;
use headless_ui_spike::*;

#[derive(Resource, Default, Debug)]
struct Clicks(Vec<String>);

fn observe_clicks(h: &mut Harness) {
    h.app.init_resource::<Clicks>();
    let names: Vec<(Entity, String)> = {
        let mut q = h.app.world_mut().query::<(Entity, &TestName)>();
        q.iter(h.app.world())
            .map(|(e, n)| (e, n.0.clone()))
            .collect()
    };
    for (e, name) in names {
        h.app.world_mut().entity_mut(e).observe(
            move |_: On<Pointer<Click>>, mut clicks: ResMut<Clicks>| {
                clicks.0.push(name.clone());
            },
        );
    }
}

fn setup() -> Harness {
    let mut h = Harness::new(Screen::default());
    spawn_slot_grid(h.app.world_mut());
    h.step(2);
    observe_clicks(&mut h);
    h
}

#[test]
fn click_lands_on_the_right_slot() {
    let mut h = setup();
    let slot = h.find_by_name("slot:3,1").unwrap();
    h.pointer_click(slot);
    let clicks = &h.app.world().resource::<Clicks>().0;
    assert_eq!(clicks.first().map(String::as_str), Some("slot:3,1"));
}

#[test]
fn every_slot_is_reachable_by_its_own_centre() {
    let mut h = setup();
    for row in 0..ROWS {
        for col in 0..COLS {
            let name = format!("slot:{col},{row}");
            let e = h.find_by_name(&name).unwrap();
            h.pointer_click(e);
            assert_eq!(
                h.app.world().resource::<Clicks>().0.first(),
                Some(&name),
                "clicking centre of {name} hit the wrong node"
            );
            h.app.world_mut().resource_mut::<Clicks>().0.clear();
        }
    }
}

#[test]
fn click_in_the_gap_hits_no_slot() {
    let mut h = setup();
    // Two pixels into the 4px gutter between column 0 and column 1.
    let gap = GRID_ORIGIN + Vec2::new(SLOT + GAP / 2.0, SLOT / 2.0);
    h.click_at(gap);
    let clicks = &h.app.world().resource::<Clicks>().0;
    assert!(
        !clicks.iter().any(|c| c.starts_with("slot:")),
        "gap click hit {clicks:?}"
    );
}

#[test]
fn hovered_and_pressed_flip() {
    let mut h = setup();
    let slot = h.find_by_name("slot:0,0").unwrap();
    let other = h.find_by_name("slot:5,2").unwrap();

    assert!(!h.app.world().get::<Hovered>(slot).unwrap().0);

    h.pointer_move_to(h.center_of(slot));
    h.step(1);
    assert!(h.app.world().get::<Hovered>(slot).unwrap().0, "hover on");
    assert!(!h.app.world().get::<Hovered>(other).unwrap().0);

    h.pointer_press();
    assert!(
        h.app.world().get::<Pressed>(slot).is_some(),
        "Pressed added on pointer down"
    );

    h.pointer_release();
    assert!(
        h.app.world().get::<Pressed>(slot).is_none(),
        "Pressed removed on pointer up"
    );

    h.pointer_move_to(h.center_of(other));
    h.step(1);
    assert!(!h.app.world().get::<Hovered>(slot).unwrap().0, "hover off");
    assert!(h.app.world().get::<Hovered>(other).unwrap().0);
}

#[test]
fn higher_global_z_index_wins() {
    let mut h = setup();
    let slot = h.find_by_name("slot:2,0").unwrap();
    let rect = h.rect_of(slot);

    let root = h.find_by_name("root").unwrap();
    h.app.world_mut().spawn((
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(rect.min.x),
            top: Val::Px(rect.min.y),
            width: Val::Px(SLOT),
            height: Val::Px(SLOT),
            ..default()
        },
        GlobalZIndex(10),
        TestName::new("overlay"),
        ChildOf(root),
    ));
    h.step(2);
    observe_clicks(&mut h);

    h.click_at(rect.center());
    let clicks = &h.app.world().resource::<Clicks>().0;
    assert_eq!(
        clicks.first().map(String::as_str),
        Some("overlay"),
        "expected the overlay to win, got {clicks:?}"
    );
}

#[test]
fn button_activate_fires_from_a_synthetic_click() {
    let mut h = setup();
    #[derive(Resource, Default)]
    struct Activated(Vec<Entity>);
    h.app.init_resource::<Activated>();
    h.app.add_observer(
        |on: On<bevy::ui_widgets::Activate>, mut a: ResMut<Activated>| {
            a.0.push(on.entity);
        },
    );
    let slot = h.find_by_name("slot:1,1").unwrap();
    h.pointer_click(slot);
    assert_eq!(h.app.world().resource::<Activated>().0, vec![slot]);
}

/// Documents the one real limitation found: a `PointerId::Custom` pointer clicks correctly but
/// never updates `Hovered`, because `update_is_hovered` hard-codes `PointerId::Mouse`.
#[test]
fn custom_pointer_clicks_but_does_not_update_hovered() {
    use bevy::picking::pointer::{Location, PointerAction, PointerButton, PointerId, PointerInput};

    let mut h = setup();
    let custom = h.spawn_custom_pointer();
    h.step(1);
    let id = *h.app.world().get::<PointerId>(custom).unwrap();
    let slot = h.find_by_name("slot:6,2").unwrap();
    let pos = h.center_of(slot);
    let target = bevy::camera::NormalizedRenderTarget::Window(
        bevy::window::WindowRef::Primary.normalize(Some(h.window)).unwrap(),
    );
    let loc = Location { target, position: pos };

    for action in [
        PointerAction::Move { delta: pos },
        PointerAction::Press(PointerButton::Primary),
        PointerAction::Release(PointerButton::Primary),
    ] {
        h.app
            .world_mut()
            .write_message(PointerInput::new(id, loc.clone(), action));
        h.step(1);
    }

    assert_eq!(
        h.app.world().resource::<Clicks>().0.first().map(String::as_str),
        Some("slot:6,2"),
        "custom pointer still produces Pointer<Click>"
    );
    assert!(
        !h.app.world().get::<Hovered>(slot).unwrap().0,
        "if this ever passes, bevy generalised update_is_hovered and the note can be dropped"
    );
}
