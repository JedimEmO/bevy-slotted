//! `mod.toml`: what a mod says about itself, and what order mods load in.
//!
//! The shape follows Factorio's `info.json` and Minecraft's `mods.toml`,
//! because modders already know it and because those two between them have
//! found every edge a load-order resolver has: optional dependencies that only
//! constrain order when present, incompatibility declarations, and version
//! requirements that have to be checked before the sort rather than after.
//!
//! ```toml
//! id = "copper_chest"
//! name = "Copper Chest"
//! version = "1.2.0"
//! api_version = 1
//!
//! [entry]
//! data = "data.lua"
//! control = "control.lua"
//!
//! [[dependencies]]
//! id = "slotted"
//! req = ">=0.1"
//!
//! [[dependencies]]
//! id = "sorter"
//! kind = "optional"
//! order = "after"
//! ```

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::str::FromStr;

use semver::{Version, VersionReq};
use serde::{Deserialize, Serialize};
use slotted_model::NamespacedError;

/// A mod's id: the namespace half of every id it registers.
///
/// `copper_chest` here means every id the mod owns starts `copper_chest:`.
/// Validated against the same character set as a
/// [`Namespaced`](slotted_model::Namespaced) namespace, so a mod can never
/// claim a namespace it cannot then write ids in.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ModId(String);

impl ModId {
    /// Validates and wraps a mod id.
    ///
    /// # Errors
    ///
    /// [`NamespacedError`] if the string is empty or holds a character outside
    /// `[a-z0-9_.-]`.
    pub fn new(raw: impl Into<String>) -> Result<Self, NamespacedError> {
        let raw = raw.into();
        // `<raw>:x` is a valid namespaced id exactly when `<raw>` is a valid
        // namespace, so the model crate's validator is the single source of
        // truth rather than a second copy of the character set.
        slotted_model::Namespaced::new(&raw, "x")?;
        Ok(Self(raw))
    }

    /// The id as a string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ModId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for ModId {
    type Err = NamespacedError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::new(s)
    }
}

impl AsRef<str> for ModId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl Serialize for ModId {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for ModId {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        Self::new(raw).map_err(serde::de::Error::custom)
    }
}

/// How badly one mod wants another.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum DependencyKind {
    /// Must be present, or loading fails.
    #[default]
    Required,
    /// Only constrains order, and only when the other mod is installed.
    Optional,
    /// Must *not* be present, or loading fails.
    Incompatible,
}

/// Where a dependency sits relative to the mod that declares it.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum LoadOrder {
    /// The dependency loads first. The default, and what a mod that reads
    /// another's registry entries needs.
    #[default]
    After,
    /// The dependency loads second, so this mod's entries are in place before
    /// it runs.
    Before,
    /// No ordering constraint at all, only a presence and version check.
    None,
}

/// One dependency declaration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Dependency {
    /// The other mod.
    pub id: ModId,
    /// A semver requirement such as `>=1.2, <2`. `None` accepts any version.
    #[serde(default)]
    pub req: Option<VersionReq>,
    /// Required, optional or incompatible.
    #[serde(default)]
    pub kind: DependencyKind,
    /// Which side of this mod it loads on.
    #[serde(default)]
    pub order: LoadOrder,
}

impl Dependency {
    /// A required dependency with no version constraint, loading first.
    pub fn required(id: ModId) -> Self {
        Self {
            id,
            req: None,
            kind: DependencyKind::Required,
            order: LoadOrder::After,
        }
    }

    /// An optional dependency: order only, and only when it is installed.
    pub fn optional(id: ModId) -> Self {
        Self {
            kind: DependencyKind::Optional,
            ..Self::required(id)
        }
    }

    /// A mod this one refuses to run beside.
    pub fn incompatible(id: ModId) -> Self {
        Self {
            kind: DependencyKind::Incompatible,
            order: LoadOrder::None,
            ..Self::required(id)
        }
    }
}

/// The script entry points a mod ships.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    /// Runs during the data stage, while registries are open.
    #[serde(default)]
    pub data: Option<String>,
    /// Runs during the control stage, after the freeze.
    #[serde(default)]
    pub control: Option<String>,
}

