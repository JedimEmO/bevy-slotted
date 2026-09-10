//! The one channel between the page and the Bevy world.
//!
//! The browser calls the `wasm-bindgen` exports whenever the user presses Run;
//! Bevy owns the `World` and only touches it inside a system. Everything in
//! between is this: a queue of requests drained in `PreUpdate` and a ring of
//! console lines filled in `PostUpdate`. Nothing else crosses.
//!
//! It is a `Mutex` rather than an `mpsc` pair because both ends are needed on
//! both sides: the page drains the console and the world drains the requests.
//! The browser runs this on one thread, so the lock never contends; the native
//! debug build and the tests use the same type, which is what makes the
//! plumbing testable without a browser.

use std::collections::VecDeque;
use std::fmt::Write as _;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError};

use bevy::prelude::Resource;

/// How many console lines the ring keeps. A busy script logs on every click;
/// the page only ever shows the tail.
pub const CONSOLE_CAPACITY: usize = 400;

/// Something the page asked the world to do.
#[derive(Debug, Clone, PartialEq)]
pub enum Request {
    /// New text for one file, before the reload that reads it.
    Write {
        /// The logical path, `scripts/<mod>/<name>`.
        path: String,
        /// The new contents.
        contents: String,
    },
    /// Re-run the data stage, freeze, remap and control stage for this mod.
    Reload {
        /// The mod id.
        mod_id: String,
    },
    /// Show or hide the in-canvas console overlay.
    ///
    /// The page has its own console pane, so in a browser the overlay starts
    /// hidden; this is how a page with no pane asks for it back after the
    /// module has already started.
    CanvasConsole(bool),
    /// Run this mod's `tests/*.lua` against the live app, one op per frame,
    /// reporting `ok` / `FAIL` lines to the console (Phase 6 contract 3.2).
    RunTests {
        /// The mod id.
        mod_id: String,
    },
    /// Put the open menu's inventories back to what a [`Snapshot`] holds.
    ///
    /// The page sends this once, right after a restart. On `wasm32` a mod that
    /// raises aborts the module (ADR 0004), so the page re-instantiates it and
    /// hands back the last snapshot it took; without this the chest would come
    /// back with the demo's starting contents and the visitor's arrangement
    /// would be gone.
    ///
    /// [`Snapshot`]: crate::snapshot::Snapshot
    Restore {
        /// A [`Snapshot`](crate::snapshot::Snapshot) as RON.
        state: String,
    },
    /// Switch the canvas to another showcase scene
    /// (docs/design/showcase-contract.md section 1).
    SetScene(showcase::Scene),
    /// Repaint the open screen in another bundled theme: `glass`, `paper` or
    /// `neon` (showcase contract section 3.3).
    SetTheme {
        /// The theme name.
        name: String,
    },
    /// Put a query into the item browser's search field, the way a category
    /// chip does. Chest scene only; a no-op anywhere else.
    BrowserSearch {
        /// The query, in the browser's search grammar.
        query: String,
    },
    /// Turn the machine scene's redstone signal on or off.
    Redstone(bool),
    /// Enter or leave the HUD position editor.
    HudEdit(bool),
    /// Put a HUD layout the page kept in `localStorage` back.
    RestoreHud {
        /// A `slotted_ui::HudLayout` as RON.
        ron: String,
    },
    /// Set the multiplayer scene's loopback conditions.
    NetConfig {
        /// One-way latency in milliseconds.
        latency_ms: u32,
        /// Percentage of messages the link throws away.
        drop_percent: u8,
    },
    /// Load the bundled recording into the Testing scene.
    ReplayLoad,
    /// Move the Testing scene's scrubber to a recorded frame.
    ReplaySeek {
        /// Recorded frame index.
        frame: u32,
    },
    /// Play or pause the loaded recording.
    ReplayPlay(bool),
    /// Push one of the Menus scene's screens by name
    /// (docs/design/showcase-refresh-contract.md section 5). Menus only.
    MenuOpen(crate::showcase::MenuScreen),
    /// Put the saved settings the page kept in `localStorage` back, or reset
    /// them when the text is empty.
    RestoreSettings {
        /// A `slotted_menu::SavedSettings` as RON.
        ron: String,
    },
    /// Start the smith's conversation again. Dialogue only.
    TalkAgain,
    /// Write one declared settings key into the value store.
    SetValue {
        /// A key `showcase::settings::spec` declares.
        key: String,
        /// The value, already of the kind the key's default has.
        value: slotted::ui::Value,
    },
}

