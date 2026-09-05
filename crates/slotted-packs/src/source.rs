//! One logical path over several physical roots.

use std::path::{Path, PathBuf};

use bevy::asset::io::{AssetReader, AssetReaderError, PathStream, Reader, VecReader};
use slotted_registry::{AssetSource, SourceError};

use crate::PackLayout;

/// The Bevy asset source id: `pack://textures/gui/chest.png`.
pub const PACK_SOURCE: &str = "pack";

/// [`AssetSource`] for the registry's data stage. `read` returns the first
/// root that has the file, in [`PackLayout::roots`] order; `list` is the
/// union of every root's listing.
#[derive(Debug, Clone, PartialEq)]
pub struct LayeredSource {
    roots: Vec<PathBuf>,
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
        // PHASE4-IMPL: B
        let _ = path;
        None
    }
}

impl AssetSource for LayeredSource {
    fn list(&self, dir: &str) -> Result<Vec<String>, SourceError> {
        // PHASE4-IMPL: B -- union over roots, deduplicated, logical paths.
        let _ = dir;
        Ok(Vec::new())
    }

    fn read(&self, path: &str) -> Result<Vec<u8>, SourceError> {
        // PHASE4-IMPL: B
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

    fn logical(path: &Path) -> String {
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
        // PHASE4-IMPL: B -- stream `source.list` as PathBufs.
        Err(AssetReaderError::NotFound(path.to_path_buf()))
    }

    async fn is_directory<'a>(&'a self, path: &'a Path) -> Result<bool, AssetReaderError> {
        // PHASE4-IMPL: B
        let _ = path;
        Ok(false)
    }
}
