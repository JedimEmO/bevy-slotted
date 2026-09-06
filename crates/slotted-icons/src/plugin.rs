//! Plugin: bake the atlas from the frozen registries, and bake it again when
//! they change.

use std::collections::HashMap;

use bevy::asset::{AssetServer, Assets};
use bevy::image::{Image, TextureAtlasLayout};
use bevy::prelude::*;
use slotted_ecs::Registries;
use slotted_registry::icon::IconDef;

use crate::atlas::{AtlasIcons, CELL, IconItem, bake_icon_atlas};
use crate::source::Icons;

/// Bakes the icon atlas from the frozen registries and inserts [`Icons`],
/// unless an `Icons` resource is already present.
///
/// The bake runs whenever `Registries` changes, so a mod reload that adds an
/// item gets an icon for it. A theme change does not touch the registries and
/// therefore does not rebake: the atlas is item data, not theme data.
///
/// With the `gpu` feature and a renderer in the app, the atlas image is a
/// render target an offscreen three-point rig draws into
/// (the `gpu` module). Without either, the CPU bake in [`crate::atlas`] is what
/// the renderer gets, and it is what the headless harness always sees.
#[derive(Debug, Clone)]
pub struct SlottedIconsPlugin {
    /// Cell size in pixels. [`CELL`] by default: 64, or 128 with `hidpi`.
    pub cell: u32,
}

impl Default for SlottedIconsPlugin {
    fn default() -> Self {
        Self { cell: CELL }
    }
}

/// Marks the [`Icons`] resource as one this plugin baked, so a rebake may
/// replace it and a game-supplied source is left alone.
#[derive(Resource, Debug, Clone, Copy)]
pub struct BakedByPlugin;

/// The fingerprint of the registry the current atlas was baked from. A bake
/// is skipped when it would produce the same picture.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub struct IconBakeFingerprint(pub u64);

impl Plugin for SlottedIconsPlugin {
    fn build(&self, app: &mut App) {
        let cell = self.cell.max(8);
        app.add_systems(
            Update,
            (move |world: &mut World| bake_if_needed(world, cell))
                .run_if(should_bake)
                .in_set(IconBakeSet),
        )
        .add_systems(
            Update,
            downgrade_failed_image_icons
                .after(IconBakeSet)
                .run_if(resource_exists::<PendingIconImages>),
        );
        #[cfg(feature = "gpu")]
        // Gated on the rig existing, because it is an exclusive system: the
        // fit walks an arbitrary scene hierarchy and both halves want the rig
        // resource mutably. Without the condition every frame of every app
        // would pay for a sync point that almost always returns immediately.
        app.add_systems(
            Update,
            crate::gpu::quiet_bake_rig
                .after(IconBakeSet)
                .run_if(resource_exists::<crate::gpu::IconBakeRig>),
        );
    }

    #[cfg(feature = "gpu")]
    fn finish(&self, app: &mut App) {
        // The render sub-app exists only once `RenderPlugin` has been built,
        // which is why the bake's readiness reporter is installed here rather
        // than in `build`.
        crate::gpu::install_progress_reporter(app);
    }
}

/// The set the bake runs in, so a game can order against it.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct IconBakeSet;

/// Cheap gate: the registries exist, and either nothing has baked yet or the
/// registries changed since.
fn should_bake(
    registries: Option<Res<Registries>>,
    existing: Option<Res<Icons>>,
    ours: Option<Res<BakedByPlugin>>,
    fingerprint: Option<Res<IconBakeFingerprint>>,
) -> bool {
    let Some(registries) = registries else {
        return false;
    };
    if existing.is_some() && ours.is_none() {
        // A game or `slotted-packs` supplied its own source; leave it alone.
        return false;
    }
    fingerprint.is_none() || registries.is_changed()
}

fn bake_if_needed(world: &mut World, cell: u32) {
    if !world.contains_resource::<Assets<Image>>()
        || !world.contains_resource::<Assets<TextureAtlasLayout>>()
    {
        tracing::warn!("no image assets; icons will all be Missing");
        return;
    }
    let Some(registries) = world.get_resource::<Registries>() else {
        return;
    };
    let items: Vec<(
        slotted_model::ItemId,
        slotted_model::Namespaced,
        Option<IconDef>,
    )> = registries
        .items
        .iter()
        .map(|(id, name, def)| (id, name.clone(), def.icon.clone()))
        .collect();
    let fingerprint = fingerprint_of(&items);
    if world.get_resource::<IconBakeFingerprint>() == Some(&IconBakeFingerprint(fingerprint)) {
        return;
    }

    let borrowed: Vec<IconItem<'_>> = items
        .iter()
        .map(|(id, name, icon)| IconItem {
            id: *id,
            name,
            icon: icon.as_ref(),
        })
        .collect();
    let baked = bake_icon_atlas(&borrowed, cell);

    let images: HashMap<slotted_model::ItemId, Handle<Image>> =
        if let Some(server) = world.get_resource::<AssetServer>() {
            baked
                .images
                .iter()
                .map(|(id, path)| (*id, server.load::<Image>(path.clone())))
                .collect()
        } else {
            // No asset server, so nothing can load the file. The items keep no
            // cell in the atlas either, so they resolve to `Missing` and draw the
            // theme's glyph. One warning, not one per frame: the bake only runs
            // when the fingerprint changed.
            for (_, path) in &baked.images {
                tracing::warn!(icon = %path, "no AssetServer; image icons draw the missing glyph");
            }
            HashMap::new()
        };

    let cpu_image = world
        .resource_mut::<Assets<Image>>()
        .add(baked.image.clone());
    #[cfg(feature = "gpu")]
    let image = if world.contains_resource::<Assets<bevy::pbr::StandardMaterial>>() {
        crate::gpu::spawn_bake_rig(world, &baked, cell).unwrap_or(cpu_image)
    } else {
        cpu_image
    };
    #[cfg(not(feature = "gpu"))]
    let image = cpu_image;

    let layout = world
        .resource_mut::<Assets<TextureAtlasLayout>>()
        .add(baked.layout);
    let atlas = AtlasIcons {
        image,
        layout,
        index: baked.index,
        images,
        missing: baked.missing,
    };
    // An `Image` icon's path is not checked here: an asset load is
    // asynchronous, so whether the file is there is not known for several
    // frames. `downgrade_failed_image_icons` answers that question later.
    world.insert_resource(PendingIconImages {
        paths: baked.images.clone(),
        atlas: atlas.clone(),
    });
    // With the `live` feature the atlas becomes the fallback rather than the
    // source: a tooltip and a detail pane get a turning 3D item, a slot in a
    // grid still gets its cell. See `crate::live`.
    #[cfg(feature = "live")]
    world.insert_resource(Icons::new(crate::live::LiveIcons::new(atlas)));
    #[cfg(not(feature = "live"))]
    world.insert_resource(Icons::new(atlas));
    world.insert_resource(BakedByPlugin);
    world.insert_resource(IconBakeFingerprint(fingerprint));
}

