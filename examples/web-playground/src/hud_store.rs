//! The playground's [`HudLayoutStorage`] adapter.
//!
//! `docs/design/showcase-contract.md` section 3.5. A browser tab has no
//! filesystem, so the file adapter `slotted-ui` ships does nothing there. What
//! a page has instead is `localStorage`, which Rust cannot reach without
//! dragging `web-sys` into the module for two calls -- and which the page is
//! already the right owner of, because it is the page that decides when to
//! save and what key to save under.
//!
//! So the adapter here is memory, and the page is the storage: the world saves
//! the layout into [`global`], the page reads the RON out with `hud_layout()`
//! and puts it in `localStorage`, and on the next visit hands it back with
//! `restore_hud_layout(ron)`. The same arrangement as the snapshot the page
//! holds across a restart, for the same reason.
//!
//! [`HudLayoutStorage`]: slotted::ui::hud_editor::HudLayoutStorage

use std::sync::OnceLock;

use slotted::ui::hud_editor::MemoryHudLayout;

/// The process-wide store the bridge and the world share.
///
/// A `static` for the same reason [`Bus::global`](crate::bus::Bus::global) is
/// one: the `wasm-bindgen` exports are free functions with no handle on the
/// `App`, and this is the one thing they and it both need to reach.
pub fn global() -> MemoryHudLayout {
    static GLOBAL: OnceLock<MemoryHudLayout> = OnceLock::new();
    GLOBAL.get_or_init(MemoryHudLayout::new).clone()
}
