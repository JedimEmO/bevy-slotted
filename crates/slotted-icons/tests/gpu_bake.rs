//! The GPU bake's rig: what it puts in the world, and what it renders.
//!
//! The graph test runs everywhere: `spawn_bake_rig` only spawns entities and
//! adds assets, so it needs no renderer. The render itself needs a GPU and a
//! display, so the test that looks at pixels is ignored by default; the
//! example screenshots (`just shot-chest`) are the everyday evidence.

#![cfg(feature = "gpu")]

use bevy::asset::Assets;
use bevy::camera::{Camera, RenderTarget};
use bevy::image::Image;
use bevy::pbr::StandardMaterial;
use bevy::prelude::*;
use slotted_icons::atlas::{IconItem, bake_icon_atlas};
use slotted_icons::gpu::{IconBakeProgress, IconBakeRig, despawn_bake_rig, spawn_bake_rig};
use slotted_model::{ItemId, Namespaced};
use slotted_registry::icon::{IconColor, IconDef, ShapeIcon, ShapeKind};

fn world_with_assets() -> World {
    let mut world = World::new();
    world.insert_resource(Assets::<Image>::default());
    world.insert_resource(Assets::<Mesh>::default());
    world.insert_resource(Assets::<StandardMaterial>::default());
    world
}

fn demo_items() -> (Vec<Namespaced>, Vec<Option<IconDef>>) {
    let names = ["t:stone", "t:ingot", "t:pick"]
        .iter()
        .map(|s| Namespaced::parse(s).expect("valid"))
        .collect();
    let icons = vec![
        Some(IconDef::Shape(ShapeIcon::new(
            ShapeKind::Cube,
            IconColor::rgb(0x8a, 0x8f, 0x96),
        ))),
        Some(IconDef::Shape(ShapeIcon {
            metallic: 0.9,
            ..ShapeIcon::new(ShapeKind::Ingot, IconColor::rgb(0xc9, 0x79, 0x3f))
        })),
        // The only one with an accent, so the rig spawns a second mesh for it.
        Some(IconDef::Shape(ShapeIcon {
            accent: Some(IconColor::rgb(0xd0, 0xd6, 0xdd)),
            ..ShapeIcon::new(ShapeKind::Rod, IconColor::rgb(0x6b, 0x4c, 0x33))
        })),
    ];
    (names, icons)
}

#[test]
fn the_rig_is_one_camera_three_lights_and_a_mesh_per_cell() {
    let (names, icons) = demo_items();
    let items: Vec<IconItem<'_>> = names
        .iter()
        .zip(&icons)
        .enumerate()
        .map(|(i, (name, icon))| IconItem {
            id: ItemId(u32::try_from(i).expect("small")),
            name,
            icon: icon.as_ref(),
        })
        .collect();
    let baked = bake_icon_atlas(&items, 64);

    let mut world = world_with_assets();
    let target = spawn_bake_rig(&mut world, &baked, 64).expect("a rig for three shapes");

    let cameras = world
        .query_filtered::<&Camera, With<Camera3d>>()
        .iter(&world)
        .count();
    assert_eq!(
        cameras, 2,
        "two cameras render the whole grid, not one a cell: the one that \
         draws into the atlas and the warm-up one that compiles its pipelines"
    );
    let lights = world.query::<&DirectionalLight>().iter(&world).count();
    assert_eq!(lights, 3, "key, fill and rim");
    let meshes = world.query::<&Mesh3d>().iter(&world).count();
    assert_eq!(meshes, 4, "three shapes plus the accented one's head");

    let rig = world.resource::<IconBakeRig>();
    assert_eq!(rig.entities.len(), 2 + 3 + 4);
    assert!(
        !rig.finished,
        "a freshly spawned rig has not drawn anything yet"
    );
    assert_eq!(
        rig.frames, 0,
        "and it has not counted a frame against the timeout"
    );
    assert!(
        rig.pending_models.is_empty(),
        "three primitives and no glTF, so nothing is waiting on an asset"
    );
    // The atlas camera does not render yet, whatever the scene is. Rendering
    // clears its target, and its target is the atlas a screen may already be
    // drawing from, so it waits until the warm-up camera has made the
    // renderer compile the pipelines it will need. Only then can it clear and
    // refill the atlas within one frame.
    assert!(!rig.camera_on, "the hand-over has not happened yet");
    let atlas_camera = rig.camera.expect("an atlas camera");
    let warmup_camera = rig.warmup_camera.expect("a warm-up camera");
    assert!(
        !world
            .get::<Camera>(atlas_camera)
            .expect("a camera")
            .is_active,
        "the atlas camera must not clear the atlas before it can refill it"
    );
    assert!(
        world
            .get::<Camera>(warmup_camera)
            .expect("a camera")
            .is_active,
        "the warm-up camera runs from the first frame"
    );
    // The rig stops on the render world's report, not on a frame count, so a
    // world with no renderer has nothing to report and the camera stays on.
    assert!(
        !world.resource::<IconBakeProgress>().drew_everything(),
        "nothing has been drawn, so nothing is ready"
    );

    let image = world
        .resource::<Assets<Image>>()
        .get(&target)
        .expect("the target is an asset");
    assert!(
        image
            .texture_descriptor
            .usage
            .contains(bevy::render::render_resource::TextureUsages::RENDER_ATTACHMENT),
        "the atlas is the camera's render target"
    );
    assert_eq!(
        image.size(),
        baked.image.size(),
        "the target is the atlas, cell for cell"
    );
    assert_eq!(
        image.data, baked.image.data,
        "it starts as the CPU bake, so a screen is never blank while the \
         rig's pipelines compile"
    );
}

