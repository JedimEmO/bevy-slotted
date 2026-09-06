//! The prediction loop end to end, under `MinimalPlugins`.

#![allow(clippy::unwrap_used)]

use std::sync::Arc;

use bevy::prelude::*;
use pretty_assertions::assert_eq;
// Named rather than glob-imported: with `bevy_ui` enabled by a sibling crate,
// `bevy::prelude` also exports a `Button`, and two globs would be ambiguous.
use slotted_ecs::{
    Authority, Carried, ClickInterpreter, Dropped, Favorite, Inventory, MenuAction, MenuClosed,
    MenuIdAllocator, MenuProperty, Modifiers, OpenMenu, PendingRoundTrips, PlayerInventories,
    PropertyChanged, SlotChanged, SlotClicked, SlotEntities, SlotRef, close_menu,
    open_container_menu, open_menu,
};
use slotted_model::{
    Actor, Button, ClickAction, DragKind, DragStage, Inventories, ItemStack, MenuDef, MenuId,
    MenuSnapshot, MenuState, PropertyDef, PropertyId, RoutingTable, SlotBehaviour, SlotIx,
    ToolbarAction,
};
use slotted_testutils::{
    RecordingAuthority, TestItems, empty_inventory, inventory, minimal_ecs_app, stack, test_items,
    test_registries,
};

// ---------------------------------------------------------------- recorders

#[derive(Resource, Default, Debug)]
struct SlotChanges(Vec<(Entity, SlotIx, Option<ItemStack>)>);

#[derive(Resource, Default, Debug)]
struct PropertyChanges(Vec<(PropertyId, i32)>);

#[derive(Resource, Default, Debug)]
struct Closes(Vec<MenuId>);

// ------------------------------------------------------------------ fixture

/// Two inventories of four slots: a container and the player's main. Slot ids
/// `0..4` are the container, `4..8` the player, and the number keys 1 to 4
/// address the player slots.
fn small_chest() -> Arc<MenuDef> {
    let mut def = MenuDef::new();
    let container = def.add_slots(MenuDef::CONTAINER, 4, SlotBehaviour::Normal);
    let main = def.add_slots(MenuDef::PLAYER_MAIN, 4, SlotBehaviour::Normal);
    def.hotbar = main.iter().collect();
    def.quick_move = RoutingTable::new()
        .route(container, [MenuDef::PLAYER_MAIN])
        .route(main, [MenuDef::CONTAINER]);
    def.listring = vec![MenuDef::CONTAINER, MenuDef::PLAYER_MAIN];
    def.properties = vec![PropertyDef {
        id: PropertyId(0),
        initial: 7,
    }];
    Arc::new(def)
}

struct Fixture {
    app: App,
    menu: Entity,
    id: MenuId,
    container: Entity,
    player: Entity,
    /// UI entities carrying a `SlotRef`, indexed by slot id.
    slots: Vec<Entity>,
    items: TestItems,
    authority: Arc<RecordingAuthority>,
}

impl Fixture {
    /// A menu whose container holds 64 stone and 10 eggs, ready to click.
    fn new(authority: Arc<RecordingAuthority>) -> Self {
        Self::with_def(authority, small_chest())
    }

    /// [`new`](Self::new) over a menu definition of the caller's choosing.
    fn with_def(authority: Arc<RecordingAuthority>, def: Arc<MenuDef>) -> Self {
        let registries = test_registries();
        let items = test_items(&registries);
        let mut app = minimal_ecs_app();
        app.insert_resource(Authority(authority.clone()))
            .init_resource::<SlotChanges>()
            .init_resource::<PropertyChanges>()
            .init_resource::<Closes>()
            .add_observer(|event: On<SlotChanged>, mut seen: ResMut<SlotChanges>| {
                let event = event.event();
                seen.0.push((event.entity, event.slot, event.stack.clone()));
            })
            .add_observer(
                |event: On<PropertyChanged>, mut seen: ResMut<PropertyChanges>| {
                    seen.0.push((event.id, event.value));
                },
            )
            .add_observer(|event: On<MenuClosed>, mut seen: ResMut<Closes>| {
                seen.0.push(event.id);
            });

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
        let slots: Vec<Entity> = (0..def.slots.len())
            .map(|i| {
                world
                    .spawn(SlotRef {
                        menu,
                        slot: SlotIx(u16::try_from(i).unwrap()),
                    })
                    .id()
            })
            .collect();

        let mut fixture = Self {
            app,
            menu,
            id,
            container,
            player,
            slots,
            items,
            authority,
        };
        // One frame so `register_slot_refs` fills `SlotEntities`.
        fixture.app.update();
        assert_eq!(
            fixture
                .app
                .world()
                .get::<SlotEntities>(fixture.menu)
                .unwrap()
                .0
                .len(),
            8
        );
        // Registering a `SlotRef` seeds it with the model's current contents,
        // so every slot has already reported once. Tests here are about what
        // a click changes, so the seeding is cleared away.
        assert_eq!(
            fixture.slot_changes().len(),
            8,
            "every registered slot reports its contents once"
        );
        assert_eq!(
            fixture.slot_changes()[0],
            (SlotIx(0), stack(items.stone, 64))
        );
        fixture.clear_slot_changes();
        fixture
    }

