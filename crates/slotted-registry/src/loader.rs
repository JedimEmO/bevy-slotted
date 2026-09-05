//! The `AssetSource` port and the data stage that runs over it.
//!
//! This crate does no IO. It states what it needs from the outside world as
//! one small trait, [`AssetSource`], and ships two adapters: [`InMemorySource`]
//! for tests and the browser, and [`DirSource`] for a plain directory tree.
//! `slotted-packs` will add a third that layers mods and resource packs, and
//! the data stage will not notice.
//!
//! # Layout
//!
//! ```text
//! data/<modid>/items/*.ron
//! data/<modid>/tags/*.ron
//! data/<modid>/recipe_types/*.ron
//! data/<modid>/recipes/*.ron
//! data/<modid>/screens/*.ron            (and widgets, tooltip_components, ...)
//! data/<modid>/patches/base/*.ron
//! data/<modid>/patches/updates/*.ron
//! data/<modid>/patches/final_fixes/*.ron
//! ```
//!
//! Entry files are read once, in [`Round::Base`], in load order. Patch files
//! are read once per round, also in load order, so a mod that needs to see
//! everything everyone registered patches in [`Round::Updates`] and a
//! compatibility pack takes the last word in [`Round::FinalFixes`].

use std::collections::BTreeMap;

use slotted_model::Namespaced;

use crate::Value;
use crate::defs::{
    HudLayerDef, IngredientTypeDef, ItemDef, RecipeDef, RecipeTypeDef, ScreenDef, TagDef,
    TooltipComponentDef, WidgetDef,
};
use crate::manifest::ModId;
use crate::patch::{self, Patch, PatchError, PatchList, RawEntries, Round};
use crate::registry::{FrozenRegistries, Registries, RegistryError, RegistryKind, Warning};

/// Why an [`AssetSource`] could not answer.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SourceError {
    /// The path is not there. Listing a directory that does not exist is *not*
    /// this error: it is an empty list, because "this mod ships no recipes" is
    /// the normal case, not a failure.
    #[error("`{0}` not found")]
    NotFound(String),
    /// The source failed for its own reasons.
    #[error("`{path}`: {message}")]
    Io {
        /// What was being read.
        path: String,
        /// What went wrong.
        message: String,
    },
}

/// Where the data stage reads bytes from.
///
/// Deliberately two methods over `&str` paths and `Vec<u8>` bytes: the browser
/// has no filesystem, a pack reader resolves one logical path across several
/// physical roots, and a test wants a `BTreeMap`. Anything richer than this
/// would force all three to agree on something they cannot.
pub trait AssetSource {
    /// Lists the files directly inside `dir`, as full paths, in any order.
    ///
    /// A directory that does not exist lists empty.
    ///
    /// # Errors
    ///
    /// [`SourceError`] if the directory exists but cannot be read.
    fn list(&self, dir: &str) -> Result<Vec<String>, SourceError>;

    /// Reads one file whole.
    ///
    /// # Errors
    ///
    /// [`SourceError::NotFound`] if there is no such file.
    fn read(&self, path: &str) -> Result<Vec<u8>, SourceError>;
}

impl<T: AssetSource + ?Sized> AssetSource for &T {
    fn list(&self, dir: &str) -> Result<Vec<String>, SourceError> {
        (**self).list(dir)
    }
    fn read(&self, path: &str) -> Result<Vec<u8>, SourceError> {
        (**self).read(path)
    }
}

/// An [`AssetSource`] over a map from path to bytes.
///
/// This is the adapter tests use, and the one a wasm build uses for assets
/// baked into the binary.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InMemorySource {
    files: BTreeMap<String, Vec<u8>>,
}

impl InMemorySource {
    /// An empty source.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds one file, replacing any file already at that path.
    #[must_use]
    pub fn with(mut self, path: impl Into<String>, contents: impl Into<Vec<u8>>) -> Self {
        self.insert(path, contents);
        self
    }

    /// Adds one file, replacing any file already at that path.
    pub fn insert(&mut self, path: impl Into<String>, contents: impl Into<Vec<u8>>) {
        self.files.insert(path.into(), contents.into());
    }

