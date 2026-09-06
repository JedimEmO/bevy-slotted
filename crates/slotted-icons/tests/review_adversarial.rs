//! Phase 7 adversarial review: what the icon bake does with data no data file
//! should contain, and with registries at the ends of their range.
//!
//! Every test here is the failure a mod author, not a widget, produces: a
//! `metallic: 5.0` typed into a Lua table, an icon path that was never
//! shipped, a registry with a thousand items or with none, a reload that
//! changes one colour. None of them may panic, and none of them may leave a
//! screen with no picture at all.

#![allow(clippy::unwrap_used)]

use std::sync::{Arc, Mutex};

use bevy::app::App;
use bevy::asset::{AssetApp, AssetPlugin};
// Only the `gpu` rig test builds asset collections by hand.
#[cfg(feature = "gpu")]
use bevy::asset::Assets;
use bevy::image::{Image, TextureAtlasLayout};
use bevy::prelude::*;
use slotted_ecs::Registries;
use slotted_icons::atlas::{IconItem, bake_icon_atlas};
use slotted_icons::source::IconRef;
use slotted_icons::{Icons, SlottedIconsPlugin};
use slotted_model::{ItemId, ItemStack, Namespaced};
use slotted_registry::defs::ItemDef;
use slotted_registry::icon::{IconColor, IconDef, ShapeIcon, ShapeKind};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn name(id: &str) -> Namespaced {
    Namespaced::parse(id).expect("a valid id")
}

/// One CPU bake of a single item with `icon`, as raw pixels.
fn pixels_of(icon: &IconDef, cell: u32) -> Vec<u8> {
    let id = name("t:only");
    let items = [IconItem {
        id: ItemId(0),
        name: &id,
        icon: Some(icon),
    }];
    let baked = bake_icon_atlas(&items, cell);
    baked
        .image
        .data
        .clone()
        .expect("the CPU bake keeps its data")
}

/// Frozen registries holding `items`, in the order given.
fn frozen(items: Vec<(&str, Option<IconDef>)>) -> Arc<slotted_registry::FrozenRegistries> {
    let mut registries = slotted_registry::Registries::default();
    for (id, icon) in items {
        let mut def = ItemDef::new(name(id));
        def.icon = icon;
        registries
            .items
            .insert(name(id), def)
            .expect("no duplicate");
    }
    let (frozen, _) = registries.freeze().expect("a freeze with no cycles");
    Arc::new(frozen)
}

/// An app with the icons plugin, asset collections, and optionally an
/// `AssetServer`. No renderer either way, which is what a headless game and
/// the test harness look like.
fn app_with(registries: Arc<slotted_registry::FrozenRegistries>, assets: bool) -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    if assets {
        app.add_plugins(AssetPlugin::default());
    }
    app.init_asset::<Image>()
        .init_asset::<TextureAtlasLayout>()
        .insert_resource(Registries(registries))
        .add_plugins(SlottedIconsPlugin::default());
    app
}

// ---------------------------------------------------------------------------
// Out-of-range material parameters
// ---------------------------------------------------------------------------

/// `metallic` and `roughness` are plain floats in a data file, so a mod can
/// write `metallic: 5.0`. Both ends clamp, and the picture is the one the
/// legal value at that end draws.
#[test]
fn metallic_and_roughness_out_of_range_draw_the_clamped_picture() {
    let shape = |metallic: f32, roughness: f32| {
        IconDef::Shape(ShapeIcon {
            metallic,
            roughness,
            ..ShapeIcon::new(ShapeKind::Cube, IconColor::rgb(0xc9, 0x79, 0x3f))
        })
    };
    assert_eq!(
        pixels_of(&shape(5.0, 0.5), 32),
        pixels_of(&shape(1.0, 0.5), 32),
        "metallic above 1 is bare metal, not brighter than metal"
    );
    assert_eq!(
        pixels_of(&shape(-4.0, 0.5), 32),
        pixels_of(&shape(0.0, 0.5), 32),
        "metallic below 0 is a dielectric"
    );
    assert_eq!(
        pixels_of(&shape(0.2, 9.0), 32),
        pixels_of(&shape(0.2, 1.0), 32),
        "roughness above 1 is chalk"
    );
    assert_eq!(
        pixels_of(&shape(0.2, -3.0), 32),
        pixels_of(&shape(0.2, 0.0), 32),
        "roughness below 0 is a mirror, and never brightens a face past its own colour"
    );
}

