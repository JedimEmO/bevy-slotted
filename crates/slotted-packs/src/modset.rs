//! Mod discovery and the pack layout.

use std::path::{Path, PathBuf};

use bevy::prelude::Resource;
use slotted_registry::{ModId, ModManifest, resolve_load_order};

use crate::ModError;

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
        // PHASE4-IMPL: B
        let _ = mods_dir;
        Ok(Self::default())
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
