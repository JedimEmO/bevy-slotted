//! The GPU bake: every icon rendered once, into a grid that *is* the atlas.
//!
//! There is no readback. One orthographic camera looks down `-Z` at a grid of
//! meshes laid out exactly on the atlas cells, and its render target is the
//! image [`AtlasIcons`](crate::AtlasIcons) hands the renderer. That is what
//! makes the path work on WebGL2, where `Readback` and buffer copies from a
//! texture are not available: nothing ever leaves the GPU.
//!
//! The rig is the fixed three-point one section 2 of
//! `docs/research/research-modern-ui.md` asks for: a key from the upper left,
//! a dim warm fill from the lower right, and a cool rim from behind. Because
//! the camera is orthographic and the lights are directional, every cell is
//! lit identically no matter where in the grid it sits.

use bevy::camera::visibility::RenderLayers;
use bevy::camera::{Camera, ClearColorConfig, Projection, RenderTarget, ScalingMode};
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::image::Image;
use bevy::pbr::{MeshMaterial3d, StandardMaterial};
use bevy::prelude::*;
use slotted_registry::icon::{ShapeIcon, ShapeKind};

use crate::atlas::BakedAtlas;

/// The render layer the bake rig lives on. Far above the viewport widget's
/// `VIEWPORT_LAYER_BASE` so the two never share a light.
pub const ICON_BAKE_LAYER: usize = 60;

/// The entities one bake spawned, so the next bake can take them down.
#[derive(Resource, Debug, Default)]
pub struct IconBakeRig {
    /// Camera, lights and meshes.
    pub entities: Vec<Entity>,
    /// Frames left before the camera is switched off. The target keeps its
    /// contents, so the rig only has to draw once.
    pub frames: u32,
}

/// How many frames the rig renders before it goes quiet.
///
/// Not one or two: a mesh pipeline is specialised and compiled asynchronously,
/// so the first frames after the rig appears draw nothing at all. Switching
/// the camera off too early leaves a blank atlas that never fills in, which is
/// exactly what the first Phase 7 screenshots showed. Four seconds at 60 Hz is
/// a generous margin, and the cost until then is one 256 px pass over a couple
/// of dozen small meshes.
const WARMUP_FRAMES: u32 = 240;

/// Edge of one atlas cell in world units. The mesh is scaled to sit inside it
/// with a small margin, so the rim light has somewhere to fall.
const CELL_WORLD: f32 = 1.0;

