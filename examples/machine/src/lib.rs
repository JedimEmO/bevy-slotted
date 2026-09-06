//! The machine demo, minus the window: the parts that read a directory.
//!
//! The simulation, the menu shape, the property ids and the face widget live
//! in the shared [`showcase::machine`] module and are re-exported here, because
//! the web playground's Machine scene drives the identical furnace
//! (`docs/design/showcase-contract.md` section 1). What is left in this file is
//! what a browser tab cannot do: run the data stage over `assets/data/`,
//! discover mods under a directory, and read the screen file off disk.
//!
//! The `sorter` mod that injects the sort button is the one under
//! `examples/modded/mods/`, shared with the modded example and the playground.
//! It injects into the wildcard `slotted:any`, so it lands on this screen
//! without ever having seen it.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use slotted::prelude::ScreenDef;
use slotted_packs::PackLayout;
use slotted_registry::{DataStage, DirSource, FrozenRegistries, ModId};

pub use showcase::machine::*;

/// The workspace's shared `assets/` directory.
pub fn assets_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets")
}

/// The shared demo mods, `examples/modded/mods/`.
pub fn mods_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../modded/mods")
}

/// The pack layout: workspace assets plus the shared demo mods.
///
/// # Panics
///
/// When a manifest is malformed: a broken checkout.
pub fn layout(mods_dir: &Path) -> PackLayout {
    PackLayout::new(assets_dir())
        .with_mods(mods_dir)
        .unwrap_or_else(|e| panic!("discovering {}: {e}", mods_dir.display()))
}

/// Reads `screens/furnace.screen.ron`.
///
/// It reads the file rather than [`showcase::machine::screen`]'s compiled-in
/// copy on purpose: a test that a screen edit broke the furnace has to fail
/// before the next `cargo build`, not after it.
///
/// # Panics
///
/// If the file is missing or does not parse: a broken checkout.
pub fn furnace_screen() -> ScreenDef {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("screens/furnace.screen.ron");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("reading {}: {e}", path.display()));
    ScreenDef::from_ron(&text).unwrap_or_else(|e| panic!("parsing {}: {e}", path.display()))
}

/// Runs the data stage over `assets/data/demo/` and `assets/data/machine/`.
///
/// The machine's own directory holds one file, the `machine:water` fluid, and
/// it is read only when it is there: the example still opens on a checkout
/// where nothing has registered a fluid yet, with the tank drawing its
/// theme fill instead of water.
///
/// # Panics
///
/// If a data file is missing or does not parse, which in an example means the
/// checkout is broken rather than that the game should limp on.
pub fn load_registries() -> Arc<FrozenRegistries> {
    let source = DirSource::new(assets_dir());
    let mut order = vec![ModId::new(DEMO_MOD).expect("`demo` is a valid mod id")];
    if assets_dir().join("data").join(MACHINE_MOD).is_dir() {
        order.push(ModId::new(MACHINE_MOD).expect("`machine` is a valid mod id"));
    }
    let loaded = DataStage::new(order)
        .load(&source)
        .unwrap_or_else(|e| panic!("loading assets/data: {e}"));
    for warning in &loaded.report.warnings {
        tracing::warn!(%warning, "machine data");
    }
    Arc::new(loaded.registries)
}
