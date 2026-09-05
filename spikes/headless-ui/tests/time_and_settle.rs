use std::time::Duration;

use bevy::prelude::*;
use headless_ui_spike::*;

#[test]
fn virtual_time_advances_by_a_fixed_delta_per_frame() {
    let mut h = Harness::new(Screen::default());
    let before = h.app.world().resource::<Time<Virtual>>().elapsed();
    h.step(10);
    let after = h.app.world().resource::<Time<Virtual>>().elapsed();
    assert_eq!(after - before, h.frame_delta() * 10);
}

#[test]
fn delta_is_identical_on_every_frame() {
    #[derive(Resource, Default)]
    struct Deltas(Vec<Duration>);

    let mut h = Harness::new(Screen::default());
    h.app.init_resource::<Deltas>();
    h.app
        .add_systems(Update, |t: Res<Time>, mut d: ResMut<Deltas>| {
            d.0.push(t.delta())
        });
    h.step(5);
    let deltas = &h.app.world().resource::<Deltas>().0;
    assert!(deltas.iter().all(|d| *d == h.frame_delta()), "{deltas:?}");
}

/// `settle()` runs frames until nothing reports itself busy. Models a motion system that
/// finishes after a known number of virtual milliseconds.
#[test]
fn settle_terminates_when_the_animation_finishes() {
    #[derive(Resource)]
    struct Anim(Duration);

    let mut h = Harness::new(Screen::default());
    h.app.insert_resource(Anim(Duration::from_millis(100)));
    h.app.add_systems(
        Update,
        |t: Res<Time>, mut anim: ResMut<Anim>, mut busy: ResMut<Busy>| {
            anim.0 = anim.0.saturating_sub(t.delta());
            busy.0 = u32::from(!anim.0.is_zero());
        },
    );

    let frames = h.settle(1000);
    // 100ms of animation at 16.667ms per frame: exactly 6 frames, deterministically.
    assert_eq!(frames, 6, "settled in {frames} frames");
    assert_eq!(h.app.world().resource::<Busy>().0, 0);
}

#[test]
fn settle_gives_up_at_max_frames_when_motion_never_ends() {
    let mut h = Harness::new(Screen::default());
    h.app
        .add_systems(Update, |mut busy: ResMut<Busy>| busy.0 = 1);
    assert_eq!(h.settle(20), 20);
}
