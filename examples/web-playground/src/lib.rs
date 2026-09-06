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
pub mod hud_store;
pub mod scene;
pub mod scenes;
pub mod showcase;
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
        // The HUD store goes in before the plugin group, which is the
        // documented way to replace an adapter: here it is the page's
        // `localStorage` behind the `HudLayoutStorage` port
        // (docs/design/showcase-contract.md section 3.5).
        .insert_resource(slotted::ui::hud_editor::HudLayoutStore::new(
            hud_store::global(),
        ))
        .add_plugins(slotted::SlottedPlugins::default().set(SlottedPacksPlugin {
            config: PacksConfig {
                // The HUD scene's `hud_clock` mod writes the time on every
                // tick, so unlike the plain playground this app asks for one.
                // Four a second is enough for a clock and cheap enough that
                // the other seven scenes never notice it.
                hud_tick: Some(std::time::Duration::from_millis(250)),
                // Nothing watches files here; a reload is always something
                // the page asked for.
                reload_on_change: false,
                // `demo` and `machine` are the base pack loaded as mods, which
                // is how a browser tab gets content a running game would have
                // registered itself (see `build.rs`). They register
                // `minecraft:` and `slotted:` ids, which for a real mod would
                // be worth a warning and here is the whole job, so those two
                // namespaces join `c` as shared.
                shared_namespaces: vec![
                    slotted_packs::plugin::SHARED_NAMESPACE.to_owned(),
                    "minecraft".to_owned(),
                    "slotted".to_owned(),
                ],
            },
        }))
        .insert_resource(ClearColor(Color::srgb(0.043, 0.055, 0.078)))
        .insert_resource(source)
        .insert_resource(bus)
        // The furnace simulation, the `machine:face_config` widget and the
        // `R` binding. Added for the whole app rather than by the Machine
        // scene: a widget has to be registered before any screen that names it
        // spawns, and the simulation stops on its own when no furnace is open.
        .add_plugins(::showcase::machine::MachineDemoPlugin)
        // `Esc` closes the chest and `E` opens it again, and the header's
        // title and capacity readout are filled in. Same plugin the windowed
        // chest example adds, so the two behave alike.
        .add_plugins(::showcase::chest::ChestDemoPlugin)
        .add_plugins(scene::ScenePlugin)
        .add_plugins(showcase::ShowcasePlugin)
        .add_message::<StartTests>()
        .add_message::<RestoreState>()
        .add_message::<SceneCommand>()
        .add_systems(PreUpdate, drain_requests)
        // After the scene switch, not merely after the bus drain. A restore
        // may ask for a scene, and a Multiplayer restore then has to wait for
        // that scene's server to exist; with the two unordered the retry could
        // run before the switch every frame and never converge.
        .add_systems(
            PreUpdate,
            apply_restore
                .after(drain_requests)
                .after(showcase::apply_scene_switch),
        )
        .add_systems(
            PreUpdate,
            apply_scene_commands.after(showcase::apply_scene_switch),
        )
        .add_systems(Update, (begin_tests, tests::run_live_tests).chain())
        .add_systems(
            PostUpdate,
            (
                pump_console,
                publish_snapshot,
                publish_replay_status,
                publish_hud_layout,
            ),
        );
    app
}

