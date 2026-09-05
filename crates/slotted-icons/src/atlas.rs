//! The baked-atlas adapter and the CPU placeholder bake.

use std::collections::HashMap;

use bevy::asset::{Handle, RenderAssetUsages};
use bevy::color::{Color, ColorToComponents, Hsla, Srgba};
use bevy::image::{Image, TextureAtlasLayout};
use bevy::math::{URect, UVec2};
use slotted_model::{ItemId, ItemStack, Namespaced};
use wgpu_types::{Extent3d, TextureDimension, TextureFormat};

use crate::source::{IconRef, IconSource};

/// One atlas, one cell per known item.
#[derive(Debug, Clone)]
pub struct AtlasIcons {
    /// The atlas image.
    pub image: Handle<Image>,
    /// Its layout.
    pub layout: Handle<TextureAtlasLayout>,
    /// Item to cell.
    pub index: HashMap<ItemId, usize>,
}

impl IconSource for AtlasIcons {
    fn icon(&self, stack: &ItemStack) -> IconRef {
        match self.index.get(&stack.id) {
            Some(&index) => IconRef::Atlas {
                image: self.image.clone(),
                layout: self.layout.clone(),
                index,
            },
            None => IconRef::Missing,
        }
    }
}

/// Output of [`bake_placeholder_atlas`], before it is added to `Assets`.
#[derive(Debug, Clone)]
pub struct PlaceholderAtlas {
    /// RGBA8 sRGB image, `cell * cols` by `cell * rows`.
    pub image: Image,
    /// One rect per item, in the order items were given.
    pub layout: TextureAtlasLayout,
    /// Item to cell.
    pub index: HashMap<ItemId, usize>,
}

/// A stable, distinct colour per item name: hue from an FNV-1a hash of the
/// full `namespace:path`, fixed saturation and lightness.
pub fn placeholder_color(name: &Namespaced) -> Color {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in name.as_str().bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    #[allow(clippy::cast_precision_loss)]
    let hue = (h % 360) as f32;
    Color::Hsla(Hsla::new(hue, 0.55, 0.58, 1.0))
}

/// Bakes a rounded, coloured square per item into a square-ish atlas on the
/// CPU. `cell` is the cell size in pixels (64 is the Phase 2 default).
///
/// Deterministic for a given item order, which the registry guarantees
/// (`ItemId(n)` is the `n`-th item).
pub fn bake_placeholder_atlas<'a>(
    items: impl ExactSizeIterator<Item = (ItemId, &'a Namespaced)>,
    cell: u32,
) -> PlaceholderAtlas {
    let count = u32::try_from(items.len().max(1)).expect("fewer than 2^32 items");
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let cols = f64::from(count).sqrt().ceil() as u32;
    let rows = count.div_ceil(cols);
    let width = cols * cell;
    let height = rows * cell;
    let mut data = vec![0u8; (width * height * 4) as usize];
    let mut layout = TextureAtlasLayout::new_empty(UVec2::new(width, height));
    let mut index = HashMap::with_capacity(count as usize);

    let inset = cell / 8;
    let radius = f64::from(cell) / 6.0;
    for (i, (id, name)) in items.enumerate() {
        #[allow(clippy::cast_possible_truncation)]
        let (cx, cy) = ((i as u32 % cols) * cell, (i as u32 / cols) * cell);
        let rgba = Srgba::from(placeholder_color(name)).to_f32_array();
        let px: [u8; 4] = std::array::from_fn(|k| {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let v = (rgba[k].clamp(0.0, 1.0) * 255.0).round() as u8;
            v
        });
        for y in inset..cell - inset {
            for x in inset..cell - inset {
                if inside_rounded_rect(x, y, inset, cell - inset, radius) {
                    let o = (((cy + y) * width + cx + x) * 4) as usize;
                    data[o..o + 4].copy_from_slice(&px);
                }
            }
        }
        let rect = URect::new(cx, cy, cx + cell, cy + cell);
        let cell_index = layout.add_texture(rect);
        index.insert(id, cell_index);
    }

    let image = Image::new(
        Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    PlaceholderAtlas {
        image,
        layout,
        index,
    }
}

fn inside_rounded_rect(x: u32, y: u32, min: u32, max: u32, radius: f64) -> bool {
    let (x, y, min, max) = (
        f64::from(x) + 0.5,
        f64::from(y) + 0.5,
        f64::from(min),
        f64::from(max),
    );
    let cx = x.clamp(min + radius, max - radius);
    let cy = y.clamp(min + radius, max - radius);
    (x - cx).hypot(y - cy) <= radius
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn bake_lays_items_out_in_a_grid() {
        let names: Vec<Namespaced> = ["t:a", "t:b", "t:c", "t:d", "t:e"]
            .iter()
            .map(|s| Namespaced::parse(s).expect("valid"))
            .collect();
        let items = names
            .iter()
            .enumerate()
            .map(|(i, n)| (ItemId(u32::try_from(i).expect("small")), n));
        let baked = bake_placeholder_atlas(items, 16);
        assert_eq!(baked.image.size(), UVec2::new(48, 32));
        assert_eq!(baked.layout.len(), 5);
        assert_eq!(baked.index[&ItemId(4)], 4);
        assert_eq!(baked.layout.textures[4], URect::new(16, 16, 32, 32));
        // Centre pixel is painted, corner pixel is transparent.
        let data = baked.image.data.as_ref().expect("cpu data");
        let at = |x: usize, y: usize| &data[(y * 48 + x) * 4..(y * 48 + x) * 4 + 4];
        assert_eq!(at(0, 0)[3], 0);
        assert_eq!(at(8, 8)[3], 255);
    }

    #[test]
    fn placeholder_colour_is_stable_and_distinct() {
        let a = Namespaced::parse("t:apple").expect("valid");
        let b = Namespaced::parse("t:stone").expect("valid");
        assert_eq!(placeholder_color(&a), placeholder_color(&a));
        assert_ne!(placeholder_color(&a), placeholder_color(&b));
    }
}
