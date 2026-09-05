//! Adversarial review of the Phase 2 prediction loop.
//!
//! Each test here tries to break a claim `docs/design/phase2-contract.md`
//! section 1 makes, rather than to demonstrate the happy path
//! `tests/prediction.rs` already covers.

#![allow(clippy::unwrap_used)]

use std::sync::Arc;
use std::time::Duration;

use bevy::prelude::*;
use pretty_assertions::assert_eq;
use slotted_ecs::{
    Authority, Carried, ClickInterpreter, Dropped, Favorite, Inventory, MenuAction,
    MenuIdAllocator, OpenMenu, PendingRoundTrips, SlotChanged, SlotClicked, SlotEntities, SlotRef,
    close_menu, open_menu,
};
use slotted_model::{
    Actor, AuthorityEvent, Button, ClickAction, DragKind, Inventories, ItemStack, MenuDef, MenuId,
    MenuSnapshot, MenuState, RoutingTable, SlotBehaviour, SlotIx, ToolbarAction,
};
use slotted_testutils::{
    RecordingAuthority, TestItems, empty_inventory, inventory, minimal_ecs_app, stack, test_items,
    test_registries,
};

// ------------------------------------------------------------------ helpers

/// Two four-slot inventories: a container and the player's main, with a
/// quick-move route between them.
fn small_chest() -> Arc<MenuDef> {
    let mut def = MenuDef::new();
    let container = def.add_slots(MenuDef::CONTAINER, 4, SlotBehaviour::Normal);
    let main = def.add_slots(MenuDef::PLAYER_MAIN, 4, SlotBehaviour::Normal);
    def.hotbar = main.iter().collect();
    def.quick_move = RoutingTable::new()
        .route(container, [MenuDef::PLAYER_MAIN])
        .route(main, [MenuDef::CONTAINER]);
    def.listring = vec![MenuDef::CONTAINER, MenuDef::PLAYER_MAIN];
    Arc::new(def)
}

/// Every `SlotChanged` the app triggered, in order.
#[derive(Resource, Default, Debug)]
struct Changes(Vec<(Entity, SlotIx, Option<ItemStack>)>);

fn recording_app(authority: Arc<dyn slotted_model::Authority>) -> App {
    let mut app = minimal_ecs_app();
    app.insert_resource(Authority(authority))
        .init_resource::<Changes>()
        .add_observer(|event: On<SlotChanged>, mut seen: ResMut<Changes>| {
            seen.0.push((event.entity, event.slot, event.stack.clone()));
        });
    app
}

fn open(world: &mut World, def: Arc<MenuDef>, inventories: Vec<Entity>) -> Entity {
    let menu = world.resource_scope(|world, mut ids: Mut<MenuIdAllocator>| {
        let mut commands = world.commands();
        open_menu(&mut commands, &mut ids, def, inventories, Actor::SURVIVAL)
    });
    world.flush();
    menu
}

/// Spawns one `SlotRef` entity per slot of `def` and returns them in order.
fn bind_slots(world: &mut World, menu: Entity, count: usize) -> Vec<Entity> {
    (0..count)
        .map(|i| {
            world
                .spawn(SlotRef {
                    menu,
                    slot: SlotIx(u16::try_from(i).unwrap()),
                })
                .id()
        })
        .collect()
}

fn changes(app: &App) -> Vec<(SlotIx, Option<ItemStack>)> {
    app.world()
        .resource::<Changes>()
        .0
        .iter()
        .map(|(_, slot, stack)| (*slot, stack.clone()))
        .collect()
}

fn clear_changes(app: &mut App) {
    app.world_mut().resource_mut::<Changes>().0.clear();
}

fn slot_of(app: &App, entity: Entity, index: usize) -> Option<ItemStack> {
    app.world()
        .get::<Inventory>(entity)
        .unwrap()
        .get(index)
        .cloned()
}

// ------------------------------------------------------- out-of-order resync

