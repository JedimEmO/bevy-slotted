//! The chest demo, minus the window.
//!
//! Everything here runs headless, which is the point: `src/main.rs` adds a 3D
//! scene and a renderer on top, and `tests/ui.rs` drives the exact same data
//! files through `slotted_test::UiHarness` with no window at all. If the two
//! could drift, the tests would stop proving anything about what you see on
//! screen.
//!
//! The three things a consumer has to supply are all here:
//!
//! 1. [`load_registries`] runs `slotted_registry`'s data stage over
//!    `assets/data/demo/` with a [`DirSource`](slotted_registry::DirSource),
//!    exactly as a game would over its own content directory.
//! 2. [`demo_screen`] reads `assets/screens/demo_chest.screen.ron` into a
//!    [`ScreenDef`].
//! 3. [`ChestDemoPlugin`] wires the screen to the keyboard: `Esc` closes it,
//!    `E` opens it again, and the title and capacity labels are filled in.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use bevy::prelude::*;
use slotted::ecs::MenuIdAllocator;
use slotted::prelude::*;
use slotted_model::{Actor, ComponentPatch, Inventory, ItemStack, MenuDef, Namespaced};
use slotted_registry::{DataStage, DirSource, FrozenRegistries, ModId};

/// The screen this example opens.
pub const CHEST: &str = "demo:chest";

/// The mod id the demo content is loaded under: `assets/data/demo/`.
pub const DEMO_MOD: &str = "demo";

/// Rows of nine in the container. Three is a single chest.
pub const ROWS: u16 = 3;

/// The first hotbar slot of [`MenuDef::chest`] with [`ROWS`] rows: 27
/// container slots then 27 player slots.
pub const HOTBAR_FIRST: u16 = 27 + 27;

/// Component key the item renderer reads for current damage.
const DAMAGE: &str = "slotted:damage";
/// Component key the item renderer reads for maximum damage.
const MAX_DAMAGE: &str = "slotted:max_damage";

/// The workspace's shared `assets/` directory.
///
/// Resolved from the crate root rather than the process's working directory,
/// so `cargo run -p chest`, `just run-chest` and a test binary launched by
/// `cargo test` all read the same files.
pub fn assets_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets")
}

/// Runs the data stage over `assets/data/demo/` and freezes.
///
/// This is the whole content pipeline the example uses: thirteen item files,
/// nine tag files, a recipe type and two recipes, read off disk, patched and
/// interned into dense ids. Nothing is registered in Rust.
///
/// # Panics
///
/// If the directory is missing or a file does not parse, which in an example
/// means the checkout is broken rather than that the game should limp on.
pub fn load_registries() -> Arc<FrozenRegistries> {
    let source = DirSource::new(assets_dir());
    let order = vec![ModId::new(DEMO_MOD).expect("`demo` is a valid mod id")];
    let loaded = DataStage::new(order)
        .load(&source)
        .unwrap_or_else(|e| panic!("loading assets/data/demo: {e}"));
    for warning in &loaded.report.warnings {
        tracing::warn!(%warning, "demo data");
    }
    tracing::info!(
        files = loaded.report.files_read(),
        entries = loaded.report.total_entries(),
        "demo data stage"
    );
    Arc::new(loaded.registries)
}

/// Reads `assets/screens/demo_chest.screen.ron`.
///
/// Phase 2 ships no `ScreenDef` asset loader, so this goes through `std::fs`
/// and `ron` rather than `AssetServer`; see docs/FOLLOWUPS.md. The upside for
/// now is that the tests read the identical bytes with no Bevy asset plumbing.
///
/// # Panics
///
/// If the file is missing or malformed.
pub fn demo_screen() -> ScreenDef {
    let path = assets_dir().join("screens/demo_chest.screen.ron");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("reading {}: {e}", path.display()));
    ScreenDef::from_ron(&text).unwrap_or_else(|e| panic!("parsing {}: {e}", path.display()))
}

