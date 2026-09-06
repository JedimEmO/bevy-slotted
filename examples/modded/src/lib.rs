//! The modded example, minus the window: the pack layout, the demo plugin
//! and the dev console, shared by `main.rs` and the harness tests.
//!
//! ```text
//! cargo run -p modded                       play with it; edit mods/*/control.lua live
//! cargo run -p modded -- --shot shots/modded.png
//! cargo run -p modded -- --reload copper_chest --shot shots/modded-reload.png
//! ```
//!
//! Nothing in this crate registers an item, a recipe or a screen. All of that
//! comes out of `mods/`, three directories of Lua and TOML that the loader
//! reads at start and re-reads when a file changes. The Rust here only opens
//! a menu over what the mods registered and draws the console that shows what
//! their scripts said.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use bevy::prelude::*;
use slotted::browser::{BrowserPhase, DefaultScreenHandler, ScreenHandlers};
use slotted::ecs::MenuIdAllocator;
use slotted::prelude::*;
use slotted_model::{Inventory, ItemStack, MenuDef, Namespaced};
use slotted_packs::{LogEntry, ModFailed, PackLayout, ScriptLogs};
use slotted_registry::FrozenRegistries;
use slotted_script::LogLevel;

/// The workspace's shared `assets/`.
pub fn assets_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets")
}

/// This example's `mods/`.
pub fn mods_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("mods")
}

/// Base assets plus every mod under `mods_dir`.
///
/// # Panics
///
/// When a manifest is malformed or the load order has a cycle: in an example
/// that means the checkout is broken.
pub fn layout(mods_dir: &Path) -> PackLayout {
    PackLayout::new(assets_dir())
        .with_mods(mods_dir)
        .unwrap_or_else(|e| panic!("discovering {}: {e}", mods_dir.display()))
}

/// The screen kind `copper_chest/data.lua` registers.
pub const CHEST_SCREEN: &str = "copper_chest:chest";

/// Rows of nine in the container.
pub const ROWS: u16 = 3;

/// Opens the copper chest screen at start and owns the dev console.
#[derive(Debug, Default, Clone, Copy)]
pub struct ModdedDemoPlugin;

impl Plugin for ModdedDemoPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ConsoleVisible>()
            .init_resource::<ConsoleErrors>()
            .add_systems(
                Startup,
                register_browser_handler.in_set(BrowserPhase::ScreenHandlers),
            )
            .add_systems(Startup, open_modded_chest.after(register_browser_handler))
            .add_systems(
                Update,
                (collect_mod_errors, toggle_console, rebuild_console).chain(),
            );
    }
}

// ---------------------------------------------------------------------------
// The menu
// ---------------------------------------------------------------------------

/// The menu behind the screen: container, player main, hotbar.
pub fn menu_def() -> Arc<MenuDef> {
    Arc::new(MenuDef::chest(ROWS))
}

/// One seeded stack: which inventory, which slot, what, how many.
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

/// What the chest starts with.
///
/// Every id here was registered by a mod's `data.lua`, not by Rust and not by
/// a `.ron` file in `assets/`. If the loader silently did nothing, this table
/// would panic on the first row.
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

/// The demo's inventories, built against the registries the mods produced.
///
/// # Panics
///
/// When an id in the demo contents table was not registered, which means a mod's data
/// stage did not run.
pub fn inventories(registries: &FrozenRegistries) -> Vec<Inventory> {
    let mut out: Vec<Inventory> = menu_def()
        .inventory_sizes()
        .into_iter()
        .map(Inventory::new)
        .collect();
    for row in CONTENTS {
        let name = Namespaced::parse(row.item)
            .unwrap_or_else(|e| panic!("bad id {:?} in the demo table: {e}", row.item));
        let id = registries
            .item_id(&name)
            .unwrap_or_else(|| panic!("{name} was not registered by any mod"));
        out[row.inventory].set(row.slot, Some(ItemStack::new(id, row.count)));
    }
    out
}

/// `BrowserPhase::ScreenHandlers`: the mod's screen takes the default browser
/// integration, so the panel docks beside it.
fn register_browser_handler(mut handlers: ResMut<ScreenHandlers>) {
    handlers.register(
        ScreenKind::new(CHEST_SCREEN),
        Arc::new(DefaultScreenHandler),
    );
}

/// `Startup`: spawn the inventories, open the menu, spawn the mod's screen.
///
/// By `Startup` the loader has already run in `PreStartup`, so `Registries`
/// holds the mods' items and `Screens` holds the mod's screen.
fn open_modded_chest(
    mut commands: Commands,
    mut ids: ResMut<MenuIdAllocator>,
    registries: Option<Res<Registries>>,
    screens: Res<Screens>,
) {
    // A test harness loads its mods after `Startup`, so there is nothing to
    // open yet and it opens the screen itself. The plugin is still worth
    // adding there for the browser handler and the console.
    let Some(registries) = registries else { return };
    let kind = ScreenKind::new(CHEST_SCREEN);
    let Some(def) = screens.get(&kind).cloned() else {
        error!("{CHEST_SCREEN} is not registered: did mods/copper_chest/data.lua run?");
        return;
    };
    let entities: Vec<Entity> = inventories(&registries)
        .into_iter()
        .map(|inventory| {
            commands
                .spawn(slotted::ecs::menu::Inventory(inventory))
                .id()
        })
        .collect();
    let menu = open_menu(
        &mut commands,
        &mut ids,
        menu_def(),
        entities,
        slotted_model::Actor::SURVIVAL,
    );
    spawn_screen(&mut commands, def, Some(menu));
}

