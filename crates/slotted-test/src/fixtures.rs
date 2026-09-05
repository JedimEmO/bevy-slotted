//! Ready-made data for tests: a small item registry and the two menus every
//! screen test starts from.
//!
//! The item table mirrors the moodboard demo, so the `chest` example and the
//! harness tests describe the same world. Eleven items is enough to cover the
//! cases that break code: two stacks of the same item that must merge, a
//! sixteen-cap item, tools that stack to one and carry durability, and a
//! legendary for rarity-dependent rendering.

use std::sync::{Arc, OnceLock};

use slotted_model::{Actor, Inventory, ItemId, ItemStack, MenuDef, Namespaced};
use slotted_registry::defs::{ItemDef, Rarity, TagDef, TagEntry};
use slotted_registry::registry::Registries;
use slotted_registry::{FrozenRegistries, Value};

use crate::fixture::MenuFixture;

/// The component key `TestRegistries` writes a tool's durability under.
pub const MAX_DURABILITY: &str = "slotted:max_durability";

/// One row of the item table: id, display name, max stack, rarity, tags, and
/// max durability (zero for anything that does not wear out).
struct Row {
    id: &'static str,
    display: &'static str,
    max_stack: u32,
    rarity: Rarity,
    tags: &'static [&'static str],
    durability: u32,
}

const ITEMS: &[Row] = &[
    Row {
        id: "minecraft:cobblestone",
        display: "Cobblestone",
        max_stack: 64,
        rarity: Rarity::Common,
        tags: &["c:stones"],
        durability: 0,
    },
    Row {
        id: "minecraft:oak_planks",
        display: "Oak Planks",
        max_stack: 64,
        rarity: Rarity::Common,
        tags: &["minecraft:planks"],
        durability: 0,
    },
    Row {
        id: "minecraft:dirt",
        display: "Dirt",
        max_stack: 64,
        rarity: Rarity::Common,
        tags: &[],
        durability: 0,
    },
    Row {
        id: "minecraft:iron_ingot",
        display: "Iron Ingot",
        max_stack: 64,
        rarity: Rarity::Common,
        tags: &["c:ingots"],
        durability: 0,
    },
    Row {
        id: "minecraft:gold_ingot",
        display: "Gold Ingot",
        max_stack: 64,
        rarity: Rarity::Uncommon,
        tags: &["c:ingots"],
        durability: 0,
    },
    Row {
        id: "minecraft:diamond",
        display: "Diamond",
        max_stack: 64,
        rarity: Rarity::Rare,
        tags: &["c:gems"],
        durability: 0,
    },
    Row {
        id: "minecraft:coal",
        display: "Coal",
        max_stack: 64,
        rarity: Rarity::Common,
        tags: &["c:coal"],
        durability: 0,
    },
    // The one item that does not stack to 64: splitting and merging tests
    // that only ever see 64 miss a whole class of off-by-one.
    Row {
        id: "minecraft:ender_pearl",
        display: "Ender Pearl",
        max_stack: 16,
        rarity: Rarity::Epic,
        tags: &[],
        durability: 0,
    },
    Row {
        id: "minecraft:iron_pickaxe",
        display: "Iron Pickaxe",
        max_stack: 1,
        rarity: Rarity::Common,
        tags: &["c:tools"],
        durability: 250,
    },
    Row {
        id: "minecraft:diamond_sword",
        display: "Diamond Sword",
        max_stack: 1,
        rarity: Rarity::Rare,
        tags: &["c:tools"],
        durability: 1561,
    },
    Row {
        id: "slotted:debug_stick",
        display: "Debug Stick",
        max_stack: 1,
        rarity: Rarity::Legendary,
        tags: &["c:tools", "slotted:dev"],
        durability: 100,
    },
];

fn id(s: &str) -> Namespaced {
    Namespaced::parse(s).unwrap_or_else(|e| panic!("bad id {s:?} in the fixture table: {e}"))
}

/// The frozen registries the fixtures build stacks against.
///
/// ```
/// use slotted_test::prelude::*;
///
/// let registries = TestRegistries::basic();
/// let pearl = TestRegistries::item("minecraft:ender_pearl");
/// assert_eq!(registries.items.get(pearl).unwrap().max_stack_size, 16);
/// ```
#[derive(Debug, Clone, Copy)]
pub struct TestRegistries;

static BASIC: OnceLock<Arc<FrozenRegistries>> = OnceLock::new();

impl TestRegistries {
    /// The eleven moodboard items, frozen once per process so
    /// every fixture and every test agrees on the numeric [`ItemId`]s.
    ///
    /// Pass it to `UiHarness::builder().registries(..)`.
    pub fn basic() -> Arc<FrozenRegistries> {
        BASIC.get_or_init(Self::build).clone()
    }

    /// The dense id of a fixture item. Panics on an item the table does not
    /// hold, which is the right outcome for a typo in a test.
    pub fn item(name: &str) -> ItemId {
        let name = id(name);
        Self::basic()
            .item_id(&name)
            .unwrap_or_else(|| panic!("{name} is not in TestRegistries::basic()"))
    }

