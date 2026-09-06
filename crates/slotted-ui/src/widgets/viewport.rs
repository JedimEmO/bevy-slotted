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

/// Spawns a viewport node with no camera. Contract 1.6.
pub fn spawn_viewport(ctx: &mut SpawnCtx<'_>, params: &ViewportParams, _tags: &Tags) -> Entity {
    // PHASE6-IMPL: A. Themed(VIEWPORT), size x size node.
    let subject = ViewportSubject::from(&params.subject);
    ctx.spawn_node((
        Node {
            width: Val::Px(params.size),
            height: Val::Px(params.size),
            ..default()
        },
        ViewportNode { camera: None },
        SemanticRole::Viewport,
        SemanticLabel(subject.label()),
        WidgetNode(crate::widgets::kinds::viewport()),
        subject,
    ))
}

/// With the `viewport` feature and not headless: creates the render target,
/// camera, light and placeholder subject for every `Added<ViewportSubject>`.
#[cfg(feature = "viewport")]
pub fn spawn_viewport_cameras(world: &mut World) {
    // PHASE6-IMPL: A. Image render target, Camera3d with RenderTarget::Image,
    // RenderLayers::layer(VIEWPORT_LAYER_BASE + n), orbit on virtual time,
    // `slotted_icons::placeholder_hue` tint for Item/Block.
    let _ = world;
}

/// With the `viewport` feature: frees the layer and despawns the camera and
/// subject of a removed viewport.
#[cfg(feature = "viewport")]
pub fn despawn_viewport_cameras(world: &mut World) {
    // PHASE6-IMPL: A.
    let _ = world;
}
