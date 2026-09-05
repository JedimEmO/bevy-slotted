//! Adversarial review pass over load-order resolution, tag inheritance, the
//! recipe index and the patch applier.

#![allow(clippy::unwrap_used)]

use pretty_assertions::assert_eq;
use slotted_model::Namespaced;
use slotted_registry::defs::{
    Ingredient, ItemDef, ItemResult, RecipeDef, RecipeTypeDef, TagDef, TagEntry,
};
use slotted_registry::manifest::{LoadOrderError, ModId, ModManifest, resolve_load_order};
use slotted_registry::patch::{Patch, PatchError, PatchOp, RawEntries, apply};
use slotted_registry::registry::{Registries, RegistryError, RegistryKind, Warning};

fn id(s: &str) -> Namespaced {
    Namespaced::parse(s).unwrap()
}

/// A manifest with the given id and dependency lines.
fn manifest(name: &str, deps: &str) -> ModManifest {
    let text =
        format!("id = \"{name}\"\nname = \"{name}\"\nversion = \"1.0.0\"\napi_version = 1\n{deps}");
    ModManifest::parse(name, &text).unwrap()
}

fn dep(other: &str, order: &str) -> String {
    format!("\n[[dependencies]]\nid = \"{other}\"\norder = \"{order}\"\n")
}

fn order_of(manifests: &[ModManifest]) -> Vec<String> {
    resolve_load_order(manifests)
        .unwrap()
        .into_iter()
        .map(|m| m.as_str().to_owned())
        .collect()
}

// ----- load order ----------------------------------------------------------

#[test]
fn a_diamond_dependency_resolves_to_one_reproducible_order() {
    // d needs b and c; both need a. Only a-first and d-last are forced, so the
    // alphabetical tie-break has to settle b before c, every time.
    let mods = [
        manifest("d", &(dep("b", "after") + &dep("c", "after"))),
        manifest("c", &dep("a", "after")),
        manifest("b", &dep("a", "after")),
        manifest("a", ""),
    ];
    assert_eq!(order_of(&mods), ["a", "b", "c", "d"]);
    // Input order must not matter.
    let reversed: Vec<ModManifest> = mods.iter().rev().cloned().collect();
    assert_eq!(order_of(&reversed), ["a", "b", "c", "d"]);
}

#[test]
fn a_before_hint_puts_the_declaring_mod_first() {
    // `zulu` says alpha loads *after* it, which inverts the alphabetical order.
    let mods = [
        manifest("zulu", &dep("alpha", "before")),
        manifest("alpha", ""),
    ];
    assert_eq!(order_of(&mods), ["zulu", "alpha"]);
}

#[test]
fn before_and_after_hints_can_be_mixed_in_one_graph() {
    // core loads first, then zulu (which core must precede and which must
    // precede alpha), then alpha.
    let mods = [
        manifest("zulu", &(dep("core", "after") + &dep("alpha", "before"))),
        manifest("alpha", ""),
        manifest("core", ""),
    ];
    assert_eq!(order_of(&mods), ["core", "zulu", "alpha"]);
}

#[test]
fn contradictory_before_and_after_hints_report_a_cycle() {
    let mods = [
        manifest("a", &dep("b", "before")),
        manifest("b", &dep("a", "before")),
    ];
    assert!(matches!(
        resolve_load_order(&mods),
        Err(LoadOrderError::Cycle(_))
    ));
}

#[test]
fn an_optional_dependency_only_orders_when_it_is_installed() {
    let text = "id = \"zulu\"\nname = \"zulu\"\nversion = \"1.0.0\"\napi_version = 1\n\
                \n[[dependencies]]\nid = \"alpha\"\nkind = \"optional\"\norder = \"before\"\n";
    let zulu = ModManifest::parse("zulu", text).unwrap();
    assert_eq!(order_of(std::slice::from_ref(&zulu)), ["zulu"]);
    assert_eq!(order_of(&[zulu, manifest("alpha", "")]), ["zulu", "alpha"]);
}