/// A `Resync` that arrives after two further predicted clicks must win: the
/// world ends up equal to the snapshot, and only the slots that actually
/// differ from what the client was showing are re-announced.
#[test]
fn a_late_resync_overrides_two_further_predictions_and_emits_only_the_differences() {
    let registries = test_registries();
    let items = test_items(&registries);
    let authority = RecordingAuthority::manual();
    let mut app = recording_app(authority.clone());

    let def = small_chest();
    let world = app.world_mut();
    let container = world
        .spawn(inventory([
            stack(items.stone, 64),
            stack(items.egg, 10),
            None,
            None,
        ]))
        .id();
    let player = world.spawn(empty_inventory(4)).id();
    let menu = open(world, def.clone(), vec![container, player]);
    let id = world.get::<OpenMenu>(menu).unwrap().id;
    let slots = bind_slots(world, menu, def.slots.len());
    app.update();
    clear_changes(&mut app);

    // Three predicted clicks, none of them acked yet: pick up slot 0, drop it
    // on slot 2, pick up slot 1.
    for (slot, button) in [(0u16, Button::Left), (2, Button::Left), (1, Button::Left)] {
        let entity = slots[usize::from(slot)];
        app.world_mut().trigger(SlotClicked {
            entity,
            button,
            modifiers: slotted_ecs::Modifiers::NONE,
        });
        app.update();
    }
    assert_eq!(app.world().resource::<PendingRoundTrips>().0, 3);
    assert_eq!(slot_of(&app, container, 0), None, "slot 0 was picked up");
    assert_eq!(
        slot_of(&app, container, 2),
        stack(items.stone, 64),
        "and dropped on slot 2"
    );
    clear_changes(&mut app);

    // The authority disagrees: nothing ever moved, except that slot 3 now
    // holds a sword the client never saw.
    let mut snapshot_inventories = Inventories::new();
    let mut authoritative = slotted_model::Inventory::new(4);
    authoritative.set(0, stack(items.stone, 64));
    authoritative.set(1, stack(items.egg, 10));
    authoritative.set(3, stack(items.sword, 1));
    snapshot_inventories.push(authoritative);
    snapshot_inventories.push(slotted_model::Inventory::new(4));
    let snapshot = MenuSnapshot {
        inventories: snapshot_inventories,
        state: MenuState::new(&def),
    };
    authority.resync(id, snapshot.clone());
    app.update();

    // The container matches the snapshot exactly, and the carried stack the
    // last prediction picked up is gone.
    for i in 0..4 {
        assert_eq!(
            slot_of(&app, container, i),
            snapshot
                .inventories
                .get(MenuDef::CONTAINER)
                .unwrap()
                .get(i)
                .cloned(),
            "container slot {i} differs from the snapshot"
        );
    }
    assert_eq!(app.world().get::<Carried>(menu).unwrap().0, None);

    // Only the slots whose contents differed from the predicted world are
    // re-announced: 0 comes back, 1 is unchanged (the prediction had already
    // emptied it, so it changes), 2 empties again, 3 gains the sword.
    let mut emitted: Vec<SlotIx> = changes(&app).into_iter().map(|(s, _)| s).collect();
    emitted.sort_by_key(|s| s.0);
    emitted.dedup();
    assert_eq!(
        emitted,
        vec![SlotIx(0), SlotIx(1), SlotIx(2), SlotIx(3)],
        "exactly the slots that differ"
    );
    // Slot 4..8 (the player) never differed and must stay silent.
    assert!(
        changes(&app).iter().all(|(s, _)| s.0 < 4),
        "untouched player slots were re-emitted: {:?}",
        changes(&app)
    );
}

/// A resync that changes nothing emits nothing.
#[test]
fn a_resync_equal_to_the_current_world_emits_no_slot_changes() {
    let registries = test_registries();
    let items = test_items(&registries);
    let authority = RecordingAuthority::manual();
    let mut app = recording_app(authority.clone());

    let def = small_chest();
    let world = app.world_mut();
    let container = world
        .spawn(inventory([stack(items.stone, 64), None, None, None]))
        .id();
    let player = world.spawn(empty_inventory(4)).id();
    let menu = open(world, def.clone(), vec![container, player]);
    let id = world.get::<OpenMenu>(menu).unwrap().id;
    bind_slots(world, menu, def.slots.len());
    app.update();
    clear_changes(&mut app);

    let mut inventories = Inventories::new();
    let mut same = slotted_model::Inventory::new(4);
    same.set(0, stack(items.stone, 64));
    inventories.push(same);
    inventories.push(slotted_model::Inventory::new(4));
    authority.resync(
        id,
        MenuSnapshot {
            inventories,
            state: MenuState::new(&def),
        },
    );
    app.update();

    assert_eq!(changes(&app), vec![], "a no-op resync must be silent");
}

// -------------------------------------------------- one inventory, two menus