/// Replaces `baked`'s CPU image with a render target the rig draws into, and
/// spawns the rig. Returns the target handle, or `None` when the atlas is
/// empty.
///
/// The target starts life holding the CPU bake, so a screen drawn before the
/// rig's pipelines finish compiling shows flat-shaded icons rather than
/// nothing at all.
pub fn spawn_bake_rig(world: &mut World, baked: &BakedAtlas, cell: u32) -> Option<Handle<Image>> {
    if baked.shapes.is_empty() {
        return None;
    }
    despawn_bake_rig(world);

    let size = baked.image.size();
    let cols = (size.x / cell).max(1);
    let rows = (size.y / cell).max(1);
    // The target starts as the CPU bake rather than as zeroes: the same
    // shapes, flat shaded. The rig overwrites it with the lit version as soon
    // as its pipelines are ready, and if the rig never draws at all the atlas
    // is still a picture rather than a hole.
    let mut image = baked.image.clone();
    image.texture_descriptor.usage |=
        bevy::render::render_resource::TextureUsages::RENDER_ATTACHMENT;
    let target = world.resource_mut::<Assets<Image>>().add(image);

    let layers = RenderLayers::layer(ICON_BAKE_LAYER);
    let mut entities = Vec::with_capacity(baked.shapes.len() + 4);

    #[allow(clippy::cast_precision_loss)]
    let (grid_w, grid_h) = (cols as f32 * CELL_WORLD, rows as f32 * CELL_WORLD);
    entities.push(
        world
            .spawn((
                Camera3d::default(),
                Camera {
                    // Behind every game camera, and it must not clear theirs.
                    order: -100,
                    clear_color: ClearColorConfig::Custom(Color::NONE),
                    ..default()
                },
                // The icons are authored as sRGB colours; tone mapping would
                // pull them away from the palette the theme was chosen with.
                Tonemapping::None,
                Projection::Orthographic(OrthographicProjection {
                    scaling_mode: ScalingMode::Fixed {
                        width: grid_w,
                        height: grid_h,
                    },
                    ..OrthographicProjection::default_3d()
                }),
                RenderTarget::Image(target.clone().into()),
                Transform::from_xyz(0.0, 0.0, 8.0).looking_at(Vec3::ZERO, Vec3::Y),
                layers.clone(),
            ))
            .id(),
    );
    for light in rig_lights() {
        entities.push(world.spawn((light.0, light.1, layers.clone())).id());
    }

    for (cell_index, shape) in baked.shapes.iter().enumerate() {
        let cell_index = u32::try_from(cell_index).unwrap_or(u32::MAX);
        #[allow(clippy::cast_precision_loss)]
        let (col, row) = ((cell_index % cols) as f32, (cell_index / cols) as f32);
        // Cell centres, in a grid whose origin is the middle of the atlas and
        // whose rows run downwards, exactly as the layout rects do.
        let x = (col + 0.5).mul_add(CELL_WORLD, -grid_w / 2.0);
        let y = grid_h / 2.0 - (row + 0.5) * CELL_WORLD;
        let mesh = world
            .resource_mut::<Assets<Mesh>>()
            .add(mesh_of(shape.shape));
        let material = world
            .resource_mut::<Assets<StandardMaterial>>()
            .add(material_of(shape, false));
        entities.push(
            world
                .spawn((
                    Mesh3d(mesh),
                    MeshMaterial3d(material),
                    Transform::from_xyz(x, y, 0.0)
                        .with_rotation(item_rotation(shape.shape))
                        .with_scale(Vec3::splat(scale_of(shape.shape))),
                    layers.clone(),
                ))
                .id(),
        );
        if let Some(accent) = accent_mesh(shape) {
            let mesh = world.resource_mut::<Assets<Mesh>>().add(accent.0);
            let material = world
                .resource_mut::<Assets<StandardMaterial>>()
                .add(material_of(shape, true));
            entities.push(
                world
                    .spawn((
                        Mesh3d(mesh),
                        MeshMaterial3d(material),
                        Transform::from_xyz(x, y, 0.0)
                            .with_rotation(item_rotation(shape.shape))
                            .with_scale(Vec3::splat(scale_of(shape.shape)))
                            * accent.1,
                        layers.clone(),
                    ))
                    .id(),
            );
        }
    }

    world.insert_resource(IconBakeRig {
        entities,
        frames: WARMUP_FRAMES,
    });
    Some(target)
}

/// Despawns whatever the last bake left in the world.
pub fn despawn_bake_rig(world: &mut World) {
    let Some(rig) = world.remove_resource::<IconBakeRig>() else {
        return;
    };
    for entity in rig.entities {
        if let Ok(entity) = world.get_entity_mut(entity) {
            entity.despawn();
        }
    }
}

/// `Update`: switches the rig's camera off once it has drawn. The target
/// image keeps its contents, so the icons stay on screen.
pub fn quiet_bake_rig(mut rig: Option<ResMut<IconBakeRig>>, mut cameras: Query<&mut Camera>) {
    let Some(rig) = rig.as_mut() else { return };
    if rig.frames == 0 {
        return;
    }
    rig.frames -= 1;
    if rig.frames > 0 {
        return;
    }
    for entity in &rig.entities {
        if let Ok(mut camera) = cameras.get_mut(*entity) {
            camera.is_active = false;
        }
    }
}

/// The fixed three-point rig: key, fill, and a cool rim from behind.
fn rig_lights() -> [(DirectionalLight, Transform); 3] {
    [
        (
            DirectionalLight {
                illuminance: 9_000.0,
                shadow_maps_enabled: false,
                ..default()
            },
            Transform::from_xyz(-4.0, 5.5, 6.0).looking_at(Vec3::ZERO, Vec3::Y),
        ),
        (
            DirectionalLight {
                color: Color::srgb(1.0, 0.94, 0.86),
                illuminance: 2_600.0,
                shadow_maps_enabled: false,
                ..default()
            },
            Transform::from_xyz(5.0, -3.0, 4.0).looking_at(Vec3::ZERO, Vec3::Y),
        ),
        (
            DirectionalLight {
                color: Color::srgb(0.55, 0.74, 1.0),
                illuminance: 11_000.0,
                shadow_maps_enabled: false,
                ..default()
            },
            Transform::from_xyz(3.0, 2.0, -6.0).looking_at(Vec3::ZERO, Vec3::Y),
        ),
    ]
}