    /// Every path the source holds, in sorted order.
    pub fn paths(&self) -> impl ExactSizeIterator<Item = &str> {
        self.files.keys().map(String::as_str)
    }

    /// How many files the source holds.
    pub fn len(&self) -> usize {
        self.files.len()
    }

    /// Whether the source holds nothing.
    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }
}

impl<K: Into<String>, V: Into<Vec<u8>>> FromIterator<(K, V)> for InMemorySource {
    fn from_iter<I: IntoIterator<Item = (K, V)>>(iter: I) -> Self {
        Self {
            files: iter
                .into_iter()
                .map(|(path, contents)| (path.into(), contents.into()))
                .collect(),
        }
    }
}

impl AssetSource for InMemorySource {
    fn list(&self, dir: &str) -> Result<Vec<String>, SourceError> {
        let prefix = format!("{}/", dir.trim_end_matches('/'));
        Ok(self
            .files
            .keys()
            .filter(|path| {
                path.strip_prefix(&prefix)
                    .is_some_and(|rest| !rest.contains('/'))
            })
            .cloned()
            .collect())
    }

    fn read(&self, path: &str) -> Result<Vec<u8>, SourceError> {
        self.files
            .get(path)
            .cloned()
            .ok_or_else(|| SourceError::NotFound(path.to_owned()))
    }
}

/// An [`AssetSource`] over a directory on disk.
///
/// Paths handed to [`list`](AssetSource::list) and [`read`](AssetSource::read)
/// are relative to the root and are rejected if they try to climb out of it.
#[cfg(feature = "std-fs")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirSource {
    root: std::path::PathBuf,
}

#[cfg(feature = "std-fs")]
impl DirSource {
    /// A source rooted at `root`.
    pub fn new(root: impl Into<std::path::PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// The directory this source reads from.
    pub fn root(&self) -> &std::path::Path {
        &self.root
    }

    /// Joins a logical path onto the root, refusing anything that escapes it.
    ///
    /// A mod ships the paths in its own data files, so a `..` here is either a
    /// bug or an attack. Either way it must not read the user's home directory.
    fn resolve(&self, path: &str) -> Result<std::path::PathBuf, SourceError> {
        let mut resolved = self.root.clone();
        for segment in path.split('/').filter(|s| !s.is_empty() && *s != ".") {
            if segment == ".." || segment.contains('\\') {
                return Err(SourceError::Io {
                    path: path.to_owned(),
                    message: "path escapes the source root".to_owned(),
                });
            }
            resolved.push(segment);
        }
        Ok(resolved)
    }
}

#[cfg(feature = "std-fs")]
impl AssetSource for DirSource {
    fn list(&self, dir: &str) -> Result<Vec<String>, SourceError> {
        let resolved = self.resolve(dir)?;
        let entries = match std::fs::read_dir(&resolved) {
            Ok(entries) => entries,
            // "this mod ships no recipes" is the common case, not an error.
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(err) => {
                return Err(SourceError::Io {
                    path: dir.to_owned(),
                    message: err.to_string(),
                });
            }
        };

        let mut paths = Vec::new();
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
            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                continue;
            };
            paths.push(format!("{}/{name}", dir.trim_end_matches('/')));
        }
        paths.sort();
        Ok(paths)
    }

    fn read(&self, path: &str) -> Result<Vec<u8>, SourceError> {
        let resolved = self.resolve(path)?;
        std::fs::read(&resolved).map_err(|err| {
            if err.kind() == std::io::ErrorKind::NotFound {
                SourceError::NotFound(path.to_owned())
            } else {
                SourceError::Io {
                    path: path.to_owned(),
                    message: err.to_string(),
                }
            }
        })
    }
}

