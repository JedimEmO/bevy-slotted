//! The moodboard chest, minus the window and minus the filesystem.
//!
//! Lifted out of `examples/chest/src/lib.rs` so the windowed example, the
//! headless tests and the web playground's Chest and Browser scenes all open
//! the identical menu over the identical contents
//! (`docs/design/showcase-contract.md` section 1). What stayed behind in the
//! example is exactly what reads a directory: `load_registries`,
//! `demo_screen` and `assets_dir`.
//!
//! The screen file itself is here, compiled in ([`crate::screens`]), because a
//! browser tab cannot wait on an asset handle in the middle of a scene switch.

use std::sync::Arc;

use bevy::prelude::*;
use slotted::browser::{BrowserPhase, DefaultScreenHandler, ScreenHandlers};
use slotted::ecs::MenuIdAllocator;
use slotted::prelude::*;
use slotted_model::{Actor, ComponentPatch, Inventory, ItemStack, MenuDef, Namespaced};
use slotted_registry::FrozenRegistries;

/// The screen this demo opens.
pub const CHEST: &str = "demo:chest";

/// The screen file, relative to the asset root. The windowed example loads it
/// through the `AssetServer` (`ScreenLoader`), which is what makes it hot
/// reloadable; [`screen`] parses the same bytes out of the binary for a page
/// and a test that have no asset plumbing to wait on.
pub const SCREEN_PATH: &str = "screens/demo_chest.screen.ron";

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

/// The compiled-in `demo:chest` screen.
pub fn screen() -> ScreenDef {
    crate::screens::parse("chest", crate::screens::CHEST_SCREEN_RON)
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

/// The demo's inventories, filled from the demo contents table against
/// `registries`.
///
/// Item ids are dense handles interned at freeze time, so the stacks have to
/// be built against the same [`FrozenRegistries`] the app runs with. A row
/// naming an id the data files did not register is skipped with a warning
/// rather than a panic: in a browser tab a panic is a blank canvas and no way
/// to find out why.
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
        let Some(id) = registries.item_id(&name) else {
            warn!("{name} is not in assets/data/demo/items; skipping that stack");
            continue;
        };
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

/// Whether the demo runs as a player who may cheat.
///
/// `cargo run -p chest -- --cheat` opens the chest as a creative actor, which
/// is the one thing that lets the browser's Ctrl+click give an item out of
/// nowhere. Survival is the default so the conservation assertion in the
/// tests keeps its teeth.
#[derive(Resource, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct CheatMode(pub bool);

/// The actor to open the chest as.
pub fn actor(cheat: bool) -> Actor {
    if cheat { Actor::CREATIVE } else { ACTOR }
}

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
/// Added by `examples/chest`, its tests and the showcase's Chest and Browser
/// scenes alike, so a keystroke does the same thing on screen, in the harness
/// and in the page.
#[derive(Debug, Clone, Copy, Default)]
pub struct ChestDemoPlugin;

impl Plugin for ChestDemoPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ChestBinding>()
            .init_resource::<CheatMode>()
            // The browser attaches only to screen kinds a handler claims, so
            // this one line is the whole of "give this screen an overlay".
            // The chest has no crafting grid, so no `TransferHandler` goes
            // with it and the `+` button stays disabled.
            .add_systems(
                Startup,
                register_browser_handler.in_set(BrowserPhase::ScreenHandlers),
            )
            .add_systems(
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

/// `BrowserPhase::ScreenHandlers`: the chest screen takes the default
/// integration, so the panel docks beside it and the exclusion zones in the
/// screen file push it clear of the rail.
///
/// The definition is registered here too, from the compiled-in copy. A browser
/// handler may not name a screen kind the registry has never heard of --
/// `slotted-browser` validates that and panics on it in a debug build -- and
/// this plugin is added long before anything opens a chest. The windowed
/// example's `AssetServer` load replaces it a frame later with the bytes on
/// disk, which is what keeps the screen file hot-reloadable.
pub fn register_browser_handler(
    mut handlers: ResMut<ScreenHandlers>,
    mut screens: ResMut<Screens>,
) {
    if screens.get(&ScreenKind::new(CHEST)).is_none() {
        screens.register(screen());
    }
    handlers.register(ScreenKind::new(CHEST), Arc::new(DefaultScreenHandler));
}

/// Records every chest screen as it appears, wherever it came from: the
/// example's own startup, the `E` key, `UiHarness::open_screen`, or a
/// showcase scene's `enter`.
pub fn track_open_screens(
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
pub fn toggle_chest_screen(
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
pub fn fill_labels(
    binding: Res<ChestBinding>,
    menus: Query<&OpenMenu>,
    inventories: Query<&slotted::ecs::menu::Inventory>,
    mut labels: Query<(&mut Text, Option<&TestId>)>,
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

    // The rail names its own buttons now: `slotted_ui::spawn_action_rail`
    // resolves `slotted.rail.<action>` through the `Localization` port and
    // falls back to the library's English, on the `Text` and the
    // `SemanticLabel` alike, so this demo only fills its own three labels.
    for (mut text, id) in &mut labels {
        let replacement = match id.map(|id| id.0.as_str()) {
            Some("title") => "Copper Chest".to_owned(),
            Some("inventory_label") => "Inventory".to_owned(),
            Some("capacity") => match used {
                Some((filled, total)) => format!("{filled} / {total} slots"),
                None => String::new(),
            },
            _ => continue,
        };
        if text.0 != replacement {
            text.0 = replacement;
        }
    }
}

/// Spawns the demo's inventory entities, opens the menu and spawns the
/// screen, taking the screen definition from the [`Screens`] registry.
///
/// The same three calls a game makes when a player right-clicks a chest.
pub fn open_chest(commands: &mut Commands, registries: &FrozenRegistries, cheat: bool) {
    let entities = spawn_inventories(commands, registries);
    commands.queue(move |world: &mut World| {
        let kind = ScreenKind::new(CHEST);
        let Some(def) = world.resource::<Screens>().get(&kind).cloned() else {
            tracing::error!("{CHEST} is not registered");
            return;
        };
        open_over(world, def, entities, cheat);
    });
}

/// The inventory entities for one chest, in menu order.
pub fn spawn_inventories(commands: &mut Commands, registries: &FrozenRegistries) -> Vec<Entity> {
    inventories(registries)
        .into_iter()
        .map(|inventory| {
            commands
                .spawn(slotted::ecs::menu::Inventory(inventory))
                .id()
        })
        .collect()
}

/// Opens `def` over `entities`, exclusively. The showcase's scenes call this
/// directly: a `SceneHandler` already holds the whole world and has no
/// `Commands` queue that would flush a frame later.
///
/// Returns the menu and the screen root.
pub fn open_over(
    world: &mut World,
    def: Arc<ScreenDef>,
    entities: Vec<Entity>,
    cheat: bool,
) -> OpenChest {
    let mut ids = world
        .remove_resource::<MenuIdAllocator>()
        .unwrap_or_default();
    let open = {
        let mut commands = world.commands();
        let menu = open_menu(&mut commands, &mut ids, menu_def(), entities, actor(cheat));
        let screen = spawn_screen(&mut commands, def, Some(menu));
        OpenChest { menu, screen }
    };
    world.insert_resource(ids);
    world.flush();
    open
}
