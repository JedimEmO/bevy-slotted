//! Bevy assets packs loads purely to be told when a file changes: a script,
//! a RON data file. The bytes the lifecycle actually uses come from
//! [`crate::LayeredSource`], so these loaders keep no state.

use std::collections::HashMap;

use bevy::asset::{Asset, AssetLoader, LoadContext, UntypedHandle, io::Reader};
use bevy::prelude::{Resource, TypePath};
use slotted_script::ModId;

/// A `.lua` file.
#[derive(Asset, TypePath, Debug, Clone, PartialEq, Eq)]
pub struct ScriptAsset {
    /// The source text.
    pub source: String,
}

/// Loads [`ScriptAsset`] from `.lua`.
#[derive(Debug, Default, Clone, Copy, TypePath)]
pub struct ScriptLoader;

/// Why a text asset failed to load.
#[derive(Debug, thiserror::Error)]
pub enum TextLoadError {
    /// IO.
    #[error(transparent)]
    Io(#[from] std::io::Error),
    /// Not UTF-8.
    #[error(transparent)]
    Utf8(#[from] std::string::FromUtf8Error),
}

impl AssetLoader for ScriptLoader {
    type Asset = ScriptAsset;
    type Settings = ();
    type Error = TextLoadError;

    async fn load(
        &self,
        reader: &mut dyn Reader,
        _settings: &(),
        _ctx: &mut LoadContext<'_>,
    ) -> Result<ScriptAsset, TextLoadError> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        Ok(ScriptAsset {
            source: String::from_utf8(bytes)?,
        })
    }

    fn extensions(&self) -> &[&str] {
        &["lua"]
    }
}

/// A `.ron` file under `pack://data/`. Kept as text; the registry parses it.
#[derive(Asset, TypePath, Debug, Clone, PartialEq, Eq)]
pub struct DataFile {
    /// The text.
    pub source: String,
}

/// Loads [`DataFile`] from `.ron`. Bevy picks the longest matching extension,
/// so `*.theme.ron` still goes to the theme loader.
#[derive(Debug, Default, Clone, Copy, TypePath)]
pub struct DataFileLoader;

impl AssetLoader for DataFileLoader {
    type Asset = DataFile;
    type Settings = ();
    type Error = TextLoadError;

    async fn load(
        &self,
        reader: &mut dyn Reader,
        _settings: &(),
        _ctx: &mut LoadContext<'_>,
    ) -> Result<DataFile, TextLoadError> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        Ok(DataFile {
            source: String::from_utf8(bytes)?,
        })
    }

    fn extensions(&self) -> &[&str] {
        &["ron"]
    }
}

/// Every handle the lifecycle holds so the watcher reports changes, by mod.
/// `AssetEvent::Modified` on any of them turns into a [`crate::ReloadMod`].
#[derive(Debug, Default, Resource)]
pub struct ModWatch {
    /// Handles per mod.
    pub by_mod: HashMap<ModId, Vec<UntypedHandle>>,
}

impl ModWatch {
    /// The mod a handle belongs to.
    pub fn owner(&self, handle: &UntypedHandle) -> Option<&ModId> {
        self.owner_of(handle.id())
    }

    /// The mod an asset id belongs to.
    pub fn owner_of(&self, id: bevy::asset::UntypedAssetId) -> Option<&ModId> {
        self.by_mod
            .iter()
            .find(|(_, handles)| handles.iter().any(|h| h.id() == id))
            .map(|(id, _)| id)
    }
}
