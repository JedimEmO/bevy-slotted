//! What a restart costs, natively.
//!
//! On `wasm32` a mod that raises takes the whole module down (ADR 0004), so
//! the page catches the trap, re-instantiates the module and hands back the
//! last snapshot it took. The browser half of that is
//! `web/playground.js` and `smoke.html`; this is the half a headless test can
//! reach, and it is the half that decides whether the chest comes back.
//!
//! The route is `publish_snapshot` -> the bus -> `Request::Restore` ->
//! `RestoreState` -> `apply_restore` -> the live inventories, over a real
//! `slotted` world with the modded example's mods loaded and its chest open.
#![allow(clippy::unwrap_used)]

use bevy::prelude::*;
use slotted_model::{ItemStack, Namespaced};
use slotted_test::prelude::*;
use web_playground::bus::{Bus, Request};

const CHEST: &str = "copper_chest:chest";

/// A live world with the modded example's mods loaded, its chest open, and
/// the playground's own snapshot systems scheduled the way `build_app`
/// schedules them.
fn playground_world() -> (UiHarness, Bus) {
    let mut harness = UiHarness::builder()
        .plugins(SlottedPlugins::headless())
        .mods_dir(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../modded/mods"))
        .resolution(1600.0, 900.0)
        .theme("glass")
        .build();
    harness
        .world_mut()
        .insert_resource(web_playground::runtime());
    let layout = harness.mod_layout();
    harness.load_mods(layout);

    let registries = harness
        .world()
        .resource::<slotted_ecs::Registries>()
        .0
        .clone();
    harness.open_mod_screen(
        CHEST,
        (
            showcase::mods::menu_def(),
            showcase::mods::inventories(&registries),
        ),
    );

    let bus = Bus::new();
    let world = harness.world_mut();
    world.insert_resource(bus.clone());
    world.init_resource::<Messages<web_playground::RestoreState>>();
    // A snapshot names the scene it was taken in, and `apply_restore` writes a
    // `SwitchScene` for it (showcase contract section 4).
    world.init_resource::<Messages<web_playground::showcase::SwitchScene>>();
    world.init_resource::<web_playground::showcase::ActiveScene>();
    world.get_resource_or_init::<Schedules>().add_systems(
        Update,
        (
            web_playground::apply_restore_for_test,
            web_playground::publish_snapshot_for_test,
        )
            .chain(),
    );
    harness.settle();
    (harness, bus)
}

/// Every stack in the menu, as `(inventory, slot, name, count)`.
fn contents(harness: &mut UiHarness) -> Vec<(usize, usize, String, u32)> {
    let registries = harness
        .world()
        .resource::<slotted_ecs::Registries>()
        .0
        .clone();
    let world = harness.world_mut();
    let mut menus = world.query::<&slotted_ecs::menu::OpenMenu>();
    let entities: Vec<Entity> = menus.iter(world).next().unwrap().inventories.clone();
    let mut out = Vec::new();
    for (index, entity) in entities.into_iter().enumerate() {
        let held = world.get::<slotted_ecs::menu::Inventory>(entity).unwrap();
        for slot in 0..held.0.len() {
            if let Some(stack) = held.0.get(slot) {
                let name = registries.items.name_of(stack.id).unwrap().to_string();
                out.push((index, slot, name, stack.count));
            }
        }
    }
    out
}

/// Replaces the container's first slot, the way a visitor dragging stacks
/// around would, so the snapshot is provably not just the demo's own table.
fn rearrange(harness: &mut UiHarness) {
    let registries = harness
        .world()
        .resource::<slotted_ecs::Registries>()
        .0
        .clone();
    let apple = registries
        .item_id(&Namespaced::parse("appleskin_like:apple").unwrap())
        .unwrap();
    let world = harness.world_mut();
    let mut menus = world.query::<&slotted_ecs::menu::OpenMenu>();
    let container = menus.iter(world).next().unwrap().inventories[0];
    let mut held = world
        .get_mut::<slotted_ecs::menu::Inventory>(container)
        .unwrap();
    held.0.set(0, Some(ItemStack::new(apple, 7)));
    held.0.set(1, None);
}

/// The whole route: the world publishes, the page holds the value across a
/// restart, and the restored world holds exactly what the old one did.
#[test]
fn a_snapshot_taken_before_a_restart_comes_back_stack_for_stack() {
    let (mut before, bus) = playground_world();
    rearrange(&mut before);
    // A frame so `publish_snapshot` runs over the rearranged inventories.
    before.settle();
    let expected = contents(&mut before);
    let snapshot = bus.snapshot();
    assert!(
        !snapshot.is_empty(),
        "the world never published a snapshot; the page would have nothing to restore"
    );
    assert!(
        expected.iter().any(|(_, slot, name, count)| *slot == 0
            && name == "appleskin_like:apple"
            && *count == 7),
        "the rearrangement never reached the snapshot: {expected:?}"
    );

    // The restart. A second world is exactly what the page gets back: the same
    // mods, the same demo contents, none of the visitor's arrangement.
    let (mut after, fresh) = playground_world();
    assert_ne!(
        contents(&mut after),
        expected,
        "the fresh world already matched, so this test proves nothing"
    );

    fresh.request(Request::Restore {
        state: snapshot.clone(),
    });
    // The request becomes a message, and the next frame applies it.
    web_playground::drain_requests_into(&fresh, after.world_mut());
    after.settle();

    assert_eq!(contents(&mut after), expected);
    let logged = fresh
        .console_history()
        .into_iter()
        .map(|line| line.text)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        logged.contains("restored the chest as it was"),
        "the page was never told the restore worked: {logged}"
    );
}

