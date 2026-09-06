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
//! script runtime is piccolo ([`slotted_script_piccolo`]) rather than Luau,
//! because Luau is C++ and cannot reach wasm at all (ADR 0001). Everything
//! between those two edges is the shipped code: the same `SlottedPlugins`, the same
//! `SlottedPacksPlugin`, the same `ModLoader::reload_mod`, and therefore the
//! same remap-by-name that keeps the chest's contents across a reload.
//!
//! The page talks to the world through [`bus::Bus`] and nothing else. See
//! `bridge` for the `wasm-bindgen` exports.

pub mod bundle;
pub mod bus;
pub mod scene;

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
/// piccolo on every target, native runs included: it is the runtime ADR 0001
/// chose for the browser, and running the same one natively is what makes the
/// native tests say something about the page. mlua cannot reach wasm at all,
/// so there is nothing to swap to here.
pub fn runtime() -> ScriptHost {
    ScriptHost::new(slotted_script_piccolo::PiccoloRuntime::new(
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
        .add_systems(PreUpdate, drain_requests)
        .add_systems(PostUpdate, pump_console);
    app
}

/// `drain_requests` under a name the integration test can add as a system.
pub fn drain_requests_for_test(
    bus: Res<Bus>,
    source: Res<EditableSource>,
    reload: MessageWriter<ReloadMod>,
    console: ResMut<scene::ConsoleErrors>,
    visible: ResMut<scene::ConsoleVisible>,
) {
    drain_requests(bus, source, reload, console, visible);
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
    mut console: ResMut<scene::ConsoleErrors>,
    mut visible: ResMut<scene::ConsoleVisible>,
) {
    let mut queued = bus.take_requests().into_iter();
    for request in queued.by_ref() {
        match request {
            Request::CanvasConsole(on) => visible.0 = on,
            Request::RunTests { mod_id } => {
                // PHASE6-IMPL: C. Start a `LiveTestRunner` over
                // `slotted_test::live::LiveDriver` for the mod's bundled
                // `tests/*.lua`; it performs one op per frame and logs results.
                bus.log(
                    "warn",
                    "test",
                    format!("tests for {mod_id} are not wired yet"),
                );
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
