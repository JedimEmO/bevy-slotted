//! The [`Theme`] asset and its RON loader.

use std::collections::HashMap;

use bevy::asset::{Asset, AssetLoader, LoadContext, io::Reader};
use bevy::color::Color;
use bevy::reflect::TypePath;
use serde::{Deserialize, Serialize};

use crate::material::{Material, ThemeColor};
use crate::role::{Role, roles};
use crate::tokens::Tokens;

/// A complete skin: tokens plus a role-to-material map.
///
/// Loaded from `*.theme.ron` by [`ThemeLoader`]. Files may omit state roles
/// (`slot.hover`); lookups fall back to the parent role (`slot`).
#[derive(Asset, TypePath, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Theme {
    /// Display name (`glass`, `paper`, `neon`).
    pub name: String,
    /// The token table.
    pub tokens: Tokens,
    /// Role to material.
    pub roles: HashMap<Role, Material>,
}

/// Why a theme failed to load.
#[derive(Debug, thiserror::Error)]
pub enum ThemeError {
    /// Read failure.
    #[error("could not read theme: {0}")]
    Io(#[from] std::io::Error),
    /// RON parse failure.
    #[error("could not parse theme: {0}")]
    Ron(#[from] ron::error::SpannedError),
    /// A `$name` colour that is not in the palette.
    #[error("unknown palette colour `{0}`")]
    UnknownPaletteColor(String),
}

impl Theme {
    /// Parses a RON theme.
    pub fn from_ron(text: &str) -> Result<Self, ThemeError> {
        Ok(ron::from_str(text)?)
    }

    /// The material for `role`, falling back through parents
    /// (`slot.hover` -> `slot`). `None` when no ancestor is defined.
    pub fn material(&self, role: &Role) -> Option<&Material> {
        let mut current = Some(role.clone());
        while let Some(r) = current {
            if let Some(m) = self.roles.get(&r) {
                return Some(m);
            }
            current = r.parent();
        }
        None
    }

    /// Resolves a theme colour: hex literal, or `$name` through the palette.
    /// Unresolvable colours come back magenta so a typo is visible, not fatal.
    pub fn color(&self, c: &ThemeColor) -> Color {
        if let Some(name) = c.palette_ref() {
            return self
                .tokens
                .palette
                .get(name)
                .and_then(ThemeColor::parse_hex)
                .unwrap_or(Color::srgb(1.0, 0.0, 1.0));
        }
        c.parse_hex().unwrap_or(Color::srgb(1.0, 0.0, 1.0))
    }

    /// Well-known roles this theme does not define even through fallback.
    pub fn missing_roles(&self) -> Vec<Role> {
        roles::ALL
            .iter()
            .filter(|r| self.material(r).is_none())
            .cloned()
            .collect()
    }

    /// Every `$name` reference that has no palette entry.
    pub fn dangling_palette_refs(&self) -> Vec<String> {
        let mut out = Vec::new();
        let mut check = |c: &ThemeColor| {
            if let Some(name) = c.palette_ref()
                && !self.tokens.palette.contains_key(name)
            {
                out.push(name.to_owned());
            }
        };
        for m in self.roles.values() {
            match m {
                Material::Solid { fill, border, .. } => {
                    check(fill);
                    border.iter().for_each(&mut check);
                }
                Material::Gradient { stops, border, .. } => {
                    for (_, c) in stops {
                        check(c);
                    }
                    border.iter().for_each(&mut check);
                }
                Material::Sliced { tint, .. } => tint.iter().for_each(&mut check),
                Material::Glass { tint, border, .. } => {
                    check(tint);
                    border.iter().for_each(&mut check);
                }
                Material::Shader { .. } => {}
                Material::Text { color, .. } => check(color),
            }
        }
        for e in self.tokens.elevation.values() {
            check(&e.color);
        }
        for c in self.tokens.rarity.values() {
            check(c);
        }
        out.sort();
        out.dedup();
        out
    }
}

/// Loads `*.theme.ron`.
#[derive(Default, TypePath)]
pub struct ThemeLoader;

impl AssetLoader for ThemeLoader {
    type Asset = Theme;
    type Settings = ();
    type Error = ThemeError;

    async fn load(
        &self,
        reader: &mut dyn Reader,
        _settings: &(),
        _load_context: &mut LoadContext<'_>,
    ) -> Result<Theme, ThemeError> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        let text = String::from_utf8_lossy(&bytes);
        Theme::from_ron(&text)
    }

    fn extensions(&self) -> &[&str] {
        &["theme.ron"]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    const GLASS: &str = include_str!("../../../assets/themes/glass.theme.ron");

    #[test]
    fn glass_theme_parses_and_is_complete() {
        let theme = Theme::from_ron(GLASS).expect("glass.theme.ron parses");
        assert_eq!(theme.name, "glass");
        assert_eq!(theme.missing_roles(), Vec::<Role>::new());
        assert_eq!(theme.dangling_palette_refs(), Vec::<String>::new());
        assert!(matches!(
            theme.material(&roles::PANEL),
            Some(Material::Glass { .. })
        ));
        // State roles fall back to their parent when omitted.
        let custom = Role::new("slot.hover.something");
        assert_eq!(theme.material(&custom), theme.material(&roles::SLOT_HOVER));
    }

    #[test]
    fn palette_refs_resolve() {
        let theme = Theme::from_ron(GLASS).expect("parses");
        let accent = theme.color(&ThemeColor::palette("accent"));
        assert_eq!(
            accent,
            Color::Srgba(bevy::color::Srgba::hex("7FD1FF").expect("hex"))
        );
        let bad = theme.color(&ThemeColor::palette("nope"));
        assert_eq!(bad, Color::srgb(1.0, 0.0, 1.0));
    }

    #[test]
    fn theme_round_trips_through_ron() {
        let theme = Theme::from_ron(GLASS).expect("parses");
        let text = ron::ser::to_string_pretty(&theme, ron::ser::PrettyConfig::default())
            .expect("serialises");
        let again = Theme::from_ron(&text).expect("re-parses");
        assert_eq!(theme, again);
    }
}
