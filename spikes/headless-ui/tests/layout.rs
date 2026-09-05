use bevy::prelude::*;
use headless_ui_spike::*;

fn approx(a: Vec2, b: Vec2) -> bool {
    (a - b).abs().max_element() < 0.01
}

#[test]
fn grid_lays_out_at_expected_rects() {
    let mut h = Harness::new(Screen::default());
    spawn_slot_grid(h.app.world_mut());
    h.step(2);

    for (col, row) in [(0, 0), (8, 0), (0, 2), (4, 1), (8, 2)] {
        let e = h
            .find_by_name(&format!("slot:{col},{row}"))
            .expect("slot exists");
        let got = h.rect_of(e);
        let want = expected_slot_rect(col, row);
        assert!(
            approx(got.min, want.min) && approx(got.max, want.max),
            "slot ({col},{row}): got {got:?} want {want:?}"
        );
    }
}

#[test]
fn layout_respects_scale_factor() {
    let mut h = Harness::new(Screen {
        width: 1280.0,
        height: 720.0,
        scale_factor: 2.0,
    });
    spawn_slot_grid(h.app.world_mut());
    h.step(2);

    let e = h.find_by_name("slot:0,0").unwrap();
    let node = h.app.world().get::<ComputedNode>(e).unwrap();
    // Physical size doubles, logical rect does not move.
    assert_eq!(node.size(), Vec2::splat(SLOT * 2.0));
    let got = h.rect_of(e);
    let want = expected_slot_rect(0, 0);
    assert!(approx(got.min, want.min), "got {got:?} want {want:?}");
}

#[test]
fn root_fills_the_virtual_window() {
    let mut h = Harness::new(Screen::default());
    spawn_slot_grid(h.app.world_mut());
    h.step(2);
    let root = h.find_by_name("root").unwrap();
    assert_eq!(h.rect_of(root).size(), Vec2::new(1280.0, 720.0));
}
