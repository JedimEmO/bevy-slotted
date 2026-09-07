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

/// A text size: a literal in px (`15` or `"15"`) or a `$name` reference into
/// `tokens.typography` (menus M1 contract 1.4). Serialises as a number when
/// literal and as the reference string otherwise.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ThemeSize(pub String);

impl ThemeSize {
    /// A literal size.
    pub fn px(size: f32) -> Self {
        Self(format!("{size}"))
    }

    /// A typography reference.
    pub fn typography(name: &str) -> Self {
        Self(format!("${name}"))
    }

    /// The typography name if this is a `$` reference.
    pub fn typography_ref(&self) -> Option<&str> {
        self.0.strip_prefix('$')
    }

    /// The literal size. `None` for references or a malformed literal.
    pub fn parse_px(&self) -> Option<f32> {
        if self.typography_ref().is_some() {
            return None;
        }
        self.0.trim().parse().ok()
    }
}

impl From<f32> for ThemeSize {
    fn from(px: f32) -> Self {
        Self::px(px)
    }
}

impl serde::Serialize for ThemeSize {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self.parse_px() {
            Some(px) => s.serialize_f32(px),
            None => s.serialize_str(&self.0),
        }
    }
}

impl<'de> serde::Deserialize<'de> for ThemeSize {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(serde::Deserialize)]
        #[serde(untagged)]
        enum Raw {
            Num(f32),
            Int(i64),
            Str(String),
        }
        Ok(match Raw::deserialize(d)? {
            Raw::Num(px) => Self::px(px),
            #[allow(clippy::cast_precision_loss)]
            Raw::Int(px) => Self::px(px as f32),
            Raw::Str(s) => Self(s),
        })
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
    /// Text style. `TextColor` + `TextFont::font_size`, and `TextFont::font`
    /// when `font` names a token with a file.
    Text {
        /// Colour.
        color: ThemeColor,
        /// Font size in px, or a `$name` into `tokens.typography`, in which
        /// case the style's font and weight apply unless named here.
        size: ThemeSize,
        /// Name of an entry in `tokens.fonts`; `None` keeps Bevy's default
        /// (or the type style's font for a `$` size).
        #[serde(default)]
        font: Option<String>,
        /// Variable-font weight, 100 to 900. `None` = 400, or the type
        /// style's weight for a `$` size.
        #[serde(default)]
        weight: Option<u16>,
        /// Drop shadow colour, one px down and right; `None` removes any
        /// `TextShadow` the widget spawned with. Glass and neon keep one
        /// under slot counts so they read over an icon; paper has none.
        #[serde(default)]
        shadow: Option<ThemeColor>,
    },
    /// A repeating image tile over the node: paper's dotted grid. `ImageNode`
    /// with `NodeImageMode::Tiled`, plus border, radius and elevation like a
    /// `Solid`. The tile should carry its own opaque ground so draw order
    /// against a `BackgroundColor` never matters.
    Tiled {
        /// Asset path of the tile image.
        image: String,
        /// Scale applied to the tile; 1.0 draws it at its pixel size.
        #[serde(default = "one")]
        scale: f32,
        /// Tint.
        #[serde(default)]
        tint: Option<ThemeColor>,
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
    /// A hatched border: the paper direction's dashed rarity outline and
    /// stamp frames. `BorderGradient` with hard stops alternating `stroke`
    /// and transparent every `dash` and `gap` px along a diagonal, so the
    /// border reads as a dashed rule, with a plain `fill` behind.
    Dashed {
        /// Fill inside the border.
        fill: ThemeColor,
        /// Dash colour.
        stroke: ThemeColor,
        /// Border width in px; sets `Node::border` on the node. `None` keeps
        /// the node's own width.
        #[serde(default)]
        width: Option<f32>,
        /// Dash length in px.
        #[serde(default = "four")]
        dash: f32,
        /// Gap length in px.
        #[serde(default = "four")]
        gap: f32,
        /// Corner radius in px.
        #[serde(default)]
        radius: Option<f32>,
        /// Elevation token name.
        #[serde(default)]
        elevation: Option<String>,
    },
    /// Opaque panel with diagonal cut corners: the neon direction's shape.
    /// With the `blur` feature (which is the "GPU materials" feature) a
    /// `MaterialNode<CutCornerMaterial>` draws the chamfered box, its border
    /// and the optional bottom accent bar with its glow. Without it, or when
    /// no `CutCornerPlugin` is present, the node falls back to a square
    /// `Solid` and the bar becomes a hard-stop `BackgroundGradient`.
    CutCorners {
        /// Fill.
        fill: ThemeColor,
        /// Border colour; `None` for no border.
        #[serde(default)]
        border: Option<ThemeColor>,
        /// Border width in px, drawn inside the shape by the shader.
        #[serde(default = "one")]
        border_width: f32,
        /// Length of each cut along the edges, in px.
        #[serde(default = "eight")]
        cut: f32,
        /// Which corners are cut.
        #[serde(default)]
        corners: Corners,
        /// Optional accent bar along the bottom edge: rarity in neon.
        #[serde(default)]
        bar: Option<ThemeColor>,
        /// Height of the accent bar in px.
        #[serde(default = "three")]
        bar_height: f32,
        /// How far the bar's glow bleeds up into the fill, in px. Zero for
        /// a flat bar.
        #[serde(default)]
        glow: f32,
        /// Elevation token name.
        #[serde(default)]
        elevation: Option<String>,
    },
}