    /// A stack of `count` of the named fixture item.
    pub fn stack(name: &str, count: u32) -> ItemStack {
        ItemStack::new(Self::item(name), count)
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
            if row.durability > 0 {
                def.components
                    .insert(id(MAX_DURABILITY), Value::from(i64::from(row.durability)));
            }
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
        let (frozen, warnings) = registries
            .freeze()
            .expect("the fixture table freezes cleanly");
        assert!(
            warnings.is_empty(),
            "fixture registries warned: {warnings:?}"
        );
        Arc::new(frozen)
    }
}

/// Fills inventory `inv` from `(index, item name, count)` triples.
fn fill(inv: &mut Inventory, contents: &[(usize, &str, u32)]) {
    for (i, name, count) in contents {
        inv.set(*i, Some(TestRegistries::stack(name, *count)));
    }
}

/// A chest menu over the moodboard's contents: the chest itself, the player's
/// main inventory and the hotbar.
///
/// ```
/// use slotted_test::prelude::*;
///
/// let chest = ChestFixture::filled();
/// // Chest, player main, hotbar.
/// assert_eq!(chest.inventories().len(), 3);
/// assert_eq!(chest.inventories()[0].get(0).unwrap().count, 64);
/// assert!(ChestFixture::empty().inventories()[0].get(0).is_none());
/// ```
#[derive(Debug, Clone)]
pub struct ChestFixture {
    /// Rows of nine. Three is a single chest, six a double.
    pub rows: u16,
    /// Chest contents as `(slot, item, count)`.
    pub chest: Vec<(usize, String, u32)>,
    /// Player main inventory contents.
    pub main: Vec<(usize, String, u32)>,
    /// Hotbar contents.
    pub hotbar: Vec<(usize, String, u32)>,
    /// Permissions of the test player.
    pub actor: Actor,
}

const CHEST_CONTENTS: &[(usize, &str, u32)] = &[
    (0, "minecraft:cobblestone", 64),
    (1, "minecraft:cobblestone", 23),
    (2, "minecraft:oak_planks", 40),
    (4, "minecraft:iron_ingot", 12),
    (5, "minecraft:gold_ingot", 3),
    (9, "minecraft:diamond", 7),
    (20, "minecraft:ender_pearl", 16),
];

const MAIN_CONTENTS: &[(usize, &str, u32)] = &[
    (0, "minecraft:dirt", 64),
    (7, "minecraft:iron_ingot", 5),
    (14, "minecraft:cobblestone", 9),
    (26, "minecraft:coal", 48),
];

const HOTBAR_CONTENTS: &[(usize, &str, u32)] = &[
    (0, "minecraft:iron_pickaxe", 1),
    (1, "minecraft:diamond_sword", 1),
    (4, "slotted:debug_stick", 1),
];

fn owned(rows: &[(usize, &str, u32)]) -> Vec<(usize, String, u32)> {
    rows.iter()
        .map(|(i, name, count)| (*i, (*name).to_owned(), *count))
        .collect()
}

fn borrowed(rows: &[(usize, String, u32)]) -> Vec<(usize, &str, u32)> {
    rows.iter()
        .map(|(i, name, count)| (*i, name.as_str(), *count))
        .collect()
}

impl ChestFixture {
    /// A three-row chest with nothing in it and an empty player.
    pub fn empty() -> Self {
        Self {
            rows: 3,
            chest: Vec::new(),
            main: Vec::new(),
            hotbar: Vec::new(),
            actor: Actor::SURVIVAL,
        }
    }

    /// A three-row chest with the moodboard's contents, and a player carrying
    /// dirt, coal, two tools and the debug stick.
    pub fn filled() -> Self {
        Self {
            rows: 3,
            chest: owned(CHEST_CONTENTS),
            main: owned(MAIN_CONTENTS),
            hotbar: owned(HOTBAR_CONTENTS),
            actor: Actor::SURVIVAL,
        }
    }

    /// Same contents, `rows` rows of nine (`6` is a double chest).
    #[must_use]
    pub fn with_rows(mut self, rows: u16) -> Self {
        self.rows = rows;
        self
    }

    /// A creative player, so `Clone` clicks and middle drags are allowed.
    #[must_use]
    pub fn creative(mut self) -> Self {
        self.actor = Actor::CREATIVE;
        self
    }

    /// The registries these stacks were built against.
    pub fn registries(&self) -> Arc<FrozenRegistries> {
        TestRegistries::basic()
    }
}

impl MenuFixture for ChestFixture {
    fn def(&self) -> Arc<MenuDef> {
        Arc::new(MenuDef::chest(self.rows))
    }

    fn inventories(&self) -> Vec<Inventory> {
        let def = MenuDef::chest(self.rows);
        let mut out: Vec<Inventory> = def
            .inventory_sizes()
            .into_iter()
            .map(Inventory::new)
            .collect();
        fill(&mut out[MenuDef::CONTAINER.index()], &borrowed(&self.chest));
        fill(
            &mut out[MenuDef::PLAYER_MAIN.index()],
            &borrowed(&self.main),
        );
        fill(
            &mut out[MenuDef::PLAYER_HOTBAR.index()],
            &borrowed(&self.hotbar),
        );
        out
    }