/// The clamp is not the same as "no effect": a metal really does read
/// differently from a dielectric, so the test above is not passing because
/// the parameter is ignored.
#[test]
fn metallic_still_changes_the_picture_inside_its_range() {
    let shape = |metallic: f32| {
        IconDef::Shape(ShapeIcon {
            metallic,
            ..ShapeIcon::new(ShapeKind::Cube, IconColor::rgb(0xc9, 0x79, 0x3f))
        })
    };
    assert_ne!(pixels_of(&shape(0.0), 32), pixels_of(&shape(1.0), 32));
}

/// No channel may leave `0..=255` on the way out, whatever the data said.
/// This is the property the clamp exists for: an unclamped `roughness: -3`
/// used to add a quarter of a unit of brightness to every lit face.
#[test]
fn no_out_of_range_parameter_can_blow_out_a_channel() {
    for (metallic, roughness) in [(-9.0, -9.0), (9.0, 9.0), (5.0, -5.0), (f32::MAX, 0.0)] {
        for kind in ShapeKind::ALL {
            let icon = IconDef::Shape(ShapeIcon {
                metallic,
                roughness,
                accent: Some(IconColor::rgb(0xff, 0xff, 0xff)),
                ..ShapeIcon::new(kind, IconColor::rgb(0xf0, 0xd0, 0x40))
            });
            // A `Vec<u8>` cannot hold an out-of-range channel; what this
            // proves is that the conversion did not panic or saturate every
            // pixel to white, which is what an unclamped shade does.
            let data = pixels_of(&icon, 16);
            assert!(
                data.chunks_exact(4).any(|px| px[3] > 0),
                "{kind:?} at metallic {metallic} roughness {roughness} drew nothing"
            );
            assert!(
                data.chunks_exact(4)
                    .filter(|px| px[3] > 0)
                    .any(|px| px[0] < 255 || px[1] < 255 || px[2] < 255),
                "{kind:?} at metallic {metallic} roughness {roughness} is a white square"
            );
        }
    }
}

/// The GPU rig fills a `StandardMaterial` from the same numbers, and Bevy's
/// PBR shader has its own ideas about a `metallic` of 5. Clamped there too.
#[cfg(feature = "gpu")]
#[test]
fn the_gpu_rig_clamps_the_same_numbers() {
    use bevy::pbr::StandardMaterial;
    use slotted_icons::gpu::spawn_bake_rig;

    let id = name("t:hot");
    let icon = IconDef::Shape(ShapeIcon {
        metallic: 5.0,
        roughness: -3.0,
        ..ShapeIcon::new(ShapeKind::Ingot, IconColor::rgb(0xc9, 0x79, 0x3f))
    });
    let items = [IconItem {
        id: ItemId(0),
        name: &id,
        icon: Some(&icon),
    }];
    let baked = bake_icon_atlas(&items, 32);

    let mut world = World::new();
    world.insert_resource(Assets::<Image>::default());
    world.insert_resource(Assets::<Mesh>::default());
    world.insert_resource(Assets::<StandardMaterial>::default());
    spawn_bake_rig(&mut world, &baked, 32).expect("a rig for one shape");

    let materials = world.resource::<Assets<StandardMaterial>>();
    assert!(!materials.is_empty(), "the rig made a material");
    for (_, material) in materials.iter() {
        assert!(
            (0.0..=1.0).contains(&material.metallic),
            "metallic {} left the range",
            material.metallic
        );
        assert!(
            (0.05..=1.0).contains(&material.perceptual_roughness),
            "roughness {} left the range",
            material.perceptual_roughness
        );
    }
}

// ---------------------------------------------------------------------------
// An icon path that is not there
// ---------------------------------------------------------------------------

