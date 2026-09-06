//! Adversarial review of `slotted-net`: the cases a happy-path test does not
//! reach.
//!
//! Four questions, in order of how badly a game breaks when the answer is
//! wrong.
//!
//! 1. A client whose picture is two state ids behind. Does the server notice,
//!    and does the client end up on the server's numbers?
//! 2. A server that restarts mid-session, so its state ids begin again at
//!    zero while the client is still counting up. Does the session recover
//!    without anyone restarting the client?
//! 3. A client that sends a click naming a slot the menu has not got. Both
//!    the action and the *predicted delta* are attacker-controlled, so this
//!    is the crate's whole untrusted surface.
//! 4. A link losing three messages in ten. Does the pair still converge, and
//!    is nothing created or destroyed on the way?
//!
//! `loopback.rs` covers the same pair on a link that mostly works. This file
//! deliberately duplicates the small harness rather than sharing one, so that
//! a change made to make one file's scenario pass cannot quietly move the
//! ground under the other's.

#![allow(clippy::unwrap_used)]

use std::collections::BTreeMap;

use pretty_assertions::assert_eq;
use slotted_model::{
    Actor, Authority, AuthorityEvent, Button, ClickAction, Delta, DragKind, DragStage, Inventories,
    ItemId, ItemStack, LookupCtx, MenuDef, MenuId, MenuSnapshot, MenuState, Namespaced, SlotIx,
    apply_click, slot_view,
};
use slotted_net::{
    ClientEnd, ClientMessage, Conditions, Loopback, MenuServer, Outcome, PeerId, RemoteAuthority,
    ServerEnd, ServerMessage, Transport,
};

// --------------------------------------------------------------- the world

const STONE: ItemId = ItemId(1);
const EGG: ItemId = ItemId(2);
const MENU: MenuId = MenuId(1);
/// A menu id no server in this file ever opens.
const ABSENT: MenuId = MenuId(99);

struct Items;

impl LookupCtx for Items {
    fn max_stack(&self, id: ItemId) -> u32 {
        match id {
            STONE => 64,
            EGG => 16,
            _ => 1,
        }
    }

    fn has_tag(&self, _id: ItemId, _tag: &Namespaced) -> bool {
        false
    }
}

#[allow(clippy::unnecessary_wraps)]
fn stack(id: ItemId, count: u32) -> Option<ItemStack> {
    Some(ItemStack::new(id, count))
}

/// Four container slots over four player slots: menu slots `0..4` are the
/// chest and `4..8` the player.
fn def() -> MenuDef {
    let mut def = MenuDef::new();
    let container = def.add_slots(MenuDef::CONTAINER, 4, slotted_model::SlotBehaviour::Normal);
    let player = def.add_slots(
        MenuDef::PLAYER_MAIN,
        4,
        slotted_model::SlotBehaviour::Normal,
    );
    def.hotbar = player.iter().collect();
    def.quick_move = slotted_model::RoutingTable::new()
        .route(container, [MenuDef::PLAYER_MAIN])
        .route(player, [MenuDef::CONTAINER]);
    def.listring = vec![MenuDef::CONTAINER, MenuDef::PLAYER_MAIN];
    def
}

fn starting_inventories() -> Inventories {
    let def = def();
    let mut inventories = Inventories::for_menu(&def);
    inventories[MenuDef::CONTAINER].set(0, stack(STONE, 64));
    inventories[MenuDef::CONTAINER].set(1, stack(EGG, 10));
    inventories[MenuDef::PLAYER_MAIN].set(2, stack(STONE, 5));
    for inventory in inventories.iter_mut() {
        inventory.clear_changed();
    }
    inventories
}

fn tally(inventories: &Inventories, carried: Option<&ItemStack>) -> BTreeMap<u32, u64> {
    let mut out: BTreeMap<u32, u64> = BTreeMap::new();
    for (_, inventory) in inventories.iter() {
        for stack in inventory.slots().iter().flatten() {
            *out.entry(stack.id.0).or_default() += u64::from(stack.count);
        }
    }
    if let Some(carried) = carried {
        *out.entry(carried.id.0).or_default() += u64::from(carried.count);
    }
    out
}

// -------------------------------------------------------------- the client

