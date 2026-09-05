//! Item icons for slotted.
//!
//! [`IconSource`] is the port: give it a stack, get back an [`IconRef`] the
//! item renderer can draw. [`AtlasIcons`] is the shipped adapter: one texture
//! atlas, one index per item. In Phase 2 the atlas is baked on the CPU by
//! [`bake_placeholder_atlas`]: a coloured rounded square per item, hue from
//! the item id hash. The offscreen-camera bake of real item models comes when
//! there are item models to render; the port and the adapter do not change.
//!
//! Why CPU for Phase 2: it needs no camera and no `bevy_render`, so it works
//! in the headless test harness and on wasm, it is deterministic (snapshot
//! tests see the same atlas every run), and the result is an ordinary
//! `Image` + `TextureAtlasLayout` pair, so the renderer path is exactly what
//! the real bake will feed later.

pub mod atlas;
pub mod plugin;
pub mod source;

#[cfg(feature = "live")]
pub mod live;

pub use atlas::{AtlasIcons, PlaceholderAtlas, bake_placeholder_atlas, placeholder_color};
pub use plugin::SlottedIconsPlugin;
pub use source::{IconRef, IconSource, Icons};

/// The names the item renderer and a game need.
pub mod prelude {
    pub use crate::{AtlasIcons, IconRef, IconSource, Icons, SlottedIconsPlugin};
}
