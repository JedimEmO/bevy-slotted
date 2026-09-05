//! The modded example, minus the window: the pack layout, the demo plugin
//! and the dev console, shared by `main.rs` and the harness tests.
//!
//! ```text
//! cargo run -p modded                       play with it; edit mods/*/control.lua live
//! cargo run -p modded -- --shot shots/modded.png
//! cargo run -p modded -- --reload copper_chest --shot shots/modded-reload.png
//! ```

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use slotted_packs::{PackLayout, ScriptLogs};

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

/// Opens the copper chest screen at start and owns the dev console.
#[derive(Debug, Default, Clone, Copy)]
pub struct ModdedDemoPlugin;

impl Plugin for ModdedDemoPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ConsoleVisible>()
            .add_systems(Startup, open_modded_chest)
            .add_systems(Update, (toggle_console, rebuild_console));
    }
}

/// Whether the console overlay shows. F8 toggles.
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

/// Lines the console shows at most.
pub const CONSOLE_LINES: usize = 12;

fn open_modded_chest(mut commands: Commands) {
    // PHASE4-IMPL: C -- same inventories as `chest::inventories`, menu def
    // from `chest::menu_def`, `open_menu` + `spawn_screen(CHEST_SCREEN)`.
    let _ = &mut commands;
}

fn toggle_console(keys: Res<ButtonInput<KeyCode>>, mut visible: ResMut<ConsoleVisible>) {
    if keys.just_pressed(KeyCode::F8) {
        visible.0 = !visible.0;
    }
}

fn rebuild_console(
    logs: Res<ScriptLogs>,
    visible: Res<ConsoleVisible>,
    mut commands: Commands,
    roots: Query<Entity, With<ConsoleRoot>>,
) {
    // PHASE4-IMPL: C -- on Changed<ScriptLogs> or Changed<ConsoleVisible>,
    // despawn the root and respawn a Panel with the last CONSOLE_LINES Text
    // children at GlobalZIndex(zbands::DEV); errors in the theme's error colour.
    let _ = (&logs, &visible, &mut commands, &roots);
}
