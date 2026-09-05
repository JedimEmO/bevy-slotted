//! Plugin and system sets.

use bevy::asset::AssetApp;
use bevy::prelude::*;

use crate::apply::{ActiveTheme, apply_theme};
use crate::motion::{ActiveMotions, Motion, advance_tweens};
use crate::theme::{Theme, ThemeLoader};

/// Theme systems, in `Update`, after the ui crate's render set.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SlottedThemeSet {
    /// Advance tweens.
    Motion,
    /// Paint roles.
    Apply,
}

/// Registers the [`Theme`] asset and loader, [`Motion`], [`ActiveTheme`]
/// and the two system sets.
///
/// Does not load a theme: the facade or the game sets `ActiveTheme`. With
/// the `blur` feature, add `blur::BackdropPlugin` as well.
#[derive(Default)]
pub struct SlottedThemePlugin;

impl Plugin for SlottedThemePlugin {
    fn build(&self, app: &mut App) {
        app.init_asset::<Theme>()
            .register_asset_loader(ThemeLoader)
            .init_resource::<ActiveTheme>()
            .init_resource::<Motion>()
            .init_resource::<ActiveMotions>()
            .configure_sets(
                Update,
                (SlottedThemeSet::Motion, SlottedThemeSet::Apply).chain(),
            )
            .add_systems(
                Update,
                (
                    advance_tweens.in_set(SlottedThemeSet::Motion),
                    apply_theme.in_set(SlottedThemeSet::Apply),
                ),
            );
    }
}
