//! The token table: numbers a theme is built from.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::material::ThemeColor;

/// Spacing scale, in logical pixels.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Spacing {
    /// 2-4px: between an icon and its count.
    pub xs: f32,
    /// Slot gap.
    pub sm: f32,
    /// Panel padding.
    pub md: f32,
    /// Between sections.
    pub lg: f32,
    /// Between panels.
    pub xl: f32,
}

/// Corner radii, in logical pixels.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Radii {
    /// Slots, small buttons.
    pub sm: f32,
    /// Buttons, tooltips.
    pub md: f32,
    /// Panels.
    pub lg: f32,
}

/// One shadow level. Becomes a `BoxShadow`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Elevation {
    /// Vertical offset.
    pub y: f32,
    /// Blur radius.
    pub blur: f32,
    /// Spread.
    pub spread: f32,
    /// Shadow colour (usually black with alpha).
    pub color: ThemeColor,
}

/// Motion durations, in milliseconds, before [`Motion`](crate::Motion) scaling.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Durations {
    /// Hover, press.
    pub fast: u32,
    /// Drop squash, fades.
    pub normal: u32,
    /// Fly-to-slot, stagger total.
    pub slow: u32,
}

/// Backdrop blur settings for `Material::Glass`. Read only with the `blur`
/// feature; harmless otherwise.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Blur {
    /// Blur radius in backdrop texels.
    pub radius: f32,
    /// Backdrop image is the viewport divided by this on each axis.
    pub backdrop_divisor: u32,
}

/// The whole table.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Tokens {
    /// Spacing scale.
    pub spacing: Spacing,
    /// Corner radii.
    pub radii: Radii,
    /// Named shadow levels (`low`, `mid`, `high` by convention).
    pub elevation: BTreeMap<String, Elevation>,
    /// Motion durations.
    pub durations: Durations,
    /// Backdrop blur.
    pub blur: Blur,
    /// Named colours that materials reference with `$name`.
    pub palette: BTreeMap<String, ThemeColor>,
    /// Rarity ring colours keyed by `slotted_registry::Rarity::as_str()`.
    pub rarity: BTreeMap<String, ThemeColor>,
}
