//! The baked-atlas adapter and the CPU bake.

use std::collections::{HashMap, HashSet};

use bevy::asset::{Handle, RenderAssetUsages};
use bevy::color::{Color, ColorToComponents, Hsla, Srgba};
use bevy::image::{Image, TextureAtlasLayout};
use bevy::math::{URect, UVec2};
use slotted_model::{ItemId, ItemStack, Namespaced};
use slotted_registry::icon::{IconColor, IconDef, ShapeIcon};
use wgpu_types::{Extent3d, TextureDimension, TextureFormat};

use crate::shape;
use crate::source::{IconRef, IconSource};

/// Cell size of the baked atlas, in pixels. 128 with the `hidpi` feature,
/// which is the upper end of the range section 2 of
/// `docs/research/research-modern-ui.md` calls for.
#[cfg(not(feature = "hidpi"))]
pub const CELL: u32 = 64;
/// Cell size of the baked atlas, in pixels.
#[cfg(feature = "hidpi")]
pub const CELL: u32 = 128;

/// One atlas, one cell per known item, plus the items that resolve to a
/// standalone image or to nothing at all.
#[derive(Debug, Clone, Default)]
pub struct AtlasIcons {
    /// The atlas image.
    pub image: Handle<Image>,
    /// Its layout.
    pub layout: Handle<TextureAtlasLayout>,
    /// Item to cell.
    pub index: HashMap<ItemId, usize>,
    /// Items whose `icon` was an `Image`: drawn from their own texture, not
    /// from the atlas, because the bake has nothing to render for them.
    pub images: HashMap<ItemId, Handle<Image>>,
    /// Items the bake could not draw, so the renderer shows the missing
    /// glyph. `Model` icons land here until the glTF loader exists.
    pub missing: HashSet<ItemId>,
}

