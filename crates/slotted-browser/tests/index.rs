//! The index: what it contains, how the search cache invalidates, and the
//! asynchronous build behind `IndexState`.

mod common;

use std::time::Instant;

use bevy::prelude::*;
use slotted_browser::index::Bitset;
use slotted_browser::{
    BrowserConfig, BrowserRuntime, HiddenEntries, IndexReady, IndexState, Ingredient, SearchCache,
    SearchChanged, SlottedBrowserPlugin,
};
use slotted_ecs::Registries;

#[test]
fn every_item_and_tag_becomes_an_entry() {
    let index = common::index();
    let registries = common::registries();
    let items = registries.items.len();
    let tags = registries.tag_index.len();
    assert_eq!(index.len(), items + tags, "items plus one entry per tag");

    let coal = index
        .id_of(&Ingredient::item(common::item("minecraft:coal")))
        .expect("coal is indexed");
    let entry = index.get(coal).expect("entry exists");
    assert_eq!(entry.display, "Coal");
    assert_eq!(entry.mod_ns, "minecraft");
    assert_eq!(entry.tags, ["c:coal"]);
    assert_eq!(
        entry.categories,
        ["demo:smelting"],
        "coal is produced by the smelting recipe"
    );
}

#[test]
fn a_subtype_expands_an_item_into_several_entries() {
    use std::sync::Arc;

    use slotted_browser::{SubtypeInterpreter, SubtypeKey, Subtypes};
    use slotted_model::{ItemId, ItemStack};
    use slotted_registry::FrozenRegistries;

    /// Two variants of every item it is registered for.
    struct TwoVariants;
    impl SubtypeInterpreter for TwoVariants {
        fn key(&self, _stack: &ItemStack, _r: &FrozenRegistries) -> SubtypeKey {
            SubtypeKey::new("a")
        }
        fn variants(&self, _item: ItemId, _r: &FrozenRegistries) -> Vec<SubtypeKey> {
            vec![SubtypeKey::new("a"), SubtypeKey::new("b")]
        }
    }

    let mut subtypes = Subtypes::default();
    subtypes.register(common::id("minecraft:iron_pickaxe"), Arc::new(TwoVariants));
    let mut inputs = common::inputs();
    inputs.subtypes = subtypes;
    let index = slotted_browser::BrowserIndex::build(&inputs);
    assert_eq!(index.len(), common::index().len() + 1, "one extra variant");
    assert!(
        index
            .id_of(&Ingredient::item_with(
                common::item("minecraft:iron_pickaxe"),
                SubtypeKey::new("b"),
            ))
            .is_some()
    );
}

#[test]
fn the_search_cache_keys_on_the_hidden_and_visibility_versions() {
    let mut cache = SearchCache::default();
    let result = std::sync::Arc::new(Vec::new());
    cache.put("iron", 1, 0, result.clone());
    assert!(cache.get("iron", 1, 0).is_some());
    assert!(cache.get("iron", 2, 0).is_none(), "hidden set changed");
    assert!(cache.get("iron", 1, 1).is_none(), "visibility changed");
    assert!(cache.get("coal", 1, 0).is_none(), "query changed");
    cache.invalidate();
    assert!(cache.get("iron", 1, 0).is_none());
}

#[test]
fn hiding_an_entry_bumps_the_version_without_a_rebuild() {
    let index = common::index();
    let mut hidden = HiddenEntries {
        set: Bitset::new(index.len()),
        version: 0,
    };
    let coal = index
        .id_of(&Ingredient::item(common::item("minecraft:coal")))
        .expect("coal is indexed");
    hidden.set_hidden(coal, true);
    assert!(hidden.set.contains(coal.index()));
    assert_eq!(hidden.version, 1);
    hidden.set_hidden(coal, false);
    assert!(!hidden.set.contains(coal.index()));
    assert_eq!(hidden.version, 2, "unhiding invalidates the cache too");
}

// ---------------------------------------------------------------------------
// The asynchronous build.
// ---------------------------------------------------------------------------