// ---------------------------------------------------------------------------
// The dev console
// ---------------------------------------------------------------------------

/// Whether the console overlay shows. `F1` and `F8` both toggle it.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConsoleVisible(pub bool);

impl Default for ConsoleVisible {
    fn default() -> Self {
        Self(true)
    }
}

/// Marker on the console root.
#[derive(Component, Debug, Clone, Copy)]
pub struct ConsoleRoot;

/// Loader failures, kept because a `ModFailed` message lives two frames and
/// the console is meant to still show it a minute later.
#[derive(Resource, Debug, Default, Clone)]
pub struct ConsoleErrors(pub Vec<String>);

/// Lines the console shows at most.
pub const CONSOLE_LINES: usize = 12;

/// Drains `ModFailed` into [`ConsoleErrors`].
fn collect_mod_errors(mut reader: MessageReader<ModFailed>, mut errors: ResMut<ConsoleErrors>) {
    for failure in reader.read() {
        let who = failure
            .mod_id
            .as_ref()
            .map_or_else(|| "loader".to_owned(), ToString::to_string);
        errors.0.push(format!("{who}: {}", failure.error));
    }
}

fn toggle_console(keys: Res<ButtonInput<KeyCode>>, mut visible: ResMut<ConsoleVisible>) {
    if keys.just_pressed(KeyCode::F1) || keys.just_pressed(KeyCode::F8) {
        visible.0 = !visible.0;
    }
}

/// The colour a line is drawn in.
fn level_colour(level: LogLevel) -> Color {
    match level {
        LogLevel::Trace | LogLevel::Debug => Color::srgb(0.55, 0.58, 0.64),
        LogLevel::Info => Color::srgb(0.82, 0.86, 0.92),
        LogLevel::Warn => Color::srgb(0.98, 0.78, 0.35),
        LogLevel::Error => Color::srgb(0.98, 0.42, 0.42),
    }
}

/// The last [`CONSOLE_LINES`] lines: errors first, then script logs.
fn console_lines(logs: &ScriptLogs, errors: &ConsoleErrors) -> Vec<(String, Color)> {
    let error_colour = level_colour(LogLevel::Error);
    let mut lines: Vec<(String, Color)> = errors
        .0
        .iter()
        .map(|message| (message.clone(), error_colour))
        .collect();
    lines.extend(logs.entries.iter().map(|entry: &LogEntry| {
        let who = entry
            .mod_id
            .as_ref()
            .map_or_else(|| "-".to_owned(), ToString::to_string);
        (
            format!("[{who}] {}", entry.message),
            level_colour(entry.level),
        )
    }));
    if lines.len() > CONSOLE_LINES {
        lines.drain(..lines.len() - CONSOLE_LINES);
    }
    lines
}

/// Rebuilds the overlay whenever the log, the errors or the toggle change.
///
/// Despawning and respawning a dozen text nodes is cheap and keeps the
/// console honest: what is on screen is exactly what is in `ScriptLogs`.
fn rebuild_console(
    logs: Res<ScriptLogs>,
    errors: Res<ConsoleErrors>,
    visible: Res<ConsoleVisible>,
    mut commands: Commands,
    roots: Query<Entity, With<ConsoleRoot>>,
) {
    if !(logs.is_changed() || errors.is_changed() || visible.is_changed()) {
        return;
    }
    for root in &roots {
        commands.entity(root).despawn();
    }
    if !visible.0 {
        return;
    }
    let lines = console_lines(&logs, &errors);
    let root = commands
        .spawn((
            ConsoleRoot,
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(12.0),
                bottom: Val::Px(12.0),
                width: Val::Px(640.0),
                max_height: Val::Px(220.0),
                padding: UiRect::all(Val::Px(8.0)),
                flex_direction: FlexDirection::Column,
                // A log reads from the bottom: when a line wraps and the
                // dozen rows outgrow `max_height`, the overflow has to be the
                // oldest line off the top, not the newest one cut in half at
                // the window's edge.
                justify_content: JustifyContent::FlexEnd,
                row_gap: Val::Px(2.0),
                overflow: Overflow::clip(),
                border_radius: BorderRadius::all(Val::Px(6.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.04, 0.05, 0.08, 0.82)),
            GlobalZIndex(zbands::DEV),
            Pickable::IGNORE,
        ))
        .id();
    for (text, colour) in lines {
        commands.spawn((
            Node::default(),
            Text::new(text),
            TextFont {
                font_size: bevy::text::FontSize::Px(12.0),
                ..default()
            },
            TextColor(colour),
            Pickable::IGNORE,
            ChildOf(root),
        ));
    }
}