/// The player's inventory is shared: a chest menu and the player's own menu
/// are open over the same `Inventory` entity. A click through one must reach
/// the slot entities of the other, because both are drawing that inventory.
#[test]
fn two_menus_over_the_same_inventory_entity_both_see_the_change() {
    let registries = test_registries();
    let items = test_items(&registries);
    let mut app = recording_app(RecordingAuthority::new());

    let def = small_chest();
    let world = app.world_mut();
    let shared = world
        .spawn(inventory([stack(items.stone, 64), None, None, None]))
        .id();
    let other_container = world.spawn(empty_inventory(4)).id();

    // Menu A draws `shared` as its container; menu B draws it as its player
    // half. Both have slot entities bound to it.
    let menu_a = open(world, def.clone(), vec![shared, other_container]);
    let menu_b = open(world, def.clone(), vec![other_container, shared]);
    let slots_a = bind_slots(world, menu_a, def.slots.len());
    let slots_b = bind_slots(world, menu_b, def.slots.len());
    app.update();
    clear_changes(&mut app);

    // Pick up slot 0 of menu A: that is `shared` slot 0, which menu B draws
    // as its slot 4.
    app.world_mut().trigger(SlotClicked {
        entity: slots_a[0],
        button: Button::Left,
        modifiers: slotted_ecs::Modifiers::NONE,
    });
    app.update();

    assert_eq!(slot_of(&app, shared, 0), None, "the model moved");
    let seen: Vec<Entity> = app
        .world()
        .resource::<Changes>()
        .0
        .iter()
        .map(|(e, _, _)| *e)
        .collect();
    assert!(
        seen.contains(&slots_a[0]),
        "menu A's own slot was told: {seen:?}"
    );
    assert!(
        seen.contains(&slots_b[4]),
        "menu B draws the same inventory slot and was not told; \
         SlotChanged is emitted per acting menu, not per inventory"
    );
}

// ------------------------------------------------------ closing mid-gesture

/// Closing a menu while a drag paint is in progress must not leave the shared
/// `ClickInterpreter` primed: the next menu would inherit the paint.
#[test]
fn closing_a_menu_mid_drag_clears_the_interpreter() {
    let registries = test_registries();
    let items = test_items(&registries);
    let mut app = recording_app(RecordingAuthority::new());

    let def = small_chest();
    let world = app.world_mut();
    let container = world
        .spawn(inventory([stack(items.stone, 64), None, None, None]))
        .id();
    let player = world.spawn(empty_inventory(4)).id();
    let menu = open(world, def.clone(), vec![container, player]);
    let id = world.get::<OpenMenu>(menu).unwrap().id;
    bind_slots(world, menu, def.slots.len());
    app.update();

    // Pick a stack up, then start painting it across the grid.
    app.world_mut().trigger(MenuAction {
        entity: menu,
        action: ClickAction::Pickup {
            slot: SlotIx(0),
            button: Button::Left,
        },
    });
    app.update();
    let start = app
        .world_mut()
        .resource_mut::<ClickInterpreter>()
        .begin_drag(DragKind::Left);
    app.world_mut().trigger(MenuAction {
        entity: menu,
        action: start,
    });
    app.update();
    assert!(app.world().resource::<ClickInterpreter>().drag.is_some());

    // The player hits escape mid-paint.
    app.world_mut().commands().queue(move |world: &mut World| {
        let mut commands = world.commands();
        close_menu(&mut commands, menu, id);
    });
    app.update();
    app.update();

    assert!(
        app.world().get_entity(menu).is_err(),
        "the menu entity is gone"
    );
    assert_eq!(
        app.world().resource::<Dropped>().0.len(),
        1,
        "the carried stack was dropped"
    );
    assert!(
        app.world().resource::<ClickInterpreter>().drag.is_none(),
        "the interpreter is still painting after the menu it painted on closed"
    );
}

// ------------------------------------------------------- pending round trips

/// Acks for menus that no longer exist, and more acks than submissions, must
/// never push the counter below zero or above what was submitted.
#[test]
fn pending_round_trips_never_goes_negative() {
    let authority = RecordingAuthority::manual();
    let mut app = recording_app(authority.clone());

    let def = small_chest();
    let world = app.world_mut();
    let container = world.spawn(empty_inventory(4)).id();
    let player = world.spawn(empty_inventory(4)).id();
    let menu = open(world, def.clone(), vec![container, player]);
    bind_slots(world, menu, def.slots.len());
    app.update();
    assert_eq!(app.world().resource::<PendingRoundTrips>().0, 0);

    // Five acks for a menu that was never opened.
    for _ in 0..5 {
        authority.push_event(AuthorityEvent::Ack {
            menu: MenuId(999),
            state_id: 0,
        });
    }
    app.update();
    assert_eq!(
        app.world().resource::<PendingRoundTrips>().0,
        0,
        "stray acks must not wrap the counter"
    );
    assert!(app.world().resource::<PendingRoundTrips>().is_idle());
}

