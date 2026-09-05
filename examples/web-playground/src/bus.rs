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
#[derive(Debug, Clone, PartialEq, Eq)]
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
                Request::Reload { mod_id } => mod_id,
                Request::Write { path, .. } => path,
                Request::CanvasConsole(on) => on.to_string(),
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
                Request::Reload { mod_id } => mod_id,
                Request::Write { path, .. } => path,
                Request::CanvasConsole(on) => on.to_string(),
            })
            .collect();
        assert_eq!(ids, ["a", "b", "c"]);
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