/// Why the data stage stopped.
///
/// Every variant carries the file it was reading, because the first thing a
/// modder needs is which of their two hundred `.ron` files is broken.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum LoadError {
    /// The source failed.
    #[error("{0}")]
    Source(#[from] SourceError),
    /// A file was not valid RON, or did not match the type it should have.
    #[error("{path}: {message}")]
    Parse {
        /// The file that failed.
        path: String,
        /// What the parser said.
        message: String,
    },
    /// A file's bytes were not UTF-8.
    #[error("{0}: not valid UTF-8")]
    NotUtf8(String),
    /// Two entries claimed the same id, or a registry refused an entry.
    #[error("{path}: {source}")]
    Registry {
        /// The file that failed.
        path: String,
        /// What the registry said.
        source: RegistryError,
    },
    /// A patch could not be applied.
    #[error("{path}: {source}")]
    Patch {
        /// The patch file that failed.
        path: String,
        /// What went wrong.
        source: PatchError,
    },
    /// A registry rejected the whole set at freeze time.
    #[error("{0}")]
    Freeze(#[from] RegistryError),
}

/// What the data stage read, and what it thought was odd.
///
/// Handed to the caller alongside the frozen registries so a game can log it,
/// a dev build can show it in a console and a test can assert on it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LoadReport {
    /// The load order that was used.
    pub mods: Vec<ModId>,
    /// Every entry file that was read, in read order.
    pub entry_files: Vec<String>,
    /// Every patch file that was read, in read order.
    pub patch_files: Vec<String>,
    /// How many entries ended up in each registry.
    pub entries: BTreeMap<RegistryKind, usize>,
    /// How many patch operations were applied.
    pub patches_applied: usize,
    /// Everything that was odd but not fatal.
    pub warnings: Vec<Warning>,
}

impl LoadReport {
    /// How many files of any kind were read.
    pub fn files_read(&self) -> usize {
        self.entry_files.len() + self.patch_files.len()
    }

    /// How many entries were registered across every registry.
    pub fn total_entries(&self) -> usize {
        self.entries.values().sum()
    }
}

/// The frozen registries and the report that came with them.
#[derive(Debug, Clone, PartialEq)]
pub struct Loaded {
    /// The frozen registry set.
    pub registries: FrozenRegistries,
    /// What was read on the way there.
    pub report: LoadReport,
}

/// Runs the data stage over an [`AssetSource`].
///
/// ```
/// # use slotted_registry::loader::{DataStage, InMemorySource};
/// # use slotted_registry::manifest::ModId;
/// let source = InMemorySource::new()
///     .with("data/base/items/chest.ron", r#"(name: "base:chest")"#);
/// let loaded = DataStage::new(vec![ModId::new("base")?]).load(&source)?;
/// assert_eq!(loaded.registries.items.len(), 1);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataStage {
    load_order: Vec<ModId>,
    root: String,
}

impl DataStage {
    /// A data stage that reads the given mods, in the given order, from
    /// `data/`.
    ///
    /// The order comes from
    /// [`resolve_load_order`](crate::manifest::resolve_load_order).
    pub fn new(load_order: Vec<ModId>) -> Self {
        Self {
            load_order,
            root: "data".to_owned(),
        }
    }

    /// Reads from `root` instead of `data`.
    #[must_use]
    pub fn with_root(mut self, root: impl Into<String>) -> Self {
        self.root = root.into();
        self
    }

    /// The load order this stage will use.
    pub fn load_order(&self) -> &[ModId] {
        &self.load_order
    }

    /// Reads every mod's data, applies every patch round, and freezes.
    ///
    /// # Errors
    ///
    /// A [`LoadError`] naming the file that failed. A tag or recipe naming an
    /// item nobody registered is a [`Warning`] in the report instead, because a
    /// mod pack missing an optional mod still has to boot.
    pub fn load(&self, source: &dyn AssetSource) -> Result<Loaded, LoadError> {
        let mut report = LoadReport {
            mods: self.load_order.clone(),
            ..LoadReport::default()
        };
        let mut raw: BTreeMap<RegistryKind, RawEntries> = RegistryKind::ALL
            .into_iter()
            .map(|kind| (kind, RawEntries::new()))
            .collect();

        for round in Round::ALL {
            if round == Round::Base {
                self.read_entries(source, &mut raw, &mut report)?;
            }
            self.apply_round(source, round, &mut raw, &mut report)?;
        }

        let mut registries = Registries::new();
        for kind in RegistryKind::ALL {
            let entries = raw.remove(&kind).unwrap_or_default();
            report.entries.insert(kind, entries.len());
            for (name, value) in entries {
                install(&mut registries, kind, &name, value)?;
            }
        }

        let (frozen, warnings) = registries.freeze()?;
        report.warnings.extend(warnings);
        Ok(Loaded {
            registries: frozen,
            report,
        })
    }

