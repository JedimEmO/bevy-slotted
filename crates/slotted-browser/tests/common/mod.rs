//! A small frozen registry for the browser tests.
//!
//! `slotted-test` owns the same table as `TestRegistries::basic()`, but the
//! dependency runs the other way (`slotted-test` depends on this crate), so
//! the fixture is duplicated here rather than imported. Keep the two in step:
//! the item ids, tags and three recipes are the ones the contract names.

#![allow(dead_code)]

use std::sync::{Arc, OnceLock};

use slotted_browser::index::BrowserIndex;
use slotted_browser::index::build::BuildInputs;
use slotted_browser::{Categories, IngredientTypes, RecipeStore, Subtypes};
use slotted_model::{ItemId, ItemStack, Namespaced};
use slotted_registry::FrozenRegistries;
use slotted_registry::defs::{
    Ingredient, ItemDef, ItemResult, Rarity, RecipeDef, RecipeTypeDef, TagDef, TagEntry,
};
use slotted_registry::registry::Registries;

struct Row {
    id: &'static str,
    display: &'static str,
    max_stack: u32,
    rarity: Rarity,
    tags: &'static [&'static str],
}

const ITEMS: &[Row] = &[
    Row {
        id: "minecraft:cobblestone",
        display: "Cobblestone",
        max_stack: 64,
        rarity: Rarity::Common,
        tags: &["c:stones"],
    },
    Row {
        id: "minecraft:oak_planks",
        display: "Oak Planks",
        max_stack: 64,
        rarity: Rarity::Common,
        tags: &["minecraft:planks"],
    },
    Row {
        id: "minecraft:dirt",
        display: "Dirt",
        max_stack: 64,
        rarity: Rarity::Common,
        tags: &[],
    },
    Row {
        id: "minecraft:iron_ingot",
        display: "Iron Ingot",
        max_stack: 64,
        rarity: Rarity::Common,
        tags: &["c:ingots"],
    },
    Row {
        id: "minecraft:gold_ingot",
        display: "Gold Ingot",
        max_stack: 64,
        rarity: Rarity::Uncommon,
        tags: &["c:ingots"],
    },
    Row {
        id: "minecraft:diamond",
        display: "Diamond",
        max_stack: 64,
        rarity: Rarity::Rare,
        tags: &["c:gems"],
    },
    Row {
        id: "minecraft:coal",
        display: "Coal",
        max_stack: 64,
        rarity: Rarity::Common,
        tags: &["c:coal"],
    },
    Row {
        id: "minecraft:ender_pearl",
        display: "Ender Pearl",
        max_stack: 16,
        rarity: Rarity::Epic,
        tags: &[],
    },
    Row {
        id: "minecraft:iron_pickaxe",
        display: "Iron Pickaxe",
        max_stack: 1,
        rarity: Rarity::Common,
        tags: &["c:tools"],
    },
    Row {
        id: "minecraft:diamond_sword",
        display: "Diamond Sword",
        max_stack: 1,
        rarity: Rarity::Rare,
        tags: &["c:tools"],
    },
    Row {
        id: "slotted:debug_stick",
        display: "Debug Stick",
        max_stack: 1,
        rarity: Rarity::Legendary,
        tags: &["c:tools", "slotted:dev"],
    },
];

/// Parses a namespaced id, panicking on a typo in a test.
pub fn id(s: &str) -> Namespaced {
    Namespaced::parse(s).unwrap_or_else(|e| panic!("bad id {s:?}: {e}"))
}

static BASIC: OnceLock<Arc<FrozenRegistries>> = OnceLock::new();

/// The eleven fixture items, their tags and the three demo recipes.
pub fn registries() -> Arc<FrozenRegistries> {
    BASIC.get_or_init(build).clone()
}

/// The dense id of a fixture item.
pub fn item(name: &str) -> ItemId {
    registries()
        .item_id(&id(name))
        .unwrap_or_else(|| panic!("{name} is not in the fixture"))
}

/// A stack of `count` of a fixture item.
pub fn stack(name: &str, count: u32) -> ItemStack {
    ItemStack::new(item(name), count)
}

