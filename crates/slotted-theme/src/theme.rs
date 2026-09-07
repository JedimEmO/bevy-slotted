//! The [`Theme`] asset and its RON loader.

use std::collections::HashMap;

use bevy::asset::{Asset, AssetLoader, LoadContext, io::Reader};
use bevy::color::Color;
use bevy::reflect::TypePath;
use serde::{Deserialize, Serialize};

use crate::material::{Material, ThemeColor, ThemeSize};
use crate::role::{Role, roles};
use crate::tokens::{Tokens, TypeStyle};

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

    /// Resolves a text size: a literal, or `$name` through the type scale.
    /// An unresolvable reference comes back as 13 px so a typo is visible
    /// in the log, not fatal.
    pub fn size(&self, size: &ThemeSize) -> f32 {
        match size.typography_ref() {
            Some(name) => self.tokens.typography.get(name).map_or_else(
                || {
                    tracing::warn!(reference = name, "unknown typography reference");
                    13.0
                },
                |style| style.size,
            ),
            None => size.parse_px().unwrap_or(13.0),
        }
    }

    /// The type style a `$name` size refers to, if any.
    pub fn type_style(&self, size: &ThemeSize) -> Option<&TypeStyle> {
        size.typography_ref()
            .and_then(|name| self.tokens.typography.get(name))
    }

    /// Every `$name` size that has no typography entry.
    pub fn dangling_typography_refs(&self) -> Vec<String> {
        let mut out: Vec<String> = self
            .roles
            .values()
            .filter_map(|m| match m {
                Material::Text { size, .. } => size.typography_ref(),
                _ => None,
            })
            .filter(|name| !self.tokens.typography.contains_key(*name))
            .map(str::to_owned)
            .collect();
        out.sort();
        out.dedup();
        out
    }

    /// Well-known roles this theme does not define even through fallback.
    pub fn missing_roles(&self) -> Vec<Role> {
        roles::ALL
            .iter()
            .filter(|r| self.material(r).is_none())
            .cloned()
            .collect()
    }

    /// Every font token a text role names that `tokens.fonts` lacks.
    pub fn dangling_font_refs(&self) -> Vec<String> {
        let mut out: Vec<String> = self
            .roles
            .values()
            .filter_map(Material::font)
            .filter(|f| !self.tokens.fonts.contains_key(*f))
            .map(str::to_owned)
            .collect();
        out.sort();
        out.dedup();
        out
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
            m.colors().into_iter().for_each(&mut check);
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
    const PAPER: &str = include_str!("../../../assets/themes/paper.theme.ron");
    const NEON: &str = include_str!("../../../assets/themes/neon.theme.ron");

    fn shipped() -> [(&'static str, Theme); 3] {
        [
            ("glass", Theme::from_ron(GLASS).expect("glass parses")),
            ("paper", Theme::from_ron(PAPER).expect("paper parses")),
            ("neon", Theme::from_ron(NEON).expect("neon parses")),
        ]
    }

    /// The three shipped themes define exactly the same set of roles, so a
    /// widget that looks right in one cannot silently lose its material in
    /// another. Phase 7's rule: a missing role is added to every theme, never
    /// special-cased.
    #[test]
    fn shipped_themes_define_the_same_roles() {
        let themes = shipped();
        let keys = |t: &Theme| {
            let mut k: Vec<String> = t.roles.keys().map(|r| r.as_str().to_owned()).collect();
            k.sort();
            k
        };
        let glass = keys(&themes[0].1);
        for (name, theme) in &themes[1..] {
            assert_eq!(keys(theme), glass, "{name} roles differ from glass");
        }
    }

    #[test]
    fn every_shipped_theme_is_complete_and_self_consistent() {
        for (name, theme) in shipped() {
            assert_eq!(theme.name, name);
            assert_eq!(theme.missing_roles(), Vec::<Role>::new(), "{name}");
            assert_eq!(
                theme.dangling_palette_refs(),
                Vec::<String>::new(),
                "{name}"
            );
            assert_eq!(theme.dangling_font_refs(), Vec::<String>::new(), "{name}");
            for rarity in ["common", "uncommon", "rare", "epic", "legendary"] {
                assert!(
                    theme.tokens.rarity.contains_key(rarity),
                    "{name} lacks {rarity}"
                );
            }
            let text = ron::ser::to_string_pretty(&theme, ron::ser::PrettyConfig::default())
                .expect("serialises");
            assert_eq!(Theme::from_ron(&text).expect("re-parses"), theme, "{name}");
        }
    }

    /// Paper is opaque: no glass anywhere, square corners, and its rarity
    /// outlines are the dashed stamps the direction asks for.
    #[test]
    fn paper_is_opaque_square_and_dashed() {
        let theme = Theme::from_ron(PAPER).expect("parses");
        for (role, material) in &theme.roles {
            assert!(
                !matches!(material, Material::Glass { .. }),
                "{role} is glass in paper"
            );
            if let Material::Solid {
                radius: Some(r), ..
            }
            | Material::Tiled {
                radius: Some(r), ..
            } = material
            {
                assert!(*r <= 2.0, "{role} has radius {r} in paper");
            }
        }
        assert!(matches!(
            theme.material(&roles::PANEL),
            Some(Material::Tiled { .. })
        ));
        assert!(matches!(
            theme.material(&Role::new("slot.rarity.rare")),
            Some(Material::Dashed { .. })
        ));
    }

    /// Neon is chamfered: the panel, slots and buttons are cut, and rarity
    /// is a bar along the bottom of the slot.
    #[test]
    fn neon_cuts_its_corners_and_bars_its_rarity() {
        let theme = Theme::from_ron(NEON).expect("parses");
        for role in [roles::PANEL, roles::SLOT, roles::BUTTON, roles::SLOT_HOVER] {
            assert!(
                matches!(theme.material(&role), Some(Material::CutCorners { .. })),
                "{role} is not cut in neon"
            );
        }
        let Some(Material::CutCorners {
            bar: Some(bar),
            glow,
            ..
        }) = theme.material(&Role::new("slot.rarity.legendary"))
        else {
            panic!("legendary rarity is not a bar");
        };
        assert_eq!(
            theme.color(bar),
            theme.color(&ThemeColor::palette("magenta"))
        );
        assert!(*glow > 0.0);
    }

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
