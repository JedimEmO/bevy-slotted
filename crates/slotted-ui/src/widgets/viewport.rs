//! `viewport`: a 3D view in a node. Phase 6 contract section 1.6. Headless
//! and without the `viewport` feature the node exists with no camera; the
//! feature adds per-viewport cameras on their own `RenderLayers`.

use bevy::prelude::*;
use bevy::ui::widget::ViewportNode;
use serde::{Deserialize, Serialize};
use slotted_model::Namespaced;

use crate::def::{Tags, ViewSubject};
use crate::screen::SpawnCtx;
use crate::semantic::{SemanticLabel, SemanticRole, WidgetNode};
use slotted_theme::{Themed, roles};

/// What a viewport shows, at runtime. The def's [`ViewSubject`] maps onto the
/// first three; `Entity` is Rust-only (entities are not data).
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub enum ViewportSubject {
    /// The player model (a capsule until models exist).
    Player,
    /// An item, as placeholder geometry tinted by its icon hue.
    Item(Namespaced),
    /// A block, likewise.
    Block(Namespaced),
    /// A live world entity; the viewport layer is added to its `RenderLayers`.
    Entity(Entity),
}

impl From<&ViewSubject> for ViewportSubject {
    fn from(subject: &ViewSubject) -> Self {
        match subject {
            ViewSubject::Player => Self::Player,
            ViewSubject::Item(id) => Self::Item(id.clone()),
            ViewSubject::Block(id) => Self::Block(id.clone()),
        }
    }
}

impl ViewportSubject {
    /// The semantic label.
    pub fn label(&self) -> String {
        match self {
            Self::Player => "player".to_owned(),
            Self::Item(id) | Self::Block(id) => id.to_string(),
            Self::Entity(e) => format!("entity {e}"),
        }
    }
}

/// First `RenderLayers` layer viewport cameras use; each live viewport takes
/// `BASE + n`.
pub const VIEWPORT_LAYER_BASE: usize = 16;

/// Which viewport layers are in use. Only with the `viewport` feature.
#[derive(Resource, Default, Debug, Clone)]
pub struct ViewportLayers(pub Vec<Option<Entity>>);

/// Parameters of `slotted:viewport`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ViewportParams {
    /// What to show.
    pub subject: ViewSubject,
    /// Edge length in px.
    #[serde(default = "size")]
    pub size: f32,
}

fn size() -> f32 {
    96.0
}

impl Default for ViewportParams {
    fn default() -> Self {
        Self {
            subject: ViewSubject::Player,
            size: 96.0,
        }
    }
}

/// The size a viewport node was spawned at, so the `viewport` feature can
/// size its render target to match.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct ViewportSize(pub f32);

/// Spawns a viewport node with no camera. Contract 1.6.
///
/// Headless, that is the whole widget: the node lays out, picks and reports
/// its subject, and `ViewportNode.camera` stays `None`.
pub fn spawn_viewport(ctx: &mut SpawnCtx<'_>, params: &ViewportParams, _tags: &Tags) -> Entity {
    let subject = ViewportSubject::from(&params.subject);
    ctx.spawn_node((
        Node {
            width: px(params.size),
            height: px(params.size),
            border: UiRect::all(px(crate::widgets::BORDER_WIDTH)),
            ..default()
        },
        Themed(roles::VIEWPORT),
        ViewportNode { camera: None },
        SemanticRole::Viewport,
        SemanticLabel(subject.label()),
        WidgetNode(crate::widgets::kinds::viewport()),
        ViewportSize(params.size),
        subject,
    ))
}