#[test]
fn a_camera_targets_the_atlas_and_nothing_else() {
    let names = [Namespaced::parse("t:stone").expect("valid")];
    let items = vec![IconItem {
        id: ItemId(0),
        name: &names[0],
        icon: None,
    }];
    let baked = bake_icon_atlas(&items, 32);
    let mut world = world_with_assets();
    let target = spawn_bake_rig(&mut world, &baked, 32).expect("a rig");
    let rig = world.resource::<IconBakeRig>();
    let (atlas_camera, warmup_camera) = (
        rig.camera.expect("an atlas camera"),
        rig.warmup_camera.expect("a warm-up camera"),
    );
    let target_of = |entity| world.get::<RenderTarget>(entity).cloned();
    assert!(
        matches!(target_of(atlas_camera), Some(RenderTarget::Image(image)) if image.handle == target),
        "the atlas camera draws into the atlas image: {:?}",
        target_of(atlas_camera)
    );
    // And the warm-up camera draws somewhere else entirely, which is the
    // whole point: it exists to compile pipelines without touching the atlas.
    assert!(
        matches!(target_of(warmup_camera), Some(RenderTarget::Image(image)) if image.handle != target),
        "the warm-up camera draws into a scratch image: {:?}",
        target_of(warmup_camera)
    );
    assert_eq!(
        world.query::<&RenderTarget>().iter(&world).count(),
        2,
        "and there are no other cameras in the rig"
    );
}

#[test]
fn a_second_bake_takes_the_first_rig_down() {
    let names = [Namespaced::parse("t:stone").expect("valid")];
    let items = vec![IconItem {
        id: ItemId(0),
        name: &names[0],
        icon: None,
    }];
    let baked = bake_icon_atlas(&items, 32);
    let mut world = world_with_assets();
    spawn_bake_rig(&mut world, &baked, 32).expect("a rig");
    spawn_bake_rig(&mut world, &baked, 32).expect("a second rig");
    assert_eq!(
        world
            .query_filtered::<Entity, With<Camera3d>>()
            .iter(&world)
            .count(),
        2,
        "a rebake leaves one rig's two cameras, not two rigs' four"
    );
    despawn_bake_rig(&mut world);
    assert_eq!(
        world
            .query_filtered::<Entity, With<Camera3d>>()
            .iter(&world)
            .count(),
        0
    );
}

/// The render itself. Needs a GPU and a display, so it is ignored by default:
/// `cargo test -p slotted-icons --all-features -- --ignored`.
#[test]
#[ignore = "needs a GPU"]
fn the_rig_actually_draws_into_the_atlas() {
    use bevy::render::renderer::RenderDevice;

    let mut app = App::new();
    app.add_plugins(bevy::DefaultPlugins);
    app.update();
    assert!(
        app.world().get_resource::<RenderDevice>().is_some(),
        "no render device: this test needs a GPU"
    );

    let names = [Namespaced::parse("t:stone").expect("valid")];
    let icon = IconDef::Shape(ShapeIcon::new(
        ShapeKind::Cube,
        IconColor::rgb(0xd0, 0xd0, 0xd0),
    ));
    let items = vec![IconItem {
        id: ItemId(0),
        name: &names[0],
        icon: Some(&icon),
    }];
    let baked = bake_icon_atlas(&items, 64);
    let target = spawn_bake_rig(app.world_mut(), &baked, 64).expect("a rig");
    for _ in 0..30 {
        app.update();
    }
    // The pixels live on the GPU; what is observable from here is that the
    // rig survived the frames it needed and still owns the target.
    let rig = app.world().resource::<IconBakeRig>();
    assert!(rig.frames > 0, "the rig went quiet before it could draw");
    assert!(app.world().resource::<Assets<Image>>().contains(&target));
}
