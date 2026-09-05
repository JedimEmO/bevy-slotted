//! The modded example in a window. See `lib.rs` for the flags.

use bevy::prelude::*;
use modded::{ModdedDemoPlugin, layout, mods_dir};
use slotted::prelude::*;
use slotted_packs::{PackSourcePlugin, PacksConfig, SlottedPacksPlugin};

fn main() {
    // PHASE4-IMPL: C -- parse --shot / --reload, 3D scene and screenshot
    // plumbing as in examples/chest/src/main.rs, `chest::ChestDemoPlugin`'s
    // scene without its chest opener.
    let layout = layout(&mods_dir());
    App::new()
        .add_plugins(PackSourcePlugin::new(layout))
        .add_plugins(DefaultPlugins)
        .add_plugins(SlottedPlugins::default().set(SlottedPacksPlugin {
            config: PacksConfig {
                hud_tick: None,
                reload_on_change: true,
            },
        }))
        .add_plugins(ModdedDemoPlugin)
        .run();
}
