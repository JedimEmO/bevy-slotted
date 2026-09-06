//! The modded demo in a browser tab, with the mods' Lua editable beside it.
//!
//! ```text
//! just playground          build dist/web-playground/
//! just serve               http://127.0.0.1:8080/web-playground/
//! cargo run -p web-playground   the same app in a window, for debugging
//! ```
//!
//! The app is `examples/modded` with two things swapped. Mods come from an
//! [`bundle::EditableSource`] compiled into the binary instead
//! of from directories, because a browser tab has no filesystem; and the
//! script runtime is inserted directly rather than by the facade's plugin, so
//! the page can hold the handle it needs. It is the same
//! [`slotted_script_luaur`] runtime a native build gets (ADR 0004). Everything
//! between those two edges is the shipped code: the same `SlottedPlugins`, the same
//! `SlottedPacksPlugin`, the same `ModLoader::reload_mod`, and therefore the
//! same remap-by-name that keeps the chest's contents across a reload.
//!
//! The page talks to the world through [`bus::Bus`] and nothing else. See
//! `bridge` for the `wasm-bindgen` exports.

pub mod bundle;
pub mod bus;
pub mod scene;
pub mod snapshot;
pub mod tests;

#[cfg(target_arch = "wasm32")]
pub mod bridge;

use std::sync::Arc;

use bevy::prelude::*;
use bundle::EditableSource;
use bus::{Bus, Request};
use slotted_packs::{
    ModFailed, ModReloaded, ModSet, PackLayout, PackSourcePlugin, PacksConfig, ReloadMod,
    ScriptHost, ScriptLog, SlottedPacksPlugin,
};
use slotted_script::ModId;

/// The window title and the page's document title.
pub const TITLE: &str = "slotted — web playground";

/// The canvas the page provides. `bevy_winit` finds it by this selector.
pub const CANVAS: &str = "#slotted-canvas";

/// The bundled mods as a pack layout.
///
/// `base` is a path only because [`PackLayout`] names one; nothing reads it,
/// because [`PackAssets`](slotted_packs::PackAssets) answers every read first.
///
/// # Errors
///
/// A malformed `mod.toml` or a dependency cycle in the bundle, which would
/// mean the checkout the build script read is broken.
pub fn layout(source: &EditableSource) -> Result<PackLayout, slotted_packs::ModError> {
    let mods = ModSet::discover_in(source, "mods", bundle::mod_ids())?;
    Ok(PackLayout {
        base: "assets".into(),
        mods,
        resource_packs: Vec::new(),
    })
}

/// Where the in-canvas console overlay comes from.
///
/// Natively the overlay is the only console there is, so it starts visible.
/// In a browser the page already shows every line in its own console pane, and
/// a second copy floating over the game hides the chest for no gain, so it
/// starts hidden. Two ways to force it back on for a page that has no pane of
/// its own, `smoke.html` among them: the `?console=canvas` query flag, or the
/// `set_canvas_console` export in `bridge`.
pub mod canvas_console {
    use std::sync::atomic::{AtomicU8, Ordering};

    /// 0: nobody said. 1: on. 2: off.
    static FORCED: AtomicU8 = AtomicU8::new(0);

    /// Forces the overlay on or off, whatever the target and the URL say.
    ///
    /// Call it before the app starts; afterwards `F1` is the toggle.
    pub fn force(on: bool) {
        FORCED.store(if on { 1 } else { 2 }, Ordering::Relaxed);
    }

