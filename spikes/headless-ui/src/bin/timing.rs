//! Wall-clock cost of a frame for the 27-slot screen, and of a full synthetic click.

use std::time::Instant;

use headless_ui_spike::*;

fn main() {
    let build_start = Instant::now();
    let mut h = Harness::new(Screen::default());
    spawn_slot_grid(h.app.world_mut());
    h.step(3);
    let build = build_start.elapsed();

    // Warm up: taffy caches layout, so the first frames are the expensive ones.
    for _ in 0..50 {
        h.app.update();
    }

    let n = 2000;
    let t = Instant::now();
    for _ in 0..n {
        h.app.update();
    }
    let idle = t.elapsed() / n;

    // Dirty layout every frame so taffy actually recomputes.
    let grid = h.find_by_name("grid").unwrap();
    let t = Instant::now();
    for i in 0..n {
        let dx = f32::from(i % 2 == 0);
        h.app
            .world_mut()
            .get_mut::<bevy::prelude::Node>(grid)
            .unwrap()
            .left = bevy::prelude::Val::Px(GRID_ORIGIN.x + dx);
        h.app.update();
    }
    let relayout = t.elapsed() / n;

    // A full pointer click is 3 frames plus message writes.
    let slot = h.find_by_name("slot:4,1").unwrap();
    let clicks = 500;
    let t = Instant::now();
    for _ in 0..clicks {
        h.pointer_click(slot);
    }
    let click = t.elapsed() / clicks;

    println!("harness build + first 3 frames : {build:?}");
    println!("app.update(), layout clean     : {idle:?}");
    println!("app.update(), layout dirty     : {relayout:?}");
    println!("pointer_click() (3 frames)     : {click:?}");
}