// -------------------------------------------------------------- dropped bin

/// `Dropped` accumulates across actions and menus and hands everything over
/// exactly once when drained.
#[test]
fn dropped_accumulates_across_actions_and_drains_once() {
    let registries = test_registries();
    let items = test_items(&registries);
    let mut app = recording_app(RecordingAuthority::new());

    let def = small_chest();
    let world = app.world_mut();
    let container = world
        .spawn(inventory([
            stack(items.stone, 64),
            stack(items.egg, 10),
            None,
            None,
        ]))
        .id();
    let player = world.spawn(empty_inventory(4)).id();
    let menu = open(world, def.clone(), vec![container, player]);
    bind_slots(world, menu, def.slots.len());
    app.update();

    // Pick up, throw outside; pick up, throw outside.
    for slot in [SlotIx(0), SlotIx(1)] {
        app.world_mut().trigger(MenuAction {
            entity: menu,
            action: ClickAction::Pickup {
                slot,
                button: Button::Left,
            },
        });
        app.update();
        app.world_mut().trigger(MenuAction {
            entity: menu,
            action: ClickAction::Pickup {
                slot: SlotIx::OUTSIDE,
                button: Button::Left,
            },
        });
        app.update();
    }

    let dropped = app.world().resource::<Dropped>().clone();
    assert_eq!(dropped.0.len(), 2, "both throws were recorded: {dropped:?}");
    assert!(dropped.0.iter().all(|d| d.menu == menu));
    assert_eq!(dropped.of(menu).count(), 2);

    let drained = app.world_mut().resource_mut::<Dropped>().drain();
    assert_eq!(drained.len(), 2);
    assert!(
        app.world().resource::<Dropped>().0.is_empty(),
        "draining empties the bin"
    );
    assert_eq!(
        app.world_mut().resource_mut::<Dropped>().drain().len(),
        0,
        "a second drain hands nothing over twice"
    );
}

// ---------------------------------------------------------------- favorites

/// A sort moves the favourite flag with the stack, and the `Favorite` marker
/// follows it onto the slot entity that now draws it.
#[test]
fn the_favorite_mirror_follows_a_sort() {
    let registries = test_registries();
    let items = test_items(&registries);
    let mut app = recording_app(RecordingAuthority::new());

    let def = small_chest();
    let world = app.world_mut();
    let mut contents = slotted_model::Inventory::new(4);
    contents.set(2, stack(items.stone, 64));
    contents.set_favorite(2, true);
    let container = world.spawn(Inventory(contents)).id();
    let player = world.spawn(empty_inventory(4)).id();
    let menu = open(world, def.clone(), vec![container, player]);
    let slots = bind_slots(world, menu, def.slots.len());
    app.update();

    let favorites = |app: &App| -> Vec<usize> {
        slots
            .iter()
            .enumerate()
            .filter(|(_, e)| app.world().get::<Favorite>(**e).is_some())
            .map(|(i, _)| i)
            .collect()
    };
    assert_eq!(favorites(&app), vec![2], "the seeded favourite is mirrored");

    app.world_mut().trigger(MenuAction {
        entity: menu,
        action: ClickAction::Toolbar(ToolbarAction::Sort {
            inventory: MenuDef::CONTAINER,
        }),
    });
    app.update();
    // `mirror_favorites` inserts through `Commands`, so the marker lands on
    // the following frame.
    app.update();

    let inv = app.world().get::<Inventory>(container).unwrap();
    let moved = (0..4).find(|i| inv.get(*i).is_some()).unwrap();
    assert!(
        inv.is_favorite(moved),
        "the model kept the flag with the stack"
    );
    assert_eq!(
        favorites(&app),
        vec![moved],
        "the marker moved with it and nowhere else"
    );
}

// ------------------------------------------------------------ virtual clock

