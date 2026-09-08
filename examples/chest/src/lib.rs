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
//!    [`ChestMenuPlugin`] adds the menus over it: `Esc` with nothing open
//!    pauses (`slotted-menu`'s pause screen), Settings on the pause opens
//!    `demo:settings`, and Quit asks before it exits.
//!
//! The last two of those, and the contents table behind them, live in the
//! shared [`showcase::chest`] module and are re-exported here, because the web
//! playground's Chest and Browser scenes open the identical chest
//! (`docs/design/showcase-contract.md` section 1). What is left in this file is
//! what a browser tab cannot do: read a directory.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use bevy::prelude::*;
use slotted::prelude::ScreenDef;
use slotted_registry::{DataStage, DirSource, FrozenRegistries, ModId};

pub use showcase::chest::*;
pub use showcase::menus::{QUIT_CONFIRM, QuitPlugin, confirm_quit};
pub use showcase::settings::{SETTINGS, SettingsDemoPlugin};

/// The menus over the chest (menus M2): [`SettingsDemoPlugin`] registers
/// `demo:settings` from its `SettingsSpec`, the `MenuConfig` points the
/// pause screen's Settings button at it, and [`QuitPlugin`] turns the pause
/// screen's Quit into a danger confirm that exits.
///
/// `Esc` is `Back` and `Menu` at once (docs/guide/input.md): with the chest
/// open it pops the chest, with nothing open it pauses, and on the pause it
/// resumes. `Start` on a pad pauses and resumes the same way.
///
/// Added by `examples/chest` and its tests, not by the playground's scenes:
/// the showcase gets its own settings scene in M4.
#[derive(Debug, Clone, Copy, Default)]
pub struct ChestMenuPlugin;

impl Plugin for ChestMenuPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((SettingsDemoPlugin, QuitPlugin))
            .insert_resource(showcase::menus::menu_config(
                "demo.menus.title",
                env!("CARGO_PKG_VERSION"),
            ));
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
