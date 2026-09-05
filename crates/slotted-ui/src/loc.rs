//! The localisation port: a key in, display text out.
//!
//! Text reaches a player from two places that cannot see each other. A screen
//! tree names a [`LocKey`] and `slotted-packs` resolves it against the Fluent
//! bundles it loaded from the base game and every mod. The item browser also
//! draws names, but `slotted-packs` depends on `slotted-browser`, so the
//! browser cannot reach `Locales` and its cards showed the key itself:
//! `copper_chest.item.copper_chest` where a player expects "Copper Chest".
//!
//! [`Localizer`] is the port that closes that. `slotted-ui` owns the trait and
//! a [`Localization`] resource holding one behind an `Arc`, defaulting to
//! [`NoLocalization`], which resolves nothing. `slotted-packs` replaces it
//! after every locale load; anything that draws text reads it. A crate that
//! wants no localisation at all, and every test that does not load a mod, gets
//! the key verbatim, which is what the Phase 2 and Phase 3 snapshots expect.

use std::sync::Arc;

use bevy::ecs::resource::Resource;

use crate::def::LocKey;

/// Resolves a localisation key against whatever catalogue the host loaded.
///
/// Implementations are shared across threads and read-only; the resource is
/// replaced rather than mutated when the catalogue changes, so a system can
/// use `Res<Localization>`'s change detection to know that every string it
/// drew is stale.
pub trait Localizer: Send + Sync + 'static {
    /// The display text for `key`, or `None` when nothing defines it.
    fn resolve(&self, key: &LocKey) -> Option<String>;
}

/// The [`Localizer`] that resolves nothing, so every key is drawn as written.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoLocalization;

impl Localizer for NoLocalization {
    fn resolve(&self, _key: &LocKey) -> Option<String> {
        None
    }
}

/// The active [`Localizer`].
///
/// `slotted-packs` inserts one over the mods' `.ftl` layers at the end of each
/// load and each reload. Everything else reads it.
#[derive(Resource, Clone)]
pub struct Localization(pub Arc<dyn Localizer>);

impl Localization {
    /// Wraps `localizer`.
    pub fn new(localizer: impl Localizer) -> Self {
        Self(Arc::new(localizer))
    }

    /// The display text for `key`, or `None` when nothing defines it.
    pub fn resolve(&self, key: &LocKey) -> Option<String> {
        self.0.resolve(key)
    }

    /// The display text for `key`, falling back to the key itself.
    ///
    /// This is what a label draws: an unresolved key on screen is a legible
    /// bug report, where an empty label is not.
    pub fn text(&self, key: &LocKey) -> String {
        self.resolve(key).unwrap_or_else(|| key.0.clone())
    }

    /// [`text`](Self::text) for a key that is only a `&str`.
    pub fn text_for(&self, key: &str) -> String {
        self.text(&LocKey(key.to_owned()))
    }
}

impl Default for Localization {
    fn default() -> Self {
        Self(Arc::new(NoLocalization))
    }
}

impl std::fmt::Debug for Localization {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Localization(..)")
    }
}

#[cfg(test)]
mod tests {
    use super::{LocKey, Localization, Localizer};
    use pretty_assertions::assert_eq;

    struct One;

    impl Localizer for One {
        fn resolve(&self, key: &LocKey) -> Option<String> {
            (key.0 == "copper_chest.item.copper_chest").then(|| "Copper Chest".to_owned())
        }
    }

    #[test]
    fn the_default_resolves_nothing_and_draws_the_key() {
        let loc = Localization::default();
        assert_eq!(loc.resolve(&LocKey("a.b".to_owned())), None);
        assert_eq!(loc.text_for("a.b"), "a.b");
    }

    #[test]
    fn a_known_key_resolves_and_an_unknown_one_stays_verbatim() {
        let loc = Localization::new(One);
        assert_eq!(
            loc.text_for("copper_chest.item.copper_chest"),
            "Copper Chest"
        );
        assert_eq!(
            loc.text_for("copper_chest.item.copper_ingot"),
            "copper_chest.item.copper_ingot"
        );
    }
}
