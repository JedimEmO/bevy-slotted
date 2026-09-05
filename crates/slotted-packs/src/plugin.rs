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
            let roots = layout.roots();
            builder = builder.with_watcher(move |sender| {
                // One `FileWatcher` per physical root, kept alive together:
                // Bevy hands a source a single watcher, and a pack layout is
                // several directories.
                let watchers: Vec<bevy::asset::io::file::FileWatcher> = roots
                    .iter()
                    .filter(|root| root.exists())
                    .filter_map(|root| {
                        bevy::asset::io::file::FileWatcher::new(
                            root.clone(),
                            sender.clone(),
                            std::time::Duration::from_millis(300),
                        )
                        .map_err(|error| {
                            tracing::warn!(root = %root.display(), %error, "cannot watch pack root");
                        })
                        .ok()
                    })
                    .collect();
                if watchers.is_empty() {
                    None
                } else {
                    Some(Box::new(MultiWatcher(watchers)))
                }
            });
        }
        app.register_asset_source(PACK_SOURCE, builder);
        app.insert_resource(layout);
    }
}

/// Several [`FileWatcher`](bevy::asset::io::file::FileWatcher)s behind the one
/// watcher Bevy allows a source, one per pack root.
#[cfg(feature = "watch")]
struct MultiWatcher(#[allow(dead_code)] Vec<bevy::asset::io::file::FileWatcher>);

#[cfg(feature = "watch")]
impl bevy::asset::io::AssetWatcher for MultiWatcher {}

/// Runtime knobs. Also inserted as a resource.
#[derive(Debug, Clone, PartialEq, Eq, Resource)]
pub struct PacksConfig {
    /// Fire `HudTick` this often; `None` never.
    pub hud_tick: Option<Duration>,
    /// Turn asset modifications into reloads.
    pub reload_on_change: bool,
    /// Namespaces every mod may register into without a warning.
    ///
    /// Registering outside your own namespace is normally worth a note,
    /// because it is how one mod reaches into another's ids. A *shared*
    /// namespace is the exception: `c` is the Fabric convention for common
    /// tags (`c:ingots`, `c:foods`) that every mod is meant to add to, and
    /// warning about it would train modders to ignore the warning. Defaults
    /// to `["c"]`; a game with its own convention replaces the list.
    pub shared_namespaces: Vec<String>,
}

impl Default for PacksConfig {
    fn default() -> Self {
        Self {
            hud_tick: None,
            reload_on_change: true,
            shared_namespaces: vec![SHARED_NAMESPACE.to_owned()],
        }
    }
}

/// The Fabric-style common-tag namespace, allowed to every mod by default.
pub const SHARED_NAMESPACE: &str = "c";

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
            .init_resource::<crate::lifecycle::ModErrors>()
            .init_resource::<crate::route::WarnedDeprecations>()
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
                    (route::collect_browser_events, emit_hud_tick).in_set(SlottedPacksSet::Collect),
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

/// `PacksConfig.hud_tick` -> a `HudTick` event, off unless the game asks.
///
/// The interval is virtual time, not the wall clock, so a paused or stepped
/// app ticks scripts exactly as often as it ticks everything else.
fn emit_hud_tick(
    config: Res<PacksConfig>,
    time: Res<Time<Virtual>>,
    mut since: Local<Duration>,
    mut pending: ResMut<PendingScriptEvents>,
) {
    let Some(interval) = config.hud_tick else {
        return;
    };
    *since += time.delta();
    if *since < interval {
        return;
    }
    *since = Duration::ZERO;
    pending.0.push_back(slotted_script::ScriptEvent::HudTick {
        elapsed_ms: u64::try_from(time.elapsed().as_millis()).unwrap_or(u64::MAX),
    });
}

/// `AssetEvent::Modified` on a watched handle -> `ReloadMod`.
fn watch_for_changes(
    config: Res<PacksConfig>,
    watch: Res<ModWatch>,
    mods: Option<Res<crate::ModSet>>,
    mut scripts: MessageReader<AssetEvent<crate::ScriptAsset>>,
    mut data: MessageReader<AssetEvent<crate::DataFile>>,
    mut ftl: MessageReader<AssetEvent<crate::FtlAsset>>,
    mut reload: MessageWriter<ReloadMod>,
) {
    if !config.reload_on_change {
        return;
    }
    let mut touched: Vec<slotted_script::ModId> = Vec::new();
    let mut note = |id: bevy::asset::UntypedAssetId| {
        if let Some(owner) = watch.owner_of(id) {
            touched.push(owner.clone());
        }
    };
    for event in scripts.read() {
        if let AssetEvent::Modified { id } = event {
            note((*id).into());
        }
    }
    for event in data.read() {
        if let AssetEvent::Modified { id } = event {
            note((*id).into());
        }
    }
    for event in ftl.read() {
        if let AssetEvent::Modified { id } = event {
            note((*id).into());
        }
    }
    if touched.is_empty() {
        return;
    }
    touched.sort();
    touched.dedup();
    // A reload re-runs the whole set anyway, so one message for the first
    // mod that changed is enough work for the frame.
    let _ = &mods;
    for mod_id in touched {
        reload.write(ReloadMod { mod_id });
    }
}

/// Drains `ReloadMod` and runs `ModLoader::reload_mod` once per distinct mod.
fn apply_reloads(world: &mut World) {
    let requested: Vec<slotted_script::ModId> = {
        let Some(mut messages) =
            world.get_resource_mut::<bevy::ecs::message::Messages<ReloadMod>>()
        else {
            return;
        };
        let mut ids: Vec<slotted_script::ModId> =
            messages.drain().map(|message| message.mod_id).collect();
        ids.dedup();
        ids
    };
    for mod_id in requested {
        if let Err(error) = ModLoader::reload_mod(world, &mod_id) {
            tracing::error!(%mod_id, %error, "reload failed; the previous state is kept");
        }
    }
}