/// A parsed `mod.toml`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModManifest {
    /// The namespace this mod owns.
    pub id: ModId,
    /// The human-readable name.
    pub name: String,
    /// The mod's own version.
    pub version: Version,
    /// Which revision of the scripting and data API the mod was written
    /// against. Bumped when a breaking change ships; see `docs/PLAN.md`.
    pub api_version: u32,
    /// What it needs, prefers and refuses.
    #[serde(default)]
    pub dependencies: Vec<Dependency>,
    /// Script entry points.
    #[serde(default)]
    pub entry: Entry,
    /// Asset root relative to the mod root, if the mod ships assets.
    #[serde(default)]
    pub assets: Option<String>,
}

/// Why a `mod.toml` could not be read.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{path}: {message}")]
pub struct ManifestError {
    /// Which file failed.
    pub path: String,
    /// What was wrong with it.
    pub message: String,
}

impl ModManifest {
    /// Parses a `mod.toml`. `path` only appears in error messages.
    ///
    /// # Errors
    ///
    /// [`ManifestError`] if the TOML is malformed, a field is missing, or the
    /// id or a version does not parse.
    pub fn parse(path: &str, text: &str) -> Result<Self, ManifestError> {
        toml::from_str(text).map_err(|err| ManifestError {
            path: path.to_owned(),
            message: err.message().to_owned(),
        })
    }

    /// Every dependency of one kind.
    pub fn deps_of(&self, kind: DependencyKind) -> impl Iterator<Item = &Dependency> {
        self.dependencies.iter().filter(move |dep| dep.kind == kind)
    }
}

/// Why a set of manifests has no valid load order.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LoadOrderError {
    /// Two manifests claimed the same id.
    #[error("mod `{0}` is installed twice")]
    Duplicate(ModId),
    /// A required dependency is not installed.
    #[error("mod `{dependent}` requires `{missing}`, which is not installed")]
    MissingDependency {
        /// The mod that asked.
        dependent: ModId,
        /// The mod that is not there.
        missing: ModId,
    },
    /// A dependency is installed but at the wrong version.
    #[error("mod `{dependent}` requires `{dependency}` {req}, but {found} is installed")]
    VersionMismatch {
        /// The mod that asked.
        dependent: ModId,
        /// The mod it asked about.
        dependency: ModId,
        /// What it wanted.
        req: VersionReq,
        /// What is installed.
        found: Version,
    },
    /// Two mods that cannot run together are both installed.
    #[error("mod `{dependent}` is incompatible with `{conflict}`, which is also installed")]
    Incompatible {
        /// The mod that declared the incompatibility.
        dependent: ModId,
        /// The mod it refuses to run beside.
        conflict: ModId,
    },
    /// Ordering constraints form a loop.
    #[error("dependency cycle: {}", .0.iter().map(ModId::as_str).collect::<Vec<_>>().join(" -> "))]
    Cycle(Vec<ModId>),
}