fn app() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(Registries(common::registries()))
        .add_plugins(SlottedBrowserPlugin {
            config: BrowserConfig {
                ui: false,
                ..default()
            },
        });
    app
}

/// Steps until the index is ready, returning how many `IndexReady` messages
/// were written on the way. A cursor is used rather than the current-update
/// buffer, which holds a message for two frames.
fn settle(app: &mut App) -> usize {
    let mut cursor = app
        .world_mut()
        .resource_mut::<Messages<IndexReady>>()
        .get_cursor();
    let mut ready = 0;
    for _ in 0..600 {
        app.update();
        ready += cursor
            .read(app.world().resource::<Messages<IndexReady>>())
            .count();
        if app.world().resource::<IndexState>().ready().is_some() {
            // Two more frames, so a repeat would be noticed and the search has
            // certainly seen the message.
            app.update();
            app.update();
            ready += cursor
                .read(app.world().resource::<Messages<IndexReady>>())
                .count();
            return ready;
        }
    }
    panic!("the index never became ready");
}

#[test]
fn the_build_runs_off_the_main_thread_and_reports_once() {
    let mut app = app();
    app.update();
    let ready = settle(&mut app);
    assert_eq!(ready, 1, "IndexReady fires exactly once per build");

    let world = app.world();
    let index = world
        .resource::<IndexState>()
        .ready()
        .expect("ready")
        .clone();
    assert_eq!(index.len(), common::index().len());
    let visible = &world.resource::<BrowserRuntime>().visible;
    assert_eq!(visible.len(), index.len(), "everything is visible at rest");
}

#[test]
fn a_search_message_narrows_the_visible_set() {
    let mut app = app();
    app.update();
    settle(&mut app);

    app.world_mut().write_message(SearchChanged {
        text: "iron".to_owned(),
    });
    app.update();

    let runtime = app.world().resource::<BrowserRuntime>();
    assert_eq!(runtime.filter_text, "iron");
    let index = app
        .world()
        .resource::<IndexState>()
        .ready()
        .expect("ready")
        .clone();
    let names: Vec<&str> = runtime
        .visible
        .iter()
        .filter_map(|id| index.get(*id).map(|e| e.display.as_str()))
        .collect();
    assert_eq!(
        names,
        ["Iron Ingot", "Iron Pickaxe", "#c:ingots", "#c:tools"],
        "tooltip search is on by default, and a tag's tooltip lists its members"
    );
}

#[test]
#[ignore = "performance; run with --ignored in release"]
fn fifty_thousand_entries_build_quickly() {
    let inputs = common::synthetic(50_000);
    let start = Instant::now();
    let index = slotted_browser::BrowserIndex::build(&inputs);
    let elapsed = start.elapsed();
    assert_eq!(index.len(), 50_000 + 10);
    assert!(
        elapsed.as_secs_f64() < 3.0,
        "50k entries took {elapsed:?}; the budget is one second in release"
    );
}

// ---------------------------------------------------------------------------
// The recipe store: what a category files, and the two lookups the recipe view
// makes.
// ---------------------------------------------------------------------------

#[test]
fn recipes_are_filed_under_the_default_category_of_their_type() {
    use slotted_browser::CategoryId;

    let categories = common::categories();
    let store = slotted_browser::RecipeStore::build(&common::registries(), &categories);
    assert_eq!(store.len(), 3);
    assert_eq!(
        store.in_category(&CategoryId::new("demo:crafting")).len(),
        2
    );
    assert_eq!(
        store.in_category(&CategoryId::new("demo:smelting")).len(),
        1
    );
    // A 3x3 type gets a grid category three cells tall.
    let height = categories
        .for_recipe_type(&common::id("demo:crafting"))
        .expect("bound")
        .size()
        .y;
    let cell = slotted_ui::widgets::SLOT_SIZE + slotted_browser::category::SLOT_GAP;
    assert!((height - 3.0 * cell).abs() < f32::EPSILON);
}

