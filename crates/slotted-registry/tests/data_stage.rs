//! End-to-end: real files on disk, through both adapters, to frozen registries.
//!
//! The fixture under `tests/fixtures/` is two mods. `base` registers items,
//! tags, a recipe type, a shaped recipe and a placeholder screen. `copper`
//! depends on it and patches it in all three rounds, which is the whole data
//! lifecycle in one directory tree.

#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};

use pretty_assertions::assert_eq;
use slotted_model::{ItemId, Namespaced};
use slotted_registry::defs::Rarity;
use slotted_registry::loader::{DataStage, DirSource, InMemorySource, Loaded};
use slotted_registry::manifest::{ModId, ModManifest, resolve_load_order};
use slotted_registry::registry::RegistryKind;

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn id(s: &str) -> Namespaced {
    Namespaced::parse(s).unwrap()
}

/// Reads every fixture manifest and sorts them the way a game would at start.
fn load_order() -> Vec<ModId> {
    let mut manifests = Vec::new();
    let mods = fixtures().join("mods");
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(&mods)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    dirs.sort();
    for dir in dirs {
        let path = dir.join("mod.toml");
        let text = std::fs::read_to_string(&path).unwrap();
        manifests.push(ModManifest::parse(&path.display().to_string(), &text).unwrap());
    }
    resolve_load_order(&manifests).unwrap()
}

/// Mirrors the fixture tree into an [`InMemorySource`], which is what a wasm
/// build does with assets baked into the binary.
fn in_memory() -> InMemorySource {
    let root = fixtures();
    let mut source = InMemorySource::new();
    let mut stack = vec![root.join("data")];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                stack.push(path);
            } else {
                let logical = path
                    .strip_prefix(&root)
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .replace('\\', "/");
                source.insert(logical, std::fs::read(&path).unwrap());
            }
        }
    }
    source
}

#[test]
fn the_fixture_manifests_sort_dependencies_first() {
    assert_eq!(
        load_order(),
        vec![ModId::new("base").unwrap(), ModId::new("copper").unwrap()],
        "copper depends on base, so base loads first"
    );
}

#[test]
fn the_fixture_loads_the_same_through_both_adapters() {
    let order = load_order();
    let from_memory = DataStage::new(order.clone()).load(&in_memory()).unwrap();
    let from_disk = DataStage::new(order)
        .load(&DirSource::new(fixtures()))
        .unwrap();
    assert_eq!(
        from_memory, from_disk,
        "the port is the only thing the data stage sees"
    );
    check(&from_disk);
}

/// Everything the fixture is supposed to produce.
fn check(loaded: &Loaded) {
    let registries = &loaded.registries;

    // Ids are the load order: base's two items, then copper's one.
    assert_eq!(registries.items.len(), 3);
    assert_eq!(registries.item_id(&id("base:chest")), Some(ItemId(0)));
    assert_eq!(registries.item_id(&id("base:plank")), Some(ItemId(1)));
    assert_eq!(registries.item_id(&id("copper:chest")), Some(ItemId(2)));
    assert_eq!(
        registries.items.name_of(ItemId(2)),
        Some(&id("copper:chest"))
    );

    // Three rounds of patches, last one wins.
    let chest = registries.items.get(ItemId(0)).unwrap();
    assert_eq!(chest.max_stack_size, 48, "final_fixes beat updates");
    assert_eq!(chest.rarity, Rarity::Rare);
    assert_eq!(
        chest.tags,
        vec![id("copper:containers"), id("base:storage")],
        "the patch inserted at index 0"
    );
    assert_eq!(
        chest.display_name.as_deref(),
        Some("Chest"),
        "a merge leaves untouched fields alone"
    );
    assert_eq!(chest.components.len(), 1);

    // Component keys are interned at freeze.
    assert!(registries.components.contains(&id("base:capacity")));

    // Tag inheritance, plus the entry copper patched in during round one.
    let mut craftable: Vec<_> = registries
        .items_with(&id("base:craftable"))
        .iter()
        .copied()
        .collect();
    craftable.sort();
    assert_eq!(craftable, vec![ItemId(0), ItemId(1), ItemId(2)]);
    assert_eq!(
        registries.items_with(&id("base:planks")),
        &[ItemId(1)].into_iter().collect()
    );
    assert!(registries.tag_index.has_tag(ItemId(2), &id("base:storage")));

    // The shaped recipe is indexed both ways, and through the tag.
    let recipe = registries.recipes.id_of(&id("base:chest")).unwrap();
    assert!(registries.recipes.get(recipe).unwrap().is_shaped());
    assert_eq!(registries.recipe_index.for_output(ItemId(0)), [recipe]);
    assert_eq!(
        registries.recipe_index.uses(ItemId(1)),
        [recipe],
        "the plank is reached through #base:planks"
    );
    let crafting = registries.recipe_types.id_of(&id("base:crafting")).unwrap();
    assert_eq!(registries.recipe_types.get(crafting).unwrap().size, (3, 3));
    assert_eq!(registries.recipe_index.by_type(crafting), [recipe]);

    // The placeholder registry kept its payload.
    assert_eq!(registries.screens.len(), 1);
    assert!(registries.screens.get_by_name(&id("base:chest")).is_some());

    // The report says what happened.
    let report = &loaded.report;
    assert_eq!(report.mods.len(), 2);
    assert_eq!(report.entry_files.len(), 8);
    assert_eq!(report.patch_files.len(), 3, "one file per round");
    assert_eq!(report.patches_applied, 4);
    assert_eq!(report.files_read(), 11);
    assert_eq!(report.entries[&RegistryKind::Items], 3);
    assert_eq!(report.entries[&RegistryKind::Tags], 2);
    assert_eq!(report.entries[&RegistryKind::Screens], 1);
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    assert_eq!(report.total_entries(), 8);
}

#[test]
fn a_dir_source_refuses_to_climb_out_of_its_root() {
    let source = DirSource::new(fixtures());
    assert_eq!(source.root(), fixtures());
    let escaped = slotted_registry::loader::AssetSource::read(&source, "../../Cargo.toml");
    assert!(escaped.is_err(), "a mod cannot read outside its root");
}

/// The `fluids` registry (Phase 6): a fluid loads like any other entry, keeps
/// its payload untouched for `slotted_ui::FluidDef` to type, and takes its
/// dense id from registration order.
#[test]
fn fluids_load_with_their_payload_intact_and_dense_ids() {
    use slotted_registry::loader::InMemorySource;
    use slotted_registry::manifest::ModId;

    let source = InMemorySource::new()
        .with(
            "data/base/fluids/water.ron",
            r##"(name: "base:water", payload: (color: "#3B7DD8B0", unit: "mB"))"##,
        )
        .with(
            "data/base/fluids/lava.ron",
            r##"(name: "base:lava", payload: (color: "#D2601AFF"))"##,
        );
    let loaded = DataStage::new(vec![ModId::new("base").unwrap()])
        .load(&source)
        .unwrap();
    let fluids = &loaded.registries.fluids;
    assert_eq!(fluids.len(), 2);

    // Files are read in sorted order, so `lava` is registered first and the
    // dense ids follow the load, not the alphabet of the ids themselves.
    let ids: Vec<&Namespaced> = fluids.iter().map(|(_, name, _)| name).collect();
    assert_eq!(ids, vec![&id("base:lava"), &id("base:water")]);

    let water = fluids.get_by_name(&id("base:water")).unwrap();
    assert!(
        !matches!(water.payload, slotted_registry::Value::Unit),
        "the payload survives the data stage untouched"
    );
    assert_eq!(water.name, id("base:water"));
}