impl IconSource for AtlasIcons {
    fn icon(&self, stack: &ItemStack) -> IconRef {
        if let Some(image) = self.images.get(&stack.id) {
            return IconRef::Image(image.clone());
        }
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

/// One item as the bake sees it.
#[derive(Debug, Clone, Copy)]
pub struct IconItem<'a> {
    /// Which item.
    pub id: ItemId,
    /// Its id, which is what the fallback colour hashes.
    pub name: &'a Namespaced,
    /// What it declared, if anything.
    pub icon: Option<&'a IconDef>,
}

/// What the bake decided to draw for an item.
#[derive(Debug, Clone, PartialEq)]
pub enum Baked {
    /// A shape rendered into the atlas.
    Shape(ShapeIcon),
    /// An asset path to load and draw directly.
    Image(String),
    /// Nothing: the renderer draws the missing glyph.
    Missing,
}

/// The plan for one item: which of the three the bake will do.
///
/// An item that declares nothing still gets a shape, in its hash colour, so
/// no screen ever shows a grid of missing-texture squares.
#[must_use]
pub fn plan(item: &IconItem<'_>) -> Baked {
    match item.icon {
        Some(IconDef::Shape(shape)) => Baked::Shape(shape.clone()),
        Some(IconDef::Image(path)) => Baked::Image(path.clone()),
        Some(IconDef::Model(_)) => Baked::Missing,
        None => Baked::Shape(shape::fallback_shape(hash_color(item.name))),
    }
}

/// Output of a bake, before it is added to `Assets`.
#[derive(Debug, Clone)]
pub struct BakedAtlas {
    /// RGBA8 sRGB image, `cell * cols` by `cell * rows`.
    pub image: Image,
    /// One rect per item that got a cell, in the order items were given.
    pub layout: TextureAtlasLayout,
    /// Item to cell.
    pub index: HashMap<ItemId, usize>,
    /// Items whose icon is a standalone image, with the path to load.
    pub images: Vec<(ItemId, String)>,
    /// Items the bake refused. Every one of them has been warned about once.
    pub missing: HashSet<ItemId>,
    /// The shape each cell holds, in cell order, for the GPU bake to render.
    pub shapes: Vec<ShapeIcon>,
}

/// Output of the CPU bake. The Phase 2 name, kept because it is what
/// `slotted-packs` and downstream games call.
pub type PlaceholderAtlas = BakedAtlas;

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

/// The same colour in the registry's own type, which is what a shape holds.
fn hash_color(name: &Namespaced) -> IconColor {
    let rgba = Srgba::from(placeholder_color(name)).to_f32_array();
    IconColor(rgba)
}

/// Bakes one cell per item on the CPU: the item's shape, flat-shaded from the
/// fixed rig in [`crate::shape`], with the cool rim light along its back
/// edge.
///
/// Deterministic for a given item order and a given set of defs, which is
/// what the snapshot tests rely on: two bakes of the same registry produce
/// byte-identical images.
pub fn bake_icon_atlas(items: &[IconItem<'_>], cell: u32) -> BakedAtlas {
    let plans: Vec<(ItemId, Baked)> = items.iter().map(|item| (item.id, plan(item))).collect();
    let shaped: Vec<(ItemId, ShapeIcon)> = plans
        .iter()
        .filter_map(|(id, baked)| match baked {
            Baked::Shape(shape) => Some((*id, shape.clone())),
            _ => None,
        })
        .collect();

    let count = u32::try_from(shaped.len().max(1)).expect("fewer than 2^32 items");
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let cols = f64::from(count).sqrt().ceil() as u32;
    let rows = count.div_ceil(cols);
    let width = cols * cell;
    let height = rows * cell;
    let mut data = vec![0u8; (width * height * 4) as usize];
    let mut layout = TextureAtlasLayout::new_empty(UVec2::new(width, height));
    let mut index = HashMap::with_capacity(shaped.len());
    let mut cell_shapes = Vec::with_capacity(shaped.len());

    for (i, (id, icon)) in shaped.into_iter().enumerate() {
        #[allow(clippy::cast_possible_truncation)]
        let (cx, cy) = ((i as u32 % cols) * cell, (i as u32 / cols) * cell);
        draw_cell(&mut data, width, cx, cy, cell, &icon);
        let rect = URect::new(cx, cy, cx + cell, cy + cell);
        index.insert(id, layout.add_texture(rect));
        cell_shapes.push(icon);
    }

    let images = plans
        .iter()
        .filter_map(|(id, baked)| match baked {
            Baked::Image(path) => Some((*id, path.clone())),
            _ => None,
        })
        .collect();
    let missing = plans
        .iter()
        .filter_map(|(id, baked)| (*baked == Baked::Missing).then_some(*id))
        .collect();

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
    BakedAtlas {
        image,
        layout,
        index,
        images,
        missing,
        shapes: cell_shapes,
    }
}

/// The Phase 2 entry point: items with no defs, every one of them a cube in
/// its hash colour. `slotted-packs` and games written against Phase 2 call
/// this; [`bake_icon_atlas`] is what a registry with `icon` fields wants.
pub fn bake_placeholder_atlas<'a>(
    items: impl ExactSizeIterator<Item = (ItemId, &'a Namespaced)>,
    cell: u32,
) -> BakedAtlas {
    let items: Vec<IconItem<'a>> = items
        .map(|(id, name)| IconItem {
            id,
            name,
            icon: None,
        })
        .collect();
    bake_icon_atlas(&items, cell)
}

/// Supersampling factor per axis. Four samples a pixel is enough to keep the
/// diagonals of a cube clean at 64 px and keeps the bake under a millisecond.
const SAMPLES: u32 = 2;

fn draw_cell(data: &mut [u8], width: u32, cx: u32, cy: u32, cell: u32, icon: &ShapeIcon) {
    let step = 1.0 / (f64::from(cell) * f64::from(SAMPLES));
    let n = f64::from(SAMPLES * SAMPLES);
    for y in 0..cell {
        for x in 0..cell {
            // Premultiplied sums: colour weighted by coverage, coverage on
            // its own, so a half-covered edge pixel keeps the body colour and
            // loses only alpha.
            let mut rgb = [0.0f64; 3];
            let mut coverage = 0.0f64;
            for sy in 0..SAMPLES {
                for sx in 0..SAMPLES {
                    let u = (f64::from(x * SAMPLES + sx) + 0.5) * step;
                    let v = (f64::from(y * SAMPLES + sy) + 0.5) * step;
                    #[allow(clippy::cast_possible_truncation)]
                    let sampled = shape::sample(icon, u as f32, v as f32);
                    if let Some(sampled) = sampled {
                        let alpha = f64::from(sampled[3]);
                        for (channel, value) in rgb.iter_mut().enumerate() {
                            *value += f64::from(sampled[channel]) * alpha;
                        }
                        coverage += alpha;
                    }
                }
            }
            if coverage <= 0.0 {
                continue;
            }
            let px: [u8; 4] = std::array::from_fn(|i| {
                let value = if i == 3 {
                    coverage / n
                } else {
                    rgb[i] / coverage
                };
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let byte = (value.clamp(0.0, 1.0) * 255.0).round() as u8;
                byte
            });
            let o = (((cy + y) * width + cx + x) * 4) as usize;
            data[o..o + 4].copy_from_slice(&px);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use slotted_registry::icon::{ShapeKind, UnknownShape};

    fn names(ids: &[&str]) -> Vec<Namespaced> {
        ids.iter()
            .map(|s| Namespaced::parse(s).expect("valid"))
            .collect()
    }

    fn items<'a>(names: &'a [Namespaced], icons: &'a [Option<IconDef>]) -> Vec<IconItem<'a>> {
        names
            .iter()
            .enumerate()
            .map(|(i, name)| IconItem {
                id: ItemId(u32::try_from(i).expect("small")),
                name,
                icon: icons.get(i).and_then(Option::as_ref),
            })
            .collect()
    }

    #[test]
    fn bake_lays_items_out_in_a_grid() {
        let names = names(&["t:a", "t:b", "t:c", "t:d", "t:e"]);
        let baked = bake_icon_atlas(&items(&names, &[]), 16);
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
    fn the_cpu_bake_is_deterministic() {
        let names = names(&["t:a", "t:b", "t:c"]);
        let icons = [
            Some(IconDef::Shape(ShapeIcon::new(
                ShapeKind::Gem,
                IconColor::rgb(80, 210, 230),
            ))),
            None,
            Some(IconDef::Shape(ShapeIcon {
                metallic: 0.9,
                ..ShapeIcon::new(ShapeKind::Ingot, IconColor::rgb(200, 140, 70))
            })),
        ];
        let one = bake_icon_atlas(&items(&names, &icons), 32);
        let two = bake_icon_atlas(&items(&names, &icons), 32);
        assert_eq!(one.image.data, two.image.data);
        assert_eq!(one.index, two.index);
    }

    #[test]
    fn a_shape_is_not_the_same_picture_as_another_shape() {
        let names = names(&["t:a"]);
        let cube = [Some(IconDef::Shape(ShapeIcon::new(
            ShapeKind::Cube,
            IconColor::rgb(140, 140, 140),
        )))];
        let sphere = [Some(IconDef::Shape(ShapeIcon::new(
            ShapeKind::Sphere,
            IconColor::rgb(140, 140, 140),
        )))];
        assert_ne!(
            bake_icon_atlas(&items(&names, &cube), 32).image.data,
            bake_icon_atlas(&items(&names, &sphere), 32).image.data
        );
    }

    #[test]
    fn an_image_icon_takes_no_cell_and_a_model_is_missing() {
        let names = names(&["t:img", "t:model", "t:shape"]);
        let icons = [
            Some(IconDef::Image("icons/sword.png".to_owned())),
            Some(IconDef::Model("models/anvil.gltf".to_owned())),
            None,
        ];
        let baked = bake_icon_atlas(&items(&names, &icons), 16);
        assert_eq!(baked.layout.len(), 1);
        assert_eq!(baked.index.len(), 1);
        assert!(baked.index.contains_key(&ItemId(2)));
        assert_eq!(
            baked.images,
            vec![(ItemId(0), "icons/sword.png".to_owned())]
        );
        assert_eq!(baked.missing, HashSet::from([ItemId(1)]));
    }

    #[test]
    fn every_item_gets_exactly_one_index() {
        let names = names(&["t:a", "t:b", "t:c", "t:d"]);
        let baked = bake_icon_atlas(&items(&names, &[]), 16);
        let mut cells: Vec<usize> = baked.index.values().copied().collect();
        cells.sort_unstable();
        assert_eq!(cells, vec![0, 1, 2, 3]);
        assert_eq!(baked.index.len(), names.len());
    }

    #[test]
    fn placeholder_colour_is_stable_and_distinct() {
        let a = Namespaced::parse("t:apple").expect("valid");
        let b = Namespaced::parse("t:stone").expect("valid");
        assert_eq!(placeholder_color(&a), placeholder_color(&a));
        assert_ne!(placeholder_color(&a), placeholder_color(&b));
    }

    #[test]
    fn an_unknown_shape_name_is_an_error_not_a_default() {
        assert_eq!(
            "wedge".parse::<ShapeKind>(),
            Err(UnknownShape("wedge".to_owned()))
        );
    }
}