    fn click(&mut self, slot: u16, button: Button, modifiers: Modifiers) {
        let entity = self.slots[usize::from(slot)];
        self.app.world_mut().trigger(SlotClicked {
            entity,
            button,
            modifiers,
        });
    }

    fn act(&mut self, action: ClickAction) {
        let menu = self.menu;
        self.app.world_mut().trigger(MenuAction {
            entity: menu,
            action,
        });
    }

    fn carried(&self) -> Option<ItemStack> {
        self.app
            .world()
            .get::<Carried>(self.menu)
            .unwrap()
            .0
            .clone()
    }

    fn state_id(&self) -> u32 {
        self.app
            .world()
            .get::<OpenMenu>(self.menu)
            .unwrap()
            .state
            .state_id
    }

    fn slot_of(&self, entity: Entity, index: usize) -> Option<ItemStack> {
        self.app
            .world()
            .get::<Inventory>(entity)
            .unwrap()
            .get(index)
            .cloned()
    }

    fn dirty(&self, entity: Entity) -> Vec<usize> {
        self.app
            .world()
            .get::<Inventory>(entity)
            .unwrap()
            .changed()
            .iter()
            .collect()
    }

    fn slot_changes(&self) -> Vec<(SlotIx, Option<ItemStack>)> {
        self.app
            .world()
            .resource::<SlotChanges>()
            .0
            .iter()
            .map(|(_, slot, stack)| (*slot, stack.clone()))
            .collect()
    }

    fn clear_slot_changes(&mut self) {
        self.app.world_mut().resource_mut::<SlotChanges>().0.clear();
    }

    fn round_trips(&self) -> u32 {
        self.app.world().resource::<PendingRoundTrips>().0
    }

    /// The hint a ghost or filter slot displays, out of the menu state.
    fn hint(&self, slot: SlotIx) -> Option<ItemStack> {
        self.app
            .world()
            .get::<OpenMenu>(self.menu)
            .unwrap()
            .state
            .hint(slot)
            .cloned()
    }
}

/// `open_menu` needs `Commands` and the allocator at once.
fn open(world: &mut World, def: Arc<MenuDef>, inventories: Vec<Entity>) -> Entity {
    let menu = world.resource_scope(|world, mut ids: Mut<MenuIdAllocator>| {
        let mut commands = world.commands();
        open_menu(&mut commands, &mut ids, def, inventories, Actor::SURVIVAL)
    });
    world.flush();
    menu
}

// -------------------------------------------------------------------- tests

#[test]
fn a_left_click_picks_up_the_stack_in_the_same_frame() {
    let mut fixture = Fixture::new(RecordingAuthority::new());
    fixture.click(0, Button::Left, Modifiers::NONE);
    fixture.app.update();

    assert_eq!(
        fixture.carried(),
        Some(ItemStack::new(fixture.items.stone, 64))
    );
    assert_eq!(fixture.slot_of(fixture.container, 0), None);
    assert_eq!(fixture.slot_changes(), vec![(SlotIx(0), None)]);
}

#[test]
fn the_authority_receives_the_action_and_the_predicted_delta() {
    let mut fixture = Fixture::new(RecordingAuthority::new());
    fixture.click(0, Button::Left, Modifiers::NONE);
    fixture.app.update();

    let submitted = fixture.authority.submitted();
    assert_eq!(submitted.len(), 1);
    let submitted = &submitted[0];
    assert_eq!(submitted.menu, fixture.id);
    assert_eq!(
        submitted.action,
        ClickAction::Pickup {
            slot: SlotIx(0),
            button: Button::Left
        }
    );
    assert_eq!(submitted.delta.slots, vec![(SlotIx(0), None)]);
    assert_eq!(
        submitted.delta.carried,
        Some(ItemStack::new(fixture.items.stone, 64))
    );
    assert_eq!(submitted.delta.state_id, 1);
}

