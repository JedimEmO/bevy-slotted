//! Adversarial review of the gap-closing round's icon work: `IconDef::Model`,
//! its stand-in, and the invariant that stops a blank atlas reaching a screen.
//!
//! The blank atlas is the reason this file exists. The GPU rig draws into the
//! image the UI samples, and a mesh pipeline is compiled asynchronously, so
//! there is a window between the rig appearing and the rig drawing. Anything
//! that clears the atlas inside that window and does not refill it leaves a
//! screen whose layout, counts and labels are all correct and whose icons are
//! holes. `just shot-chest` caught it once, on a cold shader cache, and a
//! screenshot is an expensive way to find out. These are the parts of the same
//! question that can be asked without a GPU.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::{Arc, Mutex};

use bevy::asset::{AssetApp, AssetPlugin};
use bevy::image::{Image, TextureAtlasLayout};
use bevy::prelude::*;
use slotted_ecs::Registries;
use slotted_icons::atlas::{IconItem, bake_icon_atlas};
use slotted_icons::source::IconRef;
use slotted_icons::{Icons, SlottedIconsPlugin};
use slotted_model::{ItemId, ItemStack, Namespaced};
use slotted_registry::defs::ItemDef;
use slotted_registry::icon::{IconColor, IconDef, ShapeIcon, ShapeKind};

// --------------------------------------------------------------- helpers

fn name(id: &str) -> Namespaced {
    Namespaced::parse(id).expect("a valid id")
}

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

fn app_with(registries: Arc<slotted_registry::FrozenRegistries>) -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_plugins(AssetPlugin::default())
        .init_asset::<Image>()
        .init_asset::<TextureAtlasLayout>()
        .insert_resource(Registries(registries))
        .add_plugins(SlottedIconsPlugin::default());
    app
}

/// How many pixels of one cell of `image` are not fully transparent.
///
/// This is the measurement the blank-atlas bug is about: an atlas that has
/// been cleared and not drawn is a correctly sized image full of zeroes, and
/// nothing but the alpha channel distinguishes it from one that never was.
fn ink_in_cell(image: &Image, rect: bevy::math::URect) -> usize {
    let data = image.data.as_ref().expect("a CPU bake keeps its data");
    let width = image.size().x as usize;
    let mut ink = 0;
    for y in rect.min.y..rect.max.y {
        for x in rect.min.x..rect.max.x {
            let offset = (y as usize * width + x as usize) * 4;
            if data.get(offset + 3).copied().unwrap_or(0) > 0 {
                ink += 1;
            }
        }
    }
    ink
}

// ------------------------------------------------- the CPU bake's cells

/// Every item with an icon gets a cell, and every cell has something in it.
///
/// This is the headless half of "the atlas is never published with zero
/// rendered cells". The GPU rig's target starts life as this image, so if this
/// is a picture then the worst a rig that never draws can do is leave a
/// flat-shaded one on screen.
#[test]
fn every_baked_cell_has_ink_in_it() {
    let names: Vec<Namespaced> = [
        "t:cube", "t:slab", "t:ingot", "t:gem", "t:rod", "t:sphere", "t:model",
    ]
    .iter()
    .map(|s| name(s))
    .collect();
    let icons: Vec<IconDef> = vec![
        IconDef::Shape(ShapeIcon::new(ShapeKind::Cube, IconColor::rgb(1, 2, 3))),
        IconDef::Shape(ShapeIcon::new(ShapeKind::Slab, IconColor::rgb(4, 5, 6))),
        IconDef::Shape(ShapeIcon::new(ShapeKind::Ingot, IconColor::rgb(7, 8, 9))),
        IconDef::Shape(ShapeIcon::new(ShapeKind::Gem, IconColor::rgb(10, 11, 12))),
        IconDef::Shape(ShapeIcon::new(ShapeKind::Rod, IconColor::rgb(13, 14, 15))),
        IconDef::Shape(ShapeIcon::new(
            ShapeKind::Sphere,
            IconColor::rgb(16, 17, 18),
        )),
        // The one that has no shape of its own to draw.
        IconDef::Model {
            path: "models/nothing_here.gltf".into(),
            stand_in: None,
        },
    ];
    let items: Vec<IconItem<'_>> = names
        .iter()
        .zip(&icons)
        .enumerate()
        .map(|(i, (name, icon))| IconItem {
            id: ItemId(u32::try_from(i).unwrap()),
            name,
            icon: Some(icon),
        })
        .collect();

    let baked = bake_icon_atlas(&items, 64);
    assert_eq!(baked.layout.len(), items.len(), "one cell an item");
    for (index, rect) in baked.layout.textures.iter().enumerate() {
        let ink = ink_in_cell(&baked.image, *rect);
        assert!(
            ink > 32,
            "cell {index} came out blank: {ink} opaque pixels in {rect:?}"
        );
    }
}