/// The menu behind the screen: container, player main, hotbar.
pub fn menu_def() -> Arc<MenuDef> {
    Arc::new(MenuDef::chest(ROWS))
}

/// One row of demo content: which inventory, which slot, what, how many, and
/// how worn it is.
struct Row {
    inventory: usize,
    slot: usize,
    item: &'static str,
    count: u32,
    /// Damage taken, for the items that show a durability bar.
    damage: u32,
}

const fn row(inventory: usize, slot: usize, item: &'static str, count: u32) -> Row {
    Row {
        inventory,
        slot,
        item,
        count,
        damage: 0,
    }
}

const fn worn(inventory: usize, slot: usize, item: &'static str, damage: u32) -> Row {
    Row {
        inventory,
        slot,
        item,
        count: 1,
        damage,
    }
}

const CONTAINER: usize = 0;
const MAIN: usize = 1;
const HOTBAR: usize = 2;

/// The moodboard's chest, the player's pockets and their hotbar.
///
/// Chosen so every renderer path has something to draw: a full stack and a
/// partial one of the same kind that must merge, a sixteen-cap item at its
/// cap, four rarities, and three tools at three different wear levels.
const CONTENTS: &[Row] = &[
    row(CONTAINER, 0, "minecraft:cobblestone", 64),
    row(CONTAINER, 1, "minecraft:cobblestone", 23),
    row(CONTAINER, 2, "minecraft:oak_planks", 40),
    row(CONTAINER, 4, "minecraft:iron_ingot", 12),
    row(CONTAINER, 5, "minecraft:gold_ingot", 3),
    row(CONTAINER, 6, "minecraft:copper_ingot", 31),
    row(CONTAINER, 9, "minecraft:diamond", 7),
    row(CONTAINER, 11, "minecraft:redstone", 55),
    row(CONTAINER, 20, "minecraft:ender_pearl", 16),
    row(MAIN, 0, "minecraft:dirt", 64),
    row(MAIN, 7, "minecraft:iron_ingot", 5),
    row(MAIN, 12, "minecraft:redstone", 8),
    row(MAIN, 14, "minecraft:cobblestone", 9),
    row(MAIN, 26, "minecraft:coal", 48),
    worn(HOTBAR, 0, "minecraft:iron_pickaxe", 96),
    worn(HOTBAR, 1, "minecraft:diamond_sword", 240),
    row(HOTBAR, 3, "minecraft:oak_planks", 12),
    worn(HOTBAR, 4, "slotted:debug_stick", 12),
];

fn ns(s: &str) -> Namespaced {
    Namespaced::parse(s).unwrap_or_else(|e| panic!("bad id {s:?} in the demo table: {e}"))
}

/// The demo's inventories, filled from [`CONTENTS`] against `registries`.
///
/// Item ids are dense handles interned at freeze time, so the stacks have to
/// be built against the same [`FrozenRegistries`] the app runs with.
///
/// # Panics
///
/// If the table names an item the data files do not register.
pub fn inventories(registries: &FrozenRegistries) -> Vec<Inventory> {
    let damage_id = registries.components.get(&ns(DAMAGE));
    let max_damage_id = registries.components.get(&ns(MAX_DAMAGE));

    let mut out: Vec<Inventory> = menu_def()
        .inventory_sizes()
        .into_iter()
        .map(Inventory::new)
        .collect();
    for row in CONTENTS {
        let name = ns(row.item);
        let id = registries
            .item_id(&name)
            .unwrap_or_else(|| panic!("{name} is not in assets/data/demo/items"));
        let mut stack = ItemStack::new(id, row.count);
        if row.damage > 0
            && let (Some(damage), Some(max)) = (damage_id, max_damage_id)
            && let Some(def) = registries.items.get(id)
            && let Some(limit) = def.components.get(&ns(MAX_DAMAGE))
        {
            let mut patch = ComponentPatch::new();
            patch.insert(damage, i64::from(row.damage));
            patch.insert(max, limit.clone().into_rust::<i64>().unwrap_or(0));
            stack = stack.with_patch(patch);
        }
        out[row.inventory].set(row.slot, Some(stack));
    }
    out
}

