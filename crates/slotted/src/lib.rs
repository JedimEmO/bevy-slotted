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
//! and keyboard. It is what `slotted-test` builds on.
//!
//! [`SlottedPlugins::server()`] is smaller again and is what a dedicated
//! server runs: no widgets, and with `default-features = false, features =
//! ["server"]` no `bevy_ui`, `bevy_text`, `bevy_picking`, `bevy_window`,
//! `bevy_scene` or `bevy_render` in the dependency graph either.

pub mod plugins;

#[cfg(feature = "script-luaur")]
pub use plugins::LuaurHostPlugin;
#[cfg(feature = "net")]
pub use plugins::{ClientTransport, SlottedNetPlugin};
#[cfg(feature = "ui")]
pub use plugins::{HeadlessBevyPlugins, HeadlessRenderAssets, HeadlessStack};
pub use plugins::{ServerBevyPlugins, ServerStack, SlottedPlugins};

pub use slotted_ecs as ecs;
pub use slotted_model as model;
pub use slotted_registry as registry;

#[cfg(feature = "browser")]
pub use slotted_browser as browser;
#[cfg(feature = "ui")]
pub use slotted_icons as icons;
#[cfg(feature = "net")]
pub use slotted_net as net;
#[cfg(feature = "packs")]
pub use slotted_packs as packs;
#[cfg(feature = "packs")]
pub use slotted_script as script;
#[cfg(feature = "script-luaur")]
pub use slotted_script_luaur as script_luaur;
#[cfg(feature = "ui")]
pub use slotted_theme as theme;
#[cfg(feature = "ui")]
pub use slotted_ui as ui;

/// The common names.
pub mod prelude {
    pub use crate::SlottedPlugins;
    #[cfg(feature = "net")]
    pub use crate::{ClientTransport, SlottedNetPlugin};
    #[cfg(feature = "browser")]
    pub use slotted_browser::prelude::*;
    pub use slotted_ecs::prelude::*;
    #[cfg(feature = "ui")]
    pub use slotted_icons::prelude::*;
    #[cfg(feature = "packs")]
    pub use slotted_packs::prelude::*;
    #[cfg(feature = "ui")]
    pub use slotted_theme::prelude::*;
    #[cfg(feature = "ui")]
    pub use slotted_ui::prelude::*;
}
