//! The port.

use std::sync::Arc;

use bevy::asset::Handle;
use bevy::color::Color;
use bevy::image::{Image, TextureAtlasLayout};
use bevy::prelude::Resource;
use slotted_model::{ItemId, ItemStack};

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
    /// A whole image of its own, from an `IconDef::Image`. Becomes
    /// `ImageNode { image, texture_atlas: None }`.
    Image(Handle<Image>),
    /// No texture; draw a flat colour. Becomes `BackgroundColor`.
    Solid(Color),
    /// Draw this item live, in a viewport with its own little camera. Only
    /// `LiveIcons` returns it, and only the widgets that can host a viewport
    /// (the tooltip and the browser's detail pane) act on it.
    Live(ItemId),
    /// Unknown item. The renderer draws the theme's missing-icon glyph.
    Missing,
}

/// Resolves a stack to an icon. Adapters: [`AtlasIcons`](crate::AtlasIcons),
/// and `LiveIcons` behind the `live` feature.
pub trait IconSource: Send + Sync {
    /// The icon for `stack`. Must be cheap: called for every visible slot
    /// whose content changed.
    fn icon(&self, stack: &ItemStack) -> IconRef;

    /// The source to use where a viewport cannot go: a slot in a grid, a
    /// browser card. `None` means "this source is already that".
    fn live(&self) -> Option<&dyn IconSource> {
        None
    }
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

    /// The icon a node that cannot host a viewport should draw: the live
    /// source's fallback, or the source itself.
    pub fn flat_icon(&self, stack: &ItemStack) -> IconRef {
        self.0
            .live()
            .map_or_else(|| self.0.icon(stack), |flat| flat.icon(stack))
    }
}