/// Which corners a [`Material::CutCorners`] chamfers.
// Four named flags read better in a theme file than a bit set would.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Corners {
    /// Top-left.
    #[serde(default)]
    pub top_left: bool,
    /// Top-right.
    #[serde(default)]
    pub top_right: bool,
    /// Bottom-right.
    #[serde(default)]
    pub bottom_right: bool,
    /// Bottom-left.
    #[serde(default)]
    pub bottom_left: bool,
}

impl Corners {
    /// The moodboard's diagonal: top-right and bottom-left.
    pub const DIAGONAL: Self = Self {
        top_left: false,
        top_right: true,
        bottom_right: false,
        bottom_left: true,
    };
    /// All four.
    pub const ALL: Self = Self {
        top_left: true,
        top_right: true,
        bottom_right: true,
        bottom_left: true,
    };
}

impl Default for Corners {
    fn default() -> Self {
        Self::DIAGONAL
    }
}

fn one() -> f32 {
    1.0
}

fn three() -> f32 {
    3.0
}

fn four() -> f32 {
    4.0
}

fn eight() -> f32 {
    8.0
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
            | Self::Glass { elevation, .. }
            | Self::Tiled { elevation, .. }
            | Self::Dashed { elevation, .. }
            | Self::CutCorners { elevation, .. } => elevation.as_deref(),
            _ => None,
        }
    }

    /// Every colour this material carries, for palette validation.
    pub fn colors(&self) -> Vec<&ThemeColor> {
        let mut out = Vec::new();
        match self {
            Self::Solid { fill, border, .. } => {
                out.push(fill);
                out.extend(border.iter());
            }
            Self::Gradient { stops, border, .. } => {
                out.extend(stops.iter().map(|(_, c)| c));
                out.extend(border.iter());
            }
            Self::Sliced { tint, .. } => out.extend(tint.iter()),
            Self::Glass { tint, border, .. } => {
                out.push(tint);
                out.extend(border.iter());
            }
            Self::Shader { .. } => {}
            Self::Text { color, shadow, .. } => {
                out.push(color);
                out.extend(shadow.iter());
            }
            Self::Tiled { tint, border, .. } => {
                out.extend(tint.iter());
                out.extend(border.iter());
            }
            Self::Dashed { fill, stroke, .. } => {
                out.push(fill);
                out.push(stroke);
            }
            Self::CutCorners {
                fill, border, bar, ..
            } => {
                out.push(fill);
                out.extend(border.iter());
                out.extend(bar.iter());
            }
        }
        out
    }

    /// The font token a text material names.
    pub fn font(&self) -> Option<&str> {
        match self {
            Self::Text { font, .. } => font.as_deref(),
            _ => None,
        }
    }
}