/// A `Model` with no declared stand-in is not a hole and not the missing
/// glyph: it takes a cell like everything else and the CPU bake fills it with
/// a box in a colour hashed from the path.
///
/// This is the behaviour package C chose over the older "warn and draw the
/// missing glyph", and it is the reason a headless run, a snapshot test and a
/// build with no `bevy_gltf` all show something legible. Two different paths
/// get two different colours, so the box is an identity rather than a blank.
#[test]
fn a_model_with_no_stand_in_draws_a_hashed_box_not_the_missing_glyph() {
    let mut app = app_with(frozen(vec![
        (
            "t:anvil",
            Some(IconDef::Model {
                path: "models/anvil.gltf".into(),
                stand_in: None,
            }),
        ),
        (
            "t:pick",
            Some(IconDef::Model {
                path: "models/pickaxe.gltf".into(),
                stand_in: None,
            }),
        ),
    ]));
    app.update();

    let icons = app.world().resource::<Icons>();
    for id in [0u32, 1] {
        assert!(
            matches!(
                icons.flat_icon(&ItemStack::new(ItemId(id), 1)),
                IconRef::Atlas { .. }
            ),
            "item {id} should hold a cell, not the missing glyph"
        );
    }

    // And the two cells are different pictures, so the colour really is
    // derived from the path rather than being one shared placeholder.
    let anvil = slotted_icons::shape::model_stand_in("models/anvil.gltf");
    let pick = slotted_icons::shape::model_stand_in("models/pickaxe.gltf");
    assert_ne!(
        anvil.color, pick.color,
        "two models with no stand-in are told apart by their path"
    );
    assert_eq!(
        slotted_icons::shape::model_stand_in("models/anvil.gltf").color,
        anvil.color,
        "and the same path always gets the same colour, so a rebake is stable"
    );
}

/// A declared stand-in is drawn as declared, so an author who says "this is a
/// rod" gets a rod-shaped picture rather than the box.
#[test]
fn a_declared_stand_in_is_what_the_cpu_bake_draws() {
    let declared = ShapeIcon {
        accent: Some(IconColor::rgb(0xd0, 0xd6, 0xdd)),
        ..ShapeIcon::new(ShapeKind::Rod, IconColor::rgb(0x6b, 0x4c, 0x33))
    };
    let id = name("t:pick");
    let with_stand_in = IconDef::Model {
        path: "models/pickaxe.gltf".into(),
        stand_in: Some(declared.clone()),
    };
    let plain_rod = IconDef::Shape(declared);
    let boxed = IconDef::Model {
        path: "models/pickaxe.gltf".into(),
        stand_in: None,
    };

    let bake = |icon: &IconDef| {
        let items = [IconItem {
            id: ItemId(0),
            name: &id,
            icon: Some(icon),
        }];
        bake_icon_atlas(&items, 64)
            .image
            .data
            .clone()
            .expect("data")
    };

    assert_eq!(
        bake(&with_stand_in),
        bake(&plain_rod),
        "a model's stand-in is drawn exactly as the same shape on its own would be"
    );
    assert_ne!(
        bake(&with_stand_in),
        bake(&boxed),
        "and a declared stand-in is not the hashed box"
    );
}

// ------------------------------------------------- what the rig is handed