/// An `Image` icon naming a file nobody shipped. The bake hands the path to
/// the asset server, because whether a file is there is not known
/// synchronously; when that load fails the item is moved to the missing set,
/// with one warning naming the path. The renderer then draws the theme's
/// glyph rather than a white square where a texture should be.
#[test]
fn a_missing_image_path_resolves_to_missing_and_warns_once() {
    let registries = frozen(vec![
        (
            "t:real",
            Some(IconDef::Shape(ShapeIcon::new(
                ShapeKind::Cube,
                IconColor::rgb(0x80, 0x80, 0x80),
            ))),
        ),
        (
            "t:ghost",
            Some(IconDef::Image("icons/not-shipped.png".into())),
        ),
    ]);
    let warnings = WarnCounter::default();
    let mut app = warnings.capture(|| {
        let mut app = app_with(registries, true);
        // The load is asynchronous; a handful of frames is enough for the
        // server to report that the file is not there.
        for _ in 0..120 {
            app.update();
            if app
                .world()
                .get_resource::<slotted_icons::plugin::PendingIconImages>()
                .is_none()
            {
                break;
            }
        }
        app
    });

    let icons = app.world().resource::<Icons>().clone();
    assert_eq!(
        icons.icon(&ItemStack::new(ItemId(1), 1)),
        IconRef::Missing,
        "an image that cannot be loaded is the missing glyph, not a broken handle"
    );
    assert!(
        matches!(
            icons.icon(&ItemStack::new(ItemId(0), 1)),
            IconRef::Atlas { .. } | IconRef::Live(_)
        ),
        "the shaped item beside it still has its picture"
    );
    assert_eq!(
        warnings.mentioning("not-shipped.png"),
        1,
        "one warning naming the path, not one a frame"
    );

    // And it stays put: the watch is over, so later frames neither warn
    // again nor rebuild the source.
    let after = WarnCounter::default();
    after.capture(|| {
        for _ in 0..5 {
            app.update();
        }
    });
    assert_eq!(after.mentioning("not-shipped.png"), 0);
    assert_eq!(
        app.world()
            .resource::<Icons>()
            .icon(&ItemStack::new(ItemId(1), 1)),
        IconRef::Missing
    );
}

/// A `Model` icon is not the same story: it takes a cell like any other icon
/// and the CPU bake fills that cell with its stand-in, so a headless app
/// shows a shape rather than the missing glyph and says nothing about it.
///
/// The glTF is loaded by the GPU rig, which needs a renderer this app does
/// not have; the point of the stand-in is that its absence is not an error.
#[test]
fn a_model_icon_takes_a_cell_and_draws_its_stand_in() {
    let registries = frozen(vec![(
        "t:anvil",
        Some(IconDef::Model {
            path: "models/anvil.gltf".into(),
            stand_in: None,
        }),
    )]);
    let warnings = WarnCounter::default();
    let app = warnings.capture(|| {
        let mut app = app_with(registries, true);
        app.update();
        app
    });
    // `flat_icon`, not `icon`: with the `live` feature the active source is
    // `LiveIcons`, which answers `Live` for every item the atlas knows and
    // defers to the atlas underneath it. The question here is whether the
    // *cell* exists, and that is a question for the atlas either way.
    assert!(
        matches!(
            app.world()
                .resource::<Icons>()
                .flat_icon(&ItemStack::new(ItemId(0), 1)),
            IconRef::Atlas { .. }
        ),
        "a model should get an atlas cell, not the missing glyph"
    );
    assert_eq!(
        warnings.mentioning("anvil.gltf"),
        0,
        "a model with no renderer to draw it is the stand-in doing its job, not a problem"
    );
}

// ---------------------------------------------------------------------------
// Registry sizes at both ends
// ---------------------------------------------------------------------------

/// A thousand items. The grid is the smallest square that holds them, every
/// item has its own cell, and no cell overlaps another.
#[test]
fn the_atlas_holds_a_thousand_items_in_a_square_grid() {
    let names: Vec<Namespaced> = (0..1000).map(|i| name(&format!("t:i{i}"))).collect();
    let items: Vec<IconItem<'_>> = names
        .iter()
        .enumerate()
        .map(|(i, name)| IconItem {
            id: ItemId(u32::try_from(i).unwrap()),
            name,
            icon: None,
        })
        .collect();
    let cell = 8;
    let baked = bake_icon_atlas(&items, cell);

    // 32x32 is the smallest square grid that holds 1000.
    assert_eq!(baked.image.size(), UVec2::new(32 * cell, 32 * cell));
    assert_eq!(baked.layout.len(), 1000, "one cell an item");
    assert_eq!(baked.index.len(), 1000);

    let mut seen = std::collections::HashSet::new();
    for rect in &baked.layout.textures {
        assert!(
            seen.insert((rect.min.x, rect.min.y)),
            "two items share {rect:?}"
        );
        assert!(
            rect.max.x <= 32 * cell && rect.max.y <= 32 * cell,
            "{rect:?} is off the atlas"
        );
    }
    let cells: std::collections::HashSet<usize> = baked.index.values().copied().collect();
    assert_eq!(cells.len(), 1000, "no two items index the same cell");
}

