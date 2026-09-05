//! One logical path over several physical roots.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use bevy::asset::io::{AssetReader, AssetReaderError, PathStream, Reader, VecReader};
use bevy::prelude::Resource;
use bevy::tasks::futures_lite::stream;
use slotted_registry::{AssetSource, SourceError};

use crate::PackLayout;

/// The Bevy asset source id: `pack://textures/gui/chest.png`.
pub const PACK_SOURCE: &str = "pack";

/// An [`AssetSource`] that can be shared between the lifecycle, the
/// `pack://` reader and whoever built it.
pub type SharedSource = Arc<dyn AssetSource + Send + Sync>;

/// The source [`ModLoader`](crate::ModLoader) reads mods through.
///
/// Absent, the loader builds a [`LayeredSource`] over the [`PackLayout`], which
/// is the native path and the default. A host with no usable filesystem
/// (`wasm32-unknown-unknown`, a dedicated server reading a bundle) inserts this
/// resource instead, typically over a
/// [`InMemorySource`](slotted_registry::InMemorySource) built at compile time.
///
/// The logical paths are exactly the ones [`LayeredSource`] serves:
/// `data/<mod>/items/*.ron`, `scripts/<mod>/<entry>`, `mods/<mod>/mod.toml`,
/// `locale/<lang>.ftl` and, for a source that cannot layer several roots onto
/// one path, the per-mod form `locale/<mod>/<lang>.ftl`.
#[derive(Resource, Clone)]
pub struct PackAssets(pub SharedSource);

impl PackAssets {
    /// Over any source.
    pub fn new(source: impl AssetSource + Send + Sync + 'static) -> Self {
        Self(Arc::new(source))
    }

    /// The source behind the resource.
    pub fn source(&self) -> &(dyn AssetSource + Send + Sync) {
        &*self.0
    }
}

impl std::fmt::Debug for PackAssets {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PackAssets(..)")
    }
}

/// [`AssetSource`] for the registry's data stage. `read` returns the first
/// root that has the file, in [`PackLayout::roots`] order; `list` is the
/// union of every root's listing.
#[derive(Debug, Clone, PartialEq)]
pub struct LayeredSource {
    roots: Vec<PathBuf>,
}

/// Joins a logical `a/b/c` onto `root`, refusing anything that climbs out.
///
/// Mods ship the paths inside their own files, so a `..` here is a bug or an
/// attack; either way it must not reach the user's home directory.
fn join_checked(root: &Path, path: &str) -> Option<PathBuf> {
    let mut out = root.to_path_buf();
    for segment in path.split('/').filter(|s| !s.is_empty() && *s != ".") {
        if segment == ".." || segment.contains('\\') {
            return None;
        }
        out.push(segment);
    }
    Some(out)
}

impl LayeredSource {
    /// Over `layout.roots()`.
    pub fn new(layout: &PackLayout) -> Self {
        Self {
            roots: layout.roots(),
        }
    }

    /// Over explicit roots, highest priority first.
    pub fn from_roots(roots: Vec<PathBuf>) -> Self {
        Self { roots }
    }

    /// The roots, highest priority first.
    pub fn roots(&self) -> &[PathBuf] {
        &self.roots
    }

    /// Which physical file `path` resolves to, if any.
    pub fn resolve(&self, path: &str) -> Option<PathBuf> {
        self.roots.iter().find_map(|root| {
            let candidate = join_checked(root, path)?;
            candidate.is_file().then_some(candidate)
        })
    }

    /// Whether any root has `path` as a directory.
    pub fn is_dir(&self, path: &str) -> bool {
        self.roots
            .iter()
            .any(|root| join_checked(root, path).is_some_and(|candidate| candidate.is_dir()))
    }

    /// Every direct child of `path`, files and directories alike, as logical
    /// paths, deduplicated across roots and sorted.
    ///
    /// [`AssetSource::list`] is files only, because that is all the data stage
    /// needs; Bevy's `read_directory` wants both.
    pub fn entries(&self, path: &str) -> Vec<String> {
        let prefix = path.trim_end_matches('/');
        let mut names = BTreeSet::new();
        for root in &self.roots {
            let Some(dir) = join_checked(root, path) else {
                continue;
            };
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                if let Some(name) = entry.file_name().to_str() {
                    names.insert(name.to_owned());
                }
            }
        }
        names
            .into_iter()
            .map(|name| {
                if prefix.is_empty() {
                    name
                } else {
                    format!("{prefix}/{name}")
                }
            })
            .collect()
    }
}

impl AssetSource for LayeredSource {
    fn list(&self, dir: &str) -> Result<Vec<String>, SourceError> {
        let prefix = dir.trim_end_matches('/');
        let mut names = BTreeSet::new();
        for root in &self.roots {
            let Some(resolved) = join_checked(root, dir) else {
                return Err(SourceError::Io {
                    path: dir.to_owned(),
                    message: "path escapes the pack roots".to_owned(),
                });
            };
            let entries = match std::fs::read_dir(&resolved) {
                Ok(entries) => entries,
                // "this mod ships no recipes" is the common case, not an error.
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => continue,
                Err(err) => {
                    return Err(SourceError::Io {
                        path: dir.to_owned(),
                        message: err.to_string(),
                    });
                }
            };
            for entry in entries {
                let entry = entry.map_err(|err| SourceError::Io {
                    path: dir.to_owned(),
                    message: err.to_string(),
                })?;
                let is_file = entry
                    .file_type()
                    .map_err(|err| SourceError::Io {
                        path: dir.to_owned(),
                        message: err.to_string(),
                    })?
                    .is_file();
                if !is_file {
                    continue;
                }
                if let Some(name) = entry.file_name().to_str() {
                    names.insert(name.to_owned());
                }
            }
        }
        Ok(names
            .into_iter()
            .map(|name| {
                if prefix.is_empty() {
                    name
                } else {
                    format!("{prefix}/{name}")
                }
            })
            .collect())
    }

