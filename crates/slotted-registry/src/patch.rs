//! Patch lists: how one mod edits another mod's entries.
//!
//! A mod cannot reach into another mod's `.ron` file, so it ships a patch
//! instead. Patches run against the raw [`Value`] an entry file parsed to,
//! *before* it becomes a typed def, which is what lets a patch add a field a
//! def gained in a later version and lets a def stay `deny_unknown_fields`.
//!
//! # Rounds
//!
//! Factorio settled this design: a mod that wants to see everything everyone
//! registered has to run after everyone, and a mod that wants the last word
//! has to run after that. So the data stage runs three times over the same
//! load order.
//!
//! | [`Round`] | Directory | For |
//! |---|---|---|
//! | [`Round::Base`] | `patches/base/` | edits to entries you expect to exist |
//! | [`Round::Updates`] | `patches/updates/` | edits that need every mod's entries registered |
//! | [`Round::FinalFixes`] | `patches/final_fixes/` | the last word, for compatibility packs |
//!
//! # Paths
//!
//! [`PatchOp::InsertListItem`] and [`PatchOp::RemoveListItem`] address a list
//! inside the entry with a dot-separated path: `tags`, or `extra.stages`. Each
//! segment is a map key, except a segment that parses as a number, which
//! indexes a list. A key containing a literal dot is not addressable, which is
//! the price of keeping the syntax something a modder can type.

use serde::{Deserialize, Serialize};
use slotted_model::Namespaced;

use crate::Value;
use crate::registry::RegistryKind;

/// Which pass over the load order a patch belongs to.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum Round {
    /// First pass. Entry files are read in this round too.
    #[default]
    Base,
    /// Second pass, after every mod has registered everything.
    Updates,
    /// Third and last pass.
    FinalFixes,
}

impl Round {
    /// Every round, in the order the data stage runs them.
    pub const ALL: [Self; 3] = [Self::Base, Self::Updates, Self::FinalFixes];

    /// The directory name under `data/<modid>/patches/`.
    pub const fn dir(self) -> &'static str {
        match self {
            Self::Base => "base",
            Self::Updates => "updates",
            Self::FinalFixes => "final_fixes",
        }
    }

    /// The round a directory name refers to, if any.
    pub fn from_dir(dir: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|round| round.dir() == dir)
    }
}

impl core::fmt::Display for Round {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.dir())
    }
}

/// What a patch does to its target.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PatchOp {
    /// Swap the whole entry for this value.
    Replace(Value),
    /// Merge this value into the entry: maps merge key by key, anything else
    /// overwrites. Merging a map whose value is `()` removes that key, which
    /// is how a patch clears a field.
    Merge(Value),
    /// Delete the entry.
    Remove,
    /// Insert into a list inside the entry.
    InsertListItem {
        /// Dot-separated path to the list.
        path: String,
        /// Where to insert. `None` appends.
        #[serde(default)]
        index: Option<usize>,
        /// What to insert.
        value: Value,
    },
    /// Delete one item from a list inside the entry.
    RemoveListItem {
        /// Dot-separated path to the list.
        path: String,
        /// Which item.
        index: usize,
    },
}

/// One edit to one registry entry.
///
/// The `registry` field defaults to [`RegistryKind::Items`], which is what the
/// overwhelming majority of patches touch, so most patch files are two lines.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Patch {
    /// The entry to edit.
    pub target: Namespaced,
    /// Which registry the target lives in.
    #[serde(default)]
    pub registry: RegistryKind,
    /// What to do to it.
    pub op: PatchOp,
}

impl Patch {
    /// A patch against an item.
    pub fn new(target: Namespaced, op: PatchOp) -> Self {
        Self {
            target,
            registry: RegistryKind::Items,
            op,
        }
    }

    /// The same patch, aimed at another registry.
    #[must_use]
    pub fn in_registry(mut self, registry: RegistryKind) -> Self {
        self.registry = registry;
        self
    }
}

/// A patch file: a list of patches, so one file can hold a mod's whole set of
/// edits for one round.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PatchList(pub Vec<Patch>);

impl PatchList {
    /// The patches, in file order.
    pub fn iter(&self) -> core::slice::Iter<'_, Patch> {
        self.0.iter()
    }

    /// How many patches the file holds.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether the file is empty.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl<'a> IntoIterator for &'a PatchList {
    type Item = &'a Patch;
    type IntoIter = core::slice::Iter<'a, Patch>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

