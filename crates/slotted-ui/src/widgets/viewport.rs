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
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ViewportCamera {
    /// The camera rendering into the node's target image.
    pub camera: Entity,
    /// Its light.
    pub light: Entity,
    /// The placeholder geometry, or the live entity for
    /// [`ViewportSubject::Entity`].
    pub subject: Entity,
    /// Index into [`ViewportLayers`]; the render layer is
    /// `VIEWPORT_LAYER_BASE + layer`.
    pub layer: usize,
}

/// Marks the placeholder geometry this crate spawned for a viewport, so
/// `despawn_viewport_cameras` never despawns a game entity a
/// [`ViewportSubject::Entity`] pointed at.
#[cfg(feature = "viewport")]
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct ViewportPlaceholder;

/// How fast a viewport camera orbits its subject, in radians per second of
/// virtual time.
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
                Transform::from_xyz(0.0, 1.0, ORBIT_RADIUS).looking_at(Vec3::ZERO, Vec3::Y),
                layers.clone(),
            ))
            .id();
        let light = world
            .spawn((
                DirectionalLight::default(),
                Transform::from_xyz(2.0, 4.0, 3.0).looking_at(Vec3::ZERO, Vec3::Y),
                layers.clone(),
            ))
            .id();
        let subject_entity = spawn_subject(world, &subject, &layers);

        world.entity_mut(node).insert((
            ViewportCamera {
                camera,
                light,
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

/// The placeholder geometry a subject is drawn as. No item models exist yet
/// (contract deviation 5), so an item and a block are both a tinted cuboid
/// and the player is a capsule.
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
    let (mesh, color) = match subject {
        ViewportSubject::Player => (
            Mesh::from(Capsule3d::new(0.35, 0.9)),
            Color::srgb(0.8, 0.8, 0.85),
        ),
        ViewportSubject::Item(name) | ViewportSubject::Block(name) => (
            Mesh::from(Cuboid::from_length(1.0)),
            slotted_icons::placeholder_color(name),
        ),
        ViewportSubject::Entity(_) => unreachable!("handled above"),
    };
    let mesh = world.resource_mut::<Assets<Mesh>>().add(mesh);
    let material = world
        .resource_mut::<Assets<StandardMaterial>>()
        .add(StandardMaterial {
            base_color: color,
            ..default()
        });
    world
        .spawn((
            Mesh3d(mesh),
            MeshMaterial3d(material),
            Transform::default(),
            ViewportPlaceholder,
            layers.clone(),
        ))
        .id()
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

/// `SlottedUiSet::Render`: orbits every viewport camera on `Time<Virtual>`,
/// so a paused or hand-stepped app sees the same motion a player does.
#[cfg(feature = "viewport")]
pub fn orbit_viewport_cameras(
    time: Res<Time<Virtual>>,
    viewports: Query<&ViewportCamera>,
    mut cameras: Query<&mut Transform>,
) {
    let angle = time.elapsed_secs() * ORBIT_SPEED;
    for viewport in &viewports {
        if let Ok(mut transform) = cameras.get_mut(viewport.camera) {
            *transform =
                Transform::from_xyz(ORBIT_RADIUS * angle.sin(), 1.0, ORBIT_RADIUS * angle.cos())
                    .looking_at(Vec3::ZERO, Vec3::Y);
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
            .and_then(|e| e.get::<ViewportCamera>().copied());
        if let Some(camera) = camera {
            for entity in [camera.camera, camera.light] {
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
