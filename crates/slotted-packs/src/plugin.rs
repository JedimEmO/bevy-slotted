//! The two plugins and the system sets.

use std::time::Duration;

use bevy::asset::AssetApp;
use bevy::asset::io::AssetSourceBuilder;
use bevy::prelude::*;

use crate::{
    ControlScripts, DataFileLoader, FtlLoader, LayeredAssetReader, Locales, ModFailed, ModLoader,
    ModReloaded, ModStage, ModWatch, OpenScreens, PACK_SOURCE, PackLayout, PendingScriptEvents,
    ReloadMod, ScriptLoader, ScriptLog, ScriptLogs, route,
};

/// Registers the `pack://` asset source. **Add before `AssetPlugin`**
/// (before `DefaultPlugins`): Bevy builds sources when the asset server is
/// created and ignores later registrations.
#[derive(Debug, Clone)]
pub struct PackSourcePlugin {
    /// What to layer.
    pub layout: PackLayout,
}

impl PackSourcePlugin {
    /// Over `layout`.
    pub fn new(layout: PackLayout) -> Self {
        Self { layout }
    }
}

impl Plugin for PackSourcePlugin {
    fn build(&self, app: &mut App) {
        let layout = self.layout.clone();
        let reader_layout = layout.clone();
        #[allow(unused_mut)]
        let mut builder =
            AssetSourceBuilder::new(move || Box::new(LayeredAssetReader::new(&reader_layout)));
        #[cfg(feature = "watch")]
        {
            // PHASE4-IMPL: B -- `.with_watcher(FileWatcher over every root)`.
        }
        app.register_asset_source(PACK_SOURCE, builder);
        app.insert_resource(layout);
    }
}

/// Runtime knobs. Also inserted as a resource.
#[derive(Debug, Clone, PartialEq, Eq, Resource)]
pub struct PacksConfig {
    /// Fire `HudTick` this often; `None` never.
    pub hud_tick: Option<Duration>,
    /// Turn asset modifications into reloads.
    pub reload_on_change: bool,
}

impl Default for PacksConfig {
    fn default() -> Self {
        Self {
            hud_tick: None,
            reload_on_change: true,
        }
    }
}

/// System sets in `Update`. `Collect` runs inside `SlottedUiSet::Input`;
/// `Dispatch` after it and before `SlottedEcsSet::Input`, so a script's
/// `MenuAction` is predicted in the same frame as the click.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SlottedPacksSet {
    /// Observers and message readers enqueue.
    Collect,
    /// Scripts run, commands apply.
    Dispatch,
}

/// The lifecycle plugin. Runs `ModLoader::run_all` in `PreStartup` so
/// `Registries` exists before `slotted-ui` and `slotted-browser` read it.
#[derive(Debug, Clone, Default)]
pub struct SlottedPacksPlugin {
    /// Knobs.
    pub config: PacksConfig,
}

impl Plugin for SlottedPacksPlugin {
    fn build(&self, app: &mut App) {
        app.init_state::<ModStage>()
            .init_asset::<crate::ScriptAsset>()
            .init_asset::<crate::DataFile>()
            .init_asset::<crate::FtlAsset>()
            .register_asset_loader(ScriptLoader)
            .register_asset_loader(DataFileLoader)
            .register_asset_loader(FtlLoader)
            .init_resource::<ScriptLogs>()
            .init_resource::<ControlScripts>()
            .init_resource::<ModWatch>()
            .init_resource::<OpenScreens>()
            .init_resource::<PendingScriptEvents>()
            .init_resource::<Locales>()
            .insert_resource(self.config.clone())
            .add_message::<ScriptLog>()
            .add_message::<ModFailed>()
            .add_message::<ModReloaded>()
            .add_message::<ReloadMod>()
            .add_observer(route::on_slot_clicked)
            .add_observer(route::on_widget_activate)
            .add_observer(route::on_screen_spawned)
            .add_observer(route::on_screen_closed)
            .configure_sets(
                Update,
                (
                    SlottedPacksSet::Collect.in_set(slotted_ui::SlottedUiSet::Input),
                    SlottedPacksSet::Dispatch
                        .after(slotted_ui::SlottedUiSet::Input)
                        .before(slotted_ecs::SlottedEcsSet::Input),
                ),
            )
            .add_systems(PreStartup, initial_load)
            .add_systems(
                Update,
                (
                    route::collect_browser_events.in_set(SlottedPacksSet::Collect),
                    route::dispatch_script_events.in_set(SlottedPacksSet::Dispatch),
                    (watch_for_changes, apply_reloads)
                        .chain()
                        .after(SlottedPacksSet::Dispatch),
                    crate::locale::resolve_loc_text.in_set(slotted_ui::SlottedUiSet::Render),
                ),
            );
    }
}

/// `PreStartup`: the whole pipeline. A missing `PackLayout` is not an error
/// here (the harness inserts one later and calls `run_all` itself).
fn initial_load(world: &mut World) {
    if !world.contains_resource::<PackLayout>() {
        return;
    }
    if let Err(error) = ModLoader::run_all(world) {
        tracing::error!(%error, "mod load failed");
        world.write_message(ModFailed {
            mod_id: None,
            error,
        });
    }
}

/// `AssetEvent::Modified` on a watched handle -> `ReloadMod`.
fn watch_for_changes(
    config: Res<PacksConfig>,
    watch: Res<ModWatch>,
    mut scripts: MessageReader<AssetEvent<crate::ScriptAsset>>,
    mut data: MessageReader<AssetEvent<crate::DataFile>>,
    mut ftl: MessageReader<AssetEvent<crate::FtlAsset>>,
    mut reload: MessageWriter<ReloadMod>,
) {
    // PHASE4-IMPL: B
    if !config.reload_on_change {
        return;
    }
    for _ in scripts.read() {}
    for _ in data.read() {}
    for _ in ftl.read() {}
    let _ = (&watch, &mut reload);
}

/// Drains `ReloadMod` and runs `ModLoader::reload_mod` once per distinct mod.
fn apply_reloads(world: &mut World) {
    // PHASE4-IMPL: B
    let _ = world;
}
