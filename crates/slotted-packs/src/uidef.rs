//! The `slotted_ui::def` types the pack lifecycle names.
//!
//! With the `ui` feature they are exactly `slotted-ui`'s types, re-exported so
//! that a screen this crate publishes and a screen `slotted-ui` spawns are the
//! same type. Without it, the two that the non-UI half of the lifecycle still
//! needs are declared here instead, so a dedicated server can load mods, keep
//! track of which screen kind a menu shows and read a locale catalogue with no
//! `slotted-ui` and no Bevy UI stack in the dependency graph. Both are
//! newtypes with the same shape as the originals, so the `ui` build and the
//! server build agree on what a key and a kind are.

#[cfg(feature = "ui")]
pub use slotted_ui::def::{AnchorId, LocKey, ScreenKind, UiNodeDef, WidgetKind};

/// A Fluent message id. Mirrors `slotted_ui::def::LocKey`.
#[cfg(not(feature = "ui"))]
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct LocKey(pub String);

/// Which screen a menu shows. Mirrors `slotted_ui::def::ScreenKind`.
#[cfg(not(feature = "ui"))]
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ScreenKind(pub slotted_model::Namespaced);