#[test]
fn a_mod_installed_twice_is_reported_rather_than_silently_deduplicated() {
    let mods = [manifest("a", ""), manifest("a", "")];
    assert_eq!(
        resolve_load_order(&mods),
        Err(LoadOrderError::Duplicate(ModId::new("a").unwrap()))
    );
}

// ----- tag inheritance -----------------------------------------------------

fn registries_with_tag_chain(chain: &[(&str, &str)]) -> Result<(), RegistryError> {
    let mut reg = Registries::new();
    reg.add_item(ItemDef::new(id("demo:stone"))).unwrap();
    for (name, parent) in chain {
        let mut tag = TagDef::new(id(name));
        tag.values.push(TagEntry::Tag(id(parent)));
        reg.add_tag(tag);
    }
    reg.freeze().map(|_| ())
}

#[test]
fn a_tag_that_includes_itself_is_an_error_not_a_hang() {
    let err = registries_with_tag_chain(&[("demo:a", "demo:a")]).unwrap_err();
    assert!(matches!(err, RegistryError::TagCycle(_)), "{err}");
}

#[test]
fn a_tag_that_includes_itself_indirectly_is_an_error_not_a_hang() {
    let err = registries_with_tag_chain(&[
        ("demo:a", "demo:b"),
        ("demo:b", "demo:c"),
        ("demo:c", "demo:a"),
    ])
    .unwrap_err();
    let RegistryError::TagCycle(path) = err else {
        panic!("expected a tag cycle, got {err}");
    };
    assert!(path.len() >= 2, "the cycle names its path: {path:?}");
}

#[test]
fn a_diamond_of_tags_resolves_without_duplicating_members() {
    let mut reg = Registries::new();
    reg.add_item(ItemDef::new(id("demo:oak"))).unwrap();
    reg.add_item(ItemDef::new(id("demo:birch"))).unwrap();

    let mut left = TagDef::new(id("demo:left"));
    left.values.push(TagEntry::Item(id("demo:oak")));
    reg.add_tag(left);
    let mut right = TagDef::new(id("demo:right"));
    right.values.push(TagEntry::Item(id("demo:oak")));
    right.values.push(TagEntry::Item(id("demo:birch")));
    reg.add_tag(right);
    let mut top = TagDef::new(id("demo:top"));
    top.values.push(TagEntry::Tag(id("demo:left")));
    top.values.push(TagEntry::Tag(id("demo:right")));
    reg.add_tag(top);

    let (frozen, warnings) = reg.freeze().unwrap();
    assert!(warnings.is_empty());
    assert_eq!(frozen.items_with(&id("demo:top")).len(), 2);
}

#[test]
fn an_unknown_parent_tag_is_a_warning_and_the_rest_still_resolves() {
    let mut reg = Registries::new();
    reg.add_item(ItemDef::new(id("demo:oak"))).unwrap();
    let mut tag = TagDef::new(id("demo:planks"));
    tag.values.push(TagEntry::Item(id("demo:oak")));
    tag.values.push(TagEntry::Tag(id("absent:planks")));
    reg.add_tag(tag);

    let (frozen, warnings) = reg.freeze().unwrap();
    assert_eq!(frozen.items_with(&id("demo:planks")).len(), 1);
    assert!(matches!(
        warnings.as_slice(),
        [Warning::UnknownParentTag { .. }]
    ));
}

// ----- recipe index --------------------------------------------------------