/// The player driving the demo. Survival, so `Clone` clicks are refused and
/// the conservation assertion in the tests means something.
pub const ACTOR: Actor = Actor::SURVIVAL;

/// Which chest screen and menu are currently open, and what to reopen.
///
/// The demo keeps the three inventory entities alive across a close, the way
/// a real game keeps the chest block's contents alive when the player walks
/// away, so `E` reopens the same chest rather than a fresh one.
#[derive(Resource, Debug, Default, Clone)]
pub struct ChestBinding {
    /// The open screen root and its menu, when the screen is up.
    pub open: Option<OpenChest>,
    /// The inventory entities, remembered across closes.
    pub inventories: Vec<Entity>,
    /// The menu definition to reopen with.
    pub def: Option<Arc<MenuDef>>,
    /// The actor to reopen as.
    pub actor: Actor,
}

/// The entities of an open chest screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpenChest {
    /// The menu entity.
    pub menu: Entity,
    /// The screen root.
    pub screen: Entity,
}

/// Everything the demo adds on top of `SlottedPlugins`: the key bindings, the
/// binding bookkeeping, and the two labels that Phase 2 has no locale system
/// for yet.
///
/// Added by both `main.rs` and `tests/ui.rs`, so a keystroke does the same
/// thing on screen and in the harness.
#[derive(Debug, Clone, Copy, Default)]
pub struct ChestDemoPlugin;

impl Plugin for ChestDemoPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ChestBinding>().add_systems(
            Update,
            (
                track_open_screens,
                toggle_chest_screen.after(track_open_screens),
                fill_labels.after(toggle_chest_screen),
            )
                .in_set(SlottedUiSet::Input),
        );
    }
}

/// Records every chest screen as it appears, wherever it came from: the
/// example's own startup, the `E` key, or `UiHarness::open_screen`.
fn track_open_screens(
    roots: Query<(Entity, &ScreenRoot), Added<ScreenRoot>>,
    menus: Query<&OpenMenu>,
    mut binding: ResMut<ChestBinding>,
) {
    for (entity, root) in &roots {
        if root.kind != ScreenKind::new(CHEST) {
            continue;
        }
        let Some(menu) = root.menu else { continue };
        let Ok(open) = menus.get(menu) else { continue };
        binding.inventories.clone_from(&open.inventories);
        binding.def = Some(open.def.clone());
        binding.actor = open.actor;
        binding.open = Some(OpenChest {
            menu,
            screen: entity,
        });
    }
}

/// `Esc` closes the screen and its menu; `E` opens it again over the same
/// inventories.
fn toggle_chest_screen(
    keys: Res<ButtonInput<KeyCode>>,
    mut commands: Commands,
    mut binding: ResMut<ChestBinding>,
    mut ids: ResMut<MenuIdAllocator>,
    screens: Res<Screens>,
    menus: Query<&OpenMenu>,
) {
    if keys.just_pressed(KeyCode::Escape) {
        if let Some(open) = binding.open.take() {
            slotted::ui::close_screen(&mut commands, open.screen);
            if let Ok(menu) = menus.get(open.menu) {
                slotted::ecs::close_menu(&mut commands, open.menu, menu.id);
            }
        }
        return;
    }

    if !keys.just_pressed(KeyCode::KeyE) || binding.open.is_some() {
        return;
    }
    let (Some(def), false) = (binding.def.clone(), binding.inventories.is_empty()) else {
        return;
    };
    let kind = ScreenKind::new(CHEST);
    let Some(screen_def) = screens.get(&kind).cloned() else {
        tracing::warn!("{CHEST} is not registered; nothing to reopen");
        return;
    };
    let menu = open_menu(
        &mut commands,
        &mut ids,
        def,
        binding.inventories.clone(),
        binding.actor,
    );
    let screen = spawn_screen(&mut commands, screen_def, Some(menu));
    // `track_open_screens` will confirm this next frame; setting it here stops
    // a second `E` in the meantime from opening a duplicate.
    binding.open = Some(OpenChest { menu, screen });
}

