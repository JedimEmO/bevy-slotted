//! The slotted facade: one plugin group, one prelude.
//!
//! ```no_run
//! use bevy::prelude::*;
//! use slotted::prelude::*;
//!
//! App::new()
//!     .add_plugins(DefaultPlugins)
//!     .add_plugins(SlottedPlugins::default())
//!     .run();
//! ```
//!
//! [`SlottedPlugins::headless()`] is the same group without rendering, on top
//! of the Bevy plugins ADR 0002 proved sufficient for layout, picking, focus
//! and keyboard. It is what `slotted-test` builds on and what a dedicated
//! server runs.

pub mod plugins;

pub use plugins::{HeadlessBevyPlugins, HeadlessRenderAssets, SlottedPlugins};

pub use slotted_ecs as ecs;
pub use slotted_model as model;
pub use slotted_registry as registry;

#[cfg(feature = "ui")]
pub use slotted_icons as icons;
#[cfg(feature = "ui")]
pub use slotted_theme as theme;
#[cfg(feature = "ui")]
pub use slotted_ui as ui;

/// The common names.
pub mod prelude {
    pub use crate::SlottedPlugins;
    pub use slotted_ecs::prelude::*;
    #[cfg(feature = "ui")]
    pub use slotted_icons::prelude::*;
    #[cfg(feature = "ui")]
    pub use slotted_theme::prelude::*;
    #[cfg(feature = "ui")]
    pub use slotted_ui::prelude::*;
}
