//! The machine example in a window.
//!
//! ```text
//! cargo run -p machine                       play with it; R toggles redstone
//! cargo run -p machine -- --shot shots/machine.png
//! cargo run -p machine -- --redstone         start with the signal on
//! ```

use std::path::PathBuf;

use bevy::prelude::*;
use machine::MachineDemoPlugin;

/// Command line.
#[derive(Resource, Debug, Clone, Default)]
struct Cli {
    /// Capture one frame to this path and exit.
    shot: Option<PathBuf>,
    /// Start with the redstone signal on.
    redstone: bool,
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let value = |flag: &str| args.windows(2).find(|w| w[0] == flag).map(|w| w[1].clone());
    let cli = Cli {
        shot: value("--shot").map(PathBuf::from),
        redstone: args.iter().any(|a| a == "--redstone"),
    };
    // PHASE6-IMPL: C. As `examples/modded/src/main.rs`: PackSourcePlugin over
    // `machine::layout(&machine::mods_dir())`, DefaultPlugins, the navigation
    // plugins, SlottedPlugins with packs, BackdropPlugin, the 3D scene, open
    // `machine:furnace` with `machine::menu_def()` and `inventories`, insert
    // `MachineMenu`, `--shot` via `Screenshot::primary_window()`.
    if let Some(path) = &cli.shot {
        tracing::info!(path = %path.display(), "--shot is not implemented yet");
    }
    let mut app = App::new();
    app.add_plugins(DefaultPlugins)
        .add_plugins(MachineDemoPlugin)
        .insert_resource(machine::Redstone(cli.redstone))
        .insert_resource(cli);
    app.run();
}