/// A stale share link, a hand-edited value, a snapshot from a different
/// build: none of it may empty the chest or take the tab down.
#[test]
fn an_unreadable_snapshot_leaves_the_chest_alone_and_says_so() {
    let (mut harness, bus) = playground_world();
    let before = contents(&mut harness);

    bus.request(Request::Restore {
        state: "this is not a snapshot".to_owned(),
    });
    web_playground::drain_requests_into(&bus, harness.world_mut());
    harness.settle();

    assert_eq!(contents(&mut harness), before);
    let logged = bus
        .console_history()
        .into_iter()
        .map(|line| line.text)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(logged.contains("unreadable"), "{logged}");
}

/// A restore that arrives before the chest exists waits for it rather than
/// being dropped, which is the order the page actually calls things in: the
/// module is instantiated, `restore_state` is called, and only then does
/// `Startup` open the menu.
#[test]
fn a_restore_that_arrives_before_the_menu_waits_for_it() {
    let (mut source, bus) = playground_world();
    rearrange(&mut source);
    source.settle();
    let expected = contents(&mut source);
    let snapshot = bus.snapshot();

    // A world with the systems but no menu yet.
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<Messages<web_playground::RestoreState>>()
        .init_resource::<Messages<web_playground::showcase::SwitchScene>>()
        .insert_resource(Bus::new());
    let waiting = app.world().resource::<Bus>().clone();
    app.add_systems(Update, web_playground::apply_restore_for_test);
    waiting.request(Request::Restore {
        state: snapshot.clone(),
    });
    web_playground::drain_requests_into(&waiting, app.world_mut());
    for _ in 0..3 {
        app.update();
    }
    let logged = waiting
        .console_history()
        .into_iter()
        .map(|line| line.text)
        .collect::<Vec<_>>();
    assert!(
        logged.is_empty(),
        "the restore gave up instead of waiting: {logged:?}"
    );

    // And it still applies once a real world does have one.
    let (mut late, fresh) = playground_world();
    fresh.request(Request::Restore { state: snapshot });
    web_playground::drain_requests_into(&fresh, late.world_mut());
    late.settle();
    assert_eq!(contents(&mut late), expected);
}

/// The publish is on change, not on a clock: a chest nobody touched must not
/// rewrite the same bytes every frame, and one that was touched must be
/// published without waiting for a timer.
#[test]
fn the_snapshot_is_republished_when_the_chest_changes_and_not_otherwise() {
    let (mut harness, bus) = playground_world();
    let first = bus.snapshot();
    assert!(!first.is_empty(), "the first frame published nothing");

    bus.set_snapshot("(sentinel)");
    for _ in 0..5 {
        harness.settle();
    }
    assert_eq!(
        bus.snapshot(),
        "(sentinel)",
        "an untouched chest was republished anyway"
    );

    rearrange(&mut harness);
    harness.settle();
    let republished = bus.snapshot();
    assert_ne!(
        republished, "(sentinel)",
        "a touched chest was not republished"
    );
    assert!(
        republished.contains("appleskin_like:apple\",count:7"),
        "the republished snapshot missed the change: {republished}"
    );
}