#[test]
fn an_ack_leaves_the_predicted_state_alone_and_clears_the_round_trip() {
    let authority = RecordingAuthority::manual();
    let mut fixture = Fixture::new(authority.clone());
    fixture.click(0, Button::Left, Modifiers::NONE);
    fixture.app.update();

    assert_eq!(fixture.round_trips(), 1, "submitted but not answered");
    let before = (fixture.carried(), fixture.slot_of(fixture.container, 0));

    assert!(authority.ack_next());
    fixture.clear_slot_changes();
    fixture.app.update();

    assert_eq!(fixture.round_trips(), 0);
    assert_eq!(
        (fixture.carried(), fixture.slot_of(fixture.container, 0)),
        before
    );
    assert!(
        fixture.slot_changes().is_empty(),
        "an ack changes nothing, so it emits nothing"
    );
}

#[test]
fn a_resync_overwrites_the_prediction_and_emits_only_the_changed_slots() {
    let authority = RecordingAuthority::manual();
    let mut fixture = Fixture::new(authority.clone());
    let items = fixture.items;

    fixture.click(0, Button::Left, Modifiers::NONE);
    fixture.app.update();
    assert_eq!(fixture.carried(), Some(ItemStack::new(items.stone, 64)));

    // The authority disagrees: nothing was picked up, and a sword appeared.
    let mut inventories = Inventories::new();
    inventories.push(slotted_model::Inventory::from_slots([
        stack(items.stone, 64),
        stack(items.egg, 10),
        stack(items.sword, 1),
        None,
    ]));
    inventories.push(slotted_model::Inventory::new(4));
    let snapshot = MenuSnapshot {
        inventories,
        state: MenuState {
            carried: None,
            state_id: 99,
            drag: None,
            properties: vec![7],
            hints: std::collections::BTreeMap::new(),
        },
    };
    authority.resync(fixture.id, snapshot);
    fixture.clear_slot_changes();
    fixture.app.update();

    assert_eq!(fixture.carried(), None);
    assert_eq!(fixture.state_id(), 99);
    assert_eq!(
        fixture.slot_of(fixture.container, 0),
        Some(ItemStack::new(items.stone, 64))
    );
    assert_eq!(
        fixture.slot_of(fixture.container, 2),
        Some(ItemStack::new(items.sword, 1))
    );
    assert_eq!(
        fixture.slot_changes(),
        vec![
            (SlotIx(0), stack(items.stone, 64)),
            (SlotIx(2), stack(items.sword, 1)),
        ],
        "slot 1 and the untouched player slots must not be re-emitted"
    );
    assert_eq!(fixture.round_trips(), 0);
}

#[test]
fn a_shift_click_crosses_inventory_entities_and_marks_both_dirty() {
    let mut fixture = Fixture::new(RecordingAuthority::new());
    let items = fixture.items;
    fixture.click(0, Button::Left, Modifiers::SHIFT);
    fixture.app.update();

    assert_eq!(fixture.slot_of(fixture.container, 0), None);
    assert_eq!(
        fixture.slot_of(fixture.player, 0),
        Some(ItemStack::new(items.stone, 64))
    );
    assert_eq!(fixture.dirty(fixture.container), vec![0]);
    assert_eq!(fixture.dirty(fixture.player), vec![0]);
    assert_eq!(
        fixture.authority.last().unwrap().action,
        ClickAction::QuickMove { slot: SlotIx(0) }
    );
}

#[test]
fn the_dirty_masks_are_cleared_on_the_next_frame() {
    let mut fixture = Fixture::new(RecordingAuthority::new());
    fixture.click(0, Button::Left, Modifiers::SHIFT);
    fixture.app.update();
    assert_eq!(fixture.dirty(fixture.container), vec![0]);
    fixture.app.update();
    assert!(fixture.dirty(fixture.container).is_empty());
    assert!(fixture.dirty(fixture.player).is_empty());
}

#[test]
fn a_number_key_swaps_with_the_hotbar_slot() {
    let mut fixture = Fixture::new(RecordingAuthority::new());
    let items = fixture.items;
    fixture.act(ClickInterpreter::swap(SlotIx(0), 0));
    fixture.app.update();

    assert_eq!(fixture.slot_of(fixture.container, 0), None);
    assert_eq!(
        fixture.slot_of(fixture.player, 0),
        Some(ItemStack::new(items.stone, 64))
    );
    assert_eq!(fixture.carried(), None);
    assert_eq!(
        fixture.slot_changes(),
        vec![(SlotIx(0), None), (SlotIx(4), stack(items.stone, 64)),]
    );
}