/// The `Image` icons the last bake handed to the asset server, and the atlas
/// they belong to, so a load that fails can be turned into a
/// [`IconRef::Missing`](crate::IconRef) without rebaking.
#[derive(Resource, Debug, Clone)]
pub struct PendingIconImages {
    /// Item and the path it named.
    paths: Vec<(slotted_model::ItemId, String)>,
    /// The atlas as the bake left it.
    atlas: AtlasIcons,
}

/// Turns an `Image` icon whose file is not there into the missing glyph.
///
/// An asset load is asynchronous, so the bake cannot know whether a path
/// exists; it hands the server every path and moves on. This watches those
/// loads. A failure warns once, naming the item and the path, and rebuilds
/// [`Icons`] with that item moved out of `images` and into `missing`, which
/// is what makes a slot draw the theme's glyph rather than a white square
/// where a texture should be. A load that succeeds is simply dropped from
/// the watch list, and the resource is removed once nothing is outstanding.
pub fn downgrade_failed_image_icons(world: &mut World) {
    let Some(pending) = world.get_resource::<PendingIconImages>().cloned() else {
        return;
    };
    let Some(server) = world.get_resource::<AssetServer>() else {
        return;
    };
    if !world.contains_resource::<BakedByPlugin>() {
        // A game replaced `Icons` between the bake and now. Its source is not
        // ours to rewrite, and the watch is stale.
        world.remove_resource::<PendingIconImages>();
        return;
    }
    let mut still_loading: Vec<(slotted_model::ItemId, String)> = Vec::new();
    let mut failed: Vec<(slotted_model::ItemId, String)> = Vec::new();
    for (id, path) in pending.paths {
        let Some(handle) = pending.atlas.images.get(&id) else {
            continue;
        };
        match server.load_state(handle.id()) {
            bevy::asset::LoadState::Failed(_) => failed.push((id, path)),
            bevy::asset::LoadState::Loaded => {}
            _ => still_loading.push((id, path)),
        }
    }
    if failed.is_empty() {
        if still_loading.is_empty() {
            world.remove_resource::<PendingIconImages>();
        } else {
            world.resource_mut::<PendingIconImages>().paths = still_loading;
        }
        return;
    }

    let mut atlas = pending.atlas;
    for (id, path) in &failed {
        tracing::warn!(item = ?id, icon = %path, "icon image failed to load; this item draws the missing glyph");
        atlas.images.remove(id);
        atlas.missing.insert(*id);
    }
    #[cfg(feature = "live")]
    world.insert_resource(Icons::new(crate::live::LiveIcons::new(atlas.clone())));
    #[cfg(not(feature = "live"))]
    world.insert_resource(Icons::new(atlas.clone()));
    if still_loading.is_empty() {
        world.remove_resource::<PendingIconImages>();
    } else {
        world.insert_resource(PendingIconImages {
            paths: still_loading,
            atlas,
        });
    }
}

/// FNV-1a over the item ids and their icon defs. Two registries with the same
/// fingerprint produce the same atlas, which is what lets a reload that
/// changed no item skip the bake.
fn fingerprint_of(
    items: &[(
        slotted_model::ItemId,
        slotted_model::Namespaced,
        Option<IconDef>,
    )],
) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    let mut eat = |bytes: &[u8]| {
        for byte in bytes {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x0100_0000_01b3);
        }
    };
    for (id, name, icon) in items {
        eat(&id.0.to_le_bytes());
        eat(name.as_str().as_bytes());
        // The def's `Debug` is total and stable; it is a hash input, not a
        // format anyone reads.
        eat(format!("{icon:?}").as_bytes());
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;
    use slotted_model::{ItemId, Namespaced};
    use slotted_registry::icon::{IconColor, ShapeIcon, ShapeKind};

    fn item(id: u32, name: &str, icon: Option<IconDef>) -> (ItemId, Namespaced, Option<IconDef>) {
        (ItemId(id), Namespaced::parse(name).expect("valid"), icon)
    }

    #[test]
    fn the_fingerprint_follows_the_icons_not_only_the_names() {
        let plain = vec![item(0, "t:a", None)];
        let shaped = vec![item(
            0,
            "t:a",
            Some(IconDef::Shape(ShapeIcon::new(
                ShapeKind::Gem,
                IconColor::rgb(1, 2, 3),
            ))),
        )];
        assert_eq!(fingerprint_of(&plain), fingerprint_of(&plain.clone()));
        assert_ne!(fingerprint_of(&plain), fingerprint_of(&shaped));
    }
}