/// The three-quarter view every icon is drawn from: yawed 45 degrees, then
/// tipped forward so the top face reads. The same view the CPU polygons are
/// authored in.
fn item_rotation(kind: ShapeKind) -> Quat {
    match kind {
        // A rod is read along its length, not from a corner.
        ShapeKind::Rod => Quat::from_rotation_z(std::f32::consts::FRAC_PI_4),
        _ => {
            Quat::from_rotation_x(-std::f32::consts::FRAC_PI_6)
                * Quat::from_rotation_y(std::f32::consts::FRAC_PI_4)
        }
    }
}

/// How much of the cell the mesh fills.
fn scale_of(kind: ShapeKind) -> f32 {
    match kind {
        ShapeKind::Cube | ShapeKind::Slab => 0.46,
        ShapeKind::Ingot => 0.66,
        ShapeKind::Gem | ShapeKind::Sphere => 0.62,
        ShapeKind::Rod => 0.72,
    }
}

/// The primitive a shape renders as.
pub fn mesh_of(kind: ShapeKind) -> Mesh {
    match kind {
        ShapeKind::Cube => Cuboid::from_length(1.0).into(),
        ShapeKind::Slab => Cuboid::new(1.0, 0.5, 1.0).into(),
        ShapeKind::Ingot => Cuboid::new(1.0, 0.3, 0.55).into(),
        // Two cones base to base is an octahedron with four facets a side,
        // which is close enough to a cut gem at 64 px.
        ShapeKind::Gem => Cone {
            radius: 0.5,
            height: 0.7,
        }
        .into(),
        ShapeKind::Rod => Cylinder {
            radius: 0.06,
            half_height: 0.5,
        }
        .into(),
        ShapeKind::Sphere => Sphere::new(0.5).into(),
    }
}

/// The second mesh an `accent` adds, with the transform it takes relative to
/// the body. `None` when the shape has no accent geometry.
fn accent_mesh(shape: &ShapeIcon) -> Option<(Mesh, Transform)> {
    shape.accent?;
    match shape.shape {
        // The head of a tool, sat on the end of the shaft.
        ShapeKind::Rod => Some((
            Cuboid::new(0.42, 0.16, 0.22).into(),
            Transform::from_xyz(0.0, 0.44, 0.0),
        )),
        // An inlay band around the waist of everything else.
        ShapeKind::Cube | ShapeKind::Slab | ShapeKind::Ingot => Some((
            Cuboid::new(1.04, 0.12, 1.04).into(),
            Transform::from_xyz(0.0, -0.16, 0.0),
        )),
        // A girdle: the thin bright band around the waist of a cut stone.
        ShapeKind::Gem | ShapeKind::Sphere => Some((
            Cylinder {
                radius: 0.53,
                half_height: 0.02,
            }
            .into(),
            Transform::from_xyz(0.0, 0.0, 0.0),
        )),
    }
}

fn material_of(shape: &ShapeIcon, accent: bool) -> StandardMaterial {
    let color = if accent {
        shape.accent.unwrap_or(shape.color)
    } else {
        shape.color
    };
    let [r, g, b, a] = color.0;
    StandardMaterial {
        base_color: Color::srgba(r, g, b, a),
        metallic: shape.metallic.clamp(0.0, 1.0),
        perceptual_roughness: shape.roughness.clamp(0.05, 1.0),
        ..default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use slotted_registry::icon::IconColor;

    #[test]
    fn every_shape_has_a_mesh_and_fits_its_cell() {
        for kind in ShapeKind::ALL {
            let mesh = mesh_of(kind);
            assert!(mesh.count_vertices() > 0, "{kind} has no geometry");
            assert!(scale_of(kind) > 0.0 && scale_of(kind) < 1.0);
        }
    }

    #[test]
    fn an_accent_only_adds_geometry_when_declared() {
        let plain = ShapeIcon::new(ShapeKind::Cube, IconColor::rgb(1, 2, 3));
        assert!(accent_mesh(&plain).is_none());
        let mut fancy = plain.clone();
        fancy.accent = Some(IconColor::rgb(4, 5, 6));
        assert!(accent_mesh(&fancy).is_some());
    }
}