    /// Whether [`ConsoleVisible`](crate::scene::ConsoleVisible) starts true.
    pub fn starts_visible() -> bool {
        match FORCED.load(Ordering::Relaxed) {
            1 => true,
            2 => false,
            _ => default_for_target(),
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn default_for_target() -> bool {
        true
    }

    /// `?console=canvas` in the page's own URL, and nothing else.
    ///
    /// Read through `js_sys::Reflect` rather than `web-sys`, which the bundle
    /// would otherwise carry for this one string.
    #[cfg(target_arch = "wasm32")]
    fn default_for_target() -> bool {
        let Ok(location) = js_sys::Reflect::get(
            &js_sys::global(),
            &wasm_bindgen::JsValue::from_str("location"),
        ) else {
            return false;
        };
        let Ok(search) =
            js_sys::Reflect::get(&location, &wasm_bindgen::JsValue::from_str("search"))
        else {
            return false;
        };
        search.as_string().is_some_and(|query| {
            query
                .trim_start_matches('?')
                .split('&')
                .any(|pair| pair == "console=canvas")
        })
    }
}

/// The script runtime behind the port.
///
/// The same runtime on every target (ADR 0004), so a native run of this crate
/// exercises what the browser gets and the native tests say something about
/// the page.
///
/// On wasm an uncaught Lua error aborts the module rather than returning
/// `Err` (ADR 0004). The `bridge` module installs a reporter that puts the
/// error on the browser console first, and `web/playground.js` re-instantiates
/// the module and replays the [`snapshot`] it was holding.
pub fn runtime() -> ScriptHost {
    ScriptHost::new(slotted_script_luaur::LuaurRuntime::new(
        slotted_script::Limits::default(),
    ))
}

/// Builds the app: pack source, Bevy, slotted, the demo scene and the bridge.
///
/// `bus` is the page's end of the channel; a native debug run passes a fresh
/// one and simply never sends anything down it.
pub fn build_app(bus: Bus) -> App {
    let source = EditableSource::from_bundle();
    let layout = layout(&source).unwrap_or_else(|error| panic!("the bundled mods: {error}"));
    let shared: slotted_packs::SharedSource = Arc::new(source.clone());

    let mut app = App::new();
    // Before `DefaultPlugins`: Bevy builds asset sources when the server is
    // created and ignores later registrations.
    app.add_plugins(PackSourcePlugin::new(layout).with_assets(shared))
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: TITLE.into(),
                canvas: Some(CANVAS.to_owned()),
                fit_canvas_to_parent: true,
                // The page owns the keyboard: Ctrl+Enter runs the script and
                // the editor needs every other key.
                prevent_default_event_handling: false,
                ..default()
            }),
            ..default()
        }))
        .add_plugins((
            bevy::input_focus::tab_navigation::TabNavigationPlugin,
            bevy::input_focus::directional_navigation::DirectionalNavigationPlugin,
        ))
        .insert_resource(runtime())
        .add_plugins(slotted::SlottedPlugins::default().set(SlottedPacksPlugin {
            config: PacksConfig {
                hud_tick: None,
                // Nothing watches files here; a reload is always something
                // the page asked for.
                reload_on_change: false,
                ..PacksConfig::default()
            },
        }))
        .insert_resource(ClearColor(Color::srgb(0.043, 0.055, 0.078)))
        .insert_resource(source)
        .insert_resource(bus)
        .add_plugins(scene::ScenePlugin)
        .add_message::<StartTests>()
        .add_message::<RestoreState>()
        .add_systems(PreUpdate, drain_requests)
        .add_systems(PreUpdate, apply_restore.after(drain_requests))
        .add_systems(Update, (begin_tests, tests::run_live_tests).chain())
        .add_systems(PostUpdate, (pump_console, publish_snapshot));
    app
}

/// `publish_snapshot` under a name an integration test can add as a system.
pub fn publish_snapshot_for_test(
    bus: Res<Bus>,
    registries: Option<Res<slotted::ecs::Registries>>,
    menus: Query<&slotted::ecs::menu::OpenMenu>,
    inventories: Query<Ref<slotted::ecs::menu::Inventory>>,
) {
    publish_snapshot(bus, registries, menus, inventories);
}

/// The `Request::Restore` half of `drain_requests`, for a test that has a
/// world but no loader and no `EditableSource`.
///
/// Everything else on the queue is put back, so a caller that also drives the
/// real `drain_requests` still sees its writes and reloads.
pub fn drain_requests_into(bus: &Bus, world: &mut World) {
    let mut left_over = Vec::new();
    for request in bus.take_requests() {
        match request {
            Request::Restore { state } => {
                if let Some(mut messages) = world.get_resource_mut::<Messages<RestoreState>>() {
                    messages.write(RestoreState::new(state));
                }
            }
            other => left_over.push(other),
        }
    }
    bus.requeue(left_over);
}

/// `PostUpdate`: the open menu's inventories onto the bus, where
/// `snapshot_state` can find them.
///
/// Only when one of them changed. Walking three inventories and writing RON is
/// cheap, but doing it every frame for state nobody asked for still costs more
/// than reading three change ticks, and a chest that nobody touched publishes
/// the same bytes it published last frame.
fn publish_snapshot(
    bus: Res<Bus>,
    registries: Option<Res<slotted::ecs::Registries>>,
    menus: Query<&slotted::ecs::menu::OpenMenu>,
    inventories: Query<Ref<slotted::ecs::menu::Inventory>>,
) {
    let Some(registries) = registries else { return };
    let Some(menu) = menus.iter().next() else {
        return;
    };
    let held: Vec<Ref<slotted::ecs::menu::Inventory>> = menu
        .inventories
        .iter()
        .filter_map(|entity| inventories.get(*entity).ok())
        .collect();
    if held.len() != menu.inventories.len() {
        return;
    }
    // `is_added` too: the very first frame is the one that gives the page
    // something to hold before the visitor has touched anything.
    if !held.iter().any(|held| held.is_changed() || held.is_added()) {
        return;
    }
    let snapshot = snapshot::Snapshot::capture(&registries, held.iter().map(|held| &held.0));
    match snapshot.to_ron() {
        Ok(text) => bus.set_snapshot(text),
        Err(error) => warn!("the snapshot did not serialise: {error}"),
    }
}