/// A client: its own copy of the menu, its authority, and what the server
/// made it change.
struct Client {
    def: MenuDef,
    inventories: Inventories,
    state: MenuState,
    authority: RemoteAuthority<ClientEnd>,
    corrections: Vec<(SlotIx, Option<ItemStack>)>,
    resyncs: usize,
    peer: PeerId,
}

impl Client {
    fn new(link: &Loopback) -> Self {
        let end = link.add_client();
        let peer = end.peer();
        Self {
            def: def(),
            inventories: starting_inventories(),
            state: MenuState::new(&def()),
            authority: RemoteAuthority::new(end).retry_after(2),
            corrections: Vec::new(),
            resyncs: 0,
            peer,
        }
    }

    /// Predicts `action` locally and submits it. `false` when the client's
    /// own model refused it, in which case nothing went on the wire.
    fn click(&mut self, action: ClickAction) -> bool {
        let outcome = apply_click(
            &self.def,
            &mut self.inventories,
            &mut self.state,
            action,
            &Actor::SURVIVAL,
            &Items,
        );
        let Ok(delta) = outcome else {
            return false;
        };
        self.authority.submit(MENU, action, &delta).unwrap();
        true
    }

    fn reconcile(&mut self) -> Vec<AuthorityEvent> {
        let events = self.authority.poll();
        for event in &events {
            match event {
                AuthorityEvent::Ack { .. } | AuthorityEvent::Property { .. } => {}
                AuthorityEvent::Slot { slot, stack, .. } => self.write(*slot, stack.clone()),
                AuthorityEvent::Resync { snapshot, .. } => {
                    self.resyncs += 1;
                    self.overwrite(snapshot);
                }
            }
        }
        events
    }

    fn write(&mut self, slot: SlotIx, stack: Option<ItemStack>) {
        if self.slot(slot) == stack {
            return;
        }
        self.corrections.push((slot, stack.clone()));
        let Some(sd) = self.def.slot(slot).cloned() else {
            return;
        };
        if sd.behaviour.is_ghost() {
            self.state.set_hint(slot, stack);
        } else {
            self.inventories[sd.source].set(usize::from(sd.index), stack);
        }
    }

    fn overwrite(&mut self, snapshot: &MenuSnapshot) {
        let before = self.all_slots();
        self.inventories = snapshot.inventories.clone();
        self.state = snapshot.state.clone();
        for (index, old) in before.into_iter().enumerate() {
            let ix = SlotIx(u16::try_from(index).unwrap());
            let new = self.slot(ix);
            if old != new {
                self.corrections.push((ix, new));
            }
        }
    }

    fn slot(&self, slot: SlotIx) -> Option<ItemStack> {
        slot_view(&self.def, &self.inventories, &self.state, slot).cloned()
    }

    fn all_slots(&self) -> Vec<Option<ItemStack>> {
        (0..self.def.slots.len())
            .map(|i| self.slot(SlotIx(u16::try_from(i).unwrap())))
            .collect()
    }

    fn tally(&self) -> BTreeMap<u32, u64> {
        tally(&self.inventories, self.state.carried.as_ref())
    }
}

// -------------------------------------------------------------- the server

struct World {
    link: Loopback,
    server: MenuServer,
    end: ServerEnd,
}

impl World {
    fn new(seed: u64) -> Self {
        let link = Loopback::new(seed);
        let end = link.server();
        let mut server = MenuServer::new();
        server.open(MENU, def(), starting_inventories(), Actor::SURVIVAL);
        Self { link, server, end }
    }

    fn join(&mut self) -> Client {
        let client = Client::new(&self.link);
        self.server.add_viewer(MENU, client.peer);
        client
    }

    fn pump(&mut self) -> Vec<Outcome> {
        self.server.pump(&self.end, &Items)
    }

    /// `rounds` of: the link delivers, the server answers, the link delivers
    /// again, every client reconciles.
    fn settle_for(&mut self, rounds: usize, clients: &mut [&mut Client]) -> Vec<Outcome> {
        let mut outcomes = Vec::new();
        for _ in 0..rounds {
            self.link.tick();
            outcomes.extend(self.pump());
            self.link.tick();
            for client in clients.iter_mut() {
                client.reconcile();
            }
        }
        outcomes
    }