#[test]
fn a_drag_spreads_the_carried_stack_over_three_slots() {
    let mut fixture = Fixture::new(RecordingAuthority::new());
    let items = fixture.items;
    fixture.click(0, Button::Left, Modifiers::NONE);
    fixture.app.update();
    assert_eq!(fixture.carried(), Some(ItemStack::new(items.stone, 64)));

    let drag = |stage, slot| ClickAction::Drag {
        stage,
        kind: DragKind::Left,
        slot,
    };
    fixture.act(drag(DragStage::Start, None));
    for slot in [4, 5, 6] {
        fixture.act(drag(DragStage::Add, Some(SlotIx(slot))));
    }
    fixture.act(drag(DragStage::End, None));
    fixture.app.update();

    for index in 0..3 {
        assert_eq!(
            fixture.slot_of(fixture.player, index),
            Some(ItemStack::new(items.stone, 21)),
            "player slot {index}"
        );
    }
    assert_eq!(fixture.slot_of(fixture.player, 3), None);
    assert_eq!(fixture.carried(), Some(ItemStack::new(items.stone, 1)));
    assert_eq!(
        fixture
            .app
            .world()
            .get::<OpenMenu>(fixture.menu)
            .unwrap()
            .state
            .drag,
        None
    );
    assert_eq!(fixture.authority.submitted().len(), 6, "one per stage");
}

/// A host-side `SetProperty` is the machine simulation's own write: it takes
/// the same three steps a `AuthorityEvent::Property` does, without a round
/// trip (Phase 6 contract section 0).
#[test]
fn a_set_property_writes_the_menu_state_the_child_and_the_observers() {
    let mut fixture = Fixture::new(RecordingAuthority::new());
    fixture.app.world_mut().trigger(slotted_ecs::SetProperty {
        entity: fixture.menu,
        id: PropertyId(0),
        value: 42,
    });
    fixture.app.update();

    assert_eq!(
        fixture.app.world().resource::<PropertyChanges>().0,
        vec![(PropertyId(0), 42)],
        "PropertyChanged fires on the property child"
    );
    let value = fixture
        .app
        .world_mut()
        .query::<&MenuProperty>()
        .iter(fixture.app.world())
        .find(|p| p.id == PropertyId(0))
        .map(|p| p.value);
    assert_eq!(value, Some(42));
    assert_eq!(
        fixture
            .app
            .world()
            .get::<OpenMenu>(fixture.menu)
            .unwrap()
            .state
            .properties,
        vec![42],
        "OpenMenu.state carries it too"
    );
    assert_eq!(
        fixture.app.world().resource::<PendingRoundTrips>().0,
        0,
        "a host write bypasses the authority, so nothing is pending"
    );
}

/// A property the menu does not declare is a mistake in the caller, not a new
/// property: the write is dropped with a warning rather than silently
/// growing the state vector.
#[test]
fn a_set_property_for_an_unknown_property_changes_nothing() {
    let mut fixture = Fixture::new(RecordingAuthority::new());
    fixture.app.world_mut().trigger(slotted_ecs::SetProperty {
        entity: fixture.menu,
        id: PropertyId(99),
        value: 7,
    });
    fixture.app.update();
    assert!(
        fixture
            .app
            .world()
            .resource::<PropertyChanges>()
            .0
            .is_empty()
    );
    assert_eq!(
        fixture
            .app
            .world()
            .get::<OpenMenu>(fixture.menu)
            .unwrap()
            .state
            .properties,
        vec![7],
        "the property keeps the value the menu opened with"
    );
}

#[test]
fn a_property_update_reaches_the_child_entity_and_an_observer() {
    let mut fixture = Fixture::new(RecordingAuthority::new());
    fixture.authority.property(fixture.id, PropertyId(0), 42);
    fixture.app.update();

    assert_eq!(
        fixture.app.world().resource::<PropertyChanges>().0,
        vec![(PropertyId(0), 42)]
    );
    let value = fixture
        .app
        .world_mut()
        .query::<&MenuProperty>()
        .iter(fixture.app.world())
        .find(|p| p.id == PropertyId(0))
        .map(|p| p.value);
    assert_eq!(value, Some(42));
    assert_eq!(
        fixture
            .app
            .world()
            .get::<OpenMenu>(fixture.menu)
            .unwrap()
            .state
            .properties,
        vec![42]
    );
}