/// The page handed back a snapshot after a restart.
///
/// A message rather than work done inside `drain_requests`, because the menu
/// may not exist yet: `restore_state` is called as soon as the module is up
/// and `open_chest` runs in `Startup`. An unapplied restore is held and
/// retried for up to [`RestoreState::TRIES`] frames.
#[derive(Message, Debug, Clone, PartialEq, Eq)]
pub struct RestoreState {
    /// A [`snapshot::Snapshot`] as RON.
    pub state: String,
    /// Frames left to wait for a menu to exist.
    pub tries: u8,
}

impl RestoreState {
    /// How many frames a restore waits for the menu to be spawned before it
    /// gives up and says so. The chest opens in `Startup`, so one frame is
    /// almost always enough; the rest is for a slow first asset load.
    pub const TRIES: u8 = 120;

    /// A restore of `state` with a full budget of retries.
    pub fn new(state: impl Into<String>) -> Self {
        Self {
            state: state.into(),
            tries: Self::TRIES,
        }
    }
}

/// `apply_restore` under a name an integration test can add as a system.
pub fn apply_restore_for_test(
    bus: Res<Bus>,
    pending: MessageReader<RestoreState>,
    held: Local<Option<RestoreState>>,
    registries: Option<Res<slotted::ecs::Registries>>,
    menus: Query<&slotted::ecs::menu::OpenMenu>,
    inventories: Query<&mut slotted::ecs::menu::Inventory>,
) {
    apply_restore(bus, pending, held, registries, menus, inventories);
}

/// `PreUpdate`, after `drain_requests`: puts a snapshot back into the menu.
fn apply_restore(
    bus: Res<Bus>,
    mut pending: MessageReader<RestoreState>,
    mut held: Local<Option<RestoreState>>,
    registries: Option<Res<slotted::ecs::Registries>>,
    menus: Query<&slotted::ecs::menu::OpenMenu>,
    mut inventories: Query<&mut slotted::ecs::menu::Inventory>,
) {
    // Only the newest matters: two restores in flight mean the page sent one
    // twice, and the older is by definition the staler picture.
    if let Some(latest) = pending.read().last().cloned() {
        *held = Some(latest);
    }
    let Some(request) = held.clone() else {
        return;
    };
    let ready = registries.as_ref().zip(menus.iter().next());
    let Some((registries, menu)) = ready else {
        if request.tries > 0 {
            *held = Some(RestoreState {
                tries: request.tries - 1,
                ..request
            });
        } else {
            *held = None;
            bus.log(
                "warn",
                "playground",
                "the restored state had nowhere to go: no menu was ever opened",
            );
        }
        return;
    };
    *held = None;

    let snapshot = match snapshot::Snapshot::from_ron(&request.state) {
        Ok(snapshot) => snapshot,
        Err(error) => {
            bus.log(
                "error",
                "playground",
                format!("the restored state was unreadable, keeping the demo's: {error}"),
            );
            return;
        }
    };

    let mut dropped = 0;
    for (index, entity) in menu.inventories.iter().enumerate() {
        if let Ok(mut held) = inventories.get_mut(*entity) {
            dropped += snapshot.apply_to(index, registries, &mut held.0);
        }
    }
    let note = if dropped == 0 {
        "restored the chest as it was".to_owned()
    } else {
        format!("restored the chest; {dropped} stack(s) no longer exist and were dropped")
    };
    bus.log("info", "playground", note);
}

/// `drain_requests` under a name the integration test can add as a system.
pub fn drain_requests_for_test(
    bus: Res<Bus>,
    source: Res<EditableSource>,
    reload: MessageWriter<ReloadMod>,
    start_tests: MessageWriter<StartTests>,
    restore: MessageWriter<RestoreState>,
    console: ResMut<scene::ConsoleErrors>,
    visible: ResMut<scene::ConsoleVisible>,
) {
    drain_requests(bus, source, reload, start_tests, restore, console, visible);
}

/// `begin_tests` under a name the integration test can add as a system.
///
/// The Tests tab's whole path is `Request::RunTests` -> `StartTests` ->
/// `LiveTestRunner` -> ops against the live world, and a native test that
/// skipped this step would prove only half of it.
pub fn begin_tests_for_test(world: &mut World) {
    begin_tests(world);
}

