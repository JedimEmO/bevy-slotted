//! Diagonal cut corners for the neon direction (`blur` feature, which is the
//! GPU materials feature: the same `bevy_ui_render` dependency ADR 0003 took
//! for glass).
//!
//! [`CutCornerMaterial`] draws a box whose chosen corners are chamfered by a
//! signed-distance function, with an inner border and an optional accent bar
//! along the bottom edge that glows up into the fill. The bar is clipped by
//! the same SDF, which is why rarity lives in this material rather than in a
//! gradient: a bottom-left chamfer would otherwise show the bar poking out.
//!
//! Without [`CutCornerPlugin`] the apply system paints the square fallback
//! `Paint::from_material` always carries.

use bevy::prelude::*;
use bevy::render::render_resource::AsBindGroup;
use bevy::shader::ShaderRef;
use bevy::ui_render::ui_material::UiMaterial;

use crate::apply::CutPaint;

/// The chamfered panel material. Only styling lives here; the node's size
/// reaches the shader through `UiVertexOutput`.
#[derive(AsBindGroup, Asset, TypePath, Debug, Clone)]
pub struct CutCornerMaterial {
    /// Fill.
    #[uniform(0)]
    pub fill: LinearRgba,
    /// Border colour; alpha zero for none.
    #[uniform(0)]
    pub border: LinearRgba,
    /// Accent bar colour; alpha zero for none.
    #[uniform(0)]
    pub bar: LinearRgba,
    /// `x` border width, `y` cut length, `z` bar height, `w` glow, all px.
    #[uniform(0)]
    pub params: Vec4,
    /// Cut flags per corner: top-left, top-right, bottom-right, bottom-left.
    /// `1.0` cuts.
    #[uniform(0)]
    pub corners: Vec4,
}

impl UiMaterial for CutCornerMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/cut_corner.wgsl".into()
    }
}

impl From<CutPaint> for CutCornerMaterial {
    fn from(c: CutPaint) -> Self {
        let flag = |on: bool| if on { 1.0 } else { 0.0 };
        Self {
            fill: c.fill.into(),
            border: c.border.into(),
            bar: c.bar.into(),
            params: Vec4::new(c.border_width, c.cut, c.bar_height, c.glow),
            corners: Vec4::new(
                flag(c.corners.top_left),
                flag(c.corners.top_right),
                flag(c.corners.bottom_right),
                flag(c.corners.bottom_left),
            ),
        }
    }
}

/// Registers the [`CutCornerMaterial`] pipeline. A windowed game that uses
/// the neon theme adds this; `BackdropPlugin` adds it too, so the examples
/// need nothing extra. Harmless without a render app.
#[derive(Debug, Clone, Copy, Default)]
pub struct CutCornerPlugin;

impl Plugin for CutCornerPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(bevy::ui_render::prelude::UiMaterialPlugin::<
            CutCornerMaterial,
        >::default());
    }
}
