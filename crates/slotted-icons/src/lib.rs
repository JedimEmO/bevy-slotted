//! Item icons for slotted.
//!
//! [`IconSource`] is the port: give it a stack, get back an [`IconRef`] the
//! item renderer can draw. [`AtlasIcons`] is the shipped adapter: one texture
//! atlas, one index per item.
//!
//! An item says what it looks like in its `icon` field: a texture path, a lit
//! primitive with a colour and material parameters, or a glTF model. An item
//! that says nothing gets a cube in a colour hashed from its id, so no screen
//! ever shows a grid of missing textures.
//!
//! # Two bakes, one description
//!
//! What a cell draws is decided once, as a [`shape::CellDraw`], and drawn
//! twice.
//!
//! The CPU bake ([`bake_icon_atlas`]) rasterises flat polygons. It needs no
//! camera and no `bevy_render`, so it works in the headless harness and on
//! wasm; it is deterministic, so snapshot tests see the same atlas every run;
//! and the result is an ordinary `Image` plus `TextureAtlasLayout`.
//!
//! The GPU bake (the `gpu` module, behind the `gpu` feature) renders those
//! same cells as lit meshes under a fixed three-point rig, into a grid render
//! target that *is* the atlas. Nothing is ever read back, which is what makes
//! it work on WebGL2. The target starts life holding the CPU bake, so a screen drawn
//! before the rig's pipelines have compiled shows flat-shaded icons rather
//! than nothing at all.
//!
//! With the `gltf` feature a `model` icon loads a glTF scene, is normalised to
//! a unit cube and is lit under the same rig. The shape fields written beside
//! `model` are its stand-in: what the CPU bake draws, and the angle and size
//! the model itself is rendered at.

pub mod atlas;
pub mod plugin;

#[cfg(feature = "gpu")]
pub mod gpu;
pub mod shape;
pub mod source;

#[cfg(feature = "live")]
pub mod live;

#[cfg(feature = "live")]
pub use live::LiveIcons;

pub use atlas::{
    AtlasIcons, Baked, BakedAtlas, CELL, IconItem, PlaceholderAtlas, bake_icon_atlas,
    bake_placeholder_atlas, placeholder_color,
};
pub use plugin::{BakedByPlugin, IconBakeFingerprint, IconBakeSet, SlottedIconsPlugin};
pub use source::{IconRef, IconSource, Icons};

/// The names the item renderer and a game need.
pub mod prelude {
    pub use crate::{AtlasIcons, IconRef, IconSource, Icons, SlottedIconsPlugin};
}