    /// Reads every entry file of every mod, in load order.
    fn read_entries(
        &self,
        source: &dyn AssetSource,
        raw: &mut BTreeMap<RegistryKind, RawEntries>,
        report: &mut LoadReport,
    ) -> Result<(), LoadError> {
        for mod_id in &self.load_order {
            for kind in RegistryKind::ALL {
                let dir = format!("{}/{mod_id}/{}", self.root, kind.dir());
                for path in ron_files(source, &dir)? {
                    let value: Value = parse(source, &path)?;
                    let name = entry_name(&value, &path)?;
                    let entries = raw.get_mut(&kind).expect("every kind has a bucket");
                    // A later mod redefining an earlier mod's entry is the
                    // documented way to override it, which is why this is a
                    // replace and not a duplicate error. Tags merge instead,
                    // and that happens in `install`.
                    entries.insert(name, value);
                    report.entry_files.push(path);
                }
            }
        }
        Ok(())
    }

    /// Applies one round's patch files, for every mod, in load order.
    fn apply_round(
        &self,
        source: &dyn AssetSource,
        round: Round,
        raw: &mut BTreeMap<RegistryKind, RawEntries>,
        report: &mut LoadReport,
    ) -> Result<(), LoadError> {
        for mod_id in &self.load_order {
            let dir = format!("{}/{mod_id}/patches/{}", self.root, round.dir());
            for path in ron_files(source, &dir)? {
                let patches: PatchList = parse(source, &path)?;
                for item in &patches {
                    apply_one(raw, item, &path)?;
                    report.patches_applied += 1;
                }
                report.patch_files.push(path);
            }
        }
        Ok(())
    }
}

/// Applies one patch, naming its file if it fails.
fn apply_one(
    raw: &mut BTreeMap<RegistryKind, RawEntries>,
    item: &Patch,
    path: &str,
) -> Result<(), LoadError> {
    let entries = raw
        .get_mut(&item.registry)
        .expect("every kind has a bucket");
    patch::apply(entries, item).map_err(|source| LoadError::Patch {
        path: path.to_owned(),
        source,
    })
}

/// Every `.ron` file in `dir`, sorted so the load is reproducible.
fn ron_files(source: &dyn AssetSource, dir: &str) -> Result<Vec<String>, LoadError> {
    let mut paths: Vec<String> = source
        .list(dir)?
        .into_iter()
        .filter(|path| is_ron(path))
        .collect();
    paths.sort();
    Ok(paths)
}

/// Whether a path ends in `.ron`, in any case. Logical paths, not OS paths:
/// a pack may be served over HTTP, where `std::path` means nothing.
fn is_ron(path: &str) -> bool {
    let bytes = path.as_bytes();
    bytes.len() > 4 && bytes[bytes.len() - 4..].eq_ignore_ascii_case(b".ron")
}

/// Reads and parses one file.
fn parse<T: serde::de::DeserializeOwned>(
    source: &dyn AssetSource,
    path: &str,
) -> Result<T, LoadError> {
    let bytes = source.read(path)?;
    let text = String::from_utf8(bytes).map_err(|_| LoadError::NotUtf8(path.to_owned()))?;
    ron::from_str(&text).map_err(|err| LoadError::Parse {
        path: path.to_owned(),
        message: err.to_string(),
    })
}

/// Pulls the `name` field out of a raw entry so it can be keyed before it is
/// typed. Every def has one, and a patch has to be able to find it.
fn entry_name(value: &Value, path: &str) -> Result<Namespaced, LoadError> {
    let Value::Map(map) = value else {
        return Err(LoadError::Parse {
            path: path.to_owned(),
            message: "expected a struct with a `name` field".to_owned(),
        });
    };
    let Some(Value::String(raw)) = map.get(&Value::String("name".to_owned())) else {
        return Err(LoadError::Parse {
            path: path.to_owned(),
            message: "missing or non-string `name` field".to_owned(),
        });
    };
    Namespaced::parse(raw).map_err(|err| LoadError::Parse {
        path: path.to_owned(),
        message: err.to_string(),
    })
}

