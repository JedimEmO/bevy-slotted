//! The browser index: every entry with per-field storage, built off the main
//! thread. Contract section 4. Package A.

pub mod bitset;
pub mod build;
pub mod substring;

use std::collections::BTreeMap;
use std::ops::Bound;
use std::sync::Arc;

use bevy::prelude::*;
use bevy::tasks::Task;
use slotted_registry::Rarity;

pub use bitset::Bitset;
pub use substring::SubstringIndex;

use crate::ingredient::Ingredient;

/// Dense position of an entry in [`BrowserIndex::entries`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct EntryId(pub u32);

impl EntryId {
    /// As a `usize` index.
    pub const fn index(self) -> usize {
        self.0 as usize
    }
}

/// One indexed ingredient with its precomputed search fields.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// The ingredient.
    pub ingredient: Ingredient,
    /// Player-facing name.
    pub display: String,
    /// `@` field.
    pub mod_ns: String,
    /// Rarity, for sorting and the card strip.
    pub rarity: Rarity,
    /// `#` field.
    pub tags: Vec<String>,
    /// `%` field: categories with a recipe producing this entry.
    pub categories: Vec<String>,
}

/// Low-cardinality field: each distinct string maps to the entries holding it.
/// Prefix queries scan the `BTreeMap` range; `O(log d + matches)`.
#[derive(Debug, Clone, Default)]
pub struct StringBitsetMap {
    map: BTreeMap<String, Bitset>,
    entries: usize,
}

impl StringBitsetMap {
    /// Builds from `(entry id, value)` pairs.
    pub fn build(entries: usize, values: impl IntoIterator<Item = (u32, String)>) -> Self {
        let mut map: BTreeMap<String, Bitset> = BTreeMap::new();
        for (id, v) in values {
            map.entry(v.to_lowercase())
                .or_insert_with(|| Bitset::new(entries))
                .insert(id as usize);
        }
        Self { map, entries }
    }

    /// Entries whose value starts with `prefix` (case-insensitive) or, when
    /// the value contains a `:`, whose path after the colon starts with it.
    pub fn find(&self, prefix: &str) -> Bitset {
        let prefix = prefix.to_lowercase();
        let mut out = Bitset::new(self.entries);
        for (k, set) in self
            .map
            .range::<str, _>((Bound::Included(prefix.as_str()), Bound::Unbounded))
        {
            if !k.starts_with(&prefix) {
                break;
            }
            out.or(set);
        }
        // PHASE3-IMPL: A — path-after-colon matches need a second scan.
        out
    }

    /// Distinct values.
    pub fn len(&self) -> usize {
        self.map.len()
    }

    /// Whether no value was indexed.
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
}

/// The built index. Immutable once built; rebuilt on `RebuildBrowser`.
#[derive(Debug, Clone, Default)]
pub struct BrowserIndex {
    /// Entries by [`EntryId`].
    pub entries: Vec<Entry>,
    /// Unprefixed field.
    pub names: SubstringIndex,
    /// `$` field.
    pub tooltips: SubstringIndex,
    /// `&` field.
    pub ids: SubstringIndex,
    /// `@` field.
    pub mods: StringBitsetMap,
    /// `#` field.
    pub tags: StringBitsetMap,
    /// `%` field.
    pub categories: StringBitsetMap,
    /// Position of each ingredient, for reverse lookups.
    pub by_ingredient: std::collections::HashMap<Ingredient, EntryId>,
}

impl BrowserIndex {
    /// The entry.
    pub fn get(&self, id: EntryId) -> Option<&Entry> {
        self.entries.get(id.index())
    }

    /// The id of an ingredient.
    pub fn id_of(&self, ingredient: &Ingredient) -> Option<EntryId> {
        self.by_ingredient.get(ingredient).copied()
    }

    /// Number of entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the index is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Where the index build is. Polled in `BrowserSet::Index`.
#[derive(Resource, Default)]
pub enum IndexState {
    /// No build started.
    #[default]
    Empty,
    /// Running on `AsyncComputeTaskPool`.
    Building(Task<BrowserIndex>),
    /// Done.
    Ready(Arc<BrowserIndex>),
}

impl IndexState {
    /// The index, when ready.
    pub fn ready(&self) -> Option<&Arc<BrowserIndex>> {
        match self {
            Self::Ready(index) => Some(index),
            _ => None,
        }
    }

    /// Whether a build is in flight.
    pub const fn is_building(&self) -> bool {
        matches!(self, Self::Building(_))
    }
}

/// Edit-mode blacklist. `version` bumps on every change so the search cache
/// invalidates.
#[derive(Resource, Debug, Clone, Default, PartialEq, Eq)]
pub struct HiddenEntries {
    /// Hidden entries.
    pub set: Bitset,
    /// Change counter.
    pub version: u32,
}

impl HiddenEntries {
    /// Hides or shows an entry.
    pub fn set_hidden(&mut self, id: EntryId, hidden: bool) {
        if hidden {
            self.set.insert(id.index());
        } else {
            self.set.remove(id.index());
        }
        self.version = self.version.wrapping_add(1);
    }
}
