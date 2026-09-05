//! The port.

use std::sync::Arc;

use bevy::asset::Handle;
use bevy::color::Color;
use bevy::image::{Image, TextureAtlasLayout};
use bevy::prelude::Resource;
use slotted_model::ItemStack;

/// Where to draw an item from.
#[derive(Debug, Clone, PartialEq)]
pub enum IconRef {
    /// A cell of a texture atlas. Becomes `ImageNode { image, texture_atlas: Some(..) }`.
    Atlas {
        /// The atlas image.
        image: Handle<Image>,
        /// Its layout.
        layout: Handle<TextureAtlasLayout>,
        /// Cell index.
        index: usize,
    },
    /// No texture; draw a flat colour. Becomes `BackgroundColor`.
    Solid(Color),
    /// Unknown item. The renderer draws the theme's missing-icon glyph.
    Missing,
}

/// Resolves a stack to an icon. Adapters: [`AtlasIcons`](crate::AtlasIcons),
/// and `LiveIcons` behind the `live` feature.
pub trait IconSource: Send + Sync {
    /// The icon for `stack`. Must be cheap: called for every visible slot
    /// whose content changed.
    fn icon(&self, stack: &ItemStack) -> IconRef;
}

/// The active icon source as a resource.
#[derive(Resource, Clone)]
pub struct Icons(pub Arc<dyn IconSource>);

impl Icons {
    /// Wraps an adapter.
    pub fn new(source: impl IconSource + 'static) -> Self {
        Self(Arc::new(source))
    }

    /// Shorthand for `self.0.icon(stack)`.
    pub fn icon(&self, stack: &ItemStack) -> IconRef {
        self.0.icon(stack)
    }
}
