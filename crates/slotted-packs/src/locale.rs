//! Fluent localisation, layered by load order. Contract section 2.7.

use std::collections::HashSet;
use std::ops::Deref;
use std::sync::{Arc, Mutex};

use bevy::asset::{Asset, AssetLoader, LoadContext, io::Reader};
use bevy::prelude::*;
use fluent_bundle::FluentArgs;
use fluent_bundle::FluentResource;
use fluent_bundle::concurrent::FluentBundle;
use slotted_script::ModId;
use slotted_ui::def::LocKey;
use slotted_ui::{LocText, Localization, Localizer};
use unic_langid::LanguageIdentifier;

use crate::assets::TextLoadError;

/// A `.ftl` file as text.
#[derive(Asset, TypePath, Debug, Clone, PartialEq, Eq)]
pub struct FtlAsset {
    /// The source.
    pub source: String,
}

/// Loads [`FtlAsset`] from `.ftl`.
#[derive(Debug, Default, Clone, Copy, TypePath)]
pub struct FtlLoader;

impl AssetLoader for FtlLoader {
    type Asset = FtlAsset;
    type Settings = ();
    type Error = TextLoadError;

    async fn load(
        &self,
        reader: &mut dyn Reader,
        _settings: &(),
        _ctx: &mut LoadContext<'_>,
    ) -> Result<FtlAsset, TextLoadError> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        Ok(FtlAsset {
            source: String::from_utf8(bytes)?,
        })
    }

    fn extensions(&self) -> &[&str] {
        &["ftl"]
    }
}

/// Every loaded bundle, searched in **reverse load order**, base last.
///
/// Shared behind an `Arc` so the same catalogue can be the [`Locales`]
/// resource and the [`Localization`] port `slotted-ui` and `slotted-browser`
/// read, without parsing every `.ftl` twice.
pub struct LocaleTable {
    /// The negotiated language.
    pub lang: LanguageIdentifier,
    /// `(owner, bundle)`; `None` owner is base.
    pub layers: Vec<(Option<ModId>, FluentBundle<FluentResource>)>,
    /// Keys already reported missing, so the warning is once per key and not
    /// once per frame per text node.
    missing: Mutex<HashSet<String>>,
}

impl LocaleTable {
    /// Empty, for `lang`.
    pub fn new(lang: LanguageIdentifier) -> Self {
        Self {
            lang,
            layers: Vec::new(),
            missing: Mutex::new(HashSet::new()),
        }
    }

    /// Parses `source` into a bundle layered **above** everything present.
    ///
    /// Message ids are normalised first ([`normalise_ids`]), so the contract's
    /// dotted convention key works in an `.ftl` file.
    ///
    /// # Errors
    ///
    /// The first Fluent parse error, rendered.
    pub fn push_layer(&mut self, owner: Option<ModId>, source: String) -> Result<(), String> {
        let resource = FluentResource::try_new(normalise_ids(&source)).map_err(|(_, errors)| {
            errors
                .first()
                .map_or_else(|| "unparsable ftl".to_owned(), ToString::to_string)
        })?;
        let mut bundle = FluentBundle::new_concurrent(vec![self.lang.clone()]);
        // Fluent wraps placeables in directionality isolation marks by
        // default, which would show up in every snapshot and locator.
        bundle.set_use_isolating(false);
        bundle.add_resource(resource).map_err(|errors| {
            errors
                .first()
                .map_or_else(|| "unusable ftl".to_owned(), ToString::to_string)
        })?;
        self.layers.insert(0, (owner, bundle));
        Ok(())
    }

    /// Drops every layer, keeping the language.
    pub fn clear(&mut self) {
        self.layers.clear();
        if let Ok(mut missing) = self.missing.lock() {
            missing.clear();
        }
    }

    /// The first layer that has `key`, formatted with `args`.
    ///
    /// The key is tried as written and then in its normalised form, so a
    /// `LocKey` may use the contract's dotted convention against an `.ftl`
    /// written either way.
    pub fn resolve(&self, key: &LocKey, args: Option<&FluentArgs<'_>>) -> Option<String> {
        let normalised = normalise_id(&key.0);
        for (_, bundle) in &self.layers {
            let message = bundle
                .get_message(&key.0)
                .or_else(|| bundle.get_message(&normalised));
            let Some(pattern) = message.and_then(|message| message.value()) else {
                continue;
            };
            let mut errors = Vec::new();
            let text = bundle.format_pattern(pattern, args, &mut errors);
            for error in &errors {
                tracing::warn!(key = %key.0, %error, "fluent formatting error");
            }
            return Some(text.into_owned());
        }
        None
    }

    /// `resolve`, logging once for a key nothing defines.
    fn resolve_or_warn(&self, key: &LocKey) -> Option<String> {
        if let Some(text) = self.resolve(key, None) {
            return Some(text);
        }
        if let Ok(mut missing) = self.missing.lock()
            && missing.insert(key.0.clone())
        {
            tracing::warn!(key = %key.0, "no locale layer defines this key");
        }
        None
    }
}

impl Default for LocaleTable {
    fn default() -> Self {
        Self::new(unic_langid::langid!("en-US"))
    }
}

impl Localizer for LocaleTable {
    /// The port `slotted-ui` and `slotted-browser` resolve through. It warns
    /// once per key that nothing defines, the same as a `LocText` does, so a
    /// browser card and a screen label report a missing key identically.
    fn resolve(&self, key: &LocKey) -> Option<String> {
        self.resolve_or_warn(key)
    }
}