/// `publish_snapshot` under a name an integration test can add as a system.
pub fn publish_snapshot_for_test(
    bus: Res<Bus>,
    registries: Option<Res<slotted::ecs::Registries>>,
    scene: Option<Res<showcase::ActiveScene>>,
    net: Option<Res<scenes::multiplayer::NetLink>>,
    menus: Query<&slotted::ecs::menu::OpenMenu>,
    inventories: Query<Ref<slotted::ecs::menu::Inventory>>,
) {
    publish_snapshot(bus, registries, scene, net, menus, inventories);
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
    scene: Option<Res<showcase::ActiveScene>>,
    net: Option<Res<scenes::multiplayer::NetLink>>,
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
    let mut snapshot = snapshot::Snapshot::capture(&registries, held.iter().map(|held| &held.0));
    // Which scene the visitor was in, so a restart puts them back there
    // rather than on the page's default (showcase contract section 4).
    snapshot.scene = scene.map(|scene| scene.0.id().to_owned());
    // And in the Multiplayer scene, the server's containers as well as the
    // client's mirror of them. The inventories above are client A's, which is
    // a prediction; `net` is what is actually true (contract section 4).
    snapshot.net = net.and_then(|link| scenes::multiplayer::capture(&link, &registries));
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
#[allow(clippy::too_many_arguments)]
pub fn apply_restore_for_test(
    bus: Res<Bus>,
    pending: MessageReader<RestoreState>,
    held: Local<Option<RestoreState>>,
    registries: Option<Res<slotted::ecs::Registries>>,
    switch: MessageWriter<showcase::SwitchScene>,
    net: Option<ResMut<scenes::multiplayer::NetLink>>,
    menus: Query<&slotted::ecs::menu::OpenMenu>,
    inventories: Query<&mut slotted::ecs::menu::Inventory>,
) {
    apply_restore(
        bus,
        pending,
        held,
        registries,
        switch,
        net,
        menus,
        inventories,
    );
}

/// `PreUpdate`, after `drain_requests`: puts a snapshot back into the menu.
#[allow(clippy::too_many_arguments)]
fn apply_restore(
    bus: Res<Bus>,
    mut pending: MessageReader<RestoreState>,
    mut held: Local<Option<RestoreState>>,
    registries: Option<Res<slotted::ecs::Registries>>,
    mut switch: MessageWriter<showcase::SwitchScene>,
    mut net: Option<ResMut<scenes::multiplayer::NetLink>>,
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

    // The scene first: it may despawn and respawn the menu the stacks are
    // about to go into, and the switch is applied next frame's `PreUpdate`, so
    // the restore below writes into the menu that is open now and the scene
    // change lands on top of it. A visitor who was in the Chest scene when a
    // mod raised comes back to the Chest scene.
    if let Some(scene) = snapshot.scene() {
        switch.write(showcase::SwitchScene(scene));
    }

    // A Multiplayer snapshot cannot be applied to the world that is open now:
    // it names the server's containers, and the server is built by the scene
    // the line above has only just asked for. So the request is put back and
    // retried, and lands the frame after the scene has entered, which is what
    // the retry budget was already there for. The generic path below is
    // skipped for the same reason: writing a client's mirror into whatever
    // menu happens to be open would be writing a prediction into a chest.
    if let Some(wanted) = snapshot.net.as_ref() {
        let Some(link) = net.as_mut() else {
            if request.tries > 0 {
                *held = Some(RestoreState {
                    tries: request.tries - 1,
                    ..request
                });
            } else {
                bus.log(
                    "warn",
                    "playground",
                    "the restored state was the Multiplayer scene's, but its server \
                     never came up",
                );
            }
            return;
        };
        // The server, and then a full `SetContent` per session so both clients
        // forget what they predicted. `restore` sends those; they cross the
        // loopback and land through the ordinary client path, which is the
        // same route a correction takes.
        let dropped = scenes::multiplayer::restore(link, registries, wanted);
        bus.log(
            "info",
            "playground",
            if dropped == 0 {
                "restored the server's chest and both players' pockets".to_owned()
            } else {
                format!(
                    "restored the server's chest and both players' pockets; \
                     {dropped} stack(s) no longer exist and were dropped"
                )
            },
        );
        return;
    }

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
#[allow(clippy::too_many_arguments)]
pub fn drain_requests_for_test(
    bus: Res<Bus>,
    source: Res<EditableSource>,
    reload: MessageWriter<ReloadMod>,
    start_tests: MessageWriter<StartTests>,
    restore: MessageWriter<RestoreState>,
    switch: MessageWriter<showcase::SwitchScene>,
    scenes: MessageWriter<SceneCommand>,
    console: ResMut<scene::ConsoleErrors>,
    visible: ResMut<scene::ConsoleVisible>,
) {
    drain_requests(
        bus,
        source,
        reload,
        start_tests,
        restore,
        switch,
        scenes,
        console,
        visible,
    );
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
#[allow(clippy::too_many_arguments)]
pub(crate) fn drain_requests(
    bus: Res<Bus>,
    source: Res<EditableSource>,
    mut reload: MessageWriter<ReloadMod>,
    mut start_tests: MessageWriter<StartTests>,
    mut restore: MessageWriter<RestoreState>,
    mut switch: MessageWriter<showcase::SwitchScene>,
    mut scenes: MessageWriter<SceneCommand>,
    mut console: ResMut<scene::ConsoleErrors>,
    mut visible: ResMut<scene::ConsoleVisible>,
) {
    let mut queued = bus.take_requests().into_iter();
    for request in queued.by_ref() {
        match request {
            Request::CanvasConsole(on) => visible.0 = on,
            Request::SetScene(scene) => {
                switch.write(showcase::SwitchScene(scene));
            }
            Request::RunTests { mod_id } => {
                start_tests.write(StartTests { mod_id });
            }
            Request::Restore { state } => {
                restore.write(RestoreState::new(state));
            }
            Request::Write { path, contents } => source.write(&path, &contents),
            // Everything a scene owns needs the whole world -- an asset
            // server, a `Screens` registry, a replay cursor that respawns a
            // menu -- and `drain_requests` is a plain system. They become
            // messages an exclusive system applies a step later, which is the
            // same shape `Restore` already had and for the same reason.
            Request::SetTheme { name } => {
                scenes.write(SceneCommand::SetTheme { name });
            }
            Request::BrowserSearch { query } => {
                scenes.write(SceneCommand::BrowserSearch { query });
            }
            Request::Redstone(on) => {
                scenes.write(SceneCommand::Redstone(on));
            }
            Request::HudEdit(on) => {
                scenes.write(SceneCommand::HudEdit(on));
            }
            Request::RestoreHud { ron } => {
                scenes.write(SceneCommand::RestoreHud { ron });
            }
            Request::NetConfig {
                latency_ms,
                drop_percent,
            } => {
                scenes.write(SceneCommand::NetConfig {
                    latency_ms,
                    drop_percent,
                });
            }
            Request::ReplayLoad => {
                scenes.write(SceneCommand::ReplayLoad);
            }
            Request::ReplaySeek { frame } => {
                scenes.write(SceneCommand::ReplaySeek { frame });
            }
            Request::ReplayPlay(on) => {
                scenes.write(SceneCommand::ReplayPlay(on));
            }
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

// ---------------------------------------------------------------------------
// Scene commands
// ---------------------------------------------------------------------------

/// Something one scene's controls asked for, waiting for the whole `World`.
///
/// `drain_requests` is a plain system and every one of these needs more than a
/// plain system can borrow: an `AssetServer` and a resource swap for the theme,
/// the `Screens` registry and a menu respawn for a replay seek, a `MenuServer`
/// living in a resource for the link conditions. Turning them into a message
/// applied by [`apply_scene_commands`] a step later is the same arrangement
/// [`RestoreState`] already uses.
#[derive(Message, Debug, Clone, PartialEq, Eq)]
pub enum SceneCommand {
    /// Repaint in a bundled theme.
    SetTheme {
        /// `glass`, `paper` or `neon`.
        name: String,
    },
    /// A query for the item browser's search field.
    BrowserSearch {
        /// The query, in the browser's search grammar.
        query: String,
    },
    /// The machine scene's redstone signal.
    Redstone(bool),
    /// Enter or leave the HUD position editor.
    HudEdit(bool),
    /// Put a stored HUD layout back.
    RestoreHud {
        /// A `slotted_ui::HudLayout` as RON.
        ron: String,
    },
    /// The multiplayer link's latency and loss.
    NetConfig {
        /// One-way latency in milliseconds.
        latency_ms: u32,
        /// Percentage of messages thrown away.
        drop_percent: u8,
    },
    /// Load the bundled recording.
    ReplayLoad,
    /// Move the scrubber.
    ReplaySeek {
        /// Recorded frame index.
        frame: u32,
    },
    /// Play or pause.
    ReplayPlay(bool),
}

/// `apply_scene_commands` under a name an integration test can add as a system.
pub fn apply_scene_commands_for_test(world: &mut World) {
    apply_scene_commands(world);
}

/// `PreUpdate`, exclusive, after the scene switch: everything the page asked a
/// scene for.
///
/// Every arm reports its own failure on the console and changes nothing else.
/// A control aimed at a scene that is not open is the ordinary case here, not
/// an error condition: the page keeps the last scene's URL across a reload, and
/// a slider dragged a frame after a switch has to be a line on the console
/// rather than a dead tab.
pub fn apply_scene_commands(world: &mut World) {
    let Some(mut messages) = world.get_resource_mut::<Messages<SceneCommand>>() else {
        return;
    };
    let commands: Vec<SceneCommand> = messages.drain().collect();
    if commands.is_empty() {
        return;
    }
    let bus = world.resource::<Bus>().clone();
    for command in commands {
        let outcome = match command {
            SceneCommand::SetTheme { name } => scenes::themes::apply(world, &name),
            SceneCommand::BrowserSearch { query } => {
                scenes::chest::search(world, &query);
                Ok(())
            }
            SceneCommand::Redstone(on) => {
                world.insert_resource(::showcase::machine::Redstone(on));
                Ok(())
            }
            SceneCommand::HudEdit(on) => {
                world.insert_resource(slotted::ui::hud_editor::HudEditMode(on));
                Ok(())
            }
            SceneCommand::RestoreHud { ron } => scenes::hud::restore_layout_ron(world, &ron),
            SceneCommand::NetConfig {
                latency_ms,
                drop_percent,
            } => match world.get_resource::<scenes::multiplayer::NetLink>() {
                Some(link) => {
                    link.set_conditions(latency_ms, drop_percent);
                    Ok(())
                }
                None => Err("no link: the Multiplayer scene is not open".to_owned()),
            },
            SceneCommand::ReplayLoad => scenes::testing::load(world),
            SceneCommand::ReplaySeek { frame } => scenes::testing::seek(world, frame as usize),
            SceneCommand::ReplayPlay(on) => scenes::testing::play(world, on),
        };
        if let Err(message) = outcome {
            bus.log("error", "showcase", message);
        }
    }
}

/// `PostUpdate`: the HUD layout onto the global store, where `hud_layout` can
/// find it.
///
/// The world's store and the global are the same object in the running app, so
/// this is a no-op there. It exists for the shape: everything else the page
/// reads is published, and a test that builds its own store gets to keep it to
/// itself.
pub fn publish_hud_layout(world: &mut World) {
    if world
        .get_resource::<slotted::ui::hud_editor::HudLayoutStore>()
        .is_none()
    {
        return;
    }
    if let Ok(ron) = scenes::hud::layout_ron(world) {
        let global = hud_store::global();
        if ron.is_empty() {
            global.set(None);
        } else {
            let _ = global.from_ron(&ron);
        }
    }
}

/// `PostUpdate`: the replay scrubber's position onto the bus, where
/// `replay_status` can find it.
pub fn publish_replay_status(world: &mut World) {
    let status = scenes::testing::status(world);
    world.resource::<Bus>().set_replay_status(status);
}