/// One live viewport: the entities the `viewport` feature spawned behind a
/// node, and the render layer they occupy.
#[cfg(feature = "viewport")]
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub struct ViewportCamera {
    /// The camera rendering into the node's target image.
    pub camera: Entity,
    /// Its key light. Kept for source compatibility; [`Self::lights`] is the
    /// whole rig.
    pub light: Entity,
    /// The placeholder geometry, or the live entity for
    /// [`ViewportSubject::Entity`].
    pub subject: Entity,
    /// Every light of the three-point rig, key first.
    pub lights: Vec<Entity>,
    /// Index into [`ViewportLayers`]; the render layer is
    /// `VIEWPORT_LAYER_BASE + layer`.
    pub layer: usize,
}

/// The fixed three-point rig a viewport lights its subject with.
#[cfg(feature = "viewport")]
fn rig_lights() -> [(DirectionalLight, Transform); 3] {
    [
        (
            DirectionalLight {
                illuminance: 9_000.0,
                shadow_maps_enabled: false,
                ..default()
            },
            Transform::from_xyz(-3.0, 4.0, 4.0).looking_at(Vec3::ZERO, Vec3::Y),
        ),
        (
            DirectionalLight {
                color: Color::srgb(1.0, 0.94, 0.86),
                illuminance: 2_600.0,
                shadow_maps_enabled: false,
                ..default()
            },
            Transform::from_xyz(4.0, -2.0, 3.0).looking_at(Vec3::ZERO, Vec3::Y),
        ),
        (
            DirectionalLight {
                color: Color::srgb(0.55, 0.74, 1.0),
                illuminance: 11_000.0,
                shadow_maps_enabled: false,
                ..default()
            },
            Transform::from_xyz(2.0, 1.5, -5.0).looking_at(Vec3::ZERO, Vec3::Y),
        ),
    ]
}

/// Marks the placeholder geometry this crate spawned for a viewport, so
/// `despawn_viewport_cameras` never despawns a game entity a
/// [`ViewportSubject::Entity`] pointed at.
#[cfg(feature = "viewport")]
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct ViewportPlaceholder;

/// How fast a viewport's subject turns, in radians per second of virtual
/// time. The camera and the three-point rig stay put, so the rim light keeps
/// falling on the same edge of the frame while the item rotates under it.
#[cfg(feature = "viewport")]
pub const ORBIT_SPEED: f32 = 0.6;

/// Distance from the subject, in world units.
#[cfg(feature = "viewport")]
const ORBIT_RADIUS: f32 = 2.4;

/// With the `viewport` feature and not headless: creates the render target,
/// camera, light and placeholder subject for every `Added<ViewportSubject>`.
///
/// Skipped entirely when `SlottedUiConfig.headless` is set, which is what
/// keeps a headless test from spawning a camera even with the feature on.
#[cfg(feature = "viewport")]
pub fn spawn_viewport_cameras(world: &mut World) {
    use bevy::camera::RenderTarget;
    use bevy::camera::visibility::RenderLayers;

    if world
        .get_resource::<crate::plugin::SlottedUiConfig>()
        .is_some_and(|c| c.headless)
    {
        return;
    }
    let pending: Vec<(Entity, ViewportSubject, f32)> = world
        .query_filtered::<(Entity, &ViewportSubject, Option<&ViewportSize>), Without<ViewportCamera>>()
        .iter(world)
        .map(|(e, s, size)| (e, s.clone(), size.map_or(96.0, |s| s.0)))
        .collect();

    for (node, subject, size) in pending {
        let layer = claim_layer(world, node);
        let layers = RenderLayers::layer(VIEWPORT_LAYER_BASE + layer);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let edge = size.max(1.0) as u32;
        let image = bevy::image::Image::new_target_texture(
            edge,
            edge,
            bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb,
            None,
        );
        let target = world.resource_mut::<Assets<Image>>().add(image);
        let camera = world
            .spawn((
                Camera3d::default(),
                Camera {
                    // The viewport draws its own little scene; it must not
                    // clear or claim the main camera's target.
                    order: -1 - isize::try_from(layer).unwrap_or(0),
                    ..default()
                },
                RenderTarget::Image(target.clone().into()),
                Transform::from_xyz(0.0, 0.9, ORBIT_RADIUS).looking_at(Vec3::ZERO, Vec3::Y),
                layers.clone(),
            ))
            .id();
        // The same three-point rig the icon bake uses: key from the upper
        // left, warm fill from the lower right, cool rim from behind.
        let lights: Vec<Entity> = rig_lights()
            .into_iter()
            .map(|(light, transform)| world.spawn((light, transform, layers.clone())).id())
            .collect();
        let light = lights[0];
        let subject_entity = spawn_subject(world, &subject, &layers);

        world.entity_mut(node).insert((
            ViewportCamera {
                camera,
                light,
                lights,
                subject: subject_entity,
                layer,
            },
            ImageNode::new(target),
        ));
        if let Some(mut viewport) = world.get_mut::<ViewportNode>(node) {
            viewport.camera = Some(camera);
        }
    }
}

