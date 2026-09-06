//! Live viewports: one camera and one rig per node, and none at all headless.
//!
//! The headless half is asserted in `machine_widgets.rs`; this file is the
//! other half, and it needs the `viewport` feature to exist at all.

#![cfg(feature = "viewport")]

use bevy::asset::Assets;
use bevy::image::Image;
use bevy::pbr::StandardMaterial;
use bevy::prelude::*;
use slotted_ui::plugin::SlottedUiConfig;
use slotted_ui::widgets::viewport::{
    ViewportCamera, ViewportSize, ViewportSubject, spawn_viewport_cameras,
};

fn world_with_assets(headless: bool) -> World {
    let mut world = World::new();
    world.insert_resource(Assets::<Image>::default());
    world.insert_resource(Assets::<Mesh>::default());
    world.insert_resource(Assets::<StandardMaterial>::default());
    world.insert_resource(SlottedUiConfig {
        headless,
        spawn_layers: false,
        ..SlottedUiConfig::default()
    });
    world
}

fn spawn_node(world: &mut World, id: &str) -> Entity {
    let name = slotted_model::Namespaced::parse(id).expect("valid id");
    world
        .spawn((ViewportSubject::Item(name), ViewportSize(96.0)))
        .id()
}

#[test]
fn each_live_viewport_gets_one_camera_and_one_three_point_rig() {
    let mut world = world_with_assets(false);
    let one = spawn_node(&mut world, "demo:stone");
    let two = spawn_node(&mut world, "demo:ingot");
    spawn_viewport_cameras(&mut world);

    let cameras = world
        .query_filtered::<Entity, With<Camera3d>>()
        .iter(&world)
        .count();
    assert_eq!(cameras, 2, "one camera a viewport, never one a slot");

    for node in [one, two] {
        let rig = world
            .get::<ViewportCamera>(node)
            .expect("the node owns its rig");
        assert_eq!(rig.lights.len(), 3, "key, fill and rim");
        assert_eq!(rig.lights[0], rig.light, "the key light is the first one");
        assert!(world.get::<Mesh3d>(rig.subject).is_some());
    }
    let layers: Vec<usize> = [one, two]
        .iter()
        .map(|e| world.get::<ViewportCamera>(*e).expect("a rig").layer)
        .collect();
    assert_ne!(layers[0], layers[1], "two viewports never share a layer");
}

#[test]
fn a_second_pass_does_not_spawn_a_second_camera() {
    let mut world = world_with_assets(false);
    spawn_node(&mut world, "demo:stone");
    spawn_viewport_cameras(&mut world);
    spawn_viewport_cameras(&mut world);
    assert_eq!(
        world
            .query_filtered::<Entity, With<Camera3d>>()
            .iter(&world)
            .count(),
        1
    );
}

#[test]
fn headless_spawns_no_camera_even_with_the_feature_on() {
    let mut world = world_with_assets(true);
    spawn_node(&mut world, "demo:stone");
    spawn_viewport_cameras(&mut world);
    assert_eq!(
        world
            .query_filtered::<Entity, With<Camera3d>>()
            .iter(&world)
            .count(),
        0,
        "the feature is compiled in, but a headless app still spawns nothing"
    );
}
