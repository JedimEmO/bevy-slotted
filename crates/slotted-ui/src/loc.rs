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

use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;

use bevy::ecs::resource::Resource;
use bevy::prelude::*;

use crate::def::LocKey;
use crate::semantic::LocText;

/// Resolves a localisation key against whatever catalogue the host loaded.
///
/// Implementations are shared across threads and read-only; the resource is
/// replaced rather than mutated when the catalogue changes, so a system can
/// use `Res<Localization>`'s change detection to know that every string it
/// drew is stale.
pub trait Localizer: Send + Sync + 'static {
    /// The display text for `key` with `args` substituted, or `None` when
    /// nothing defines it (menus M1 contract 2.3).
    fn resolve(&self, key: &LocKey, args: &LocArgs) -> Option<String>;
}

/// Arguments to a localised string: Fluent variables by name.
pub type LocArgs = BTreeMap<String, crate::values::Value>;

/// No arguments.
pub fn no_args() -> &'static LocArgs {
    static EMPTY: std::sync::OnceLock<LocArgs> = std::sync::OnceLock::new();
    EMPTY.get_or_init(BTreeMap::new)
}

/// The [`Localizer`] that resolves nothing, so every key is drawn as written.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoLocalization;

impl Localizer for NoLocalization {
    fn resolve(&self, _key: &LocKey, _args: &LocArgs) -> Option<String> {
        None
    }
}

/// The active [`Localizer`], plus fallbacks (menus M2 contract 2.1).
///
/// `slotted-packs` sets the primary over the mods' `.ftl` layers at the end
/// of each load and each reload through [`set_primary`](Self::set_primary),
/// which keeps the fallbacks; a library crate that ships English defaults
/// for its own keys pushes one with [`push_fallback`](Self::push_fallback).
/// Everything else reads it.
#[derive(Resource, Clone)]
pub struct Localization {
    /// The catalogue asked first.
    pub primary: Arc<dyn Localizer>,
    /// Asked in order when the primary has no answer.
    pub fallbacks: Vec<Arc<dyn Localizer>>,
}

impl Localization {
    /// Wraps `localizer` as the primary, with no fallbacks.
    pub fn new(localizer: impl Localizer) -> Self {
        Self::from_arc(Arc::new(localizer))
    }

    /// Wraps a shared localizer as the primary, with no fallbacks.
    pub fn from_arc(localizer: Arc<dyn Localizer>) -> Self {
        Self {
            primary: localizer,
            fallbacks: Vec::new(),
        }
    }

    /// Replaces the primary and keeps the fallbacks.
    pub fn set_primary(&mut self, localizer: impl Localizer) {
        self.primary = Arc::new(localizer);
    }

    /// Replaces the primary with a shared localizer and keeps the fallbacks.
    pub fn set_primary_arc(&mut self, localizer: Arc<dyn Localizer>) {
        self.primary = localizer;
    }

    /// Adds a fallback after the existing ones.
    pub fn push_fallback(&mut self, localizer: impl Localizer) {
        self.fallbacks.push(Arc::new(localizer));
    }

    /// The display text for `key`, or `None` when nothing defines it.
    pub fn resolve(&self, key: &LocKey) -> Option<String> {
        self.resolve_with(key, no_args())
    }

    /// The display text for `key` with `args`, or `None`: the primary, then
    /// each fallback in order.
    pub fn resolve_with(&self, key: &LocKey, args: &LocArgs) -> Option<String> {
        self.primary
            .resolve(key, args)
            .or_else(|| self.fallbacks.iter().find_map(|f| f.resolve(key, args)))
    }

    /// The display text for `key`, falling back to the key itself.
    ///
    /// This is what a label draws: an unresolved key on screen is a legible
    /// bug report, where an empty label is not.
    pub fn text(&self, key: &LocKey) -> String {
        self.resolve(key).unwrap_or_else(|| key.0.clone())
    }

    /// [`text`](Self::text) with arguments.
    pub fn text_with(&self, key: &LocKey, args: &LocArgs) -> String {
        self.resolve_with(key, args)
            .unwrap_or_else(|| key.0.clone())
    }

    /// [`text`](Self::text) for a key that is only a `&str`.
    pub fn text_for(&self, key: &str) -> String {
        self.text(&LocKey(key.to_owned()))
    }
}

impl Default for Localization {
    fn default() -> Self {
        Self::new(NoLocalization)
    }
}

impl std::fmt::Debug for Localization {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Localization(..)")
    }
}

/// `SlottedUiSet::Render`: writes `Text` for every new [`LocText`], and for
/// all of them when [`Localization`] is replaced.
///
/// This lives here, beside the component and the port, rather than in the
/// crate that happens to load Fluent files. A game with its own catalogue
/// swaps the resource and expects its labels to repaint; before this system
/// moved, that repaint only happened when `slotted-packs` was in the app, so
/// a screen built on `slotted-ui` alone showed whatever the catalogue said at
/// spawn time for ever.
///
/// A key nothing defines is drawn as written, which is what makes an
/// unresolved key a legible bug report rather than an empty label. That also
/// applies to a key that *stops* resolving: switching to a catalogue that has
/// lost an entry puts the key back rather than leaving the previous
/// language's text on screen.
pub fn resolve_loc_text(
    locales: Res<Localization>,
    mut texts: Query<(Entity, &LocText, &mut Text)>,
    fresh: Query<Entity, Or<(Added<LocText>, Changed<LocText>)>>,
) {
    let all = locales.is_changed();
    // Two `&mut Text` queries would conflict, so the freshly spawned (or
    // re-argued) entities arrive as a set of ids and the write goes through
    // the one query.
    let fresh: HashSet<Entity> = if all {
        HashSet::new()
    } else {
        fresh.iter().collect()
    };
    if !all && fresh.is_empty() {
        return;
    }
    for (entity, key, mut text) in &mut texts {
        if !all && !fresh.contains(&entity) {
            continue;
        }
        let want = locales.text_with(&key.key, &key.args);
        if text.0 != want {
            text.0 = want;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{LocArgs, LocKey, Localization, Localizer};
    use pretty_assertions::assert_eq;

    struct One;

    impl Localizer for One {
        fn resolve(&self, key: &LocKey, _args: &LocArgs) -> Option<String> {
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
