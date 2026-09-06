//! What the page can call.
//!
//! Seven functions, all of them free, because the browser holds no handle on
//! the `App`. They talk to the world through [`Bus::global`] and to the editor's
//! text through the same bundle the loader reads, so a `reload_mod` here and a
//! file save on disk take the identical path through
//! `ModLoader::reload_mod`: re-run the data stage, freeze, remap every live
//! stack by name, re-run the control stage. That remap is why the chest still
//! holds its 64 copper ingots after the edit.

use wasm_bindgen::prelude::*;

use crate::bundle;
use crate::bus::{Bus, Line, Request};

/// Starts the app. `wasm-bindgen`'s generated `default()` calls this.
#[wasm_bindgen(start)]
pub fn start() {
    console_error_panic_hook::set_once();
    crate::build_app(Bus::global()).run();
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