#[test]
fn recipe_usage_reaches_every_item_of_an_inherited_tag() {
    let mut reg = Registries::new();
    reg.add_item(ItemDef::new(id("demo:oak"))).unwrap();
    reg.add_item(ItemDef::new(id("demo:birch"))).unwrap();
    reg.add_item(ItemDef::new(id("demo:chest"))).unwrap();

    let mut inner = TagDef::new(id("demo:soft_planks"));
    inner.values.push(TagEntry::Item(id("demo:birch")));
    reg.add_tag(inner);
    let mut outer = TagDef::new(id("demo:planks"));
    outer.values.push(TagEntry::Item(id("demo:oak")));
    outer.values.push(TagEntry::Tag(id("demo:soft_planks")));
    reg.add_tag(outer);

    reg.add_recipe_type(RecipeTypeDef::new(id("demo:crafting")))
        .unwrap();
    // The same tag twice, which is what a shaped recipe does across two cells.
    reg.add_recipe(RecipeDef::shapeless(
        id("demo:chest"),
        id("demo:crafting"),
        vec![
            Ingredient::Tag(id("demo:planks")),
            Ingredient::Tag(id("demo:planks")),
        ],
        ItemResult::one(id("demo:chest")),
    ))
    .unwrap();

    let (frozen, warnings) = reg.freeze().unwrap();
    assert!(warnings.is_empty(), "{warnings:?}");
    let recipe = frozen
        .recipe_index
        .for_output(frozen.item_id(&id("demo:chest")).unwrap())[0];
    for item in ["demo:oak", "demo:birch"] {
        let uses = frozen.recipe_index.uses(frozen.item_id(&id(item)).unwrap());
        assert_eq!(uses, [recipe], "{item} is reachable exactly once");
    }
}

#[test]
fn a_recipe_with_an_unregistered_type_is_fatal() {
    let mut reg = Registries::new();
    reg.add_item(ItemDef::new(id("demo:chest"))).unwrap();
    reg.add_recipe(RecipeDef::shapeless(
        id("demo:chest"),
        id("demo:nobody_registered_this"),
        vec![],
        ItemResult::one(id("demo:chest")),
    ))
    .unwrap();
    assert!(matches!(
        reg.freeze(),
        Err(RegistryError::UnknownRecipeType { .. })
    ));
}

// ----- patches -------------------------------------------------------------

fn entries() -> RawEntries {
    let mut entries = RawEntries::new();
    entries.insert(
        id("demo:stone"),
        ron::from_str("(max_stack_size: 64, tags: [\"demo:rock\"])").unwrap(),
    );
    entries
}

#[test]
fn removing_a_target_that_is_not_there_errors_without_panicking() {
    let mut entries = entries();
    let patch = Patch::new(id("demo:absent"), PatchOp::Remove);
    assert_eq!(
        apply(&mut entries, &patch),
        Err(PatchError::UnknownTarget {
            target: id("demo:absent"),
            registry: RegistryKind::Items,
        })
    );
    assert_eq!(entries.len(), 1, "the failed patch changed nothing");
}

#[test]
fn removing_the_same_target_twice_errors_the_second_time() {
    let mut entries = entries();
    let patch = Patch::new(id("demo:stone"), PatchOp::Remove);
    assert_eq!(apply(&mut entries, &patch), Ok(()));
    assert!(apply(&mut entries, &patch).is_err());
    assert!(entries.is_empty());
}

#[test]
fn a_list_op_past_the_end_reports_the_range_rather_than_panicking() {
    let mut entries = entries();
    let patch = Patch::new(
        id("demo:stone"),
        PatchOp::RemoveListItem {
            path: "tags".to_owned(),
            index: 9,
        },
    );
    assert!(matches!(
        apply(&mut entries, &patch),
        Err(PatchError::IndexOutOfRange { len: 1, .. })
    ));
    let insert = Patch::new(
        id("demo:stone"),
        PatchOp::InsertListItem {
            path: "tags".to_owned(),
            index: Some(9),
            value: ron::from_str("\"demo:other\"").unwrap(),
        },
    );
    assert!(matches!(
        apply(&mut entries, &insert),
        Err(PatchError::IndexOutOfRange { .. })
    ));
}

#[test]
fn a_list_op_on_a_missing_path_errors() {
    let mut entries = entries();
    let patch = Patch::new(
        id("demo:stone"),
        PatchOp::RemoveListItem {
            path: "no.such.path".to_owned(),
            index: 0,
        },
    );
    assert!(apply(&mut entries, &patch).is_err());
}
