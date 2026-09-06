//! `SlottedPlugins::server()` is a claim, and this is the assertion.
//!
//! The claim is that a dedicated server built on the facade runs a real
//! authority loop with no UI anywhere: mods load, the schedule ticks, and a
//! `MenuServer` pumped from that schedule answers a client that does have a
//! screen. `just server-check` proves the *dependency graph* has no `bevy_ui`
//! in it, on both targets, which is a different question from whether the
//! thing runs.
//!
//! The file compiles only in the profile it is about --
//! `--no-default-features --features server` -- because the assertion is
//! partly about what is *absent*. With `ui` also enabled, `SlottedPlugins`
//! adds `SlottedUiPlugin` and the group stops being a server stack, so
//! running these under `--all-features` would prove the opposite of what they
//! say. `just server-check` runs them; see the recipe.

#![cfg(all(feature = "server", not(feature = "ui")))]
#![allow(clippy::unwrap_used)]

use bevy::prelude::*;

use slotted::SlottedPlugins;
use slotted::model::{
    Actor, Authority, AuthorityEvent, Button, ClickAction, Inventories, Inventory, ItemId,
    ItemStack, LookupCtx, MenuDef, MenuId, MenuState, Namespaced, SlotIx, apply_click,
};
use slotted::net::{Loopback, MenuServer, PeerId, RemoteAuthority, ServerEnd};

const STONE: ItemId = ItemId(1);

#[derive(Debug)]
struct Items;

impl LookupCtx for Items {
    fn max_stack(&self, _id: ItemId) -> u32 {
        64
    }

    fn has_tag(&self, _id: ItemId, _tag: &Namespaced) -> bool {
        false
    }
}

/// The authority, living in the server's world the way a dedicated server
/// holds it: pumped from the schedule, not from the test body.
#[derive(Resource)]
struct Authoritative {
    server: MenuServer,
    end: ServerEnd,
    /// Ticks the pump ran, so the test can prove the schedule drove it rather
    /// than the test having driven it by hand.
    pumps: u32,
}

/// What a dedicated server's tick is: hand everything that arrived to the
/// authority and let it answer.
fn pump_the_authority(mut authority: ResMut<Authoritative>) {
    let Authoritative { server, end, pumps } = &mut *authority;
    server.pump(end, &Items);
    *pumps += 1;
}

/// A chest with four slots and the player's own four.
fn def() -> MenuDef {
    let mut def = MenuDef::new();
    def.add_slots(MenuDef::CONTAINER, 4, slotted::model::SlotBehaviour::Normal);
    def.add_slots(
        MenuDef::PLAYER_MAIN,
        4,
        slotted::model::SlotBehaviour::Normal,
    );
    def
}

/// One click, from a client with no server plugins to a server with no client
/// plugins, driven by the server app's own schedule.
///
/// This is the whole shape of a dedicated server in one test: the app runs
/// frames, the pump answers, and the client's `Authority` reports the ack the
/// prediction loop is waiting for. Nothing here mentions a widget, a window
/// or a renderer, and in this build none of them are compiled.
#[test]
fn the_server_stack_answers_a_click_from_a_client_with_no_ui_present() {
    let link = Loopback::new(1);
    let alice = PeerId(1);
    let client_end = link.client(alice);

    let mut server = MenuServer::new();
    let chest = server.add_shared(Inventory::from_slots([
        Some(ItemStack::new(STONE, 32)),
        None,
        None,
        None,
    ]));
    let pockets = server.add_private(alice, Inventory::new(4));
    let menu = MenuId(1);
    server
        .open(menu, alice, def(), vec![chest, pockets], Actor::SURVIVAL)
        .unwrap();

    // `MinimalPlugins` is deliberately not added on top: `ServerBevyPlugins`
    // already carries the task pool, the clock, the states and the runner, and
    // Bevy panics on a plugin added twice. `server()` is the whole stack a
    // dedicated server adds, which is what makes it worth asserting on.
    let mut app = App::new();
    app.add_plugins(SlottedPlugins::server())
        .insert_resource(Authoritative {
            server,
            end: link.server(),
            pumps: 0,
        })
        .add_systems(Update, pump_the_authority);

    // The client is an ordinary `RemoteAuthority`, with no app around it.
    let client = RemoteAuthority::new(client_end);
    let mut inventories = Inventories::new();
    inventories.push(Inventory::from_slots([
        Some(ItemStack::new(STONE, 32)),
        None,
        None,
        None,
    ]));
    inventories.push(Inventory::new(4));
    let def = def();
    let mut state = MenuState::new(&def);
    let action = ClickAction::Pickup {
        slot: SlotIx(0),
        button: Button::Left,
    };
    let delta = apply_click(
        &def,
        &mut inventories,
        &mut state,
        action,
        &Actor::SURVIVAL,
        &Items,
    )
    .unwrap();
    client.submit(menu, action, &delta).unwrap();

    // Frames of the server app, not hand-driven pumps.
    let mut events = Vec::new();
    for _ in 0..8 {
        link.tick();
        app.update();
        link.tick();
        events.extend(client.poll());
    }

    assert!(
        app.world().resource::<Authoritative>().pumps >= 8,
        "the server's own schedule ran the authority"
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e, AuthorityEvent::Ack { menu: m, state_id: 1 } if *m == menu)),
        "the client's prediction was acked by the server app: {events:?}"
    );
    assert_eq!(
        client.in_flight(),
        0,
        "and the round trip came back down again"
    );

    let authority = app.world().resource::<Authoritative>();
    assert_eq!(
        authority.server.slot(menu, SlotIx(0)),
        None,
        "the stone left the chest on the authoritative side"
    );
    assert_eq!(
        authority.server.snapshot(menu).unwrap().state.carried,
        Some(ItemStack::new(STONE, 32)),
        "and is on the session's cursor"
    );
}

/// The mod lifecycle is compiled into this build and the UI half of it is
/// not, which is the split finding 7 was about.
///
/// A server that could not load mods would not be a server; one that pulled
/// `bevy_ui` in to do it would not be this one.
#[test]
fn the_server_stack_carries_the_pack_lifecycle_and_no_ui_registries() {
    let mut app = App::new();
    app.add_plugins(SlottedPlugins::server());
    app.update();

    let world = app.world();
    assert!(
        world
            .get_resource::<slotted::packs::lifecycle::ScriptLogs>()
            .is_some(),
        "the pack lifecycle is running"
    );
    assert!(
        world
            .get_resource::<State<slotted::packs::lifecycle::ModStage>>()
            .is_some(),
        "with its stage machine, which is what `run_all` walks"
    );
    assert!(
        world
            .get_resource::<slotted::packs::lifecycle::ScriptHost>()
            .is_some(),
        "with a script runtime, so control scripts answer events server-side"
    );
}