impl IntoIterator for PatchList {
    type Item = Patch;
    type IntoIter = std::vec::IntoIter<Patch>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

/// Why a patch could not be applied.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum PatchError {
    /// The patch named an entry that no mod registered.
    #[error("patch target `{target}` is not registered in the {registry} registry")]
    UnknownTarget {
        /// The id the patch aimed at.
        target: Namespaced,
        /// The registry it looked in.
        registry: RegistryKind,
    },
    /// A path segment addressed something that is not there.
    #[error("path `{path}` does not exist in `{target}`")]
    NoSuchPath {
        /// The id being patched.
        target: Namespaced,
        /// The path that missed.
        path: String,
    },
    /// A path reached a value that is not a list, but the op needs one.
    #[error("path `{path}` in `{target}` is not a list")]
    NotAList {
        /// The id being patched.
        target: Namespaced,
        /// The path that landed on a non-list.
        path: String,
    },
    /// A list index was past the end.
    #[error("index {index} is out of range for the {len}-item list at `{path}` in `{target}`")]
    IndexOutOfRange {
        /// The id being patched.
        target: Namespaced,
        /// The path to the list.
        path: String,
        /// The index that was asked for.
        index: usize,
        /// How long the list actually is.
        len: usize,
    },
}

/// The raw entries of one registry, keyed by id, before they become typed defs.
pub type RawEntries = indexmap::IndexMap<Namespaced, Value>;

/// Applies one patch to a registry's raw entries.
///
/// # Errors
///
/// [`PatchError::UnknownTarget`] if the entry is not there, and the path errors
/// if a list op addresses something that is not a list of the right length.
pub fn apply(entries: &mut RawEntries, patch: &Patch) -> Result<(), PatchError> {
    let missing = || PatchError::UnknownTarget {
        target: patch.target.clone(),
        registry: patch.registry,
    };

    match &patch.op {
        PatchOp::Remove => {
            entries.shift_remove(&patch.target).ok_or_else(missing)?;
            Ok(())
        }
        PatchOp::Replace(value) => {
            let slot = entries.get_mut(&patch.target).ok_or_else(missing)?;
            *slot = value.clone();
            Ok(())
        }
        PatchOp::Merge(value) => {
            let slot = entries.get_mut(&patch.target).ok_or_else(missing)?;
            merge(slot, value.clone());
            Ok(())
        }
        PatchOp::InsertListItem { path, index, value } => {
            let slot = entries.get_mut(&patch.target).ok_or_else(missing)?;
            let list = list_at(slot, path, &patch.target)?;
            let at = index.unwrap_or(list.len());
            if at > list.len() {
                return Err(PatchError::IndexOutOfRange {
                    target: patch.target.clone(),
                    path: path.clone(),
                    index: at,
                    len: list.len(),
                });
            }
            list.insert(at, value.clone());
            Ok(())
        }
        PatchOp::RemoveListItem { path, index } => {
            let slot = entries.get_mut(&patch.target).ok_or_else(missing)?;
            let list = list_at(slot, path, &patch.target)?;
            if *index >= list.len() {
                return Err(PatchError::IndexOutOfRange {
                    target: patch.target.clone(),
                    path: path.clone(),
                    index: *index,
                    len: list.len(),
                });
            }
            list.remove(*index);
            Ok(())
        }
    }
}

/// Recursively merges `incoming` into `base`.
///
/// Two maps merge key by key so a patch can raise one field without restating
/// the entry. Anything else overwrites, because there is no sensible way to
/// merge two numbers. A key whose incoming value is `()` is deleted, which
/// gives a patch a way to clear an optional field.
pub fn merge(base: &mut Value, incoming: Value) {
    match (base, incoming) {
        (Value::Map(base_map), Value::Map(incoming_map)) => {
            for (key, value) in incoming_map {
                if matches!(value, Value::Unit) {
                    base_map.remove(&key);
                } else if let Some(existing) = base_map.get_mut(&key) {
                    merge(existing, value);
                } else {
                    base_map.insert(key, value);
                }
            }
        }
        (slot, incoming) => *slot = incoming,
    }
}

