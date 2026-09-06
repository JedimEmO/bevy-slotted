//! Drive a slotted screen from `cargo test`: no window, no GPU, virtual time.
//!
//! ```no_run
//! use slotted_test::prelude::*;
//!
//! let mut h = UiHarness::builder()
//!     .plugins(SlottedPlugins::headless())
//!     .resolution(1280.0, 720.0)
//!     .build();
//! h.settle();
//! let slots = h.find_all(&by::role(SemanticRole::Slot));
//! assert!(slots.is_empty());
//! ```
//!
//! The harness owns the three workarounds ADR 0002 found necessary without a
//! renderer: it fills the UI camera's `target_info`, relies on the facade's
//! `HeadlessRenderAssets`, and drives a `PointerId::Mouse` pointer because
//! `Hovered` is hard-wired to it. Nothing in `slotted-ui` knows this crate
//! exists; locators read the same semantic components `bevy_a11y` reads.

pub mod actions;
pub mod browser;
pub mod fixture;
pub mod fixtures;
pub mod harness;
#[cfg(feature = "script")]
pub mod live;
pub mod locator;
#[cfg(feature = "script")]
pub mod lua_tests;
#[cfg(feature = "script")]
pub mod mods;
pub mod queries;
pub mod replay;
pub mod tree;

pub use browser::Browser;
pub use fixture::{MenuFixture, Opened, ScreenSource};
pub use fixtures::{ChestFixture, PlayerFixture, TestRegistries};
pub use harness::{SettleTimeout, UiHarness, UiHarnessBuilder};
pub use locator::{Locator, by, describe};
#[cfg(feature = "script")]
pub use lua_tests::{LuaFixture, LuaTestReport, LuaTestResult, StepOutcome, TestDriver};
pub use replay::{ReplayError, ReplayReport};
pub use tree::{ItemSummary, ScreenTree, TreeNode};

/// `insta` RON snapshot of a [`ScreenTree`]. Stable across themes and layout
/// tweaks; changes when roles, tags, labels, visibility or items change.
///
/// ```no_run
/// # use slotted_test::prelude::*;
/// # fn f(h: &UiHarness) {
/// assert_tree_snapshot!(h.screen_tree());
/// # }
/// ```
#[cfg(feature = "snapshots")]
#[macro_export]
macro_rules! assert_tree_snapshot {
    ($tree:expr) => {
        $crate::insta::assert_ron_snapshot!($tree)
    };
    ($name:expr, $tree:expr) => {
        $crate::insta::assert_ron_snapshot!($name, $tree)
    };
}

/// `insta` snapshot of a [`ScreenTree`]'s [`Display`](std::fmt::Display)
/// form: an indented text tree, one node per line.
///
/// The same data as [`assert_tree_snapshot!`], in a shape that reads better
/// in a review diff. Both are stable across themes and layout tweaks.
///
/// ```no_run
/// # use slotted_test::prelude::*;
/// # fn f(h: &UiHarness) {
/// assert_tree_text_snapshot!(h.screen_tree());
/// # }
/// ```
#[cfg(feature = "snapshots")]
#[macro_export]
macro_rules! assert_tree_text_snapshot {
    ($tree:expr) => {
        $crate::insta::assert_snapshot!(($tree).to_string())
    };
    ($name:expr, $tree:expr) => {
        $crate::insta::assert_snapshot!($name, ($tree).to_string())
    };
}

#[cfg(feature = "snapshots")]
#[doc(hidden)]
pub use insta;

/// Everything a test needs.
pub mod prelude {
    pub use crate::{
        Browser, ChestFixture, Locator, MenuFixture, Opened, PlayerFixture, ScreenTree,
        TestRegistries, TreeNode, UiHarness, UiHarnessBuilder, by,
    };
    #[cfg(feature = "snapshots")]
    pub use crate::{assert_tree_snapshot, assert_tree_text_snapshot};
    pub use slotted::prelude::*;
    pub use slotted_ecs::Modifiers;
    pub use slotted_model::{Actor, Button, ClickAction, ItemId, ItemStack, MenuDef, SlotIx};
    pub use slotted_ui::{ScreenKind, SemanticRole};
}
