//! Backdrop blur for glass panels (`blur` feature). Lifted from
//! `spikes/glass-ui` and ADR 0003.
//!
//! A second camera renders the scene the primary camera sees into a
//! quarter-resolution image; [`GlassPanelMaterial`] samples it with a 13-tap
//! tent, deriving its screen UV from `@builtin(position)`, so no layout data
//! ever reaches the material.

use bevy::asset::Handle;
use bevy::image::Image;
use bevy::prelude::*;
use bevy::render::render_resource::AsBindGroup;
use bevy::shader::ShaderRef;
use bevy::ui_render::ui_material::UiMaterial;

/// The frosted panel material. Only styling lives here.
#[derive(AsBindGroup, Asset, TypePath, Debug, Clone)]
pub struct GlassPanelMaterial {
    /// Tint over the backdrop.
    #[uniform(0)]
    pub tint: LinearRgba,
    /// Border colour.
    #[uniform(0)]
    pub edge_color: LinearRgba,
    /// Blur radius in backdrop texels.
    #[uniform(0)]
    pub blur_radius: f32,
    /// 1.0 = sample the backdrop, 0.0 = flat tint.
    #[uniform(0)]
    pub blur_enabled: f32,
    /// Padding for uniform alignment.
    #[uniform(0)]
    pub pad: Vec2,
    /// The blurred scene.
    #[texture(1)]
    #[sampler(2)]
    pub backdrop: Handle<Image>,
}

impl UiMaterial for GlassPanelMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/glass_panel.wgsl".into()
    }
}

/// Marks the game's primary camera so the backdrop camera can follow it.
#[derive(Component, Debug, Default, Clone, Copy)]
pub struct BackdropSource;

/// The backdrop camera spawned by [`BackdropPlugin`].
#[derive(Component, Debug, Default, Clone, Copy)]
pub struct BackdropCamera;

/// Handle of the backdrop image, for the glass material.
#[derive(Resource, Debug, Clone)]
pub struct BackdropImage(pub Handle<Image>);

/// Spawns the backdrop camera and keeps its transform synced to the
/// [`BackdropSource`] camera every frame.
// PHASE2-IMPL: agent B. Port `setup_backdrop` and `sync_backdrop_camera` from
// spikes/glass-ui/src/main.rs; register `UiMaterialPlugin::<GlassPanelMaterial>`.
#[derive(Default)]
pub struct BackdropPlugin;

impl Plugin for BackdropPlugin {
    fn build(&self, _app: &mut App) {}
}
