//! The copper chest the Lua mods build: the playground's original scene.
//!
//! Nothing here registers an item, a recipe or a screen. All of that comes out
//! of `examples/modded/mods/*/data.lua`; this is only the menu shape and the
//! table of stacks to seed it with, which is what a game would keep in its own
//! world data.
//!
//! `docs/design/showcase-contract.md` section 3.4 asked for
//! `web_playground::scene` to become this module. The half that moved is the
//! half a scene handler needs; the in-canvas console overlay stayed in the
//! playground, because it is page furniture rather than part of any scene.

use std::sync::Arc;

use bevy::prelude::*;
use slotted_model::{Inventory, ItemStack, MenuDef, Namespaced};
use slotted_registry::FrozenRegistries;

/// The screen kind `copper_chest/data.lua` registers.
pub const CHEST_SCREEN: &str = "copper_chest:chest";

/// Rows of nine in the container.
pub const ROWS: u16 = 3;

/// The menu behind the screen: container, player main, hotbar.
pub fn menu_def() -> Arc<MenuDef> {
    Arc::new(MenuDef::chest(ROWS))
}

struct Row {
    inventory: usize,
    slot: usize,
    item: &'static str,
    count: u32,
}

const fn row(inventory: usize, slot: usize, item: &'static str, count: u32) -> Row {
    Row {
        inventory,
        slot,
        item,
        count,
    }
}

const CONTAINER: usize = 0;
const MAIN: usize = 1;
const HOTBAR: usize = 2;

/// What the chest starts with. Every id was registered by a mod's `data.lua`.
const CONTENTS: &[Row] = &[
    row(CONTAINER, 0, "copper_chest:copper_chest", 4),
    row(CONTAINER, 1, "copper_chest:copper_ingot", 64),
    row(CONTAINER, 2, "copper_chest:copper_ingot", 31),
    row(CONTAINER, 5, "appleskin_like:apple", 12),
    row(CONTAINER, 11, "copper_chest:copper_chest", 1),
    row(MAIN, 0, "copper_chest:copper_ingot", 8),
    row(MAIN, 7, "appleskin_like:apple", 3),
    row(HOTBAR, 0, "copper_chest:copper_chest", 2),
    row(HOTBAR, 2, "appleskin_like:apple", 16),
];

/// The demo's inventories. Rows naming an unregistered id are skipped with a
/// warning rather than a panic: in a browser tab a panic is a blank canvas and
/// no way to find out why.
pub fn inventories(registries: &FrozenRegistries) -> Vec<Inventory> {
    let mut out: Vec<Inventory> = menu_def()
        .inventory_sizes()
        .into_iter()
        .map(Inventory::new)
        .collect();
    for row in CONTENTS {
        let Ok(name) = Namespaced::parse(row.item) else {
            warn!("bad id {:?} in the demo table", row.item);
            continue;
        };
        let Some(id) = registries.item_id(&name) else {
            warn!("{name} was not registered by any mod");
            continue;
        };
        out[row.inventory].set(row.slot, Some(ItemStack::new(id, row.count)));
    }
    out
}
