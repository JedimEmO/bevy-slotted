//! Internal test doubles and builders for the slotted workspace.
//!
//! This crate is a dev-dependency of the other members and is never published.
//! It holds the fake authorities ([`RecordingAuthority`],
//! [`RejectingAuthority`]), the inventory and registry builders every crate's
//! tests share, and a headless [`App`](bevy::prelude::App) factory. Consumer-facing UI
//! automation lives in `slotted-test` instead, which is a normal published
//! crate. See `docs/PLAN.md` sections 4.3 and 4.12.
//!
//! ```
//! use slotted_testutils::{RecordingAuthority, ecs_app_with};
//!
//! let authority = RecordingAuthority::new();
//! let mut app = ecs_app_with(authority.clone());
//! app.update();
//! assert!(authority.is_empty());
//! ```

pub mod app;
pub mod authority;
pub mod builders;
#[cfg(feature = "conformance")]
pub mod conformance;

pub use app::{ecs_app_with, minimal_ecs_app};
pub use authority::{RecordingAuthority, RejectingAuthority, Submitted};
pub use builders::{
    TestItems, TestLookup, empty_inventory, food_tag, id, inventory, spawn_inventories, stack,
    test_items, test_registries, tools_tag,
};
