//! What the page can call.
//!
//! All free functions, because the browser holds no handle on the `App`. They talk to the world through [`Bus::global`] and to the editor's
//! text through the same bundle the loader reads, so a `reload_mod` here and a
//! file save on disk take the identical path through
//! `ModLoader::reload_mod`: re-run the data stage, freeze, remap every live
//! stack by name, re-run the control stage. That remap is why the chest still
//! holds its 64 copper ingots after the edit.

use wasm_bindgen::prelude::*;

use crate::bundle;
use crate::bus::{Bus, Line, Request};
use crate::scenes::themes as showcase_themes;
use crate::showcase::{self, Scene};

/// Starts the app. `wasm-bindgen`'s generated `default()` calls this.
///
/// Two hooks go in before the app does, and both exist for the same reason:
/// on `wasm32` a Lua error raised by a mod is a trap that takes the module
/// down (ADR 0004), so anything the page is going to learn about it has to be
/// said before that happens.
#[wasm_bindgen(start)]
pub fn start() {
    console_error_panic_hook::set_once();
    install_lua_error_reporter();
    crate::build_app(Bus::global()).run();
}

/// Puts a mod's Lua error on the browser console the moment it is raised.
///
/// The adapter calls this from the `xpcall` message handler, which Luau runs
/// *before* it throws, so the text is out of the module before the trap. The
/// page reads it back out of the console when it handles the
/// `WebAssembly.RuntimeError` and repeats it in its own console pane, which is
/// the only place a visitor is looking.
///
/// It also goes on the bus, for the native run and for the case where the
/// error did not abort anything.
fn install_lua_error_reporter() {
    slotted_script_luaur::install_error_reporter(|raised| {
        let detail = if raised.traceback.is_empty() {
            String::new()
        } else {
            format!("\n{}", raised.traceback)
        };
        let text = format!("{}: {}{detail}", raised.script, raised.message);
        console_error(&format!("{LUA_ERROR_PREFIX}{text}"));
        Bus::global().log("error", raised.script.clone(), text);
    });
}

/// What [`install_lua_error_reporter`] prefixes its `console.error` with, so
/// the page can pick its own line out of everything else on the console.
pub const LUA_ERROR_PREFIX: &str = "slotted-lua-error: ";

/// `console.error(text)`, through `js_sys` rather than a `web-sys` dependency
/// the bundle would otherwise carry for one call.
fn console_error(text: &str) {
    let Ok(console) = js_sys::Reflect::get(&js_sys::global(), &JsValue::from_str("console")) else {
        return;
    };
    let Ok(error) = js_sys::Reflect::get(&console, &JsValue::from_str("error")) else {
        return;
    };
    if let Ok(error) = error.dyn_into::<js_sys::Function>() {
        let _ = error.call1(&console, &JsValue::from_str(text));
    }
}

/// The open menu's inventories, as RON, for the page to hold across a restart.
///
/// The world republishes this about once a second (`SNAPSHOT_EVERY`), so what
/// comes back is at most that stale. An empty string means nothing has been
/// published yet, and the page should keep whatever it already had.
#[wasm_bindgen]
pub fn snapshot_state() -> JsValue {
    JsValue::from_str(&Bus::global().snapshot())
}

/// Puts a [`snapshot_state`] value back into the running app.
///
/// Called once after a restart, before or after the module has finished
/// booting: the request waits for the chest to exist rather than being
/// dropped. A value that is not a snapshot is reported on the console and the
/// demo's own starting contents are kept.
#[wasm_bindgen]
pub fn restore_state(state: JsValue) {
    let Some(state) = state.as_string() else {
        return;
    };
    if state.is_empty() {
        return;
    }
    Bus::global().request(Request::Restore { state });
}

/// The mods and their editable files, as JSON.
///
/// ```json
/// [{"id":"copper_chest","files":[{"name":"data.lua","path":"scripts/..."}]}]
/// ```
#[wasm_bindgen]
pub fn list_mods() -> JsValue {
    JsValue::from_str(&bundle::mods_json())
}

/// One bundled mod file's text.
///
/// `name` is the file name the editor shows (`data.lua`), not a path.
///
/// # Errors
///
/// A mod or a file the bundle does not carry, thrown into the page as a
/// `TypeError`. The page picks both out of its own URL, so a wrong one is a
/// rejected promise rather than an aborted module.
#[wasm_bindgen]
pub fn get_mod_file(mod_id: &str, name: &str) -> Result<String, JsValue> {
    bundle::read_mod_file(mod_id, name).map_err(|error| JsValue::from_str(&error))
}

/// Replaces a mod's chunks and reloads it.
///
/// Either source may be empty, meaning "leave that file alone". The write and
/// the reload are queued together and applied in order in the next
/// `PreUpdate`, so the loader never reads half an edit.
#[wasm_bindgen]
pub fn reload_mod(mod_id: &str, data_src: &str, control_src: &str) {
    let bus = Bus::global();
    if !data_src.is_empty() {
        bus.request(Request::Write {
            path: format!("scripts/{mod_id}/data.lua"),
            contents: data_src.to_owned(),
        });
    }
    if !control_src.is_empty() {
        bus.request(Request::Write {
            path: format!("scripts/{mod_id}/control.lua"),
            contents: control_src.to_owned(),
        });
    }
    bus.request(Request::Reload {
        mod_id: mod_id.to_owned(),
    });
}