/// Sorts mods into the order the data stage runs them.
///
/// Dependencies come first (or last, for [`LoadOrder::Before`]), and mods with
/// no constraint between them run in alphabetical id order. The alphabetical
/// tie-break is not cosmetic: it is what makes a load order reproducible
/// between two installs of the same mod set, which is what makes registry ids
/// reproducible, which is what makes a save file portable.
///
/// # Errors
///
/// A [`LoadOrderError`] for a duplicate id, a missing or mis-versioned required
/// dependency, an installed incompatibility, or a cycle.
pub fn resolve_load_order(manifests: &[ModManifest]) -> Result<Vec<ModId>, LoadOrderError> {
    let mut installed: BTreeMap<&ModId, &ModManifest> = BTreeMap::new();
    for manifest in manifests {
        if installed.insert(&manifest.id, manifest).is_some() {
            return Err(LoadOrderError::Duplicate(manifest.id.clone()));
        }
    }

    // Presence, version and incompatibility are checked before any sorting, so
    // a broken mod set reports what is wrong with it rather than a cycle in a
    // graph the user cannot see.
    for manifest in manifests {
        for dep in &manifest.dependencies {
            let present = installed.get(&dep.id);
            match (dep.kind, present) {
                (DependencyKind::Incompatible, Some(_)) => {
                    return Err(LoadOrderError::Incompatible {
                        dependent: manifest.id.clone(),
                        conflict: dep.id.clone(),
                    });
                }
                (DependencyKind::Required, None) => {
                    return Err(LoadOrderError::MissingDependency {
                        dependent: manifest.id.clone(),
                        missing: dep.id.clone(),
                    });
                }
                (DependencyKind::Required | DependencyKind::Optional, Some(other)) => {
                    if let Some(req) = &dep.req
                        && !req.matches(&other.version)
                    {
                        return Err(LoadOrderError::VersionMismatch {
                            dependent: manifest.id.clone(),
                            dependency: dep.id.clone(),
                            req: req.clone(),
                            found: other.version.clone(),
                        });
                    }
                }
                _ => {}
            }
        }
    }

    // Edges point from the mod that loads first to the mod that loads second.
    let mut after: BTreeMap<&ModId, BTreeSet<&ModId>> =
        installed.keys().map(|id| (*id, BTreeSet::new())).collect();
    let mut incoming: BTreeMap<&ModId, usize> = installed.keys().map(|id| (*id, 0)).collect();

    for manifest in manifests {
        for dep in &manifest.dependencies {
            if dep.kind == DependencyKind::Incompatible || dep.order == LoadOrder::None {
                continue;
            }
            let Some(other) = installed.get(&dep.id) else {
                continue;
            };
            let (first, second) = match dep.order {
                LoadOrder::After => (&other.id, &manifest.id),
                LoadOrder::Before => (&manifest.id, &other.id),
                LoadOrder::None => unreachable!("filtered above"),
            };
            if after
                .get_mut(first)
                .expect("every installed mod has a bucket")
                .insert(second)
            {
                *incoming
                    .get_mut(second)
                    .expect("every installed mod has a count") += 1;
            }
        }
    }

    // Kahn's algorithm over a `BTreeSet` frontier, so the next mod out is
    // always the alphabetically smallest of the ones that are ready.
    let mut ready: BTreeSet<&ModId> = incoming
        .iter()
        .filter(|(_, count)| **count == 0)
        .map(|(id, _)| *id)
        .collect();
    let mut order = Vec::with_capacity(installed.len());
    while let Some(next) = ready.iter().next().copied() {
        ready.remove(next);
        order.push(next.clone());
        for dependent in &after[next] {
            let count = incoming
                .get_mut(dependent)
                .expect("every installed mod has a count");
            *count -= 1;
            if *count == 0 {
                ready.insert(dependent);
            }
        }
    }

    if order.len() != installed.len() {
        return Err(LoadOrderError::Cycle(find_cycle(&after, &incoming)));
    }
    Ok(order)
}

/// Names one loop among the mods the sort could not place.
fn find_cycle(
    after: &BTreeMap<&ModId, BTreeSet<&ModId>>,
    incoming: &BTreeMap<&ModId, usize>,
) -> Vec<ModId> {
    let stuck: BTreeSet<&ModId> = incoming
        .iter()
        .filter(|(_, count)| **count > 0)
        .map(|(id, _)| *id)
        .collect();
    let mut path: Vec<&ModId> = Vec::new();
    let mut current = *stuck.iter().next().expect("a cycle has at least one mod");
    loop {
        if let Some(start) = path.iter().position(|seen| *seen == current) {
            let mut cycle: Vec<ModId> = path[start..].iter().map(|id| (*id).clone()).collect();
            cycle.push(current.clone());
            return cycle;
        }
        path.push(current);
        current = after[current]
            .iter()
            .find(|next| stuck.contains(*next))
            .copied()
            .expect("a stuck mod has a stuck successor");
    }
}