    fn settle(&mut self, clients: &mut [&mut Client]) -> Vec<Outcome> {
        self.settle_for(8, clients)
    }

    fn slot(&self, slot: u16) -> Option<ItemStack> {
        self.server.slot(MENU, SlotIx(slot))
    }

    fn tally(&self) -> BTreeMap<u32, u64> {
        let snapshot = self.server.snapshot(MENU).unwrap();
        tally(&snapshot.inventories, snapshot.state.carried.as_ref())
    }
}

fn left(slot: u16) -> ClickAction {
    ClickAction::Pickup {
        slot: SlotIx(slot),
        button: Button::Left,
    }
}

// ---------------------------------------------------------------- 1. stale

/// A client two state ids behind is corrected, and lands on the server's
/// numbers rather than on its own.
///
/// The drift is made the way it happens in a game: a second player moves
/// things while the first player's screen is not being told about it. Here
/// the second client's `SetSlot` messages are simply never delivered, which
/// is what a dropped packet on a link with no retransmission for slot writes
/// amounts to.
#[test]
fn a_client_two_state_ids_stale_is_corrected_onto_the_servers_numbers() {
    let mut world = World::new(11);
    let mut stale = world.join();
    let mut mover = world.join();

    // The mover empties the chest's stone into the player inventory, two
    // clicks, so the server's state id advances by two.
    assert!(mover.click(left(0)));
    assert!(mover.click(left(4)));
    let outcomes = world.settle(&mut [&mut mover]);
    assert_eq!(outcomes, vec![Outcome::Acked, Outcome::Acked]);
    assert_eq!(world.server.state_id(MENU), Some(2));

    // The stale client never reconciled, so it still believes it is at zero
    // and that slot 0 holds the stone.
    assert_eq!(stale.state.state_id, 0);
    assert_eq!(stale.slot(SlotIx(0)), stack(STONE, 64));
    assert_eq!(world.slot(0), None, "the server knows better");

    // It clicks on what it thinks is there. The action is legal against the
    // server's picture too -- slot 0 is empty and an empty hand on an empty
    // slot is `NothingToDo` -- so what saves the client is the state id, not
    // the action being impossible.
    let predicted_stack = stale.slot(SlotIx(0));
    assert!(stale.click(left(0)));
    assert_eq!(
        stale.state.carried, predicted_stack,
        "the client shows itself holding the stone it thinks it picked up"
    );

    let outcomes = world.settle(&mut [&mut stale]);
    assert!(
        outcomes.iter().any(|o| matches!(
            o,
            Outcome::Corrected | Outcome::Refused(slotted_model::ClickError::NothingToDo)
        )),
        "a two-id drift must not be acked: {outcomes:?}"
    );
    assert_eq!(
        stale.state.carried, None,
        "and the client is put back to holding nothing"
    );
    assert_eq!(
        stale.all_slots(),
        {
            let snapshot = world.server.snapshot(MENU).unwrap();
            let def = def();
            (0..def.slots.len())
                .map(|i| {
                    slot_view(
                        &def,
                        &snapshot.inventories,
                        &snapshot.state,
                        SlotIx(u16::try_from(i).unwrap()),
                    )
                    .cloned()
                })
                .collect::<Vec<_>>()
        },
        "the client ends on exactly the server's slots"
    );
    assert_eq!(
        stale.state.state_id,
        world.server.state_id(MENU).unwrap(),
        "and on the server's state id, not a number of its own"
    );
    assert_eq!(stale.authority.in_flight(), 0, "nothing left waiting");
}

// -------------------------------------------------------------- 2. restart