/// Walks `path` and returns the list it names.
fn list_at<'a>(
    root: &'a mut Value,
    path: &str,
    target: &Namespaced,
) -> Result<&'a mut Vec<Value>, PatchError> {
    let mut current = root;
    for segment in path.split('.').filter(|segment| !segment.is_empty()) {
        current = match current {
            Value::Map(map) => {
                map.get_mut(&Value::String(segment.to_owned()))
                    .ok_or_else(|| PatchError::NoSuchPath {
                        target: target.clone(),
                        path: path.to_owned(),
                    })?
            }
            Value::Seq(seq) => {
                let index: usize = segment.parse().map_err(|_| PatchError::NoSuchPath {
                    target: target.clone(),
                    path: path.to_owned(),
                })?;
                seq.get_mut(index).ok_or_else(|| PatchError::NoSuchPath {
                    target: target.clone(),
                    path: path.to_owned(),
                })?
            }
            _ => {
                return Err(PatchError::NoSuchPath {
                    target: target.clone(),
                    path: path.to_owned(),
                });
            }
        };
    }
    match current {
        Value::Seq(seq) => Ok(seq),
        _ => Err(PatchError::NotAList {
            target: target.clone(),
            path: path.to_owned(),
        }),
    }
}

#[allow(clippy::unwrap_used)]
#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use slotted_model::Namespaced;

    use super::{Patch, PatchError, PatchList, PatchOp, RawEntries, Round, apply};
    use crate::Value;
    use crate::defs::ItemDef;
    use crate::registry::RegistryKind;

    fn id(s: &str) -> Namespaced {
        Namespaced::parse(s).unwrap()
    }

    fn entries() -> RawEntries {
        let mut entries = RawEntries::new();
        entries.insert(
            id("base:chest"),
            ron::from_str(
                r#"(name: "base:chest", max_stack_size: 64, tags: ["base:storage"], components: { "base:tint": 1 })"#,
            )
            .unwrap(),
        );
        entries
    }

    fn typed(entries: &RawEntries, name: &str) -> ItemDef {
        entries.get(&id(name)).unwrap().clone().into_rust().unwrap()
    }

    #[test]
    fn rounds_map_to_directories_both_ways() {
        for round in Round::ALL {
            assert_eq!(Round::from_dir(round.dir()), Some(round));
            assert_eq!(round.to_string(), round.dir());
        }
        assert_eq!(Round::from_dir("nope"), None);
        assert_eq!(Round::default(), Round::Base);
        assert!(Round::Base < Round::Updates && Round::Updates < Round::FinalFixes);
    }

    #[test]
    fn merge_raises_one_field_and_leaves_the_rest() {
        let mut entries = entries();
        apply(
            &mut entries,
            &Patch::new(
                id("base:chest"),
                PatchOp::Merge(ron::from_str("(max_stack_size: 16)").unwrap()),
            ),
        )
        .unwrap();
        let item = typed(&entries, "base:chest");
        assert_eq!(item.max_stack_size, 16);
        assert_eq!(item.tags, vec![id("base:storage")], "untouched");
    }

    #[test]
    fn merge_recurses_into_nested_maps_and_a_unit_clears_a_key() {
        let mut entries = entries();
        apply(
            &mut entries,
            &Patch::new(
                id("base:chest"),
                PatchOp::Merge(
                    ron::from_str(r#"(components: { "base:glow": 2 }, tags: [])"#).unwrap(),
                ),
            ),
        )
        .unwrap();
        assert_eq!(typed(&entries, "base:chest").components.len(), 2);
        assert!(typed(&entries, "base:chest").tags.is_empty());

        apply(
            &mut entries,
            &Patch::new(
                id("base:chest"),
                PatchOp::Merge(ron::from_str(r#"(components: { "base:tint": () })"#).unwrap()),
            ),
        )
        .unwrap();
        let components = typed(&entries, "base:chest").components;
        assert_eq!(components.len(), 1);
        assert!(components.contains_key(&id("base:glow")));
    }

    #[test]
    fn replace_swaps_the_whole_entry_and_remove_deletes_it() {
        let mut entries = entries();
        apply(
            &mut entries,
            &Patch::new(
                id("base:chest"),
                PatchOp::Replace(ron::from_str(r#"(name: "base:chest")"#).unwrap()),
            ),
        )
        .unwrap();
        assert_eq!(typed(&entries, "base:chest").max_stack_size, 64);
        assert!(typed(&entries, "base:chest").tags.is_empty());

        apply(&mut entries, &Patch::new(id("base:chest"), PatchOp::Remove)).unwrap();
        assert!(entries.is_empty());
    }

    #[test]
    fn list_items_go_in_and_come_out_by_path() {
        let mut entries = entries();
        apply(
            &mut entries,
            &Patch::new(
                id("base:chest"),
                PatchOp::InsertListItem {
                    path: "tags".to_owned(),
                    index: Some(0),
                    value: Value::String("base:containers".to_owned()),
                },
            ),
        )
        .unwrap();
        assert_eq!(
            typed(&entries, "base:chest").tags,
            vec![id("base:containers"), id("base:storage")]
        );

        apply(
            &mut entries,
            &Patch::new(
                id("base:chest"),
                PatchOp::InsertListItem {
                    path: "tags".to_owned(),
                    index: None,
                    value: Value::String("base:wooden".to_owned()),
                },
            ),
        )
        .unwrap();
        assert_eq!(typed(&entries, "base:chest").tags.len(), 3, "None appends");

        apply(
            &mut entries,
            &Patch::new(
                id("base:chest"),
                PatchOp::RemoveListItem {
                    path: "tags".to_owned(),
                    index: 1,
                },
            ),
        )
        .unwrap();
        assert_eq!(
            typed(&entries, "base:chest").tags,
            vec![id("base:containers"), id("base:wooden")]
        );
    }

    #[test]
    fn a_path_walks_through_maps_and_list_indices() {
        let mut entries = RawEntries::new();
        entries.insert(
            id("base:widget"),
            ron::from_str(r#"(name: "base:widget", rows: [(cells: ["a"])])"#).unwrap(),
        );
        apply(
            &mut entries,
            &Patch::new(
                id("base:widget"),
                PatchOp::InsertListItem {
                    path: "rows.0.cells".to_owned(),
                    index: None,
                    value: Value::String("b".to_owned()),
                },
            ),
        )
        .unwrap();
        let text = ron::to_string(entries.get(&id("base:widget")).unwrap()).unwrap();
        assert!(text.contains(r#"["a","b"]"#), "{text}");
    }

    #[test]
    fn every_way_a_patch_can_miss_says_what_it_wanted() {
        let mut entries = entries();
        assert_eq!(
            apply(
                &mut entries,
                &Patch::new(id("base:absent"), PatchOp::Remove)
            )
            .unwrap_err(),
            PatchError::UnknownTarget {
                target: id("base:absent"),
                registry: RegistryKind::Items,
            }
        );
        let bad_path = apply(
            &mut entries,
            &Patch::new(
                id("base:chest"),
                PatchOp::RemoveListItem {
                    path: "nope.deeper".to_owned(),
                    index: 0,
                },
            ),
        )
        .unwrap_err();
        assert!(
            matches!(bad_path, PatchError::NoSuchPath { .. }),
            "{bad_path}"
        );

        let not_a_list = apply(
            &mut entries,
            &Patch::new(
                id("base:chest"),
                PatchOp::RemoveListItem {
                    path: "max_stack_size".to_owned(),
                    index: 0,
                },
            ),
        )
        .unwrap_err();
        assert!(
            matches!(not_a_list, PatchError::NotAList { .. }),
            "{not_a_list}"
        );
        assert!(not_a_list.to_string().contains("is not a list"));

        let out_of_range = apply(
            &mut entries,
            &Patch::new(
                id("base:chest"),
                PatchOp::RemoveListItem {
                    path: "tags".to_owned(),
                    index: 9,
                },
            ),
        )
        .unwrap_err();
        assert_eq!(
            out_of_range,
            PatchError::IndexOutOfRange {
                target: id("base:chest"),
                path: "tags".to_owned(),
                index: 9,
                len: 1,
            }
        );
    }

    #[test]
    fn a_patch_file_parses_as_a_list() {
        let list: PatchList = ron::from_str(
            r#"[
                (target: "base:chest", op: Merge((max_stack_size: 16))),
                (target: "base:crafting", registry: recipe_types, op: Remove),
            ]"#,
        )
        .unwrap();
        assert_eq!(list.len(), 2);
        assert!(!list.is_empty());
        assert_eq!(list.iter().count(), 2);
        assert_eq!((&list).into_iter().count(), 2);
        assert_eq!(list.0[0].registry, RegistryKind::Items);
        assert_eq!(list.0[1].registry, RegistryKind::RecipeTypes);
        assert_eq!(list.into_iter().count(), 2);
    }

    #[test]
    fn a_patch_can_be_aimed_at_another_registry_in_code() {
        let patch =
            Patch::new(id("base:sorter"), PatchOp::Remove).in_registry(RegistryKind::Screens);
        assert_eq!(patch.registry, RegistryKind::Screens);
        let text = ron::to_string(&patch).unwrap();
        assert_eq!(ron::from_str::<Patch>(&text).unwrap(), patch);
    }
}
