use bevy::input::keyboard::Key;
use bevy::prelude::*;
use bevy::ui::auto_directional_navigation::AutoDirectionalNavigator;
use headless_ui_spike::*;

fn name_of(h: &Harness, e: Option<Entity>) -> Option<String> {
    e.and_then(|e| h.app.world().get::<TestName>(e).map(|n| n.0.clone()))
}

#[test]
fn tab_moves_focus_through_the_grid() {
    let mut h = Harness::new(Screen::default());
    spawn_slot_grid(h.app.world_mut());
    h.step(2);

    let first = h.find_by_name("slot:0,0").unwrap();
    h.set_focus(Some(first));
    h.step(1);

    h.key_press(KeyCode::Tab, Key::Tab);
    assert_eq!(name_of(&h, h.focus()).as_deref(), Some("slot:1,0"));

    h.key_press(KeyCode::Tab, Key::Tab);
    assert_eq!(name_of(&h, h.focus()).as_deref(), Some("slot:2,0"));
}

#[test]
fn shift_tab_moves_focus_backwards() {
    let mut h = Harness::new(Screen::default());
    spawn_slot_grid(h.app.world_mut());
    h.step(2);
    h.set_focus(h.find_by_name_ref("slot:4,0"));
    h.step(1);

    h.hold_key(KeyCode::ShiftLeft);
    h.key_press(KeyCode::Tab, Key::Tab);
    assert_eq!(name_of(&h, h.focus()).as_deref(), Some("slot:3,0"));
    h.release_key(KeyCode::ShiftLeft);
}

/// Arrow keys are not wired up by bevy; `AutoDirectionalNavigator` is a `SystemParam` a game
/// drives itself. This is the system `slotted-ui` will own, exercised through the harness.
#[test]
fn arrow_keys_navigate_the_grid_geometrically() {
    let mut h = Harness::new(Screen::default());
    spawn_slot_grid(h.app.world_mut());
    h.app.add_systems(Update, arrow_navigation);
    h.step(2);

    h.set_focus(h.find_by_name_ref("slot:4,1"));
    h.step(1);

    h.key_press(KeyCode::ArrowRight, Key::ArrowRight);
    assert_eq!(name_of(&h, h.focus()).as_deref(), Some("slot:5,1"));

    h.key_press(KeyCode::ArrowDown, Key::ArrowDown);
    assert_eq!(name_of(&h, h.focus()).as_deref(), Some("slot:5,2"));

    h.key_press(KeyCode::ArrowLeft, Key::ArrowLeft);
    assert_eq!(name_of(&h, h.focus()).as_deref(), Some("slot:4,2"));

    h.key_press(KeyCode::ArrowUp, Key::ArrowUp);
    assert_eq!(name_of(&h, h.focus()).as_deref(), Some("slot:4,1"));
}

fn arrow_navigation(keys: Res<ButtonInput<KeyCode>>, mut nav: AutoDirectionalNavigator) {
    use bevy::math::CompassOctant;
    let dir = if keys.just_pressed(KeyCode::ArrowRight) {
        CompassOctant::East
    } else if keys.just_pressed(KeyCode::ArrowLeft) {
        CompassOctant::West
    } else if keys.just_pressed(KeyCode::ArrowUp) {
        CompassOctant::North
    } else if keys.just_pressed(KeyCode::ArrowDown) {
        CompassOctant::South
    } else {
        return;
    };
    let _ = nav.navigate(dir);
}

/// A plain digit key reaches an ordinary system through `ButtonInput<KeyCode>`, exactly as it
/// would with winit feeding the messages.
#[test]
fn digit_key_reaches_a_system() {
    #[derive(Resource, Default)]
    struct Digits(Vec<u8>);

    let mut h = Harness::new(Screen::default());
    h.app.init_resource::<Digits>();
    h.app
        .add_systems(Update, |keys: Res<ButtonInput<KeyCode>>, mut d: ResMut<Digits>| {
            for (code, n) in [(KeyCode::Digit1, 1u8), (KeyCode::Digit2, 2)] {
                if keys.just_pressed(code) {
                    d.0.push(n);
                }
            }
        });
    spawn_slot_grid(h.app.world_mut());
    h.step(2);

    h.key_press(KeyCode::Digit2, Key::Character("2".into()));
    h.key_press(KeyCode::Digit1, Key::Character("1".into()));
    assert_eq!(h.app.world().resource::<Digits>().0, vec![2, 1]);
}

/// Enter on a focused `Button` activates it without any pointer involvement.
#[test]
fn enter_activates_the_focused_button() {
    #[derive(Resource, Default)]
    struct Activated(Vec<Entity>);

    let mut h = Harness::new(Screen::default());
    spawn_slot_grid(h.app.world_mut());
    h.app.init_resource::<Activated>();
    h.app.add_observer(
        |on: On<bevy::ui_widgets::Activate>, mut a: ResMut<Activated>| a.0.push(on.entity),
    );
    h.step(2);

    let slot = h.find_by_name("slot:7,0").unwrap();
    h.set_focus(Some(slot));
    h.step(1);
    h.key_press(KeyCode::Enter, Key::Enter);
    assert_eq!(h.app.world().resource::<Activated>().0, vec![slot]);
}