/// The geometry a subject is drawn as: the item's own [`ShapeKind`] mesh,
/// lit with the same material the icon bake gives it, so the live view and
/// the atlas cell are the same object. The player is still a capsule.
#[cfg(feature = "viewport")]
fn spawn_subject(
    world: &mut World,
    subject: &ViewportSubject,
    layers: &bevy::camera::visibility::RenderLayers,
) -> Entity {
    if let ViewportSubject::Entity(entity) = subject {
        // A live world entity keeps its place in the world and gains the
        // viewport's layer, so it is visible in both.
        if let Ok(mut e) = world.get_entity_mut(*entity) {
            let existing = e
                .get::<bevy::camera::visibility::RenderLayers>()
                .cloned()
                .unwrap_or_default();
            e.insert(existing.union(layers));
        }
        return *entity;
    }
    let (mesh, material, scale) = match subject {
        ViewportSubject::Player => (
            Mesh::from(Capsule3d::new(0.35, 0.9)),
            StandardMaterial {
                base_color: Color::srgb(0.8, 0.8, 0.85),
                ..default()
            },
            1.0,
        ),
        ViewportSubject::Item(name) | ViewportSubject::Block(name) => {
            let shape = shape_of(world, name);
            let [r, g, b, a] = shape.color.0;
            (
                slotted_icons::gpu::mesh_of(shape.shape),
                StandardMaterial {
                    base_color: Color::srgba(r, g, b, a),
                    metallic: shape.metallic.clamp(0.0, 1.0),
                    perceptual_roughness: shape.roughness.clamp(0.05, 1.0),
                    ..default()
                },
                0.85,
            )
        }
        ViewportSubject::Entity(_) => unreachable!("handled above"),
    };
    let mesh = world.resource_mut::<Assets<Mesh>>().add(mesh);
    let material = world
        .resource_mut::<Assets<StandardMaterial>>()
        .add(material);
    world
        .spawn((
            Mesh3d(mesh),
            MeshMaterial3d(material),
            Transform::from_scale(Vec3::splat(scale)),
            ViewportPlaceholder,
            layers.clone(),
        ))
        .id()
}

/// The shape an item declared, or a cube in its hash colour.
#[cfg(feature = "viewport")]
fn shape_of(world: &World, name: &Namespaced) -> slotted_registry::icon::ShapeIcon {
    world
        .get_resource::<slotted_ecs::Registries>()
        .and_then(|r| r.0.items.id_of(name).and_then(|id| r.0.items.get(id)))
        .and_then(|def| def.icon.as_ref().and_then(|icon| icon.shape().cloned()))
        .unwrap_or_else(|| {
            let rgba = bevy::color::Srgba::from(slotted_icons::placeholder_color(name));
            slotted_icons::shape::fallback_shape(slotted_registry::icon::IconColor([
                rgba.red, rgba.green, rgba.blue, rgba.alpha,
            ]))
        })
}