/// A server restarts mid-session: same menu id, fresh state ids beginning at
/// zero, no memory of any client's sequence numbers. The client is still
/// counting up from where it was and does not know anything happened.
///
/// This is the case a `state_id` alone cannot survive, because after a
/// restart the server's id is *lower* than the client's rather than higher,
/// and it is why the protocol carries a per-click `seq` as well.
#[test]
fn a_server_restart_with_fresh_state_ids_forces_one_full_resync_and_recovers() {
    let mut world = World::new(12);
    let mut client = world.join();

    for slot in [0, 4, 1] {
        assert!(client.click(left(slot)));
        world.settle(&mut [&mut client]);
    }
    assert_eq!(client.state.state_id, 3);
    assert_eq!(world.server.state_id(MENU), Some(3));
    let before = client.resyncs;

    // The restart. Everything the server knew about this session is gone.
    world.server = MenuServer::new();
    world
        .server
        .open(MENU, def(), starting_inventories(), Actor::SURVIVAL);
    world.server.add_viewer(MENU, client.peer);
    assert_eq!(world.server.state_id(MENU), Some(0));

    // The client's next click carries state id 3 and a sequence number the
    // fresh server has never seen.
    assert!(client.click(left(1)));
    let outcomes = world.settle(&mut [&mut client]);

    assert!(
        outcomes.contains(&Outcome::Corrected),
        "a client ahead of the server must be corrected, not acked: {outcomes:?}"
    );
    assert_eq!(
        client.resyncs,
        before + 1,
        "exactly one full container comes back, not one per click for ever"
    );
    assert_eq!(
        client.state.state_id,
        world.server.state_id(MENU).unwrap(),
        "the client adopts the restarted server's numbering"
    );
    assert_eq!(client.all_slots().len(), def().slots.len());
    assert_eq!(
        client.tally(),
        world.tally(),
        "and holds exactly the restarted server's items"
    );

    // And the session keeps working: the very next click is acked outright.
    let acked = world.settle(&mut [&mut client]);
    assert!(acked.iter().all(|o| *o != Outcome::UnknownMenu));
    assert!(client.click(left(0)));
    let outcomes = world.settle(&mut [&mut client]);
    assert_eq!(
        outcomes,
        vec![Outcome::Acked],
        "once resynced the client predicts correctly again: {outcomes:?}"
    );
    assert_eq!(client.authority.in_flight(), 0);
}

// ------------------------------------------------------------- 3. hostile

/// Everything a hostile client controls, pointed at slots that do not exist.
///
/// The action and the predicted [`Delta`] both come off the wire, so both are
/// attacker input. The server must refuse each one, keep its own items
/// untouched, and never panic. A panic here is a remote denial of service on
/// a real server, which is why this is a whole test rather than a line in
/// another one.
/// A delta claiming slots far outside the menu, so that even the comparison
/// the server makes against its own delta is fed nonsense.
fn poisoned_delta() -> Delta {
    Delta {
        slots: vec![
            (SlotIx(0), stack(STONE, u32::MAX)),
            (SlotIx(60000), stack(EGG, 4096)),
            (SlotIx::OUTSIDE, None),
        ],
        carried: stack(STONE, u32::MAX),
        dropped: vec![ItemStack::new(EGG, u32::MAX)],
        state_id: u32::MAX,
        taken_from_output: Some(SlotIx(60000)),
    }
}

/// Every shape of "a slot that is not there" the wire format can carry.
fn hostile_actions() -> Vec<ClickAction> {
    vec![
        // Straight past the end of the slot table.
        left(60000),
        left(u16::MAX - 1),
        ClickAction::Pickup {
            slot: SlotIx::OUTSIDE,
            button: Button::Left,
        },
        ClickAction::QuickMove {
            slot: SlotIx(60000),
        },
        ClickAction::Swap {
            slot: SlotIx(60000),
            hotbar: 0,
        },
        ClickAction::Swap {
            slot: SlotIx(0),
            hotbar: 250,
        },
        ClickAction::Throw {
            slot: SlotIx(60000),
            all: true,
        },
        ClickAction::PickupAll {
            slot: SlotIx(60000),
            reverse: false,
        },
        // A drag whose painted slot is outside, started legally.
        ClickAction::Drag {
            stage: DragStage::Start,
            kind: DragKind::Left,
            slot: None,
        },
        ClickAction::Drag {
            stage: DragStage::Add,
            kind: DragKind::Left,
            slot: Some(SlotIx(60000)),
        },
        // An `Add` with no slot at all, which the wire format allows.
        ClickAction::Drag {
            stage: DragStage::Add,
            kind: DragKind::Left,
            slot: None,
        },
        ClickAction::Drag {
            stage: DragStage::End,
            kind: DragKind::Left,
            slot: None,
        },
    ]
}

