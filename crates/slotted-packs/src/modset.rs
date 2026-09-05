//! Mod discovery and the pack layout.

use std::path::{Path, PathBuf};

use bevy::prelude::Resource;
use slotted_registry::{AssetSource, ManifestError, ModId, ModManifest, resolve_load_order};

use crate::ModError;

/// The file every mod directory must contain.
pub const MANIFEST_FILE: &str = "mod.toml";

/// Where one mod's parts live on disk. Everything the lifecycle needs from a
/// [`ModEntry`] without touching the manifest again.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModPaths {
    /// The directory holding `mod.toml`.
    pub root: PathBuf,
    /// `root/<manifest.assets>` or `root`.
    pub assets: PathBuf,
    /// `root/<entry.data>`.
    pub data: Option<PathBuf>,
    /// `root/<entry.control>`.
    pub control: Option<PathBuf>,
    /// `root/locale`.
    pub locale: PathBuf,
}

/// One discovered mod.
#[derive(Debug, Clone, PartialEq)]
pub struct ModEntry {
    /// Parsed `mod.toml`.
    pub manifest: ModManifest,
    /// The directory holding `mod.toml`.
    pub root: PathBuf,
}

impl ModEntry {
    /// The id.
    pub fn id(&self) -> &ModId {
        &self.manifest.id
    }

    /// Where this mod's assets live: `root/<manifest.assets>` or `root`.
    pub fn asset_root(&self) -> PathBuf {
        match &self.manifest.assets {
            Some(sub) => self.root.join(sub),
            None => self.root.clone(),
        }
    }

    /// `root/<entry.data>` if the manifest names a data script.
    pub fn data_script(&self) -> Option<PathBuf> {
        self.manifest.entry.data.as_ref().map(|p| self.root.join(p))
    }

    /// `root/<entry.control>` if the manifest names a control script.
    pub fn control_script(&self) -> Option<PathBuf> {
        self.manifest
            .entry
            .control
            .as_ref()
            .map(|p| self.root.join(p))
    }

    /// Where this mod's `<lang>.ftl` files live.
    pub fn locale_dir(&self) -> PathBuf {
        self.root.join("locale")
    }

    /// Everything the lifecycle reads, resolved once.
    pub fn paths(&self) -> ModPaths {
        ModPaths {
            root: self.root.clone(),
            assets: self.asset_root(),
            data: self.data_script(),
            control: self.control_script(),
            locale: self.locale_dir(),
        }
    }
}

/// A `mod.toml` that could not be used, as a [`ManifestError`].
fn manifest_error(path: &Path, message: impl Into<String>) -> ModError {
    ModError::Manifest(ManifestError {
        path: path.to_string_lossy().into_owned(),
        message: message.into(),
    })
}

/// The mods to load, in load order.
#[derive(Debug, Clone, PartialEq, Default, Resource)]
pub struct ModSet {
    mods: Vec<ModEntry>,
}