/// A snapshot carries the whole property vector, and every widget bound to a
/// property reads the `MenuProperty` child rather than the vector. Replacing
/// `MenuState.properties` alone therefore left tanks, bars and progress arrows
/// drawing the value from before the snapshot: the model said 42 and the
/// component still said 7. Snapshots take the same write path as a
/// per-property message.
#[test]
fn a_resync_updates_the_property_child_and_fires_property_changed() {
    let authority = RecordingAuthority::manual();
    let mut fixture = Fixture::new(authority.clone());
    let items = fixture.items;

    let mut inventories = Inventories::new();
    inventories.push(slotted_model::Inventory::from_slots([
        stack(items.stone, 64),
        None,
        None,
        None,
    ]));
    inventories.push(slotted_model::Inventory::new(4));
    authority.resync(
        fixture.id,
        MenuSnapshot {
            inventories,
            state: MenuState {
                carried: None,
                state_id: 12,
                drag: None,
                properties: vec![42],
                hints: std::collections::BTreeMap::new(),
            },
        },
    );
    fixture.app.update();

    let value = fixture
        .app
        .world_mut()
        .query::<&MenuProperty>()
        .iter(fixture.app.world())
        .find(|p| p.id == PropertyId(0))
        .map(|p| p.value);
    assert_eq!(
        value,
        Some(42),
        "the property entity moved with the snapshot"
    );
    assert_eq!(
        fixture
            .app
            .world()
            .get::<OpenMenu>(fixture.menu)
            .unwrap()
            .state
            .properties,
        vec![42]
    );
    assert_eq!(
        fixture.app.world().resource::<PropertyChanges>().0,
        vec![(PropertyId(0), 42)],
        "and everything observing the property was told once"
    );
}

/// The other half of the same rule: a snapshot that leaves a property where it
/// was must not announce a change, or a script subscribed to `property_changed`
/// would see one event per resync for a value nobody moved.
#[test]
fn a_resync_that_moves_no_property_fires_nothing() {
    let authority = RecordingAuthority::manual();
    let mut fixture = Fixture::new(authority.clone());

    let mut inventories = Inventories::new();
    inventories.push(slotted_model::Inventory::new(4));
    inventories.push(slotted_model::Inventory::new(4));
    authority.resync(
        fixture.id,
        MenuSnapshot {
            inventories,
            state: MenuState {
                carried: None,
                state_id: 5,
                drag: None,
                // The value the menu opened with.
                properties: vec![7],
                hints: std::collections::BTreeMap::new(),
            },
        },
    );
    fixture.app.update();

    assert!(
        fixture
            .app
            .world()
            .resource::<PropertyChanges>()
            .0
            .is_empty(),
        "no property moved, so no `PropertyChanged`"
    );
}

#[test]
fn closing_a_menu_drops_the_carried_stack_and_announces_the_close() {
    let mut fixture = Fixture::new(RecordingAuthority::new());
    let items = fixture.items;
    fixture.click(0, Button::Left, Modifiers::NONE);
    fixture.app.update();

    let (menu, id) = (fixture.menu, fixture.id);
    let world = fixture.app.world_mut();
    close_menu(&mut world.commands(), menu, id);
    world.flush();

    assert_eq!(world.resource::<Closes>().0, vec![id]);
    assert_eq!(
        world
            .resource::<Dropped>()
            .of(menu)
            .cloned()
            .collect::<Vec<_>>(),
        vec![ItemStack::new(items.stone, 64)]
    );
    assert!(world.get_entity(menu).is_err(), "the menu entity is gone");
}

#[test]
fn state_ids_increase_strictly_across_mutating_actions() {
    let mut fixture = Fixture::new(RecordingAuthority::new());
    fixture.click(0, Button::Left, Modifiers::NONE);
    fixture.app.update();
    fixture.click(2, Button::Left, Modifiers::NONE);
    fixture.app.update();
    fixture.click(1, Button::Left, Modifiers::SHIFT);
    fixture.app.update();

    let ids: Vec<u32> = fixture
        .authority
        .submitted()
        .iter()
        .map(|s| s.delta.state_id)
        .collect();
    assert_eq!(ids, vec![1, 2, 3]);
    assert!(ids.windows(2).all(|w| w[1] > w[0]));
    assert_eq!(fixture.state_id(), 3);
    assert_eq!(fixture.round_trips(), 0);
}

