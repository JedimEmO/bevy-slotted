//! Fluent localisation, layered by load order. Contract section 2.7.

use bevy::asset::{Asset, AssetLoader, LoadContext, io::Reader};
use bevy::prelude::*;
use fluent_bundle::FluentArgs;
use fluent_bundle::FluentResource;
use fluent_bundle::concurrent::FluentBundle;
use slotted_script::ModId;
use slotted_ui::LocText;
use slotted_ui::def::LocKey;
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
#[derive(Resource)]
pub struct Locales {
    /// The negotiated language.
    pub lang: LanguageIdentifier,
    /// `(owner, bundle)`; `None` owner is base.
    pub layers: Vec<(Option<ModId>, FluentBundle<FluentResource>)>,
}

impl Locales {
    /// Empty, for `lang`.
    pub fn new(lang: LanguageIdentifier) -> Self {
        Self {
            lang,
            layers: Vec::new(),
        }
    }

    /// Parses `source` into a bundle layered **above** everything present.
    ///
    /// # Errors
    ///
    /// The first Fluent parse error, rendered.
    pub fn push_layer(&mut self, owner: Option<ModId>, source: String) -> Result<(), String> {
        // PHASE4-IMPL: B
        let _ = (owner, source);
        Ok(())
    }

    /// The first layer that has `key`, formatted with `args`.
    pub fn resolve(&self, key: &LocKey, args: Option<&FluentArgs<'_>>) -> Option<String> {
        // PHASE4-IMPL: B
        let _ = (key, args);
        None
    }
}

impl Default for Locales {
    fn default() -> Self {
        Self::new(unic_langid::langid!("en-US"))
    }
}

/// Writes `Text` for every new `LocText`, and for all of them when `Locales`
/// changed. Unresolved keys stay verbatim. Runs in `SlottedUiSet::Render`.
pub fn resolve_loc_text(
    locales: Res<Locales>,
    mut texts: Query<(Entity, &LocText, &mut Text)>,
    fresh: Query<Entity, Added<LocText>>,
) {
    // PHASE4-IMPL: B -- if locales.is_changed() rewrite all, else only `fresh`.
    let _ = (&locales, &mut texts, &fresh);
}
