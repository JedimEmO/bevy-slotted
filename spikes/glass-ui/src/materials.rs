//! The two `UiMaterial`s the spike proves out.

use bevy::prelude::*;
use bevy::render::render_resource::AsBindGroup;
use bevy::shader::ShaderRef;

/// Frosted glass panel that samples a quarter-resolution copy of the scene behind it.
///
/// Note what is *not* in here: the node's screen rect. The UI material fragment shader gets
/// `@builtin(position)` (framebuffer pixel coords) via `UiVertexOutput.position`, and
/// `view.viewport` from the group(0) view uniform, so screen UV is derived in the shader.
/// The only per-panel data is styling.
#[derive(AsBindGroup, Asset, TypePath, Debug, Clone)]
pub struct GlassPanelMaterial {
    #[uniform(0)]
    pub tint: LinearRgba,
    #[uniform(0)]
    pub edge_color: LinearRgba,
    /// Blur radius in texels of the backdrop image.
    #[uniform(0)]
    pub blur_radius: f32,
    /// 1.0 = sample the backdrop, 0.0 = flat tint only.
    #[uniform(0)]
    pub blur_enabled: f32,
    #[uniform(0)]
    pub _pad: Vec2,
    #[texture(1)]
    #[sampler(2)]
    pub backdrop: Handle<Image>,
}

impl UiMaterial for GlassPanelMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/glass_panel.wgsl".into()
    }
}

/// Rarity ring + animated shimmer + soft outer glow for a "legendary" slot.
#[derive(AsBindGroup, Asset, TypePath, Debug, Clone)]
pub struct SlotGlowMaterial {
    #[uniform(0)]
    pub rarity_color: LinearRgba,
    /// `x` glow inset in px, `y` corner radius in px, `z` ring thickness in px, `w` shimmer speed.
    #[uniform(0)]
    pub params: Vec4,
}

impl UiMaterial for SlotGlowMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/slot_glow.wgsl".into()
    }
}