#[test]
fn two_open_menus_do_not_interfere() {
    let authority = RecordingAuthority::new();
    let mut fixture = Fixture::new(authority.clone());
    let items = fixture.items;
    let def = small_chest();

    let (second, second_container, second_slot) = {
        let world = fixture.app.world_mut();
        let container = world
            .spawn(inventory([stack(items.egg, 5), None, None, None]))
            .id();
        let player = world.spawn(empty_inventory(4)).id();
        let menu = open(world, def, vec![container, player]);
        let slot = world
            .spawn(SlotRef {
                menu,
                slot: SlotIx(0),
            })
            .id();
        (menu, container, slot)
    };
    fixture.app.update();

    let first_id = fixture.id;
    let second_id = fixture.app.world().get::<OpenMenu>(second).unwrap().id;
    assert_ne!(first_id, second_id);

    fixture.click(0, Button::Left, Modifiers::NONE);
    fixture.app.world_mut().trigger(SlotClicked {
        entity: second_slot,
        button: Button::Right,
        modifiers: Modifiers::NONE,
    });
    fixture.app.update();

    assert_eq!(
        fixture.carried(),
        Some(ItemStack::new(items.stone, 64)),
        "the first menu carries the whole stone stack"
    );
    assert_eq!(
        fixture.app.world().get::<Carried>(second).unwrap().0,
        Some(ItemStack::new(items.egg, 3)),
        "the second menu carries half the eggs"
    );
    assert_eq!(fixture.slot_of(fixture.container, 0), None);
    assert_eq!(
        fixture.slot_of(second_container, 0),
        Some(ItemStack::new(items.egg, 2))
    );

    let menus: Vec<MenuId> = authority.submitted().iter().map(|s| s.menu).collect();
    assert_eq!(menus.len(), 2);
    assert!(menus.contains(&first_id) && menus.contains(&second_id));
    assert_eq!(fixture.round_trips(), 0);
}

#[test]
fn a_second_click_inside_the_double_click_window_gathers_the_kind() {
    let authority = RecordingAuthority::new();
    let mut fixture = Fixture::new(authority.clone());
    let items = fixture.items;
    // Spread the stone so there is something left to gather.
    {
        let mut container = fixture
            .app
            .world_mut()
            .get_mut::<Inventory>(fixture.container)
            .unwrap();
        container.set(0, stack(items.stone, 10));
        container.set(2, stack(items.stone, 20));
    }

    // A frame between the two clicks, as a real double click has: collect-all
    // is gated on the cursor already holding something, and the first click
    // is what puts it there.
    fixture.click(0, Button::Left, Modifiers::NONE);
    fixture.app.update();
    fixture.click(0, Button::Left, Modifiers::NONE);
    fixture.app.update();

    let actions: Vec<ClickAction> = authority.submitted().iter().map(|s| s.action).collect();
    assert_eq!(
        actions,
        vec![
            ClickAction::Pickup {
                slot: SlotIx(0),
                button: Button::Left
            },
            ClickAction::PickupAll {
                slot: SlotIx(0),
                reverse: false
            },
        ]
    );
    assert_eq!(fixture.carried(), Some(ItemStack::new(items.stone, 30)));
    assert_eq!(fixture.slot_of(fixture.container, 2), None);
}

#[test]
fn favorites_are_mirrored_onto_the_slot_entity() {
    let mut fixture = Fixture::new(RecordingAuthority::new());
    let slot = fixture.slots[0];
    assert!(!fixture.app.world().entity(slot).contains::<Favorite>());

    fixture.act(ClickAction::Toolbar(ToolbarAction::ToggleFavorite {
        slot: SlotIx(0),
    }));
    fixture.app.update();
    assert!(fixture.app.world().entity(slot).contains::<Favorite>());
    assert!(
        fixture
            .app
            .world()
            .get::<Inventory>(fixture.container)
            .unwrap()
            .is_favorite(0)
    );

    fixture.act(ClickAction::Toolbar(ToolbarAction::ToggleFavorite {
        slot: SlotIx(0),
    }));
    fixture.app.update();
    assert!(!fixture.app.world().entity(slot).contains::<Favorite>());
}