/// Writes the header strings.
///
/// The screen file names localisation keys, and Phase 2 has no locale loader,
/// so a `Text` node renders its key. A game would resolve these through
/// Fluent; the demo substitutes them directly and recomputes the capacity
/// readout as the chest fills and empties.
fn fill_labels(
    binding: Res<ChestBinding>,
    menus: Query<&OpenMenu>,
    inventories: Query<&slotted::ecs::menu::Inventory>,
    tags: Query<&Tags>,
    mut labels: Query<(&mut Text, Option<&TestId>, Option<&ChildOf>)>,
    mut announced: Query<(&Tags, &mut SemanticLabel)>,
) {
    let used = binding
        .open
        .and_then(|open| menus.get(open.menu).ok())
        .and_then(|menu| menu.inventories.first().copied())
        .and_then(|entity| inventories.get(entity).ok())
        .map(|inventory| {
            let filled = (0..inventory.len())
                .filter(|i| inventory.get(*i).is_some())
                .count();
            (filled, inventory.len())
        });

    for (mut text, id, parent) in &mut labels {
        // The rail names its buttons after the action ids in the screen file,
        // and its label is a child of the button that carries the tag.
        let rail_action = parent
            .and_then(|parent| tags.get(parent.parent()).ok())
            .and_then(|tags| tags.get("action"))
            .and_then(rail_label);
        let replacement = match (id.map(|id| id.0.as_str()), rail_action) {
            (Some("title"), _) => "Copper Chest".to_owned(),
            (Some("inventory_label"), _) => "Inventory".to_owned(),
            (Some("capacity"), _) => match used {
                Some((filled, total)) => format!("{filled} / {total} slots"),
                None => String::new(),
            },
            (_, Some(label)) => label.to_owned(),
            _ => continue,
        };
        if text.0 != replacement {
            text.0 = replacement;
        }
    }
    relabel_rail_buttons(&mut announced);
}

/// A rail button announces itself to a screen reader through
/// `SemanticLabel`, which the spawn wrote from the action id, so the human
/// name has to reach both.
fn relabel_rail_buttons(announced: &mut Query<(&Tags, &mut SemanticLabel)>) {
    for (tags, mut label) in announced {
        let Some(name) = tags.get("action").and_then(rail_label) else {
            continue;
        };
        if label.0 != name {
            label.0 = name.to_owned();
        }
    }
}

/// The human name of a rail action, or `None` for an action this demo does
/// not name.
fn rail_label(action: &str) -> Option<&'static str> {
    match action {
        "sort" => Some("Sort"),
        "quick_stack" => Some("Quick stack"),
        "deposit_all" => Some("Deposit all"),
        "loot_all" => Some("Loot all"),
        _ => None,
    }
}

/// Spawns the demo's inventory entities, opens the menu and spawns the
/// screen. Returns the screen root.
///
/// The same three calls a game makes when a player right-clicks a chest.
pub fn open_chest(commands: &mut Commands, registries: &FrozenRegistries) {
    let inventories = inventories(registries);
    let entities: Vec<Entity> = inventories
        .into_iter()
        .map(|inventory| {
            commands
                .spawn(slotted::ecs::menu::Inventory(inventory))
                .id()
        })
        .collect();
    commands.queue(move |world: &mut World| {
        let kind = ScreenKind::new(CHEST);
        let Some(def) = world.resource::<Screens>().get(&kind).cloned() else {
            tracing::error!("{CHEST} is not registered");
            return;
        };
        let mut ids = world
            .remove_resource::<MenuIdAllocator>()
            .unwrap_or_default();
        {
            let mut commands = world.commands();
            let menu = open_menu(&mut commands, &mut ids, menu_def(), entities, ACTOR);
            spawn_screen(&mut commands, def, Some(menu));
        }
        world.insert_resource(ids);
        world.flush();
    });
}
