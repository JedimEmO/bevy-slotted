//! The headless stack is a load-bearing list, not a convenience.
//!
//! ADR 0002 settled which Bevy plugins `bevy_ui` needs with no renderer and no
//! window, and which four asset types have to be registered by hand in their
//! place. `slotted-test` builds every harness on this group, so a plugin
//! dropped from the list here surfaces as a confusing panic three crates away.
//! These tests keep the failure local.
#![allow(clippy::unwrap_used)]

use bevy::asset::Assets;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use slotted::prelude::*;
use slotted::{HeadlessBevyPlugins, HeadlessRenderAssets};

/// Workaround 2: `bevy_render` normally registers these four, and three
/// systems fail parameter validation on frame one without them.
#[test]
fn the_render_asset_types_are_registered() {
    let mut app = App::new();
    app.add_plugins((
        bevy::app::TaskPoolPlugin::default(),
        bevy::asset::AssetPlugin::default(),
        HeadlessRenderAssets,
    ));
    let world = app.world();
    assert!(world.get_resource::<Assets<bevy::image::Image>>().is_some());
    assert!(
        world
            .get_resource::<Assets<bevy::image::TextureAtlasLayout>>()
            .is_some()
    );
    assert!(world.get_resource::<Assets<bevy::mesh::Mesh>>().is_some());
    assert!(
        world
            .get_resource::<Assets<bevy::mesh::skinning::SkinnedMeshInverseBindposes>>()
            .is_some()
    );
}

/// The group spawns a primary window at the requested resolution and survives
/// a frame with no renderer attached.
#[test]
fn the_bevy_group_runs_a_frame_with_a_window_and_no_renderer() {
    let mut app = App::new();
    app.add_plugins(HeadlessBevyPlugins {
        width: 800.0,
        height: 600.0,
        scale_factor: 2.0,
    });
    app.update();

    let mut q = app
        .world_mut()
        .query_filtered::<&Window, With<PrimaryWindow>>();
    let window = q.single(app.world()).expect("one primary window");
    assert_eq!(window.resolution.physical_width(), 1600);
    assert_eq!(window.resolution.physical_height(), 1200);
    assert!((window.resolution.scale_factor() - 2.0).abs() < f32::EPSILON);

    // No RenderPlugin, so nothing should have registered a render app.
    assert!(app.world().get_resource::<Assets<Image>>().is_some());
}

/// The full stack: Bevy's plugins plus every slotted plugin, one frame, no
/// panic. This is what `UiHarness::builder().plugins(..)` receives.
#[test]
fn the_headless_stack_builds_and_runs() {
    let mut app = App::new();
    app.add_plugins(SlottedPlugins::headless());
    app.update();

    // The ecs plugin defaults the Authority port at startup rather than
    // forcing every game to insert one.
    assert!(
        app.world()
            .get_resource::<slotted_ecs::Authority>()
            .is_some(),
        "SlottedEcsPlugin defaults Authority to LocalAuthority"
    );
    // The ui plugin owns the screen registry the harness's open_screen reads.
    assert!(app.world().get_resource::<slotted_ui::Screens>().is_some());
}

/// `SlottedPlugins` on its own assumes Bevy's plugins are already there, so a
/// windowed game adds `DefaultPlugins` and then this. Only the flag differs.
#[test]
fn the_group_carries_the_headless_flag() {
    assert!(!SlottedPlugins::default().headless);
    assert!(SlottedPlugins { headless: true }.headless);
}