/// Deserialises one raw entry into its typed def and registers it.
fn install(
    registries: &mut Registries,
    kind: RegistryKind,
    name: &Namespaced,
    value: Value,
) -> Result<(), LoadError> {
    let path = format!("{}/{name}", kind.dir());

    match kind {
        RegistryKind::Items => {
            let def: ItemDef = typed_entry(value, &path)?;
            registries
                .add_item(def)
                .map_err(|source| LoadError::Registry { path, source })?;
        }
        RegistryKind::Tags => {
            let def: TagDef = typed_entry(value, &path)?;
            registries.add_tag(def);
        }
        RegistryKind::RecipeTypes => {
            let def: RecipeTypeDef = typed_entry(value, &path)?;
            registries
                .add_recipe_type(def)
                .map_err(|source| LoadError::Registry { path, source })?;
        }
        RegistryKind::Recipes => {
            let def: RecipeDef = typed_entry(value, &path)?;
            registries
                .add_recipe(def)
                .map_err(|source| LoadError::Registry { path, source })?;
        }
        RegistryKind::Screens => {
            let def: ScreenDef = typed_entry(value, &path)?;
            registries
                .screens
                .insert(name.clone(), def)
                .map_err(|source| LoadError::Registry { path, source })?;
        }
        RegistryKind::Widgets => {
            let def: WidgetDef = typed_entry(value, &path)?;
            registries
                .widgets
                .insert(name.clone(), def)
                .map_err(|source| LoadError::Registry { path, source })?;
        }
        RegistryKind::TooltipComponents => {
            let def: TooltipComponentDef = typed_entry(value, &path)?;
            registries
                .tooltip_components
                .insert(name.clone(), def)
                .map_err(|source| LoadError::Registry { path, source })?;
        }
        RegistryKind::HudLayers => {
            let def: HudLayerDef = typed_entry(value, &path)?;
            registries
                .hud_layers
                .insert(name.clone(), def)
                .map_err(|source| LoadError::Registry { path, source })?;
        }
        RegistryKind::IngredientTypes => {
            let def: IngredientTypeDef = typed_entry(value, &path)?;
            registries
                .ingredient_types
                .insert(name.clone(), def)
                .map_err(|source| LoadError::Registry { path, source })?;
        }
    }
    Ok(())
}

/// Turns a patched [`Value`] into a typed def.
fn typed_entry<T: serde::de::DeserializeOwned>(value: Value, path: &str) -> Result<T, LoadError> {
    value.into_rust().map_err(|err| LoadError::Parse {
        path: path.to_owned(),
        message: err.to_string(),
    })
}