    fn actor(&self) -> Actor {
        self.actor
    }
}

/// The player's own inventory window: crafting result and grid, armor, main,
/// hotbar and offhand.
///
/// ```
/// use slotted_test::prelude::*;
///
/// let player = PlayerFixture::filled();
/// let inventories = player.inventories();
/// assert_eq!(inventories.len(), MenuDef::player().inventory_sizes().len());
/// ```
#[derive(Debug, Clone)]
pub struct PlayerFixture {
    /// Main inventory contents.
    pub main: Vec<(usize, String, u32)>,
    /// Hotbar contents.
    pub hotbar: Vec<(usize, String, u32)>,
    /// Permissions of the test player.
    pub actor: Actor,
}

impl PlayerFixture {
    /// Nothing anywhere.
    pub fn empty() -> Self {
        Self {
            main: Vec::new(),
            hotbar: Vec::new(),
            actor: Actor::SURVIVAL,
        }
    }

    /// The same player contents [`ChestFixture::filled`] gives.
    pub fn filled() -> Self {
        Self {
            main: owned(MAIN_CONTENTS),
            hotbar: owned(HOTBAR_CONTENTS),
            actor: Actor::SURVIVAL,
        }
    }

    /// A creative player.
    #[must_use]
    pub fn creative(mut self) -> Self {
        self.actor = Actor::CREATIVE;
        self
    }

    /// The registries these stacks were built against.
    pub fn registries(&self) -> Arc<FrozenRegistries> {
        TestRegistries::basic()
    }
}

impl MenuFixture for PlayerFixture {
    fn def(&self) -> Arc<MenuDef> {
        Arc::new(MenuDef::player())
    }

    fn inventories(&self) -> Vec<Inventory> {
        let def = MenuDef::player();
        let mut out: Vec<Inventory> = def
            .inventory_sizes()
            .into_iter()
            .map(Inventory::new)
            .collect();
        fill(
            &mut out[MenuDef::PLAYER_MAIN.index()],
            &borrowed(&self.main),
        );
        fill(
            &mut out[MenuDef::PLAYER_HOTBAR.index()],
            &borrowed(&self.hotbar),
        );
        out
    }

    fn actor(&self) -> Actor {
        self.actor
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_covers_the_cases_that_break_code() {
        let r = TestRegistries::basic();
        assert_eq!(r.items.len(), ITEMS.len());
        assert!(ITEMS.len() >= 10, "the contract asks for about ten items");
        let pearl = r.items.get(TestRegistries::item("minecraft:ender_pearl"));
        assert_eq!(pearl.map(|d| d.max_stack_size), Some(16));
        let stick = r.items.get(TestRegistries::item("slotted:debug_stick"));
        assert_eq!(stick.map(|d| d.rarity), Some(Rarity::Legendary));
        let tools = r.items_with(&id("c:tools"));
        assert_eq!(tools.len(), 3, "two tools plus the debug stick");
        for name in ["minecraft:iron_pickaxe", "minecraft:diamond_sword"] {
            let def = r.items.get(TestRegistries::item(name)).expect("in table");
            assert_eq!(def.max_stack_size, 1);
            assert!(def.components.contains_key(&id(MAX_DURABILITY)));
        }
    }

    #[test]
    fn ids_are_stable_across_calls() {
        assert_eq!(
            TestRegistries::item("minecraft:diamond"),
            TestRegistries::item("minecraft:diamond")
        );
        assert!(Arc::ptr_eq(
            &TestRegistries::basic(),
            &TestRegistries::basic()
        ));
    }

    #[test]
    fn chest_fixtures_match_their_menu_def() {
        for fixture in [ChestFixture::empty(), ChestFixture::filled()] {
            let sizes = fixture.def().inventory_sizes();
            let lengths: Vec<usize> = fixture.inventories().iter().map(Inventory::len).collect();
            assert_eq!(sizes, lengths);
        }
        let filled = ChestFixture::filled().inventories();
        assert_eq!(filled[0].get(0).map(|s| s.count), Some(64));
        assert_eq!(filled[0].get(3), None);
        assert_eq!(
            filled[MenuDef::PLAYER_HOTBAR.index()].get(0).map(|s| s.id),
            Some(TestRegistries::item("minecraft:iron_pickaxe"))
        );
    }

    #[test]
    fn player_fixture_matches_its_menu_def() {
        let fixture = PlayerFixture::filled();
        let sizes = fixture.def().inventory_sizes();
        let lengths: Vec<usize> = fixture.inventories().iter().map(Inventory::len).collect();
        assert_eq!(sizes, lengths);
        assert_eq!(fixture.actor(), Actor::SURVIVAL);
        assert_eq!(PlayerFixture::empty().creative().actor(), Actor::CREATIVE);
    }

    #[test]
    fn a_double_chest_has_twice_the_rows() {
        let double = ChestFixture::filled().with_rows(6);
        assert_eq!(double.inventories()[0].len(), 54);
    }
}
