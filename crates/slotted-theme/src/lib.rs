//! Themes for slotted: tokens, roles, materials, motion.
//!
//! A [`Theme`] is a RON asset mapping semantic [`Role`]s (`panel`, `slot`,
//! `slot.hover`, `tooltip.frame`...) to [`Material`]s, plus a token table for
//! spacing, radii, elevation, durations, blur and colours. Widgets never name
//! colours; they carry a [`Themed`] role and the apply system paints them
//! whenever the theme loads, changes on disk or is swapped.
//!
//! [`Motion`] is the duration scale and reduced-motion switch every animation
//! in the ui crate reads. `ActiveMotions` counts running tweens so a test
//! harness can wait for them.
//!
//! Backdrop blur for glass panels and the neon theme's cut corners are behind
//! the `blur` feature (ADR 0003): both are `UiMaterial` shaders. Without it
//! `Material::Glass` still deserialises and renders as a solid tint, and
//! `Material::CutCorners` as a square solid.

pub mod apply;
pub mod material;
pub mod motion;
pub mod plugin;
pub mod role;
pub mod theme;
pub mod tokens;

#[cfg(feature = "blur")]
pub mod blur;
#[cfg(feature = "blur")]
pub mod cut;

pub use apply::{
    ActiveTheme, CutPaint, GlassPaint, Paint, SlicedPaint, Themed, TiledPaint, apply_theme,
};
pub use material::{Corners, Material, ThemeColor, ThemeSize};
pub use motion::{
    ActiveMotions, Easing, Motion, MotionPreset, Tween, TweenTarget, TweenValue, advance_tweens,
};
pub use plugin::{SlottedThemePlugin, SlottedThemeSet};
pub use role::{Role, roles};
pub use theme::{Theme, ThemeError, ThemeLoader};
pub use tokens::{
    Blur, Durations, Elevation, FontToken, MotionSpec, MotionTokens, Radii, Sizes, Spacing, Tokens,
};

/// The names a widget author needs.
pub mod prelude {
    pub use crate::{
        ActiveTheme, Material, Motion, MotionPreset, Role, SlottedThemePlugin, SlottedThemeSet,
        Theme, ThemeColor, Themed, roles,
    };
}