/// The loaded locale catalogue, as a resource.
///
/// A handle on the same [`LocaleTable`] the [`Localization`] port holds, so
/// `Locales` and the port never disagree. Every method of the table is
/// reachable through `Deref`.
#[derive(Resource, Clone)]
pub struct Locales(pub Arc<LocaleTable>);

impl Locales {
    /// Wraps `table`.
    pub fn new(table: LocaleTable) -> Self {
        Self(Arc::new(table))
    }

    /// The port over the same catalogue.
    pub fn port(&self) -> Localization {
        Localization(self.0.clone())
    }
}

impl Deref for Locales {
    type Target = LocaleTable;

    fn deref(&self) -> &LocaleTable {
        &self.0
    }
}

impl Default for Locales {
    fn default() -> Self {
        Self::new(LocaleTable::default())
    }
}

/// A Fluent identifier for `key`.
///
/// Contract 2.7's convention key is `copper_chest.item.copper_chest`, and a
/// Fluent identifier is `[A-Za-z][A-Za-z0-9_-]*`: the dots are not legal, and
/// one of them makes `FluentResource::try_new` reject the whole file with
/// "Expected a token starting with =". Dots become dashes, on both the parse
/// and the lookup side, so the documented convention works and a mod that
/// already writes dashes is unchanged.
pub fn normalise_id(key: &str) -> String {
    key.replace('.', "-")
}

/// Rewrites the identifier of every message and term definition in an `.ftl`
/// source with [`normalise_id`].
///
/// Only the identifier at the start of a definition line is touched: a value,
/// an indented continuation, an attribute and a comment are all left exactly as
/// the author wrote them.
pub fn normalise_ids(source: &str) -> String {
    let mut out = String::with_capacity(source.len());
    // `split_inclusive` keeps the newline, so nothing has to be re-added.
    for line in source.split_inclusive('\n') {
        let starts_definition = line
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic() || c == '-');
        match line.find('=').filter(|_| starts_definition) {
            Some(equals) => {
                out.push_str(&line[..equals].replace('.', "-"));
                out.push_str(&line[equals..]);
            }
            None => out.push_str(line),
        }
    }
    out
}

/// Writes `Text` for every new `LocText`, and for all of them when `Locales`
/// changed. Unresolved keys stay verbatim. Runs in `SlottedUiSet::Render`.
pub fn resolve_loc_text(
    locales: Res<Localization>,
    mut texts: Query<(Entity, &LocText, &mut Text)>,
    fresh: Query<Entity, Added<LocText>>,
) {
    let all = locales.is_changed();
    // Two `&mut Text` queries would conflict, so the freshly spawned entities
    // arrive as a set of ids and the write goes through the one query.
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
        if let Some(resolved) = locales.resolve(&key.0)
            && text.0 != resolved
        {
            text.0 = resolved;
        }
    }
}

/// Reads `locale/<lang>.ftl` from base and every mod, in load order, so the
/// last mod loaded wins.
pub(crate) fn load_locales(
    world: &mut World,
    layout: &crate::PackLayout,
    source: &crate::LayeredSource,
) {
    let lang = world
        .get_resource::<Locales>()
        .map_or_else(|| unic_langid::langid!("en-US"), |l| l.lang.clone());
    let mut locales = LocaleTable::new(lang.clone());
    let file = format!("locale/{lang}.ftl");

    // Base first, so a mod's layer sits above it.
    if let Ok(bytes) = std::fs::read(layout.base.join(&file))
        && let Ok(text) = String::from_utf8(bytes)
        && let Err(error) = locales.push_layer(None, text)
    {
        tracing::warn!(%error, "base locale file is not valid Fluent");
    }
    let _ = source;

    for entry in layout.mods.iter() {
        let path = entry.locale_dir().join(format!("{lang}.ftl"));
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        let Ok(text) = String::from_utf8(bytes) else {
            continue;
        };
        let owner = crate::lifecycle::script_mod_id(entry.id());
        if let Err(error) = locales.push_layer(Some(owner), text) {
            tracing::warn!(mod_id = %entry.id(), %error, "locale file is not valid Fluent");
        }
    }
    // The resource and the port are the one catalogue, so a browser card and a
    // `LocText` can never resolve the same key differently. Replacing the port
    // rather than mutating it is what lets a reader use change detection.
    let locales = Locales::new(locales);
    world.insert_resource(locales.port());
    world.insert_resource(locales);
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn only_definition_identifiers_are_normalised() {
        let source = "# a comment with a.dot
copper_chest.item.chest = Copper Chest
    .tooltip = holds a.lot
-brand.name = Copper
already-dashed = fine
";
        assert_eq!(
            normalise_ids(source),
            "# a comment with a.dot
copper_chest-item-chest = Copper Chest
    .tooltip = holds a.lot
-brand-name = Copper
already-dashed = fine
"
        );
    }

    #[test]
    fn a_dotted_key_resolves_against_a_dotted_ftl() {
        let mut locales = LocaleTable::default();
        locales
            .push_layer(None, "copper_chest.item.chest = Copper Chest\n".to_owned())
            .expect("the dots are normalised away");
        assert_eq!(
            locales.resolve(&LocKey("copper_chest.item.chest".to_owned()), None),
            Some("Copper Chest".to_owned())
        );
        // The dashed form a mod may already use keeps working, both ways round.
        assert_eq!(
            locales.resolve(&LocKey("copper_chest-item-chest".to_owned()), None),
            Some("Copper Chest".to_owned())
        );
        assert_eq!(
            locales.resolve(&LocKey("nobody.knows".to_owned()), None),
            None
        );
    }
}