/// No items at all: an empty registry still produces an atlas, because the
/// image is a texture handle a widget already holds. It is one empty cell,
/// not a zero-sized texture a GPU refuses.
#[test]
fn an_empty_registry_bakes_an_empty_atlas_rather_than_a_zero_sized_image() {
    let baked = bake_icon_atlas(&[], 16);
    assert_eq!(baked.image.size(), UVec2::new(16, 16));
    assert_eq!(baked.layout.len(), 0);
    assert!(baked.index.is_empty());
    assert!(baked.missing.is_empty());

    let app = {
        let mut app = app_with(frozen(Vec::new()), true);
        app.update();
        app
    };
    assert_eq!(
        app.world()
            .resource::<Icons>()
            .icon(&ItemStack::new(ItemId(0), 1)),
        IconRef::Missing,
        "asking an empty atlas for an item is a missing glyph, not a panic"
    );
}

// ---------------------------------------------------------------------------
// Reload
// ---------------------------------------------------------------------------

/// A reload that changes one item's colour rebakes exactly once: the
/// fingerprint moved, so the bake runs, and it does not run again on the
/// frames after. A reload that changes nothing does not rebake at all.
#[test]
fn a_changed_icon_rebakes_exactly_once() {
    use slotted_icons::plugin::IconBakeFingerprint;

    let orange = |shade: u8| {
        Some(IconDef::Shape(ShapeIcon::new(
            ShapeKind::Ingot,
            IconColor::rgb(shade, 0x79, 0x3f),
        )))
    };
    let mut app = app_with(frozen(vec![("t:ingot", orange(0xc9))]), true);
    app.update();
    let first = *app.world().resource::<IconBakeFingerprint>();
    let first_image = image_handle(&app);

    // Frames with nothing changed: no rebake.
    for _ in 0..3 {
        app.update();
    }
    assert_eq!(*app.world().resource::<IconBakeFingerprint>(), first);
    assert_eq!(
        image_handle(&app),
        first_image,
        "the atlas was not replaced"
    );

    // The reload: `slotted-packs` installs a new `Registries` with the same
    // item under a new colour.
    app.world_mut()
        .insert_resource(Registries(frozen(vec![("t:ingot", orange(0x40))])));
    app.update();
    let second = *app.world().resource::<IconBakeFingerprint>();
    assert_ne!(second, first, "a changed icon is a changed fingerprint");
    assert_ne!(image_handle(&app), first_image, "and a fresh atlas");

    // And exactly once: the frames after the reload leave it alone.
    for _ in 0..3 {
        app.update();
    }
    assert_eq!(*app.world().resource::<IconBakeFingerprint>(), second);

    // A reload that reinstalls the same data is a no-op for the atlas even
    // though `Registries` changed, because the fingerprint is over the data.
    let handle = image_handle(&app);
    app.world_mut()
        .insert_resource(Registries(frozen(vec![("t:ingot", orange(0x40))])));
    app.update();
    assert_eq!(*app.world().resource::<IconBakeFingerprint>(), second);
    assert_eq!(image_handle(&app), handle, "same data, same atlas");
}