#[allow(clippy::unwrap_used)]
#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use semver::{Version, VersionReq};

    use super::{
        Dependency, DependencyKind, LoadOrder, LoadOrderError, ModId, ModManifest,
        resolve_load_order,
    };

    fn mod_id(s: &str) -> ModId {
        ModId::new(s).unwrap()
    }

    fn manifest(id: &str, deps: Vec<Dependency>) -> ModManifest {
        ModManifest {
            id: mod_id(id),
            name: id.to_owned(),
            version: Version::new(1, 0, 0),
            api_version: 1,
            dependencies: deps,
            entry: super::Entry::default(),
            assets: None,
        }
    }

    fn order(manifests: &[ModManifest]) -> Vec<String> {
        resolve_load_order(manifests)
            .unwrap()
            .iter()
            .map(ToString::to_string)
            .collect()
    }

    #[test]
    fn mod_ids_use_the_namespace_character_set() {
        assert_eq!(mod_id("copper_chest").as_str(), "copper_chest");
        assert!(ModId::new("Copper").is_err());
        assert!(ModId::new("copper:chest").is_err());
        assert!(ModId::new("").is_err());
        assert_eq!("a-b.c".parse::<ModId>().unwrap().as_ref(), "a-b.c");
    }

    #[test]
    fn a_full_manifest_parses() {
        let manifest = ModManifest::parse(
            "mod.toml",
            r#"
id = "copper_chest"
name = "Copper Chest"
version = "1.2.0"
api_version = 1
assets = "assets"

[entry]
data = "data.lua"
control = "control.lua"

[[dependencies]]
id = "slotted"
req = ">=0.1"

[[dependencies]]
id = "sorter"
kind = "optional"
order = "before"

[[dependencies]]
id = "old_chest"
kind = "incompatible"
"#,
        )
        .unwrap();

        assert_eq!(manifest.id, mod_id("copper_chest"));
        assert_eq!(manifest.name, "Copper Chest");
        assert_eq!(manifest.version, Version::new(1, 2, 0));
        assert_eq!(manifest.api_version, 1);
        assert_eq!(manifest.assets.as_deref(), Some("assets"));
        assert_eq!(manifest.entry.data.as_deref(), Some("data.lua"));
        assert_eq!(manifest.entry.control.as_deref(), Some("control.lua"));
        assert_eq!(manifest.dependencies.len(), 3);
        assert_eq!(
            manifest.dependencies[0].req,
            Some(VersionReq::parse(">=0.1").unwrap())
        );
        assert_eq!(manifest.dependencies[0].kind, DependencyKind::Required);
        assert_eq!(manifest.dependencies[0].order, LoadOrder::After);
        assert_eq!(manifest.dependencies[1].order, LoadOrder::Before);
        assert_eq!(manifest.deps_of(DependencyKind::Incompatible).count(), 1);
    }

    #[test]
    fn a_minimal_manifest_parses() {
        let manifest = ModManifest::parse(
            "mod.toml",
            "id = \"base\"\nname = \"Base\"\nversion = \"0.1.0\"\napi_version = 1\n",
        )
        .unwrap();
        assert!(manifest.dependencies.is_empty());
        assert_eq!(manifest.entry, super::Entry::default());
    }

    #[test]
    fn a_broken_manifest_names_its_file() {
        let err = ModManifest::parse("mods/x/mod.toml", "id = \"Bad\"").unwrap_err();
        assert!(err.to_string().starts_with("mods/x/mod.toml: "), "{err}");
        let missing = ModManifest::parse("mod.toml", "id = \"a\"").unwrap_err();
        assert!(missing.message.contains("name"), "{}", missing.message);
    }

    #[test]
    fn independent_mods_sort_alphabetically() {
        let manifests = vec![
            manifest("zebra", vec![]),
            manifest("alpha", vec![]),
            manifest("middle", vec![]),
        ];
        assert_eq!(order(&manifests), ["alpha", "middle", "zebra"]);
    }

    #[test]
    fn a_dependency_loads_before_its_dependent() {
        let manifests = vec![
            manifest("aaa", vec![Dependency::required(mod_id("zzz"))]),
            manifest("zzz", vec![]),
        ];
        assert_eq!(
            order(&manifests),
            ["zzz", "aaa"],
            "the dependency wins over the alphabet"
        );
    }

    #[test]
    fn before_puts_the_dependent_first() {
        let manifests = vec![
            manifest(
                "zzz",
                vec![Dependency {
                    order: LoadOrder::Before,
                    ..Dependency::required(mod_id("aaa"))
                }],
            ),
            manifest("aaa", vec![]),
        ];
        assert_eq!(order(&manifests), ["zzz", "aaa"]);
    }

    #[test]
    fn an_order_of_none_checks_presence_without_constraining_order() {
        let manifests = vec![
            manifest(
                "zzz",
                vec![Dependency {
                    order: LoadOrder::None,
                    ..Dependency::required(mod_id("aaa"))
                }],
            ),
            manifest("aaa", vec![]),
        ];
        assert_eq!(order(&manifests), ["aaa", "zzz"]);
    }

    #[test]
    fn an_optional_dependency_orders_when_present_and_is_ignored_when_not() {
        let with = vec![
            manifest("aaa", vec![Dependency::optional(mod_id("zzz"))]),
            manifest("zzz", vec![]),
        ];
        assert_eq!(order(&with), ["zzz", "aaa"]);

        let without = vec![manifest("aaa", vec![Dependency::optional(mod_id("zzz"))])];
        assert_eq!(order(&without), ["aaa"]);
    }

    #[test]
    fn a_missing_required_dependency_is_an_error() {
        let manifests = vec![manifest("aaa", vec![Dependency::required(mod_id("zzz"))])];
        assert_eq!(
            resolve_load_order(&manifests).unwrap_err(),
            LoadOrderError::MissingDependency {
                dependent: mod_id("aaa"),
                missing: mod_id("zzz"),
            }
        );
    }

    #[test]
    fn a_version_that_does_not_match_is_an_error() {
        let manifests = vec![
            manifest(
                "aaa",
                vec![Dependency {
                    req: Some(VersionReq::parse(">=2").unwrap()),
                    ..Dependency::required(mod_id("zzz"))
                }],
            ),
            manifest("zzz", vec![]),
        ];
        let err = resolve_load_order(&manifests).unwrap_err();
        assert!(
            matches!(err, LoadOrderError::VersionMismatch { .. }),
            "{err}"
        );
        assert!(err.to_string().contains("1.0.0 is installed"), "{err}");
    }

    #[test]
    fn an_installed_incompatibility_is_an_error() {
        let manifests = vec![
            manifest("aaa", vec![Dependency::incompatible(mod_id("zzz"))]),
            manifest("zzz", vec![]),
        ];
        assert_eq!(
            resolve_load_order(&manifests).unwrap_err(),
            LoadOrderError::Incompatible {
                dependent: mod_id("aaa"),
                conflict: mod_id("zzz"),
            }
        );
        let alone = vec![manifest(
            "aaa",
            vec![Dependency::incompatible(mod_id("zzz"))],
        )];
        assert_eq!(order(&alone), ["aaa"]);
    }

    #[test]
    fn a_cycle_is_an_error_that_names_the_loop() {
        let manifests = vec![
            manifest("aaa", vec![Dependency::required(mod_id("bbb"))]),
            manifest("bbb", vec![Dependency::required(mod_id("ccc"))]),
            manifest("ccc", vec![Dependency::required(mod_id("aaa"))]),
        ];
        let err = resolve_load_order(&manifests).unwrap_err();
        let LoadOrderError::Cycle(path) = &err else {
            panic!("expected a cycle, got {err}");
        };
        assert_eq!(path.first(), path.last());
        assert_eq!(path.len(), 4);
    }

    #[test]
    fn a_mod_installed_twice_is_an_error() {
        let manifests = vec![manifest("aaa", vec![]), manifest("aaa", vec![])];
        assert_eq!(
            resolve_load_order(&manifests).unwrap_err(),
            LoadOrderError::Duplicate(mod_id("aaa"))
        );
    }

    #[test]
    fn a_diamond_keeps_the_alphabetical_tie_break() {
        let manifests = vec![
            manifest("base", vec![]),
            manifest("zeta", vec![Dependency::required(mod_id("base"))]),
            manifest("beta", vec![Dependency::required(mod_id("base"))]),
            manifest(
                "top",
                vec![
                    Dependency::required(mod_id("zeta")),
                    Dependency::required(mod_id("beta")),
                ],
            ),
        ];
        assert_eq!(order(&manifests), ["base", "beta", "zeta", "top"]);
    }

    #[test]
    fn a_manifest_survives_a_serde_round_trip() {
        let manifest = manifest("aaa", vec![Dependency::optional(mod_id("bbb"))]);
        let text = toml::to_string(&manifest).unwrap();
        assert_eq!(ModManifest::parse("mod.toml", &text).unwrap(), manifest);
    }
}