/// The rig's render target starts as the CPU bake, pixel for pixel.
///
/// This is the invariant that keeps a screen from going blank while the
/// pipelines compile. If the target were created empty, every frame between
/// the rig appearing and its first successful draw would sample transparent
/// pixels, and on a cold shader cache that window is seconds long.
#[cfg(feature = "gpu")]
#[test]
fn the_rigs_target_starts_as_the_cpu_bake_rather_than_as_zeroes() {
    use bevy::asset::Assets;
    use bevy::pbr::StandardMaterial;
    use slotted_icons::gpu::{despawn_bake_rig, spawn_bake_rig};

    let mut world = World::new();
    world.insert_resource(Assets::<Image>::default());
    world.insert_resource(Assets::<Mesh>::default());
    world.insert_resource(Assets::<StandardMaterial>::default());

    let id = name("t:cube");
    let icon = IconDef::Shape(ShapeIcon::new(
        ShapeKind::Cube,
        IconColor::rgb(0xd0, 0xd0, 0xd0),
    ));
    let items = [IconItem {
        id: ItemId(0),
        name: &id,
        icon: Some(&icon),
    }];
    let baked = bake_icon_atlas(&items, 64);
    let cpu = baked.image.data.clone().expect("data");

    let target = spawn_bake_rig(&mut world, &baked, 64).expect("a rig for one item");
    let images = world.resource::<Assets<Image>>();
    let image = images.get(&target).expect("the target is a real asset");
    assert_eq!(
        image.data.as_ref().expect("the target carries the seed"),
        &cpu,
        "the target the UI samples holds the CPU bake from the first frame"
    );
    assert!(
        ink_in_cell(image, baked.layout.textures[0]) > 32,
        "and that seed is a picture, not an empty image of the right size"
    );

    despawn_bake_rig(&mut world);
}

// ---------------------------------------------- a glTF that never arrives

/// A model whose file will never load still holds a cell drawn from its
/// stand-in, and the bake says nothing about it.
///
/// The load itself is the rig's job and the rig needs a render world, which
/// no test in this workspace can build: libtest runs each test on a spawned
/// thread, winit refuses to create an event loop off the main thread, and
/// without `WinitPlugin` `RenderPlugin` never puts a `RenderDevice` in the
/// main world, so `bevy_pbr`'s own systems panic on the first frame.
/// `tests/gpu_bake.rs`'s ignored render test has the same problem and fails
/// the same way when it is actually run. What is reachable from here is the
/// half that matters to a player: the cell the failed load falls back to was
/// already drawn before the rig ever started, so a load that never lands
/// costs a lit picture rather than a hole.
///
/// The whole path is covered instead by `just shot-chest` plus
/// `just shot-check`, which measures the icons in the captured pixels.
#[test]
fn a_model_whose_file_will_never_load_still_holds_a_drawn_cell() {
    let stand_in = ShapeIcon::new(ShapeKind::Rod, IconColor::rgb(9, 9, 9));
    let warnings = Warnings::default();
    let app = warnings.capture(|| {
        let mut app = app_with(frozen(vec![(
            "t:ghost",
            Some(IconDef::Model {
                path: "models/this_file_does_not_exist.gltf".into(),
                stand_in: Some(stand_in.clone()),
            }),
        )]));
        app.update();
        app
    });

    assert!(
        matches!(
            app.world()
                .resource::<Icons>()
                .flat_icon(&ItemStack::new(ItemId(0), 1)),
            IconRef::Atlas { .. }
        ),
        "the item draws from a cell, not the missing glyph"
    );
    assert_eq!(
        warnings.mentioning("this_file_does_not_exist"),
        0,
        "the bake says nothing: whether the file is there is the rig's \
         question, asked several frames later, and the stand-in is the answer \
         either way"
    );

    // And that cell is the stand-in, not an empty square.
    let id = name("t:ghost");
    let items = [IconItem {
        id: ItemId(0),
        name: &id,
        icon: Some(&IconDef::Model {
            path: "models/this_file_does_not_exist.gltf".into(),
            stand_in: Some(stand_in),
        }),
    }];
    let baked = bake_icon_atlas(&items, 64);
    assert!(
        ink_in_cell(&baked.image, baked.layout.textures[0]) > 32,
        "the cell a failed load falls back to was already a picture"
    );
}

// ------------------------------------------------------ warning capture

#[derive(Clone, Default)]
struct Warnings {
    lines: Arc<Mutex<Vec<String>>>,
}

impl Warnings {
    fn capture<T>(&self, f: impl FnOnce() -> T) -> T {
        tracing::subscriber::with_default(Collector(self.lines.clone()), f)
    }

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
        // Exactly `WARN`: Bevy reports a failed asset load at `ERROR` as well,
        // and this counts what this crate said about it.
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

struct Fields<'a>(&'a mut String);

impl tracing::field::Visit for Fields<'_> {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        use std::fmt::Write as _;
        let _ = write!(self.0, " {}={value:?}", field.name());
    }

    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        use std::fmt::Write as _;
        let _ = write!(self.0, " {}={value}", field.name());
    }
}