    fn read(&self, path: &str) -> Result<Vec<u8>, SourceError> {
        for root in &self.roots {
            let Some(candidate) = join_checked(root, path) else {
                return Err(SourceError::Io {
                    path: path.to_owned(),
                    message: "path escapes the pack roots".to_owned(),
                });
            };
            match std::fs::read(&candidate) {
                Ok(bytes) => return Ok(bytes),
                // Not in this root; the next one down may have it.
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
                Err(err) => {
                    return Err(SourceError::Io {
                        path: path.to_owned(),
                        message: err.to_string(),
                    });
                }
            }
        }
        Err(SourceError::NotFound(path.to_owned()))
    }
}

/// The `pack://` [`AssetReader`]: the same layering for textures, themes,
/// screens, scripts and locale files.
#[derive(Debug, Clone)]
pub struct LayeredAssetReader {
    source: LayeredSource,
}

impl LayeredAssetReader {
    /// Over `layout.roots()`.
    pub fn new(layout: &PackLayout) -> Self {
        Self {
            source: LayeredSource::new(layout),
        }
    }

    /// The underlying source.
    pub const fn source(&self) -> &LayeredSource {
        &self.source
    }

    pub(crate) fn logical(path: &Path) -> String {
        path.to_string_lossy().replace('\\', "/")
    }
}

impl AssetReader for LayeredAssetReader {
    async fn read<'a>(&'a self, path: &'a Path) -> Result<impl Reader + 'a, AssetReaderError> {
        let logical = Self::logical(path);
        match self.source.read(&logical) {
            Ok(bytes) => Ok(VecReader::new(bytes)),
            Err(SourceError::NotFound(_)) => Err(AssetReaderError::NotFound(path.to_path_buf())),
            Err(SourceError::Io { message, .. }) => {
                Err(AssetReaderError::Io(std::io::Error::other(message).into()))
            }
        }
    }

    async fn read_meta<'a>(&'a self, path: &'a Path) -> Result<impl Reader + 'a, AssetReaderError> {
        // Packs ship no `.meta` files.
        Err::<VecReader, _>(AssetReaderError::NotFound(path.to_path_buf()))
    }

    async fn read_directory<'a>(
        &'a self,
        path: &'a Path,
    ) -> Result<Box<PathStream>, AssetReaderError> {
        let logical = Self::logical(path);
        if !logical.is_empty() && !self.source.is_dir(&logical) {
            return Err(AssetReaderError::NotFound(path.to_path_buf()));
        }
        let entries: Vec<PathBuf> = self
            .source
            .entries(&logical)
            .into_iter()
            .map(PathBuf::from)
            .collect();
        Ok(Box::new(stream::iter(entries)))
    }

    async fn is_directory<'a>(&'a self, path: &'a Path) -> Result<bool, AssetReaderError> {
        Ok(self.source.is_dir(&Self::logical(path)))
    }
}

/// The `pack://` [`AssetReader`] over any [`AssetSource`].
///
/// [`LayeredAssetReader`] is the filesystem form; this is the one a host with
/// no filesystem registers, so `pack://` resolves against the same
/// [`PackAssets`] the loader reads mods through and a `ScriptAsset` handle
/// still points at real bytes.
#[derive(Clone)]
pub struct SourceAssetReader {
    source: SharedSource,
}

impl SourceAssetReader {
    /// Over `source`.
    pub fn new(source: SharedSource) -> Self {
        Self { source }
    }
}

impl std::fmt::Debug for SourceAssetReader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SourceAssetReader(..)")
    }
}

impl AssetReader for SourceAssetReader {
    async fn read<'a>(&'a self, path: &'a Path) -> Result<impl Reader + 'a, AssetReaderError> {
        let logical = LayeredAssetReader::logical(path);
        match self.source.read(&logical) {
            Ok(bytes) => Ok(VecReader::new(bytes)),
            Err(SourceError::NotFound(_)) => Err(AssetReaderError::NotFound(path.to_path_buf())),
            Err(SourceError::Io { message, .. }) => {
                Err(AssetReaderError::Io(std::io::Error::other(message).into()))
            }
        }
    }

    async fn read_meta<'a>(&'a self, path: &'a Path) -> Result<impl Reader + 'a, AssetReaderError> {
        Err::<VecReader, _>(AssetReaderError::NotFound(path.to_path_buf()))
    }

    async fn read_directory<'a>(
        &'a self,
        path: &'a Path,
    ) -> Result<Box<PathStream>, AssetReaderError> {
        let logical = LayeredAssetReader::logical(path);
        let entries: Vec<PathBuf> = self
            .source
            .list(&logical)
            .unwrap_or_default()
            .into_iter()
            .map(PathBuf::from)
            .collect();
        Ok(Box::new(stream::iter(entries)))
    }

    async fn is_directory<'a>(&'a self, path: &'a Path) -> Result<bool, AssetReaderError> {
        let logical = LayeredAssetReader::logical(path);
        // The port lists files only, so a directory is one that lists something.
        Ok(!self.source.list(&logical).unwrap_or_default().is_empty())
    }
}