#[test]
fn open_container_menu_fills_the_handles_the_definition_needs() {
    let mut app = minimal_ecs_app();
    let registries = test_registries();
    let items = test_items(&registries);
    let def = small_chest();

    let world = app.world_mut();
    let container = world
        .spawn(inventory([stack(items.egg, 1), None, None, None]))
        .id();
    let player_main = world.spawn(empty_inventory(4)).id();
    world.insert_resource(PlayerInventories {
        main: Some(player_main),
        ..PlayerInventories::default()
    });
    let menu = world.resource_scope(|world, mut ids: Mut<MenuIdAllocator>| {
        let player = *world.resource::<PlayerInventories>();
        let mut commands = world.commands();
        open_container_menu(
            &mut commands,
            &mut ids,
            def,
            container,
            &player,
            Actor::SURVIVAL,
        )
    });
    world.flush();

    let open = world.get::<OpenMenu>(menu).unwrap();
    assert_eq!(open.inventories, vec![container, player_main]);

    // A definition whose armor, offhand and crafting handles nobody supplied
    // gets a correctly sized inventory spawned for each.
    let player_def = Arc::new(MenuDef::player());
    let sizes = player_def.inventory_sizes();
    let result = world.spawn(empty_inventory(sizes[0])).id();
    let big_main = world.spawn(empty_inventory(sizes[1])).id();
    let hotbar = world.spawn(empty_inventory(sizes[2])).id();
    world.insert_resource(PlayerInventories {
        main: Some(big_main),
        hotbar: Some(hotbar),
        ..PlayerInventories::default()
    });
    let menu = world.resource_scope(|world, mut ids: Mut<MenuIdAllocator>| {
        let player = *world.resource::<PlayerInventories>();
        let mut commands = world.commands();
        open_container_menu(
            &mut commands,
            &mut ids,
            player_def,
            result,
            &player,
            Actor::SURVIVAL,
        )
    });
    world.flush();
    let entities = world.get::<OpenMenu>(menu).unwrap().inventories.clone();
    assert_eq!(entities.len(), sizes.len());
    assert_eq!(&entities[..3], &[result, big_main, hotbar]);
    for (entity, size) in entities.iter().zip(&sizes) {
        assert_eq!(
            world.get::<Inventory>(*entity).unwrap().len(),
            *size,
            "handle backing {entity} must hold {size} slots"
        );
    }
}

/// A host-side `SetSlot` is the machine simulation moving its own output: it
/// writes the backing inventory and tells the slot entity, without a click,
/// a prediction or a round trip (Phase 6 contract section 0).
#[test]
fn a_set_slot_writes_the_inventory_and_the_slot_entity() {
    let mut fixture = Fixture::new(RecordingAuthority::new());
    let moved = ItemStack::new(fixture.items.egg, 3);
    fixture.app.world_mut().trigger(slotted_ecs::SetSlot {
        entity: fixture.menu,
        slot: SlotIx(4),
        stack: Some(moved.clone()),
    });
    fixture.app.update();

    assert_eq!(
        fixture.slot_of(fixture.player, 0),
        Some(moved.clone()),
        "the backing inventory holds it"
    );
    assert!(
        fixture.app.world().resource::<SlotChanges>().0.contains(&(
            fixture.slots[4],
            SlotIx(4),
            Some(moved)
        )),
        "SlotChanged fires on the registered slot entity"
    );
    assert_eq!(
        fixture.app.world().resource::<PendingRoundTrips>().0,
        0,
        "a host write bypasses the authority, so nothing is pending"
    );
}

/// A slot the menu does not have is a mistake in the caller: the write is
/// dropped with a warning rather than growing an inventory.
#[test]
fn a_set_slot_for_an_unknown_slot_changes_nothing() {
    let mut fixture = Fixture::new(RecordingAuthority::new());
    let before = fixture.app.world().resource::<SlotChanges>().0.len();
    fixture.app.world_mut().trigger(slotted_ecs::SetSlot {
        entity: fixture.menu,
        slot: SlotIx(99),
        stack: Some(ItemStack::new(fixture.items.egg, 1)),
    });
    fixture.app.update();
    assert_eq!(
        fixture.app.world().resource::<SlotChanges>().0.len(),
        before,
        "nothing was told about a slot that does not exist"
    );
}

// ------------------------------------------------------- refusal and resync

/// The snapshot the authority forces a refused client back to: the container
/// untouched, nothing carried, and a state id far ahead of the client's.
fn authoritative(items: &TestItems) -> MenuSnapshot {
    let mut inventories = Inventories::new();
    inventories.push(slotted_model::Inventory::from_slots([
        stack(items.stone, 64),
        stack(items.egg, 10),
        None,
        None,
    ]));
    inventories.push(slotted_model::Inventory::new(4));
    MenuSnapshot {
        inventories,
        state: MenuState {
            state_id: 42,
            properties: vec![7],
            ..MenuState::new(&small_chest())
        },
    }
}