impl ModSet {
    /// Reads every `<mods_dir>/*/mod.toml` and sorts by dependencies.
    ///
    /// # Errors
    ///
    /// [`ModError::Manifest`], [`ModError::LoadOrder`], or [`ModError::Io`]
    /// for an unreadable directory. A missing `mods_dir` is an empty set.
    pub fn discover(mods_dir: &Path) -> Result<Self, ModError> {
        let entries = match std::fs::read_dir(mods_dir) {
            Ok(entries) => entries,
            // No `mods/` at all is a game with no mods installed, not a fault.
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(err) => {
                return Err(ModError::Io {
                    path: mods_dir.to_string_lossy().into_owned(),
                    message: err.to_string(),
                });
            }
        };

        let mut dirs: Vec<PathBuf> = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|err| ModError::Io {
                path: mods_dir.to_string_lossy().into_owned(),
                message: err.to_string(),
            })?;
            let is_dir = entry
                .file_type()
                .map_err(|err| ModError::Io {
                    path: mods_dir.to_string_lossy().into_owned(),
                    message: err.to_string(),
                })?
                .is_dir();
            if is_dir {
                dirs.push(entry.path());
            }
        }
        // `read_dir` order is filesystem order; sorting keeps discovery, and
        // therefore every error message and tie-break, reproducible.
        dirs.sort();

        let mut parsed = Vec::with_capacity(dirs.len());
        for dir in dirs {
            let manifest_path = dir.join(MANIFEST_FILE);
            let text = match std::fs::read_to_string(&manifest_path) {
                Ok(text) => text,
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                    return Err(manifest_error(
                        &manifest_path,
                        "no mod.toml in this directory",
                    ));
                }
                Err(err) => {
                    return Err(ModError::Io {
                        path: manifest_path.to_string_lossy().into_owned(),
                        message: err.to_string(),
                    });
                }
            };
            parsed.push((dir, manifest_path, text));
        }

        Self::from_texts(parsed)
    }

    /// Discovery through an [`AssetSource`], for hosts with no filesystem.
    ///
    /// The port lists files, never directories, so the caller names the mod
    /// directories; `<mods_dir>/<id>/mod.toml` is read for each.
    ///
    /// # Errors
    ///
    /// As [`discover`](Self::discover).
    pub fn discover_in(
        source: &dyn AssetSource,
        mods_dir: &str,
        ids: impl IntoIterator<Item = String>,
    ) -> Result<Self, ModError> {
        let mut parsed = Vec::new();
        for id in ids {
            let dir = PathBuf::from(mods_dir).join(&id);
            let logical = format!("{}/{id}/{MANIFEST_FILE}", mods_dir.trim_end_matches('/'));
            let manifest_path = dir.join(MANIFEST_FILE);
            let bytes = source
                .read(&logical)
                .map_err(|err| manifest_error(&manifest_path, err.to_string()))?;
            let text = String::from_utf8(bytes)
                .map_err(|err| manifest_error(&manifest_path, err.to_string()))?;
            parsed.push((dir, manifest_path, text));
        }
        Self::from_texts(parsed)
    }

    /// Parses every `(dir, manifest path, text)` and orders the result.
    fn from_texts(parsed: Vec<(PathBuf, PathBuf, String)>) -> Result<Self, ModError> {
        let mut manifests = Vec::with_capacity(parsed.len());
        for (dir, manifest_path, text) in parsed {
            let display = manifest_path.to_string_lossy().into_owned();
            let manifest = ModManifest::parse(&display, &text)?;
            // The directory name is how a modder finds the mod again, and how
            // `mods/<id>/data/<id>/` lines up. A mismatch is always a mistake.
            let dir_name = dir.file_name().unwrap_or_default().to_string_lossy();
            if manifest.id.as_str() != dir_name {
                return Err(manifest_error(
                    &manifest_path,
                    format!(
                        "id `{}` does not match the directory name `{dir_name}`",
                        manifest.id
                    ),
                ));
            }
            manifests.push((dir, manifest));
        }
        Self::from_manifests(manifests)
    }

    /// Builds a set from parsed manifests, sorted by dependencies.
    ///
    /// # Errors
    ///
    /// [`ModError::LoadOrder`].
    pub fn from_manifests(entries: Vec<(PathBuf, ModManifest)>) -> Result<Self, ModError> {
        let manifests: Vec<ModManifest> = entries.iter().map(|(_, m)| m.clone()).collect();
        let order = resolve_load_order(&manifests)?;
        let mut mods = Vec::with_capacity(entries.len());
        for id in &order {
            if let Some((root, manifest)) = entries.iter().find(|(_, m)| &m.id == id) {
                mods.push(ModEntry {
                    manifest: manifest.clone(),
                    root: root.clone(),
                });
            }
        }
        Ok(Self { mods })
    }

    /// Mods in load order.
    pub fn iter(&self) -> impl DoubleEndedIterator<Item = &ModEntry> + ExactSizeIterator {
        self.mods.iter()
    }

    /// Ids in load order.
    pub fn load_order(&self) -> Vec<ModId> {
        self.mods.iter().map(|m| m.manifest.id.clone()).collect()
    }

    /// The entry for `id`.
    pub fn get(&self, id: &ModId) -> Option<&ModEntry> {
        self.mods.iter().find(|m| &m.manifest.id == id)
    }

    /// How many.
    pub fn len(&self) -> usize {
        self.mods.len()
    }

    /// Whether there are none.
    pub fn is_empty(&self) -> bool {
        self.mods.is_empty()
    }
}

/// Where everything is read from.
#[derive(Debug, Clone, PartialEq, Resource)]
pub struct PackLayout {
    /// The game's own assets (`assets/`).
    pub base: PathBuf,
    /// Discovered mods.
    pub mods: ModSet,
    /// Resource pack roots; the last listed wins.
    pub resource_packs: Vec<PathBuf>,
}

impl PackLayout {
    /// A layout with `base` and no mods or packs.
    pub fn new(base: impl Into<PathBuf>) -> Self {
        Self {
            base: base.into(),
            mods: ModSet::default(),
            resource_packs: Vec::new(),
        }
    }

    /// Discovers `mods_dir` into the layout.
    ///
    /// # Errors
    ///
    /// As [`ModSet::discover`].
    pub fn with_mods(mut self, mods_dir: &Path) -> Result<Self, ModError> {
        self.mods = ModSet::discover(mods_dir)?;
        Ok(self)
    }

    /// Physical roots in **lookup** order: packs (last first), mods (reverse
    /// load order), base.
    pub fn roots(&self) -> Vec<PathBuf> {
        let mut roots: Vec<PathBuf> = self.resource_packs.iter().rev().cloned().collect();
        roots.extend(self.mods.iter().rev().map(ModEntry::asset_root));
        roots.push(self.base.clone());
        roots
    }
}