/// The page pressed Run tests. A message rather than a direct start: the
/// runner needs the whole `World` and `drain_requests` is a plain system.
#[derive(Message, Debug, Clone, PartialEq, Eq)]
pub struct StartTests {
    /// Which mod's bundled tests.
    pub mod_id: String,
}

/// `Update`, exclusive: one `StartTests` begins a run, replacing whatever was
/// running. A second press while a run is in flight is a restart, which is
/// what pressing it again means.
fn begin_tests(world: &mut World) {
    let Some(mut messages) = world.get_resource_mut::<Messages<StartTests>>() else {
        return;
    };
    let Some(request) = messages.drain().last() else {
        return;
    };
    let bus = world.resource::<Bus>().clone();
    bus.log(
        "info",
        "test",
        format!("running {}'s tests", request.mod_id),
    );
    match tests::LiveTestRunner::start(world, &request.mod_id) {
        Ok(runner) => world.insert_resource(runner),
        Err(message) => bus.log("error", "test", format!("{}: {message}", request.mod_id)),
    }
}

/// `PreUpdate`: what the page asked for, up to and including one reload.
///
/// A `Write` lands in the source the loader reads, so the `Reload` right
/// behind it sees the new text. The order in the queue is the order they are
/// applied, and that is why this stops at the first reload: a second edit
/// queued in the same frame must not overwrite the file the first reload has
/// not read yet. Everything after the reload goes back on the queue for the
/// next frame, so two fast reloads run as two reloads, in order, each over
/// its own text.
fn drain_requests(
    bus: Res<Bus>,
    source: Res<EditableSource>,
    mut reload: MessageWriter<ReloadMod>,
    mut start_tests: MessageWriter<StartTests>,
    mut restore: MessageWriter<RestoreState>,
    mut console: ResMut<scene::ConsoleErrors>,
    mut visible: ResMut<scene::ConsoleVisible>,
) {
    let mut queued = bus.take_requests().into_iter();
    for request in queued.by_ref() {
        match request {
            Request::CanvasConsole(on) => visible.0 = on,
            Request::RunTests { mod_id } => {
                start_tests.write(StartTests { mod_id });
            }
            Request::Restore { state } => {
                restore.write(RestoreState::new(state));
            }
            Request::Write { path, contents } => source.write(&path, &contents),
            Request::Reload { mod_id } => match ModId::new(&mod_id) {
                Ok(mod_id) => {
                    bus.log("info", "playground", format!("reloading {mod_id}"));
                    reload.write(ReloadMod { mod_id });
                    break;
                }
                Err(error) => {
                    let message = format!("`{mod_id}` is not a mod id: {error}");
                    bus.log("error", "playground", message.clone());
                    console.0.push(message);
                }
            },
        }
    }
    let left_over: Vec<Request> = queued.collect();
    if !left_over.is_empty() {
        bus.requeue(left_over);
    }
}

/// `pump_console` under a name an integration test can add as a system.
pub fn pump_console_for_test(
    bus: Res<Bus>,
    logs: MessageReader<ScriptLog>,
    failures: MessageReader<ModFailed>,
    reloaded: MessageReader<ModReloaded>,
) {
    pump_console(bus, logs, failures, reloaded);
}

/// `PostUpdate`: what the world said, onto the page's console.
fn pump_console(
    bus: Res<Bus>,
    mut logs: MessageReader<ScriptLog>,
    mut failures: MessageReader<ModFailed>,
    mut reloaded: MessageReader<ModReloaded>,
) {
    for log in logs.read() {
        let who = log
            .mod_id
            .as_ref()
            .map_or_else(|| "host".to_owned(), ToString::to_string);
        bus.log(level_name(log.level), who, log.message.clone());
    }
    for failure in failures.read() {
        let who = failure
            .mod_id
            .as_ref()
            .map_or_else(|| "loader".to_owned(), ToString::to_string);
        bus.log("error", who, failure.error.to_string());
    }
    for event in reloaded.read() {
        bus.log(
            "info",
            "playground",
            format!("{} reloaded; the chest kept its contents", event.mod_id),
        );
    }
}

fn level_name(level: slotted_script::LogLevel) -> &'static str {
    match level {
        slotted_script::LogLevel::Trace => "trace",
        slotted_script::LogLevel::Debug => "debug",
        slotted_script::LogLevel::Info => "info",
        slotted_script::LogLevel::Warn => "warn",
        slotted_script::LogLevel::Error => "error",
    }
}