#[test]
fn for_output_and_uses_group_by_category_and_follow_tags() {
    use slotted_browser::CategoryId;

    let categories = common::categories();
    let registries = common::registries();
    let store = slotted_browser::RecipeStore::build(&registries, &categories);

    let pickaxe = Ingredient::item(common::item("minecraft:iron_pickaxe"));
    let produced = store.recipes_for(&pickaxe, &registries, &categories);
    assert_eq!(produced.len(), 1);
    assert_eq!(produced[0].0, CategoryId::new("demo:crafting"));
    assert_eq!(produced[0].1.len(), 1);
    assert!(
        store
            .recipes_for(
                &Ingredient::item(common::item("minecraft:dirt")),
                &registries,
                &categories
            )
            .is_empty(),
        "nothing makes dirt"
    );

    // Planks are consumed by the shaped pickaxe directly, by the sword through
    // `#minecraft:planks`, and by the smelting recipe through the same tag.
    let planks = Ingredient::item(common::item("minecraft:oak_planks"));
    let uses = store.uses(&planks, &registries, &categories);
    let by_category: Vec<(String, usize)> = uses
        .iter()
        .map(|(c, r)| (c.0.to_string(), r.len()))
        .collect();
    assert_eq!(
        by_category,
        [
            ("demo:crafting".to_owned(), 2),
            ("demo:smelting".to_owned(), 1)
        ]
    );

    // A tag ingredient is the union over its members, which here is the same set.
    let tag = Ingredient::tag(common::id("minecraft:planks"));
    assert_eq!(store.uses(&tag, &registries, &categories), uses);
}

#[test]
fn a_shaped_recipe_lays_out_on_the_grid_and_a_shapeless_one_row_major() {
    use bevy::math::Vec2;
    use slotted_browser::{CategoryId, IngredientTypes, SlotRole};
    use slotted_ui::widgets::SLOT_SIZE;

    let categories = common::categories();
    let registries = common::registries();
    let store = slotted_browser::RecipeStore::build(&registries, &categories);
    let types = IngredientTypes::with_builtins();
    let cell = SLOT_SIZE + slotted_browser::category::SLOT_GAP;

    let find = |name: &str| {
        store
            .in_category(&CategoryId::new("demo:crafting"))
            .iter()
            .copied()
            .find(|r| {
                registries
                    .recipes
                    .get(r.0)
                    .is_some_and(|d| d.name == common::id(name))
            })
            .expect("filed under demo:crafting")
    };

    // `["III", " P ", " P "]` on a 3x3 grid.
    let shaped = store
        .layout(
            find("demo:iron_pickaxe"),
            None,
            &registries,
            &categories,
            &types,
        )
        .expect("lays out");
    let inputs: Vec<Vec2> = shaped
        .with_role(SlotRole::Input)
        .map(|(_, s)| s.pos)
        .collect();
    assert_eq!(
        inputs,
        [
            Vec2::new(0.0, 0.0),
            Vec2::new(cell, 0.0),
            Vec2::new(2.0 * cell, 0.0),
            Vec2::new(cell, cell),
            Vec2::new(cell, 2.0 * cell),
        ]
    );
    assert_eq!(shaped.with_role(SlotRole::Output).count(), 1);
    assert!(shaped.arrow.is_some());

    // The shapeless sword fills the first row, and its tag input expands.
    let shapeless = store
        .layout(
            find("demo:diamond_sword"),
            None,
            &registries,
            &categories,
            &types,
        )
        .expect("lays out");
    let inputs: Vec<&slotted_browser::RecipeSlot> = shapeless
        .with_role(SlotRole::Input)
        .map(|(_, s)| s)
        .collect();
    assert_eq!(inputs.len(), 3);
    assert_eq!(inputs[2].pos, Vec2::new(2.0 * cell, 0.0));
    assert_eq!(
        inputs[2].alternatives,
        [Ingredient::item(common::item("minecraft:oak_planks"))],
        "the tag expanded to its one member"
    );

    // The focus moves to the front of the alternatives it appears in.
    let planks = Ingredient::item(common::item("minecraft:oak_planks"));
    let focused = store
        .layout(
            find("demo:diamond_sword"),
            Some(&planks),
            &registries,
            &categories,
            &types,
        )
        .expect("lays out");
    assert_eq!(focused.slots[2].alternatives[0], planks);
}
