//! Plugin.

use bevy::asset::Assets;
use bevy::image::{Image, TextureAtlasLayout};
use bevy::prelude::*;
use slotted_ecs::Registries;

use crate::atlas::{AtlasIcons, bake_placeholder_atlas};
use crate::source::Icons;

/// Bakes the placeholder atlas from the frozen registries at startup and
/// inserts [`Icons`], unless an `Icons` resource is already present.
///
/// Requires `Assets<Image>` and `Assets<TextureAtlasLayout>` to exist; the
/// headless plugin group registers them (ADR 0002).
#[derive(Debug, Clone)]
pub struct SlottedIconsPlugin {
    /// Cell size in pixels.
    pub cell: u32,
}

impl Default for SlottedIconsPlugin {
    fn default() -> Self {
        Self { cell: 64 }
    }
}

impl Plugin for SlottedIconsPlugin {
    fn build(&self, app: &mut App) {
        let cell = self.cell;
        app.add_systems(
            Startup,
            move |mut commands: Commands,
                  existing: Option<Res<Icons>>,
                  registries: Option<Res<Registries>>,
                  mut images: ResMut<Assets<Image>>,
                  mut layouts: ResMut<Assets<TextureAtlasLayout>>| {
                if existing.is_some() {
                    return;
                }
                let Some(registries) = registries else {
                    tracing::warn!("no Registries resource; icons will all be Missing");
                    return;
                };
                let baked = bake_placeholder_atlas(
                    registries.items.iter().map(|(id, name, _)| (id, name)),
                    cell,
                );
                let icons = AtlasIcons {
                    image: images.add(baked.image),
                    layout: layouts.add(baked.layout),
                    index: baked.index,
                };
                commands.insert_resource(Icons::new(icons));
            },
        );
    }
}