#[test]
fn a_click_naming_a_slot_outside_the_menu_is_refused_without_panicking() {
    let mut world = World::new(13);
    let client = world.join();
    let end = client.authority.transport().clone();
    let before = world.server.snapshot(MENU).unwrap();
    let poisoned = poisoned_delta();

    for (seq, action) in hostile_actions().into_iter().enumerate() {
        end.send(
            PeerId::SERVER,
            ClientMessage::ClickContainer {
                menu: MENU,
                state_id: 0,
                seq: u32::try_from(seq).unwrap(),
                action,
                predicted: poisoned.clone(),
            },
        )
        .unwrap();
        world.link.tick();
        let outcomes = world.pump();
        assert!(
            outcomes
                .iter()
                .all(|o| matches!(o, Outcome::Refused(_) | Outcome::Corrected)),
            "{action:?} should be refused or corrected, not accepted: {outcomes:?}"
        );
    }

    // A click and a resync request about a menu this server has not got.
    for message in [
        ClientMessage::RequestResync { menu: ABSENT },
        ClientMessage::ClickContainer {
            menu: ABSENT,
            state_id: 0,
            seq: 900,
            action: left(0),
            predicted: poisoned.clone(),
        },
    ] {
        end.send(PeerId::SERVER, message).unwrap();
        world.link.tick();
        assert_eq!(world.pump(), vec![Outcome::UnknownMenu]);
    }

    let after = world.server.snapshot(MENU).unwrap();
    assert_eq!(
        tally(&before.inventories, before.state.carried.as_ref()),
        tally(&after.inventories, after.state.carried.as_ref()),
        "nothing hostile moved an item"
    );
    assert_eq!(
        before.state.carried, after.state.carried,
        "and nothing ended up in the cursor"
    );

    // The link is still usable afterwards: a legal click from the same peer
    // is still served, so a refusal is not a disconnect.
    let mut client = client;
    assert!(client.click(left(0)));
    let outcomes = world.settle(&mut [&mut client]);
    assert!(
        outcomes.contains(&Outcome::Acked) || outcomes.contains(&Outcome::Corrected),
        "the session survives the hostile burst: {outcomes:?}"
    );
}

/// A slot the server *has* but that is `Disabled`, and a hotbar index past
/// the end of the hotbar. These are inside the table and so take a different
/// path from the out-of-range ones above.
#[test]
fn a_click_on_a_disabled_slot_is_refused_and_changes_nothing() {
    let mut world = World::new(14);
    let mut disabled = def();
    disabled.slots[2].behaviour = slotted_model::SlotBehaviour::Disabled;
    world.server.close(MENU);
    world
        .server
        .open(MENU, disabled, starting_inventories(), Actor::SURVIVAL);
    let client = world.join();
    let end = client.authority.transport().clone();
    let before = world.tally();

    end.send(
        PeerId::SERVER,
        ClientMessage::ClickContainer {
            menu: MENU,
            state_id: 0,
            seq: 0,
            action: left(2),
            predicted: Delta::default(),
        },
    )
    .unwrap();
    world.link.tick();
    let outcomes = world.pump();
    assert!(
        matches!(outcomes.as_slice(), [Outcome::Refused(_)]),
        "a disabled slot is refused: {outcomes:?}"
    );
    assert_eq!(world.tally(), before);
    assert_eq!(
        world.server.state_id(MENU),
        Some(0),
        "and moves no state id"
    );
}

// ----------------------------------------------------------------- 4. loss