/// One line the console shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    /// `trace`, `debug`, `info`, `warn` or `error`.
    pub level: String,
    /// The mod that said it, or the host.
    pub who: String,
    /// The message.
    pub text: String,
}

impl Line {
    /// A line as the page's JSON: `{"level":..,"who":..,"text":..}`.
    pub fn to_json(&self) -> String {
        format!(
            "{{\"level\":{},\"who\":{},\"text\":{}}}",
            quote(&self.level),
            quote(&self.who),
            quote(&self.text)
        )
    }
}

/// A JSON string literal. Small enough not to be worth a dependency the wasm
/// bundle would carry.
pub fn quote(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[derive(Debug, Default)]
struct Inner {
    requests: VecDeque<Request>,
    console: VecDeque<Line>,
    /// Lines the page has not collected yet, for `subscribe_console`.
    pending: VecDeque<Line>,
    /// The last snapshot the world published, as RON, for `snapshot_state`.
    ///
    /// The page cannot reach into the `World`, and the exports are free
    /// functions with no handle on the app, so the world pushes and the page
    /// pulls. Empty until the first publish.
    snapshot: String,
    /// The scene the world is showing, for `current_scene`. Same push-pull
    /// arrangement as the snapshot.
    scene: Option<showcase::Scene>,
    /// The replay scrubber's position, as the JSON `replay_status` returns.
    /// Same push-pull arrangement again: the export is a free function and
    /// cannot reach into the `World` to ask.
    replay_status: String,
    /// The kind of the screen on top of the stack, for `current_screen`;
    /// empty when nothing is open. Published by the world once a frame.
    screen: String,
    /// The saved settings as RON, for `settings_ron`. The
    /// [`PageSettings`](crate::settings_store::PageSettings) store writes it
    /// on every save and reads it back as the load, so the bus is the one
    /// copy and the page's `localStorage` mirrors it.
    settings: String,
}

/// The shared queue, held by the page side and by the world alike.
#[derive(Resource, Debug, Clone, Default)]
pub struct Bus(Arc<Mutex<Inner>>);

impl Bus {
    /// An empty bus.
    pub fn new() -> Self {
        Self::default()
    }

    /// The process-wide bus the `wasm-bindgen` exports use.
    ///
    /// The exports are free functions the browser calls with no handle on the
    /// app, so the one thing they share with it has to be reachable from a
    /// `static`.
    pub fn global() -> Self {
        static GLOBAL: OnceLock<Bus> = OnceLock::new();
        GLOBAL.get_or_init(Bus::new).clone()
    }

    /// Queues a request for the next `PreUpdate`.
    pub fn request(&self, request: Request) {
        self.lock().requests.push_back(request);
    }

    /// Takes everything queued, oldest first.
    pub fn take_requests(&self) -> Vec<Request> {
        self.lock().requests.drain(..).collect()
    }

    /// Puts requests back at the head of the queue, keeping their order.
    ///
    /// The world takes the whole queue and then stops at the first reload,
    /// because a reload has to read the write in front of it and not the one
    /// behind it. What is left over comes back here for the next frame.
    pub fn requeue(&self, requests: Vec<Request>) {
        let mut inner = self.lock();
        for request in requests.into_iter().rev() {
            inner.requests.push_front(request);
        }
    }

    /// Appends a console line.
    pub fn log(&self, level: impl Into<String>, who: impl Into<String>, text: impl Into<String>) {
        let line = Line {
            level: level.into(),
            who: who.into(),
            text: text.into(),
        };
        let mut inner = self.lock();
        inner.console.push_back(line.clone());
        inner.pending.push_back(line);
        while inner.console.len() > CONSOLE_CAPACITY {
            inner.console.pop_front();
        }
        while inner.pending.len() > CONSOLE_CAPACITY {
            inner.pending.pop_front();
        }
    }

    /// Publishes the world's current state, replacing whatever was there.
    pub fn set_snapshot(&self, state: impl Into<String>) {
        self.lock().snapshot = state.into();
    }

    /// The last published state, or an empty string before the first publish.
    pub fn snapshot(&self) -> String {
        self.lock().snapshot.clone()
    }

    /// Publishes which scene the world is showing.
    pub fn set_scene(&self, scene: showcase::Scene) {
        self.lock().scene = Some(scene);
    }

    /// The scene the world last said it was showing, or `None` before the
    /// first frame.
    pub fn scene(&self) -> Option<showcase::Scene> {
        self.lock().scene
    }

    /// Publishes the replay scrubber's position, as JSON.
    pub fn set_replay_status(&self, status: impl Into<String>) {
        self.lock().replay_status = status.into();
    }

    /// The last published scrubber position, or an empty recording before the
    /// Testing scene has published one.
    pub fn replay_status(&self) -> String {
        let status = self.lock().replay_status.clone();
        if status.is_empty() {
            "{\"frame\":0,\"frames\":0,\"playing\":false}".to_owned()
        } else {
            status
        }
    }

    /// Publishes the kind of the screen on top of the stack, or an empty
    /// string when nothing is open.
    pub fn set_screen(&self, kind: impl Into<String>) {
        self.lock().screen = kind.into();
    }

    /// The kind of the screen the world last said was on top, or an empty
    /// string before the first frame and while nothing is open.
    pub fn screen(&self) -> String {
        self.lock().screen.clone()
    }

    /// Publishes the saved settings as RON; an empty string means nothing is
    /// saved.
    pub fn set_settings(&self, ron: impl Into<String>) {
        self.lock().settings = ron.into();
    }

    /// The saved settings as RON, or an empty string when nothing was saved.
    pub fn settings(&self) -> String {
        self.lock().settings.clone()
    }

    /// Takes the lines written since the last call.
    pub fn drain_console(&self) -> Vec<Line> {
        self.lock().pending.drain(..).collect()
    }

    /// Every line still in the ring, oldest first. The page uses this once, to
    /// show what happened before it attached.
    pub fn console_history(&self) -> Vec<Line> {
        self.lock().console.iter().cloned().collect()
    }

    /// How many lines the ring holds.
    pub fn len(&self) -> usize {
        self.lock().console.len()
    }

    /// Whether nothing has been logged.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_come_back_in_order_and_only_once() {
        let bus = Bus::new();
        bus.request(Request::Reload { mod_id: "a".into() });
        bus.request(Request::Reload { mod_id: "b".into() });
        let ids: Vec<String> = bus
            .take_requests()
            .into_iter()
            .map(|r| match r {
                Request::Reload { mod_id } | Request::RunTests { mod_id } => mod_id,
                Request::Write { path, .. } | Request::Restore { state: path } => path,
                Request::SetTheme { name } => name,
                Request::BrowserSearch { query } => query,
                Request::RestoreHud { ron } | Request::RestoreSettings { ron } => ron,
                Request::CanvasConsole(on)
                | Request::Redstone(on)
                | Request::HudEdit(on)
                | Request::ReplayPlay(on) => on.to_string(),
                Request::SetScene(scene) => scene.id().to_owned(),
                Request::NetConfig { latency_ms, .. } => latency_ms.to_string(),
                Request::ReplayLoad => "load".to_owned(),
                Request::ReplaySeek { frame } => frame.to_string(),
                Request::MenuOpen(which) => which.id().to_owned(),
                Request::TalkAgain => "talk".to_owned(),
                Request::SetValue { key, .. } => key,
            })
            .collect();
        assert_eq!(ids, ["a", "b"]);
        assert!(bus.take_requests().is_empty());
    }

    #[test]
    fn requeued_requests_keep_their_order_and_come_back_first() {
        let bus = Bus::new();
        bus.request(Request::Reload { mod_id: "c".into() });
        bus.requeue(vec![
            Request::Reload { mod_id: "a".into() },
            Request::Reload { mod_id: "b".into() },
        ]);
        let ids: Vec<String> = bus
            .take_requests()
            .into_iter()
            .map(|r| match r {
                Request::Reload { mod_id } | Request::RunTests { mod_id } => mod_id,
                Request::Write { path, .. } | Request::Restore { state: path } => path,
                Request::SetTheme { name } => name,
                Request::BrowserSearch { query } => query,
                Request::RestoreHud { ron } | Request::RestoreSettings { ron } => ron,
                Request::CanvasConsole(on)
                | Request::Redstone(on)
                | Request::HudEdit(on)
                | Request::ReplayPlay(on) => on.to_string(),
                Request::SetScene(scene) => scene.id().to_owned(),
                Request::NetConfig { latency_ms, .. } => latency_ms.to_string(),
                Request::ReplayLoad => "load".to_owned(),
                Request::ReplaySeek { frame } => frame.to_string(),
                Request::MenuOpen(which) => which.id().to_owned(),
                Request::TalkAgain => "talk".to_owned(),
                Request::SetValue { key, .. } => key,
            })
            .collect();
        assert_eq!(ids, ["a", "b", "c"]);
    }

    #[test]
    fn the_snapshot_slot_holds_the_last_publish() {
        let bus = Bus::new();
        assert_eq!(bus.snapshot(), "");
        bus.set_snapshot("(inventories:[])");
        bus.set_snapshot("(inventories:[(len:9,slots:[])])");
        assert_eq!(bus.snapshot(), "(inventories:[(len:9,slots:[])])");
    }

    #[test]
    fn the_settings_slot_is_empty_until_a_save_and_holds_the_last_one() {
        let bus = Bus::new();
        assert_eq!(bus.settings(), "");
        bus.set_settings("(values:{})");
        bus.set_settings("(values:{\"settings.ui_scale\":Float(1.25)})");
        assert_eq!(
            bus.settings(),
            "(values:{\"settings.ui_scale\":Float(1.25)})"
        );
        bus.set_settings("");
        assert_eq!(bus.settings(), "", "a reset empties it again");
    }

    #[test]
    fn draining_the_console_leaves_the_history() {
        let bus = Bus::new();
        bus.log("info", "copper_chest", "hello");
        assert_eq!(bus.drain_console().len(), 1);
        assert!(bus.drain_console().is_empty());
        assert_eq!(bus.console_history().len(), 1);
    }

    #[test]
    fn the_ring_forgets_the_oldest_line() {
        let bus = Bus::new();
        for i in 0..CONSOLE_CAPACITY + 10 {
            bus.log("info", "m", format!("line {i}"));
        }
        assert_eq!(bus.len(), CONSOLE_CAPACITY);
        assert_eq!(bus.console_history()[0].text, format!("line {}", 10));
    }

    #[test]
    fn json_escapes_what_a_script_can_put_in_a_message() {
        let line = Line {
            level: "warn".into(),
            who: "m".into(),
            text: "a \"quoted\"\nline".into(),
        };
        assert_eq!(
            line.to_json(),
            r#"{"level":"warn","who":"m","text":"a \"quoted\"\nline"}"#
        );
    }
}
