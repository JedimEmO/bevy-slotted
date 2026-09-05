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
//! Backdrop blur for glass panels is behind the `blur` feature (ADR 0003).
//! Without it `Material::Glass` still deserialises and renders as a solid tint.

pub mod apply;
pub mod material;
pub mod motion;
pub mod plugin;
pub mod role;
pub mod theme;
pub mod tokens;

#[cfg(feature = "blur")]
pub mod blur;

pub use apply::{ActiveTheme, Themed, apply_theme};
pub use material::{Material, ThemeColor};
pub use motion::{ActiveMotions, Motion, MotionPreset, Tween, TweenTarget};
pub use plugin::{SlottedThemePlugin, SlottedThemeSet};
pub use role::{Role, roles};
pub use theme::{Theme, ThemeError, ThemeLoader};
pub use tokens::{Blur, Durations, Elevation, Radii, Spacing, Tokens};

/// The names a widget author needs.
pub mod prelude {
    pub use crate::{
        ActiveTheme, Material, Motion, MotionPreset, Role, SlottedThemePlugin, SlottedThemeSet,
        Theme, ThemeColor, Themed, roles,
    };
}