/// Three messages in ten thrown away, a few hundred random clicks, and the
/// two sides still end up holding the same items in the same slots.
///
/// Conservation is checked at the end rather than per click: the point is
/// that a lost ack, a lost container and a lost slot write between them
/// cannot invent or destroy an item, however they interleave.
#[test]
fn thirty_percent_loss_still_converges_and_conserves() {
    let mut world = World::new(2024);
    let mut client = world.join();
    world.link.set_conditions(Conditions::lossy(30));

    let start = world.tally();
    let actions: Vec<ClickAction> = (0..300)
        .map(|i| {
            let slot = u16::try_from(i % 8).unwrap();
            match i % 5 {
                0 => left(slot),
                1 => ClickAction::Pickup {
                    slot: SlotIx(slot),
                    button: Button::Right,
                },
                2 => ClickAction::QuickMove { slot: SlotIx(slot) },
                3 => ClickAction::Swap {
                    slot: SlotIx(slot),
                    hotbar: u8::try_from(i % 4).unwrap(),
                },
                _ => ClickAction::PickupAll {
                    slot: SlotIx(slot),
                    reverse: false,
                },
            }
        })
        .collect();

    for action in actions {
        client.click(action);
        world.settle_for(2, &mut [&mut client]);
    }

    assert!(
        world.link.dropped() > 100,
        "the link really did lose messages: {}",
        world.link.dropped()
    );

    // Now let it recover with the loss still on. Retransmission is the client's
    // only tool, so this needs enough rounds for the retry timer to fire
    // repeatedly, not a fixed handful.
    world.link.set_conditions(Conditions::PERFECT);
    world.settle_for(64, &mut [&mut client]);

    assert_eq!(
        client.authority.in_flight(),
        0,
        "every click was eventually answered"
    );
    assert_eq!(
        client.state.state_id,
        world.server.state_id(MENU).unwrap(),
        "the two agree on how many times the menu changed"
    );
    assert_eq!(
        client.all_slots(),
        {
            let snapshot = world.server.snapshot(MENU).unwrap();
            let def = def();
            (0..def.slots.len())
                .map(|i| {
                    slot_view(
                        &def,
                        &snapshot.inventories,
                        &snapshot.state,
                        SlotIx(u16::try_from(i).unwrap()),
                    )
                    .cloned()
                })
                .collect::<Vec<_>>()
        },
        "and on what is in every slot"
    );
    assert_eq!(
        world.tally(),
        start,
        "and the server conserved every item across the whole run"
    );
    assert_eq!(client.tally(), start, "as did the client");
}

/// A resync request that the link eats must be asked for again.
///
/// `Transport::send` reports `Ok` for a message the link then drops -- that is
/// what a real unreliable transport does, and the loopback models it. The
/// client marks the menu as awaiting a resync when the *send* succeeds, so if
/// nothing re-asks, a client whose only outstanding request was lost waits for
/// a container that is never coming, and no amount of polling gets it back.
#[test]
fn a_resync_request_the_link_eats_is_asked_for_again() {
    let world = World::new(15);
    let client = Client::new(&world.link);
    let end = client.authority.transport().clone();

    // Lose everything, ask, then let the link work again.
    world.link.set_conditions(Conditions::lossy(100));
    assert!(matches!(
        client.authority.request_resync(MENU),
        Ok(slotted_model::ResyncRequest::Pending)
    ));
    world.link.tick();
    assert_eq!(
        world.link.server().poll().len(),
        0,
        "the request was eaten, as this test set the link up to do"
    );
    assert!(world.link.dropped() >= 1);

    world.link.set_conditions(Conditions::PERFECT);
    let mut asked = 0;
    for _ in 0..64 {
        world.link.tick();
        asked += world
            .link
            .server()
            .poll()
            .into_iter()
            .filter(|(_, m)| matches!(m, ClientMessage::RequestResync { .. }))
            .count();
        client.authority.poll();
    }
    let _ = end;

    assert!(
        asked > 0,
        "a lost resync request is never repeated, so the client waits for a \
         container that will never arrive"
    );
}

/// An ack for a submission the client does not recognise is reported *and*
/// answered with a resync request, so a client that has forgotten why it is
/// out of date still gets the truth.
#[test]
fn an_ack_for_an_unknown_submission_still_asks_for_the_container() {
    let world = World::new(16);
    let client = Client::new(&world.link);
    let server_end = world.link.server();

    server_end
        .send(
            client.peer,
            ServerMessage::Ack {
                menu: MENU,
                state_id: 7,
                seq: 4242,
            },
        )
        .unwrap();
    world.link.tick();

    let events = client.authority.poll();
    assert!(
        events
            .iter()
            .any(|e| matches!(e, AuthorityEvent::Ack { state_id: 7, .. })),
        "the ack is reported so the caller's round-trip count is not stuck: {events:?}"
    );
    world.link.tick();
    let asked: Vec<ClientMessage> = server_end
        .poll()
        .into_iter()
        .map(|(_, m)| m)
        .filter(|m| matches!(m, ClientMessage::RequestResync { menu } if *menu == MENU))
        .collect();
    assert_eq!(
        asked.len(),
        1,
        "and one container is asked for, not none and not one a frame"
    );
}
