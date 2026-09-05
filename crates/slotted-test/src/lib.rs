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
pub mod fixture;
pub mod harness;
pub mod locator;
pub mod queries;
pub mod tree;

pub use fixture::{MenuFixture, Opened};
pub use harness::{SettleTimeout, UiHarness, UiHarnessBuilder};
pub use locator::{Locator, by};
pub use tree::{ItemSummary, ScreenTree, TreeNode};

/// `insta` RON snapshot of a [`ScreenTree`]. Stable across themes and layout
/// tweaks; changes when roles, tags, labels, visibility or items change.
#[macro_export]
macro_rules! assert_tree_snapshot {
    ($tree:expr) => {
        $crate::insta::assert_ron_snapshot!($tree)
    };
    ($name:expr, $tree:expr) => {
        $crate::insta::assert_ron_snapshot!($name, $tree)
    };
}

#[doc(hidden)]
pub use insta;

/// Everything a test needs.
pub mod prelude {
    pub use crate::{
        Locator, MenuFixture, Opened, ScreenTree, TreeNode, UiHarness, UiHarnessBuilder,
        assert_tree_snapshot, by,
    };
    pub use slotted::prelude::*;
    pub use slotted_ecs::Modifiers;
    pub use slotted_model::{Actor, Button, ClickAction, ItemId, ItemStack, MenuDef, SlotIx};
    pub use slotted_ui::{ScreenKind, SemanticRole};
}
