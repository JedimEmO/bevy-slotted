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
//!    `assets/data/demo/` with a [`DirSource`],
//!    exactly as a game would over its own content directory.
//! 2. [`demo_screen`] reads `assets/screens/demo_chest.screen.ron` into a
//!    [`slotted::prelude::ScreenDef`].
//! 3. [`ChestDemoPlugin`] wires the screen to the keyboard: `Esc` closes it,
//!    `E` opens it again, and the title and capacity labels are filled in.
//!    [`ChestSettingsPlugin`] adds the settings screen over it: `Tab` (the
//!    `Menu` action) pushes `demo:settings` as a modal, `Esc` pops it.
//!
//! The last two of those, and the contents table behind them, live in the
//! shared [`showcase::chest`] module and are re-exported here, because the web
//! playground's Chest and Browser scenes open the identical chest
//! (`docs/design/showcase-contract.md` section 1). What is left in this file is
//! what a browser tab cannot do: read a directory.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use bevy::prelude::*;
use slotted::prelude::{ScreenDef, SlottedUiSet};
use slotted::ui::UiActionEmit;
use slotted_registry::{DataStage, DirSource, FrozenRegistries, ModId};

pub use showcase::chest::*;
pub use showcase::settings::{SETTINGS, SettingsDemoPlugin};

/// The settings screen over the chest (menus M1): [`SettingsDemoPlugin`]
/// registers `demo:settings`, seeds and guards its `ValueStore`, and this
/// plugin opens it on the `Menu` action (`Tab`, or Start on a pad) as a
/// modal over whatever is up. `Back` pops it through the stack, so `Esc`
/// closes it and focus returns to the chest.
///
/// Added by `examples/chest` and its tests, not by the playground's scenes:
/// the showcase gets its own settings scene in M4.
#[derive(Debug, Clone, Copy, Default)]
pub struct ChestSettingsPlugin;

impl Plugin for ChestSettingsPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(SettingsDemoPlugin).add_systems(
            Update,
            showcase::settings::open_settings_on_menu
                .in_set(SlottedUiSet::Input)
                .after(UiActionEmit),
        );
    }
}

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
/// The windowed example does not use this: `src/main.rs` loads the same file
/// through the `AssetServer` so it hot-reloads. This is the shortcut for
/// tests, which want the tree with no asset plumbing and no waiting; both
/// paths end in `ScreenDef::from_ron` over the identical bytes.
///
/// It reads the file rather than [`showcase::chest::screen`]'s compiled-in
/// copy on purpose: a test that a screen edit broke the chest has to fail
/// before the next `cargo build`, not after it.
///
/// # Panics
///
/// If the file is missing or malformed.
pub fn demo_screen() -> ScreenDef {
    let path = assets_dir().join(SCREEN_PATH);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("reading {}: {e}", path.display()));
    ScreenDef::from_ron(&text).unwrap_or_else(|e| panic!("parsing {}: {e}", path.display()))
}