/// The double-click window is 250 ms of accumulated `Time<Virtual>`, and the
/// comparison is strict. Two clicks a millisecond apart gather; two clicks a
/// full window apart do not, even when the frame between them was clamped to
/// `Time<Virtual>::max_delta`, which defaults to the same 250 ms. Real time
/// passing between the clicks changes nothing either way.
#[test]
fn the_double_click_window_reads_virtual_time_only() {
    /// A menu whose container holds two half stacks of the same item.
    fn two_half_stacks(frame: Duration) -> (App, Entity, Entity, Vec<Entity>, TestItems) {
        let registries = test_registries();
        let items = test_items(&registries);
        let mut app = recording_app(RecordingAuthority::new());
        app.insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(frame));
        let def = small_chest();
        let world = app.world_mut();
        let container = world
            .spawn(inventory([
                stack(items.stone, 32),
                stack(items.stone, 32),
                None,
                None,
            ]))
            .id();
        let player = world.spawn(empty_inventory(4)).id();
        let menu = open(world, def.clone(), vec![container, player]);
        let slots = bind_slots(world, menu, def.slots.len());
        app.update();
        (app, menu, container, slots, items)
    }

    fn click(app: &mut App, slot: Entity) {
        app.world_mut().trigger(SlotClicked {
            entity: slot,
            button: Button::Left,
            modifiers: slotted_ecs::Modifiers::NONE,
        });
        app.update();
    }

    let window = ClickInterpreter::default().double_click_window;

    // Frames of one millisecond: the two clicks fall inside the window and
    // the second gathers the other half stack onto the cursor.
    let (mut app, menu, container, slots, items) = two_half_stacks(Duration::from_millis(1));
    click(&mut app, slots[0]);
    click(&mut app, slots[0]);
    assert_eq!(
        app.world().get::<Carried>(menu).unwrap().0,
        stack(items.stone, 64),
        "a double click inside the window gathers"
    );
    assert_eq!(slot_of(&app, container, 1), None);

    // One frame exactly as long as the window: the boundary case. Only
    // virtual time moved, and a gap equal to the window is two clicks.
    let (mut app, menu, container, slots, items) = two_half_stacks(window);
    click(&mut app, slots[0]);
    assert_eq!(
        app.world().get::<Carried>(menu).unwrap().0,
        stack(items.stone, 32),
        "the first click picked slot 0 up"
    );
    std::thread::sleep(Duration::from_millis(2));
    app.update();
    click(&mut app, slots[0]);

    assert_eq!(
        app.world().get::<Carried>(menu).unwrap().0,
        None,
        "the second click put the stack back instead of gathering"
    );
    assert_eq!(
        slot_of(&app, container, 1),
        stack(items.stone, 32),
        "the far-apart second click must not gather slot 1"
    );
}

/// A click on a different slot inside the window is never a double click.
#[test]
fn a_second_click_on_another_slot_is_not_a_double_click() {
    let registries = test_registries();
    let items = test_items(&registries);
    let mut app = recording_app(RecordingAuthority::new());

    let def = small_chest();
    let world = app.world_mut();
    let container = world
        .spawn(inventory([
            stack(items.stone, 32),
            stack(items.stone, 32),
            None,
            None,
        ]))
        .id();
    let player = world.spawn(empty_inventory(4)).id();
    let menu = open(world, def.clone(), vec![container, player]);
    let slots = bind_slots(world, menu, def.slots.len());
    app.update();

    for slot in [0usize, 1] {
        app.world_mut().trigger(SlotClicked {
            entity: slots[slot],
            button: Button::Left,
            modifiers: slotted_ecs::Modifiers::NONE,
        });
        app.update();
    }

    // Pick up slot 0, then click slot 1: the carried 32 merges into slot 1,
    // which is a place, not a gather.
    assert_eq!(app.world().get::<Carried>(menu).unwrap().0, None);
    assert_eq!(slot_of(&app, container, 0), None);
    assert_eq!(slot_of(&app, container, 1), stack(items.stone, 64));
}

/// `SlotEntities` is a reverse index; a despawned slot entity must not leave
/// the menu triggering at a dead entity.
#[test]
fn a_menu_with_no_slot_entities_still_predicts() {
    let registries = test_registries();
    let items = test_items(&registries);
    let _ = TestItems::default();
    let mut app = recording_app(RecordingAuthority::new());

    let def = small_chest();
    let world = app.world_mut();
    let container = world
        .spawn(inventory([stack(items.stone, 64), None, None, None]))
        .id();
    let player = world.spawn(empty_inventory(4)).id();
    let menu = open(world, def.clone(), vec![container, player]);
    app.update();

    assert!(app.world().get::<SlotEntities>(menu).unwrap().0.is_empty());
    app.world_mut().trigger(MenuAction {
        entity: menu,
        action: ClickAction::Pickup {
            slot: SlotIx(0),
            button: Button::Left,
        },
    });
    app.update();

    assert_eq!(
        app.world().get::<Carried>(menu).unwrap().0,
        stack(items.stone, 64),
        "a headless menu still predicts"
    );
    assert_eq!(changes(&app), vec![], "and triggers nothing at nobody");
}