fn build() -> Arc<FrozenRegistries> {
    let mut registries = Registries::new();
    let mut tags: Vec<(Namespaced, Vec<TagEntry>)> = Vec::new();
    for row in ITEMS {
        let mut def = ItemDef::new(id(row.id));
        def.display_name = Some(row.display.to_owned());
        def.max_stack_size = row.max_stack;
        def.rarity = row.rarity;
        def.tags = row.tags.iter().map(|t| id(t)).collect();
        for tag in row.tags {
            let tag = id(tag);
            let entry = TagEntry::Item(id(row.id));
            match tags.iter_mut().find(|(name, _)| *name == tag) {
                Some((_, values)) => values.push(entry),
                None => tags.push((tag, vec![entry])),
            }
        }
        registries
            .add_item(def)
            .unwrap_or_else(|e| panic!("fixture item {}: {e}", row.id));
    }
    for (name, values) in tags {
        registries.add_tag(TagDef {
            name,
            values,
            replace: false,
        });
    }
    add_recipes(&mut registries);
    let (frozen, warnings) = registries.freeze().expect("the fixture freezes cleanly");
    assert!(warnings.is_empty(), "fixture warned: {warnings:?}");
    Arc::new(frozen)
}

fn add_recipes(registries: &mut Registries) {
    let mut crafting = RecipeTypeDef::new(id("demo:crafting"));
    crafting.size = (3, 3);
    registries
        .add_recipe_type(crafting)
        .expect("demo:crafting is fresh");
    registries
        .add_recipe_type(RecipeTypeDef::new(id("demo:smelting")))
        .expect("demo:smelting is fresh");

    let mut pickaxe = RecipeDef::shapeless(
        id("demo:iron_pickaxe"),
        id("demo:crafting"),
        Vec::new(),
        ItemResult::one(id("minecraft:iron_pickaxe")),
    );
    pickaxe.shape = Some(vec!["III".into(), " P ".into(), " P ".into()]);
    pickaxe
        .key
        .insert('I', Ingredient::Item(id("minecraft:iron_ingot")));
    pickaxe
        .key
        .insert('P', Ingredient::Item(id("minecraft:oak_planks")));
    registries.add_recipe(pickaxe).expect("pickaxe is fresh");

    registries
        .add_recipe(RecipeDef::shapeless(
            id("demo:diamond_sword"),
            id("demo:crafting"),
            vec![
                Ingredient::Item(id("minecraft:diamond")),
                Ingredient::Item(id("minecraft:diamond")),
                Ingredient::Tag(id("minecraft:planks")),
            ],
            ItemResult::one(id("minecraft:diamond_sword")),
        ))
        .expect("sword is fresh");

    registries
        .add_recipe(RecipeDef::shapeless(
            id("demo:coal"),
            id("demo:smelting"),
            vec![Ingredient::Tag(id("minecraft:planks"))],
            ItemResult::one(id("minecraft:coal")),
        ))
        .expect("coal is fresh");
}

/// Categories with the defaults bound, as the `Categories` phase leaves them.
pub fn categories() -> Categories {
    let mut categories = Categories::default();
    categories.bind_defaults(&registries());
    categories
}

/// Everything `BrowserIndex::build` needs over the fixture.
pub fn inputs() -> BuildInputs {
    let categories = categories();
    let recipes = RecipeStore::build(&registries(), &categories);
    BuildInputs {
        loc: slotted_ui::Localization::default(),
        registries: registries(),
        types: IngredientTypes::with_builtins(),
        subtypes: Subtypes::default(),
        categories,
        recipes,
    }
}

/// The built index over the fixture.
pub fn index() -> BrowserIndex {
    BrowserIndex::build(&inputs())
}

/// A synthetic registry of `n` items across ten namespaces and ten tags, for
/// the index build benchmark and the perf test.
pub fn synthetic(n: u32) -> BuildInputs {
    let mut registries = Registries::new();
    let mut tags: Vec<(Namespaced, Vec<TagEntry>)> = (0..10)
        .map(|t| (id(&format!("c:group_{t}")), Vec::new()))
        .collect();
    for i in 0..n {
        let name = format!("mod{}:item_{i}", i % 10);
        let mut def = ItemDef::new(id(&name));
        def.display_name = Some(format!("Synthetic Item {i}"));
        def.rarity = Rarity::Common;
        let tag = usize::try_from(i % 10).unwrap_or(0);
        def.tags = vec![tags[tag].0.clone()];
        tags[tag].1.push(TagEntry::Item(id(&name)));
        registries.add_item(def).expect("synthetic ids are unique");
    }
    for (name, values) in tags {
        registries.add_tag(TagDef {
            name,
            values,
            replace: false,
        });
    }
    let (frozen, _) = registries.freeze().expect("the synthetic set freezes");
    let registries = Arc::new(frozen);
    let categories = Categories::default();
    let recipes = RecipeStore::build(&registries, &categories);
    BuildInputs {
        loc: slotted_ui::Localization::default(),
        registries,
        types: IngredientTypes::with_builtins(),
        subtypes: Subtypes::default(),
        categories,
        recipes,
    }
}