/// Runs the mod's `tests/*.lua` against the live app (Phase 6 contract 3.2).
/// Results arrive on the console as `test` lines.
#[wasm_bindgen]
pub fn run_tests(mod_id: &str) {
    Bus::global().request(Request::RunTests {
        mod_id: mod_id.to_owned(),
    });
}

/// Shows or hides the in-canvas console overlay.
///
/// In a browser the overlay starts hidden, because the page has its own
/// console pane and a second copy of the same lines only covers the chest.
/// `?console=canvas` in the URL is the other way to ask for it, and the one
/// that works before the module has started.
#[wasm_bindgen]
pub fn set_canvas_console(on: bool) {
    Bus::global().request(Request::CanvasConsole(on));
}

// ---------------------------------------------------------------------------
// The showcase (docs/design/showcase-contract.md section 4)
// ---------------------------------------------------------------------------

/// The eight scenes as the JSON the rail renders: id, title, caption, the
/// three things to try, and whether the scene is real yet.
#[wasm_bindgen]
pub fn list_scenes() -> JsValue {
    JsValue::from_str(&showcase::scenes_json())
}

/// Switches the canvas to `id`.
///
/// # Errors
///
/// An unknown id, as a `TypeError`. Every scene in the table is real, so there
/// is no second failure any more; `list_scenes` still reports `ready` for a
/// page that greys entries out.
#[wasm_bindgen]
pub fn set_scene(id: &str) -> Result<(), JsValue> {
    let scene = Scene::from_id(id).ok_or_else(|| not_a_scene(id))?;
    Bus::global().request(Request::SetScene(scene));
    Ok(())
}

/// The id of the scene the canvas is showing, or an empty string before the
/// first frame.
#[wasm_bindgen]
pub fn current_scene() -> JsValue {
    JsValue::from_str(Bus::global().scene().map_or("", Scene::id))
}

/// Applies a theme to the open scene: `glass`, `paper` or `neon`.
///
/// The screen is not respawned: the theme system resolves tokens and materials
/// per frame from whatever handle `ActiveTheme` holds, so the panel repaints
/// under an open cursor, a focus ring and a half-finished drag.
///
/// # Errors
///
/// A name that is not one of the three bundled themes.
#[wasm_bindgen]
pub fn set_theme(name: &str) -> Result<(), JsValue> {
    if showcase_themes::path_of(name).is_none() {
        return Err(js_sys::TypeError::new(&format!(
            "`{name}` is not a bundled theme; the page has {}",
            showcase_themes::THEMES.join(", ")
        ))
        .into());
    }
    Bus::global().request(Request::SetTheme {
        name: name.to_owned(),
    });
    Ok(())
}

/// Puts a query into the item browser's search field, the same way a category
/// chip does: the field shows it and a visitor can carry on typing from there.
///
/// Outside the Browser scene it does nothing rather than failing. The page
/// keeps the last scene in its URL and offers the search chips as a control
/// block, so a chip pressed a frame after a switch is ordinary and not an
/// error.
#[wasm_bindgen]
pub fn browser_search(query: &str) {
    Bus::global().request(Request::BrowserSearch {
        query: query.to_owned(),
    });
}

/// Turns the machine's redstone signal on or off.
///
/// It writes the resource whichever scene is open, because the machine's
/// simulation is the only thing that reads it and it only runs while the
/// Machine scene has a furnace. Pressing the toggle elsewhere is harmless and
/// the setting is there when the visitor comes back.
#[wasm_bindgen]
pub fn machine_redstone(on: bool) {
    Bus::global().request(Request::Redstone(on));
}

/// Enters or leaves the HUD position editor.
///
/// While it is on, every HUD layer is pickable and draggable and wears an
/// outline. Leaving the HUD scene turns it off again.
#[wasm_bindgen]
pub fn hud_edit(on: bool) {
    Bus::global().request(Request::HudEdit(on));
}

/// The HUD layout as RON, for the page to keep in `localStorage`.
///
/// An empty string means the visitor has not moved a layer, and the page
/// should leave whatever it had stored alone.
///
/// # Errors
///
/// A layout that will not serialise, which for a map of names to numbers means
/// something is very wrong.
#[wasm_bindgen]
pub fn hud_layout() -> Result<String, JsValue> {
    crate::hud_store::global().to_ron().map_err(|e| {
        js_sys::TypeError::new(&format!("the HUD layout did not serialise: {e}")).into()
    })
}