/// The lowest free layer index, claimed for `node`.
#[cfg(feature = "viewport")]
fn claim_layer(world: &mut World, node: Entity) -> usize {
    let mut layers = world.get_resource_or_init::<ViewportLayers>();
    if let Some(free) = layers.0.iter().position(Option::is_none) {
        layers.0[free] = Some(node);
        return free;
    }
    layers.0.push(Some(node));
    layers.0.len() - 1
}

/// `SlottedUiSet::Render`: turns every viewport's subject on `Time<Virtual>`,
/// so a paused or hand-stepped app sees the same motion a player does.
///
/// The subject turns and the rig does not, which is what keeps the rim light
/// on the same edge of the frame the whole way round.
#[cfg(feature = "viewport")]
pub fn orbit_viewport_cameras(
    time: Res<Time<Virtual>>,
    viewports: Query<(&ViewportCamera, &ViewportSubject)>,
    mut transforms: Query<&mut Transform>,
) {
    let angle = time.elapsed_secs() * ORBIT_SPEED;
    for (viewport, subject) in &viewports {
        // A live world entity is the game's to move; only the placeholder
        // geometry this crate spawned turns.
        if matches!(subject, ViewportSubject::Entity(_)) {
            continue;
        }
        if let Ok(mut transform) = transforms.get_mut(viewport.subject) {
            let tilt = Quat::from_rotation_x(-0.32);
            let spin = Quat::from_rotation_y(angle);
            if transform.rotation != tilt * spin {
                transform.rotation = tilt * spin;
            }
        }
    }
}

/// With the `viewport` feature: frees the layer and despawns the camera and
/// subject of a removed viewport.
#[cfg(feature = "viewport")]
pub fn despawn_viewport_cameras(world: &mut World) {
    let claimed: Vec<(usize, Entity)> = world
        .get_resource::<ViewportLayers>()
        .map(|l| {
            l.0.iter()
                .enumerate()
                .filter_map(|(i, e)| e.map(|e| (i, e)))
                .collect()
        })
        .unwrap_or_default();
    for (layer, node) in claimed {
        // The node is gone, or it lost its `ViewportSubject` (a widget can be
        // repurposed); either way the little scene behind it goes with it.
        let live = world
            .get_entity(node)
            .is_ok_and(|e| e.contains::<ViewportSubject>());
        if live {
            continue;
        }
        let camera = world
            .get_entity(node)
            .ok()
            .and_then(|e| e.get::<ViewportCamera>().cloned());
        if let Some(camera) = camera {
            for entity in std::iter::once(camera.camera).chain(camera.lights.iter().copied()) {
                if let Ok(entity) = world.get_entity_mut(entity) {
                    entity.despawn();
                }
            }
            // A `ViewportSubject::Entity` is the game's, not ours: only the
            // placeholder geometry this crate spawned is despawned.
            if world.get::<ViewportPlaceholder>(camera.subject).is_some()
                && let Ok(entity) = world.get_entity_mut(camera.subject)
            {
                entity.despawn();
            }
        }
        if let Ok(mut node) = world.get_entity_mut(node) {
            node.remove::<ViewportCamera>();
        }
        if let Some(mut layers) = world.get_resource_mut::<ViewportLayers>()
            && let Some(slot) = layers.0.get_mut(layer)
        {
            *slot = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_subject_labels_itself_by_name() {
        assert_eq!(ViewportSubject::Player.label(), "player");
        let id = Namespaced::parse("machine:furnace").expect("id");
        assert_eq!(
            ViewportSubject::Block(id.clone()).label(),
            "machine:furnace"
        );
        assert_eq!(ViewportSubject::Item(id).label(), "machine:furnace");
    }

    #[test]
    fn a_def_subject_maps_onto_the_runtime_one() {
        let id = Namespaced::parse("machine:furnace").expect("id");
        assert_eq!(
            ViewportSubject::from(&ViewSubject::Block(id.clone())),
            ViewportSubject::Block(id)
        );
        assert_eq!(
            ViewportSubject::from(&ViewSubject::Player),
            ViewportSubject::Player
        );
    }
}