fn image_handle(app: &App) -> Handle<Image> {
    let icons = app.world().resource::<Icons>();
    let flat = icons.flat_icon(&ItemStack::new(ItemId(0), 1));
    match flat {
        IconRef::Atlas { image, .. } => image,
        other => panic!("expected the atlas, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// LiveIcons and cameras
// ---------------------------------------------------------------------------

/// The `live` feature is compiled in, and a headless app still spawns no
/// camera: `LiveIcons` only ever answers a question, and the bake falls back
/// to the CPU picture when there is no renderer to draw one. ADR 0003's
/// budget is one camera a tooltip; zero is what a test run costs.
///
/// The other half, one camera per live viewport node, is
/// `slotted-ui/tests/live_viewport.rs`, which is where the node lives.
#[cfg(feature = "live")]
#[test]
fn live_icons_spawn_no_camera_in_a_headless_app() {
    let registries = frozen(vec![(
        "t:stone",
        Some(IconDef::Shape(ShapeIcon::new(
            ShapeKind::Cube,
            IconColor::rgb(0x8a, 0x8f, 0x96),
        ))),
    )]);
    let mut app = app_with(registries, true);
    for _ in 0..4 {
        app.update();
    }
    let cameras = app
        .world_mut()
        .query_filtered::<Entity, With<Camera>>()
        .iter(app.world())
        .count();
    assert_eq!(cameras, 0, "a headless bake spawns no camera at all");

    let icons = app.world().resource::<Icons>();
    assert_eq!(
        icons.icon(&ItemStack::new(ItemId(0), 1)),
        IconRef::Live(ItemId(0)),
        "the item is one a viewport could show"
    );
    assert!(
        matches!(
            icons.flat_icon(&ItemStack::new(ItemId(0), 1)),
            IconRef::Atlas { .. }
        ),
        "and a slot in a grid still gets a flat cell"
    );
}

/// With a renderer, the bake rig is one camera for the whole atlas, not one
/// a cell. Needs a GPU and a display, so it is ignored by default:
/// `cargo test -p slotted-icons --all-features -- --ignored`.
#[cfg(all(feature = "gpu", feature = "live"))]
#[test]
#[ignore = "needs a GPU"]
fn a_renderer_gets_one_bake_camera_for_the_whole_atlas() {
    use bevy::render::renderer::RenderDevice;

    let registries = frozen(
        (0..12)
            .map(|i| {
                (
                    Box::leak(format!("t:i{i}").into_boxed_str()) as &str,
                    Some(IconDef::Shape(ShapeIcon::new(
                        ShapeKind::Gem,
                        IconColor::rgb(0x50, 0xd2, 0xe6),
                    ))),
                )
            })
            .collect(),
    );
    let mut app = App::new();
    app.add_plugins(bevy::DefaultPlugins);
    app.update();
    assert!(
        app.world().get_resource::<RenderDevice>().is_some(),
        "no render device: this test needs a GPU"
    );
    app.insert_resource(Registries(registries))
        .add_plugins(SlottedIconsPlugin::default());
    app.update();

    let rig_cameras = app
        .world_mut()
        .query_filtered::<Entity, With<Camera3d>>()
        .iter(app.world())
        .count();
    assert_eq!(rig_cameras, 1, "one orthographic camera renders every cell");
}

// ---------------------------------------------------------------------------
// A warning counter
// ---------------------------------------------------------------------------

/// Counts `WARN` events while it is installed, keeping their text.
///
/// Hand-rolled rather than pulled from `tracing-subscriber`: the crate needs
/// one number out of the log, and a dev-dependency on a subscriber stack to
/// get it would be the larger thing.
#[derive(Default, Clone)]
struct WarnCounter {
    lines: Arc<Mutex<Vec<String>>>,
}

impl WarnCounter {
    /// Runs `f` with this counter installed as the global default subscriber
    /// for the current thread.
    fn capture<T>(&self, f: impl FnOnce() -> T) -> T {
        tracing::subscriber::with_default(Collector(self.lines.clone()), f)
    }

    /// How many warnings mention `needle`.
    fn mentioning(&self, needle: &str) -> usize {
        self.lines
            .lock()
            .expect("no panic while holding the lock")
            .iter()
            .filter(|line| line.contains(needle))
            .count()
    }
}

struct Collector(Arc<Mutex<Vec<String>>>);

impl tracing::Subscriber for Collector {
    fn enabled(&self, metadata: &tracing::Metadata<'_>) -> bool {
        *metadata.level() <= tracing::Level::WARN
    }

    fn new_span(&self, _: &tracing::span::Attributes<'_>) -> tracing::span::Id {
        tracing::span::Id::from_u64(1)
    }

    fn record(&self, _: &tracing::span::Id, _: &tracing::span::Record<'_>) {}

    fn record_follows_from(&self, _: &tracing::span::Id, _: &tracing::span::Id) {}

    fn event(&self, event: &tracing::Event<'_>) {
        // Exactly `WARN`. Bevy reports a failed asset load at `ERROR` as
        // well, and this counts what this crate said, not what the asset
        // server said about the same file.
        if *event.metadata().level() != tracing::Level::WARN {
            return;
        }
        let mut text = String::new();
        event.record(&mut Fields(&mut text));
        self.0
            .lock()
            .expect("no panic while holding the lock")
            .push(text);
    }

    fn enter(&self, _: &tracing::span::Id) {}

    fn exit(&self, _: &tracing::span::Id) {}
}

/// Flattens every field of an event into one string, so a warning can be
/// matched on whatever it named the offending value.
struct Fields<'a>(&'a mut String);

impl tracing::field::Visit for Fields<'_> {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        use std::fmt::Write;
        let _ = write!(self.0, " {}={value:?}", field.name());
    }

    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        use std::fmt::Write;
        let _ = write!(self.0, " {}={value}", field.name());
    }
}
