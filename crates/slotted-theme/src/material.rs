//! What a role paints.

use std::collections::BTreeMap;

use bevy::color::{Color, Srgba};
use serde::{Deserialize, Serialize};

/// A colour in a theme file: `"#RRGGBB"`, `"#RRGGBBAA"`, or `"$name"` for a
/// palette entry. Resolved by [`Theme::color`](crate::Theme::color).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ThemeColor(pub String);

impl ThemeColor {
    /// A literal hex colour.
    pub fn hex(s: &str) -> Self {
        Self(s.to_owned())
    }

    /// A palette reference.
    pub fn palette(name: &str) -> Self {
        Self(format!("${name}"))
    }

    /// The palette name if this is a `$` reference.
    pub fn palette_ref(&self) -> Option<&str> {
        self.0.strip_prefix('$')
    }

    /// Parses a literal. `None` for references or malformed hex.
    pub fn parse_hex(&self) -> Option<Color> {
        if self.palette_ref().is_some() {
            return None;
        }
        Srgba::hex(&self.0).ok().map(Color::Srgba)
    }
}

/// How a role is drawn. The apply system maps each variant onto `bevy_ui`
/// components; see the contract for the exact component set per variant.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Material {
    /// Flat fill with optional border. `BackgroundColor` + `BorderColor` +
    /// `Node::border_radius` + optional `BoxShadow`.
    Solid {
        /// Fill.
        fill: ThemeColor,
        /// Border colour; `None` for no border.
        #[serde(default)]
        border: Option<ThemeColor>,
        /// Corner radius in px. `None` keeps the node's own.
        #[serde(default)]
        radius: Option<f32>,
        /// Elevation token name.
        #[serde(default)]
        elevation: Option<String>,
    },
    /// Linear gradient fill. `BackgroundGradient` + border + radius.
    Gradient {
        /// Angle in degrees, 0 = bottom to top.
        angle: f32,
        /// `(position 0..1, colour)`.
        stops: Vec<(f32, ThemeColor)>,
        /// Border colour.
        #[serde(default)]
        border: Option<ThemeColor>,
        /// Corner radius in px.
        #[serde(default)]
        radius: Option<f32>,
        /// Elevation token name.
        #[serde(default)]
        elevation: Option<String>,
    },
    /// Nine-sliced image. `ImageNode` with `NodeImageMode::Sliced`.
    Sliced {
        /// Asset path of the image.
        image: String,
        /// Border width of the slicer in image pixels.
        border: f32,
        /// Scale applied to the corner slices.
        #[serde(default = "one")]
        scale: f32,
        /// Tint.
        #[serde(default)]
        tint: Option<ThemeColor>,
    },
    /// Frosted glass. With the `blur` feature: `MaterialNode<GlassPanelMaterial>`
    /// sampling the backdrop. Without it: rendered as `Solid { fill: tint }`
    /// with the tint's alpha raised by `fallback_alpha_boost`.
    Glass {
        /// Tint over the blurred backdrop.
        tint: ThemeColor,
        /// Blur radius override in backdrop texels; `None` uses `tokens.blur`.
        #[serde(default)]
        blur: Option<f32>,
        /// Border colour.
        #[serde(default)]
        border: Option<ThemeColor>,
        /// Corner radius in px.
        #[serde(default)]
        radius: Option<f32>,
        /// Elevation token name.
        #[serde(default)]
        elevation: Option<String>,
        /// Added to the tint alpha when blur is unavailable so the panel stays
        /// legible over the world.
        #[serde(default = "default_alpha_boost")]
        fallback_alpha_boost: f32,
    },
    /// A custom `UiMaterial` registered by a game or a later phase. The apply
    /// system logs and skips it in Phase 2.
    Shader {
        /// Asset path of the WGSL file.
        shader: String,
        /// Scalar parameters.
        #[serde(default)]
        params: BTreeMap<String, f32>,
    },
    /// Text style. `TextColor` + `TextFont::font_size`.
    Text {
        /// Colour.
        color: ThemeColor,
        /// Font size in px.
        size: f32,
    },
}

fn one() -> f32 {
    1.0
}

fn default_alpha_boost() -> f32 {
    0.35
}

impl Material {
    /// The elevation token this material asks for, if any.
    pub fn elevation(&self) -> Option<&str> {
        match self {
            Self::Solid { elevation, .. }
            | Self::Gradient { elevation, .. }
            | Self::Glass { elevation, .. } => elevation.as_deref(),
            _ => None,
        }
    }
}