/// Puts a [`hud_layout`] value back, or resets the layout when `ron` is empty.
///
/// Called once at boot with whatever `localStorage` held. It takes effect the
/// next time the HUD scene is entered, and immediately if it is already open.
///
/// An empty string is the Reset button: it forgets the stored layout and puts
/// every layer back where its own definition asked for it, so the next
/// `hud_layout()` comes back empty and the page can clear its key. That is a
/// separate meaning from a parse failure, which is what an empty string would
/// otherwise be.
///
/// # Errors
///
/// The text is neither empty nor a HUD layout.
#[wasm_bindgen]
pub fn restore_hud_layout(ron: &str) -> Result<(), JsValue> {
    Bus::global().request(Request::RestoreHud {
        ron: ron.to_owned(),
    });
    Ok(())
}

/// Sets the loopback link's one-way latency in milliseconds and the percentage
/// of messages it throws away.
///
/// Both sliders take effect on the next message; anything already in flight
/// keeps the delivery time it was given, which is what a real link does when
/// the network changes under it.
#[wasm_bindgen]
pub fn net_config(latency_ms: u32, drop_percent: u8) {
    Bus::global().request(Request::NetConfig {
        latency_ms,
        drop_percent,
    });
}

/// Loads the bundled recording into the Testing scene and opens the chest it
/// was made over.
#[wasm_bindgen]
pub fn replay_load() {
    Bus::global().request(Request::ReplayLoad);
}

/// Seeks the loaded recording to `frame`.
///
/// Seeking backwards reopens the chest and re-feeds every frame from the
/// start, because an input stream does not run in reverse.
#[wasm_bindgen]
pub fn replay_seek(frame: u32) {
    Bus::global().request(Request::ReplaySeek { frame });
}

/// Plays or pauses the loaded recording. Pressing play at the end starts over.
#[wasm_bindgen]
pub fn replay_play(on: bool) {
    Bus::global().request(Request::ReplayPlay(on));
}

/// `{"frame":n,"frames":n,"playing":bool}` for the scrubber.
///
/// Published by the world once a frame, the same push-pull the snapshot uses:
/// a zero-length recording is what comes back before anything is loaded.
#[wasm_bindgen]
pub fn replay_status() -> JsValue {
    JsValue::from_str(&Bus::global().replay_status())
}

/// What a stubbed export used to throw. Every scene is real now, so nothing
/// throws it any more; the constant stays because `web/playground.js` and
/// `smoke.html` test for the prefix and a page built against the stubs must
/// keep working against the finished module.
pub const NOT_YET_PREFIX: &str = "not yet: ";

fn not_a_scene(id: &str) -> JsValue {
    js_sys::TypeError::new(&format!("`{id}` is not a showcase scene")).into()
}

/// Console lines written since the last call, as JSON.
///
/// The page polls this on an animation frame. [`subscribe_console`] is the
/// push form of the same queue; a page that uses one should not use the other.
#[wasm_bindgen]
pub fn drain_console() -> JsValue {
    JsValue::from_str(&to_json(&Bus::global().drain_console()))
}

/// Every line still in the ring, for a page that attached late.
#[wasm_bindgen]
pub fn console_history() -> JsValue {
    JsValue::from_str(&to_json(&Bus::global().console_history()))
}

thread_local! {
    /// The page's console callback, when it asked to be pushed to.
    static SUBSCRIBER: std::cell::RefCell<Option<js_sys::Function>> =
        const { std::cell::RefCell::new(None) };
}

/// Calls `callback(json)` with new console lines, on every animation frame.
///
/// The callback is invoked from a `requestAnimationFrame` the browser already
/// runs for the canvas, so a subscriber never re-enters the Bevy schedule.
#[wasm_bindgen]
pub fn subscribe_console(callback: js_sys::Function) {
    SUBSCRIBER.with(|slot| *slot.borrow_mut() = Some(callback));
    pump();
}

/// One drain and one callback, then a request for the next frame.
fn pump() {
    let lines = Bus::global().drain_console();
    SUBSCRIBER.with(|slot| {
        if let Some(callback) = slot.borrow().as_ref()
            && !lines.is_empty()
        {
            let _ = callback.call1(&JsValue::NULL, &JsValue::from_str(&to_json(&lines)));
        }
    });
    let closure = Closure::once_into_js(pump);
    if let Some(window) = web_sys_window() {
        let _ = window.request_animation_frame(closure.unchecked_ref());
    }
}

/// `window`, without a `web-sys` dependency the bundle would otherwise carry
/// only for this one call.
fn web_sys_window() -> Option<AnimationFrames> {
    js_sys::global()
        .dyn_into::<js_sys::Object>()
        .ok()
        .map(AnimationFrames)
}

/// The `requestAnimationFrame` half of the global object.
struct AnimationFrames(js_sys::Object);

impl AnimationFrames {
    fn request_animation_frame(&self, callback: &js_sys::Function) -> Result<JsValue, JsValue> {
        let raf = js_sys::Reflect::get(&self.0, &JsValue::from_str("requestAnimationFrame"))?;
        raf.dyn_into::<js_sys::Function>()?.call1(&self.0, callback)
    }
}

fn to_json(lines: &[Line]) -> String {
    let items: Vec<String> = lines.iter().map(Line::to_json).collect();
    format!("[{}]", items.join(","))
}