#[test]
fn a_refused_submission_asks_the_authority_for_a_snapshot() {
    let authority = RecordingAuthority::new();
    let mut fixture = Fixture::new(authority.clone());
    authority.answer_resync_with(authoritative(&fixture.items));
    authority.fail_with(Some(slotted_model::AuthorityError::Rejected(
        slotted_model::ClickError::NotAllowed,
    )));

    // The client predicts the pickup, the authority refuses it.
    fixture.click(0, Button::Left, Modifiers::default());
    fixture.app.update();

    assert_eq!(
        authority.resync_requests(),
        vec![fixture.id],
        "the refusal is answered by asking for the truth, not by guessing"
    );
    assert_eq!(fixture.carried(), None, "the prediction was rolled back");
    assert_eq!(
        fixture.slot_of(fixture.container, 0),
        Some(stack(fixture.items.stone, 64).unwrap())
    );
    assert_eq!(fixture.state_id(), 42, "the authority's state id wins");
    assert_eq!(
        fixture.round_trips(),
        0,
        "the request counted as a round trip and the resync closed it"
    );
}

#[test]
fn a_refusal_an_authority_cannot_answer_redraws_from_local_state() {
    let authority = RecordingAuthority::new();
    let mut fixture = Fixture::new(authority.clone());
    authority.fail_with(Some(slotted_model::AuthorityError::Disconnected));

    fixture.click(0, Button::Left, Modifiers::default());
    fixture.app.update();

    assert_eq!(authority.resync_requests(), vec![fixture.id]);
    assert_eq!(
        fixture.carried(),
        stack(fixture.items.stone, 64),
        "with no snapshot to apply the local prediction stands"
    );
    assert_eq!(
        fixture.slot_changes().len(),
        9,
        "the predicted slot, then every slot of the menu re-emitted"
    );
    assert_eq!(fixture.round_trips(), 0);
}

// ------------------------------------------------------------ ghost hints

/// A menu whose slot 2 is a ghost: it shows an item without holding one.
fn ghost_chest() -> Arc<MenuDef> {
    let mut def = MenuDef::clone(&small_chest());
    def.slots[2].behaviour = SlotBehaviour::Ghost;
    Arc::new(def)
}

#[test]
fn a_ghost_slot_shows_a_hint_that_no_inventory_holds() {
    let authority = RecordingAuthority::new();
    let mut fixture = Fixture::with_def(authority, ghost_chest());
    let egg = stack(fixture.items.egg, 1).unwrap();

    // Pick up the eggs, then click the ghost slot: the hint is set and the
    // carried stack is not consumed.
    fixture.click(1, Button::Left, Modifiers::default());
    fixture.app.update();
    fixture.clear_slot_changes();
    fixture.click(2, Button::Left, Modifiers::default());
    fixture.app.update();

    assert_eq!(
        fixture.carried(),
        stack(fixture.items.egg, 10),
        "a hint costs nothing"
    );
    assert_eq!(
        fixture.slot_changes(),
        vec![(SlotIx(2), Some(egg.clone()))],
        "the slot entity is told what to draw"
    );
    assert_eq!(
        fixture.slot_of(fixture.container, 2),
        None,
        "and no inventory holds it"
    );
    assert_eq!(fixture.hint(SlotIx(2)), Some(egg));

    // Clicking it with an empty hand clears the hint again.
    fixture.act(ClickAction::Pickup {
        slot: SlotIx(SlotIx::OUTSIDE.0),
        button: Button::Left,
    });
    fixture.app.update();
    fixture.clear_slot_changes();
    fixture.click(2, Button::Left, Modifiers::default());
    fixture.app.update();
    assert_eq!(fixture.slot_changes(), vec![(SlotIx(2), None)]);
    assert_eq!(fixture.hint(SlotIx(2)), None);
}

#[test]
fn set_slot_on_a_ghost_writes_the_hint_not_the_inventory() {
    let authority = RecordingAuthority::new();
    let mut fixture = Fixture::with_def(authority, ghost_chest());
    let egg = ItemStack::new(fixture.items.egg, 1);
    fixture.app.world_mut().trigger(slotted_ecs::SetSlot {
        entity: fixture.menu,
        slot: SlotIx(2),
        stack: Some(egg.clone()),
    });
    fixture.app.update();

    assert_eq!(fixture.hint(SlotIx(2)), Some(egg.clone()));
    assert_eq!(fixture.slot_of(fixture.container, 2), None);
    assert!(fixture.app.world().resource::<SlotChanges>().0.contains(&(
        fixture.slots[2],
        SlotIx(2),
        Some(egg)
    )));
}