#[allow(clippy::unwrap_used)]
#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use slotted_model::{ItemId, Namespaced};

    use super::{AssetSource, DataStage, InMemorySource, LoadError, SourceError};
    use crate::manifest::ModId;
    use crate::registry::{RegistryKind, Warning};

    fn id(s: &str) -> Namespaced {
        Namespaced::parse(s).unwrap()
    }

    fn mod_id(s: &str) -> ModId {
        ModId::new(s).unwrap()
    }

    /// `base` registers a chest and a crafting recipe; `copper` patches the
    /// chest's stack size in a later round.
    fn two_mods() -> InMemorySource {
        InMemorySource::new()
            .with(
                "data/base/items/chest.ron",
                r#"(name: "base:chest", max_stack_size: 64, tags: ["base:storage"])"#,
            )
            .with(
                "data/base/items/plank.ron",
                r#"(name: "base:plank", max_stack_size: 64)"#,
            )
            .with(
                "data/base/tags/planks.ron",
                r#"(name: "base:planks", values: ["base:plank"])"#,
            )
            .with(
                "data/base/recipe_types/crafting.ron",
                r#"(name: "base:crafting", size: (3, 3))"#,
            )
            .with(
                "data/base/recipes/chest.ron",
                r##"(
                    name: "base:chest",
                    recipe_type: "base:crafting",
                    ingredients: ["#base:planks"],
                    result: (item: "base:chest", count: 1),
                )"##,
            )
            .with(
                "data/copper/items/copper_chest.ron",
                r#"(name: "copper:chest", max_stack_size: 64)"#,
            )
            .with(
                "data/copper/patches/updates/base_chest.ron",
                r#"[(target: "base:chest", op: Merge((max_stack_size: 16)))]"#,
            )
    }

    #[test]
    fn an_in_memory_source_lists_one_directory_level() {
        let source = two_mods();
        assert_eq!(
            source.list("data/base/items").unwrap(),
            ["data/base/items/chest.ron", "data/base/items/plank.ron"]
        );
        assert_eq!(source.list("data/base/items/").unwrap().len(), 2);
        assert!(
            source.list("data/base").unwrap().is_empty(),
            "not recursive"
        );
        assert!(source.list("data/absent").unwrap().is_empty());
        assert_eq!(
            source.read("data/absent.ron").unwrap_err(),
            SourceError::NotFound("data/absent.ron".to_owned())
        );
        assert!(!source.is_empty());
        assert_eq!(source.paths().count(), source.len());
    }

    #[test]
    fn a_reference_forwards_to_the_source() {
        let source = two_mods();
        let by_ref: &dyn AssetSource = &source;
        assert_eq!(by_ref.list("data/base/items").unwrap().len(), 2);
        assert!(by_ref.read("data/base/items/chest.ron").is_ok());
    }

    #[test]
    fn a_source_collects_from_pairs() {
        let source: InMemorySource = [("a.ron", "()")].into_iter().collect();
        assert_eq!(source.len(), 1);
    }

    #[test]
    fn the_data_stage_loads_patches_and_freezes() {
        let loaded = DataStage::new(vec![mod_id("base"), mod_id("copper")])
            .load(&two_mods())
            .unwrap();

        let registries = &loaded.registries;
        assert_eq!(registries.items.len(), 3);
        assert_eq!(registries.item_id(&id("base:chest")), Some(ItemId(0)));
        assert_eq!(registries.item_id(&id("base:plank")), Some(ItemId(1)));
        assert_eq!(
            registries.item_id(&id("copper:chest")),
            Some(ItemId(2)),
            "load order decides the numbers"
        );
        assert_eq!(
            registries
                .items
                .get_by_name(&id("base:chest"))
                .unwrap()
                .max_stack_size,
            16,
            "the later mod's patch won"
        );

        let plank = registries.item_id(&id("base:plank")).unwrap();
        let recipe = registries.recipes.id_of(&id("base:chest")).unwrap();
        assert_eq!(registries.recipe_index.uses(plank), [recipe]);
        assert_eq!(
            registries.recipe_index.for_output(ItemId(0)),
            [recipe],
            "the chest is the output"
        );

        let report = &loaded.report;
        assert_eq!(report.mods, vec![mod_id("base"), mod_id("copper")]);
        assert_eq!(report.entry_files.len(), 6);
        assert_eq!(report.patch_files.len(), 1);
        assert_eq!(report.patches_applied, 1);
        assert_eq!(report.files_read(), 7);
        assert_eq!(report.total_entries(), 6);
        assert_eq!(report.entries[&RegistryKind::Items], 3);
        assert_eq!(report.entries[&RegistryKind::Recipes], 1);
        assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    }

    #[test]
    fn patches_run_in_round_order_so_final_fixes_wins() {
        let source = two_mods()
            .with(
                "data/copper/patches/base/chest.ron",
                r#"[(target: "base:chest", op: Merge((max_stack_size: 1)))]"#,
            )
            .with(
                "data/copper/patches/final_fixes/chest.ron",
                r#"[(target: "base:chest", op: Merge((max_stack_size: 99)))]"#,
            );
        let loaded = DataStage::new(vec![mod_id("base"), mod_id("copper")])
            .load(&source)
            .unwrap();
        assert_eq!(
            loaded
                .registries
                .items
                .get_by_name(&id("base:chest"))
                .unwrap()
                .max_stack_size,
            99
        );
        assert_eq!(loaded.report.patches_applied, 3);
    }

    #[test]
    fn a_patch_can_remove_another_mods_entry() {
        let source = two_mods().with(
            "data/copper/patches/updates/drop_plank.ron",
            r#"[(target: "base:plank", op: Remove)]"#,
        );
        let loaded = DataStage::new(vec![mod_id("base"), mod_id("copper")])
            .load(&source)
            .unwrap();
        assert_eq!(loaded.registries.items.len(), 2);
        assert_eq!(loaded.registries.item_id(&id("base:plank")), None);
        assert_eq!(
            loaded.report.warnings,
            vec![Warning::UnknownTagMember {
                tag: id("base:planks"),
                item: id("base:plank"),
            }],
            "the tag that named it now warns"
        );
    }

    #[test]
    fn a_patch_against_a_missing_entry_names_its_file() {
        let source = two_mods().with(
            "data/copper/patches/updates/oops.ron",
            r#"[(target: "absent:thing", op: Remove)]"#,
        );
        let err = DataStage::new(vec![mod_id("base"), mod_id("copper")])
            .load(&source)
            .unwrap_err();
        let LoadError::Patch { path, .. } = &err else {
            panic!("expected a patch error, got {err}");
        };
        assert_eq!(path, "data/copper/patches/updates/oops.ron");
    }

    #[test]
    fn a_broken_file_names_itself() {
        let source = InMemorySource::new().with("data/base/items/bad.ron", "(name: ");
        let err = DataStage::new(vec![mod_id("base")])
            .load(&source)
            .unwrap_err();
        assert!(
            err.to_string().starts_with("data/base/items/bad.ron:"),
            "{err}"
        );

        let nameless = InMemorySource::new().with("data/base/items/x.ron", "(max_stack_size: 1)");
        let err = DataStage::new(vec![mod_id("base")])
            .load(&nameless)
            .unwrap_err();
        assert!(err.to_string().contains("`name` field"), "{err}");

        let not_utf8 = InMemorySource::new().with("data/base/items/x.ron", vec![0xff, 0xfe]);
        let err = DataStage::new(vec![mod_id("base")])
            .load(&not_utf8)
            .unwrap_err();
        assert!(err.to_string().contains("UTF-8"), "{err}");

        let bad_id = InMemorySource::new().with("data/base/items/x.ron", r#"(name: "NOPE")"#);
        let err = DataStage::new(vec![mod_id("base")])
            .load(&bad_id)
            .unwrap_err();
        assert!(err.to_string().contains("NOPE"), "{err}");
    }

    #[test]
    fn a_field_the_def_does_not_have_is_an_error_after_patching() {
        let source = InMemorySource::new()
            .with("data/base/items/x.ron", r#"(name: "base:x")"#)
            .with(
                "data/base/patches/updates/x.ron",
                r#"[(target: "base:x", op: Merge((nonsense: 1)))]"#,
            );
        let err = DataStage::new(vec![mod_id("base")])
            .load(&source)
            .unwrap_err();
        assert!(err.to_string().contains("items/base:x"), "{err}");
    }

    #[test]
    fn a_later_mod_can_redefine_an_earlier_mods_entry() {
        let source = InMemorySource::new()
            .with(
                "data/base/items/x.ron",
                r#"(name: "shared:x", max_stack_size: 4)"#,
            )
            .with(
                "data/copper/items/x.ron",
                r#"(name: "shared:x", max_stack_size: 8)"#,
            );
        let loaded = DataStage::new(vec![mod_id("base"), mod_id("copper")])
            .load(&source)
            .unwrap();
        assert_eq!(loaded.registries.items.len(), 1);
        assert_eq!(
            loaded
                .registries
                .items
                .get_by_name(&id("shared:x"))
                .unwrap()
                .max_stack_size,
            8
        );
    }

    #[test]
    fn a_custom_root_is_honoured() {
        let source = InMemorySource::new().with("mods/base/items/x.ron", r#"(name: "base:x")"#);
        let stage = DataStage::new(vec![mod_id("base")]).with_root("mods");
        assert_eq!(stage.load_order(), [mod_id("base")]);
        assert_eq!(stage.load(&source).unwrap().registries.items.len(), 1);
    }

    #[test]
    fn an_empty_load_order_produces_empty_registries() {
        let loaded = DataStage::new(Vec::new()).load(&two_mods()).unwrap();
        assert!(loaded.registries.items.is_empty());
        assert_eq!(loaded.report.total_entries(), 0);
    }
}
