//! Sessions over a shared store: who may touch what, and what a retried click
//! is answered with.
//!
//! Three review findings meet here.
//!
//! 1. The server used to act on any message that named a menu it had, without
//!    asking whether the sender had any business with it. Every message kind
//!    is now checked, and every one of them is checked here with a peer that
//!    does not own the session.
//! 2. One `MenuState`, one actor and one set of inventories used to be shared
//!    by every viewer, so two players at one chest shared a cursor and a drag,
//!    while two menus over one chest each got a private copy of it. Sessions
//!    and a container store separate the two: the chest is one inventory, the
//!    cursor is not.
//! 3. A retransmitted click used to be answered with an ack whatever the
//!    original answer had been, so a lost correction was never repeated and
//!    the client stayed wrong for ever. The answer is recorded per sequence
//!    number and replayed by kind.

#![allow(clippy::unwrap_used)]

mod common;

use pretty_assertions::assert_eq;
use slotted_model::{
    Actor, Authority, Button, ClickAction, Delta, MenuDef, MenuId, SlotIx, ToolbarAction,
    apply_click,
};
use slotted_net::{
    ClientMessage, Conditions, MenuServer, OpenError, Outcome, Owner, PeerId, Refusal,
    ServerMessage, Transport,
};

use common::{
    BURN, EGG, Items, Revocable, STONE, Shared, World, def, force_retransmit, furnace_def, left,
    lose, mirrored_def, stack, starting_player, tally,
};

// ------------------------------------------------------ 1. who may touch it

/// Every message kind, sent by a peer that does not own the session it names.
///
/// The answer is a refusal and a refusal only. Not a snapshot, not a slot
/// write, not an ack: a peer must not learn what is in a container it has no
/// business with, and must not be able to move anything in it.
#[test]
fn every_message_kind_from_a_foreign_peer_is_refused_and_changes_nothing() {
    let mut world = World::new(1);
    let owner = world.join();
    let intruder = world.join();
    let before = world.server.snapshot(owner.menu).unwrap();
    let intruder_end = intruder.end();

    let messages = [
        ClientMessage::ClickContainer {
            menu: owner.menu,
            state_id: 0,
            seq: 0,
            action: left(0),
            predicted: Delta::default(),
        },
        ClientMessage::RequestResync { menu: owner.menu },
        ClientMessage::CloseMenu { menu: owner.menu },
    ];
    for message in messages {
        intruder_end.send(PeerId::SERVER, message.clone()).unwrap();
        world.link.tick();
        assert_eq!(
            world.pump(),
            vec![Outcome::Denied(Refusal::NotYours)],
            "{message:?} named another peer's session"
        );
    }

    world.link.tick();
    let answers: Vec<ServerMessage> = intruder_end.poll().into_iter().map(|(_, m)| m).collect();
    assert_eq!(answers.len(), 3, "one answer each: {answers:?}");
    assert!(
        answers.iter().all(|m| matches!(
            m,
            ServerMessage::Refused {
                reason: Refusal::NotYours,
                ..
            }
        )),
        "and every one of them carries no state: {answers:?}"
    );

    assert_eq!(
        world.server.snapshot(owner.menu).unwrap(),
        before,
        "the owner's session is untouched"
    );
    assert!(
        world.server.owner_of(owner.menu).is_some(),
        "and still open: a foreign close closes nothing"
    );
}

/// A session that does not exist at all is refused the same way, and the
/// refusal does not say whether it ever existed.
#[test]
fn a_message_about_a_session_nobody_has_is_refused_as_unknown() {
    let mut world = World::new(2);
    let client = world.join();
    let end = client.end();

    end.send(
        PeerId::SERVER,
        ClientMessage::RequestResync { menu: MenuId(4242) },
    )
    .unwrap();
    world.link.tick();
    assert_eq!(world.pump(), vec![Outcome::Denied(Refusal::UnknownMenu)]);
}

#[test]
fn access_revoked_mid_session_refuses_the_owners_own_messages() {
    let allowed = std::sync::Arc::new(Revocable(std::sync::atomic::AtomicBool::new(true)));
    let policy = std::sync::Arc::clone(&allowed);
    let mut world = World::with_server(3, MenuServer::with_access(Shared(policy)));
    let mut client = world.join();

    assert!(client.click(left(0)));
    assert_eq!(world.settle(&mut [&mut client]), vec![Outcome::Acked]);

    allowed.0.store(false, std::sync::atomic::Ordering::Relaxed);
    assert!(client.click(left(1)));
    let outcomes = world.settle(&mut [&mut client]);
    assert!(
        outcomes
            .iter()
            .all(|o| matches!(o, Outcome::Denied(Refusal::Denied))),
        "a revoked session answers nothing else: {outcomes:?}"
    );
    assert_eq!(
        world.container_cell(1),
        stack(EGG, 10),
        "and the eggs did not move"
    );
    assert_eq!(client.authority.refusals(), 1);
    assert_eq!(
        client.authority.in_flight(),
        0,
        "the client stops waiting on a refusal rather than retrying for ever"
    );
}

/// An inventory private to one player cannot be bound into another player's
/// session, whatever the access policy says. This is the invariant that keeps
/// one player out of another's pockets.
#[test]
fn a_private_inventory_cannot_be_bound_into_another_peers_session() {
    let mut world = World::with_server(4, MenuServer::with_access(slotted_net::OwnerOnly));
    let alice = PeerId(1);
    let bob = PeerId(2);
    let alices_pockets = world.server.add_private(alice, starting_player());

    assert_eq!(
        world.server.store().owner(alices_pockets),
        Some(Owner::Private(alice))
    );
    let error = world
        .server
        .open(
            MenuId(1),
            bob,
            def(),
            vec![world.container, alices_pockets],
            Actor::SURVIVAL,
        )
        .unwrap_err();
    assert_eq!(error, OpenError::NotYours { id: alices_pockets });
    assert!(
        world.server.owner_of(MenuId(1)).is_none(),
        "and a refused open leaves no half-built session behind"
    );

    // Alice's own session over the same inventory is fine.
    world
        .server
        .open(
            MenuId(1),
            alice,
            def(),
            vec![world.container, alices_pockets],
            Actor::SURVIVAL,
        )
        .unwrap();
    assert_eq!(world.server.owner_of(MenuId(1)), Some(alice));
}

/// A peer may close its own session, and only its own.
#[test]
fn a_peer_closes_its_own_session_and_the_server_forgets_it() {
    let mut world = World::new(5);
    let client = world.join();
    let end = client.end();

    end.send(
        PeerId::SERVER,
        ClientMessage::CloseMenu { menu: client.menu },
    )
    .unwrap();
    world.link.tick();
    assert_eq!(world.pump(), vec![Outcome::Closed]);
    assert!(world.server.owner_of(client.menu).is_none());

    world.link.tick();
    client.authority.poll();
    assert_eq!(
        client.authority.take_closed(),
        vec![client.menu],
        "and the client is told, so it stops submitting into a closed session"
    );
}

// ------------------------------------------- 2. shared items, private state

/// Two players at one chest: their edits meet in one inventory, and nothing
/// else about them does.
#[test]
fn two_sessions_share_the_container_and_share_nothing_else() {
    let mut world = World::new(6);
    let mut a = world.join();
    let mut b = world.join();
    assert_ne!(a.menu, b.menu);

    // A picks the chest's stone up. It leaves the shared inventory, so B is
    // told; it lands on A's cursor, which is A's alone.
    assert!(a.click(left(0)));
    world.settle(&mut [&mut a, &mut b]);

    assert_eq!(world.container_cell(0), None, "one chest, one write");
    assert_eq!(b.slot(0), None, "and B was told about it");
    assert_eq!(a.state.carried, stack(STONE, 64));
    assert_eq!(
        world.server.snapshot(b.menu).unwrap().state.carried,
        None,
        "B's cursor is empty: a cursor is session state, not container state"
    );

    // Both players' own inventories start the same and stay separate.
    assert!(b.click(left(6)), "B picks up its own stone");
    world.settle(&mut [&mut a, &mut b]);
    assert_eq!(
        world.server.snapshot(a.menu).unwrap().inventories[MenuDef::PLAYER_MAIN].get(2),
        stack(STONE, 5).as_ref(),
        "A's own inventory is untouched by B emptying B's"
    );
    assert_eq!(
        world.server.snapshot(b.menu).unwrap().inventories[MenuDef::PLAYER_MAIN].get(2),
        None
    );
}

/// One container, two sessions that number their slots differently. A change
/// reaches each of them at its own index, because a slot index is a fact
/// about a session.
#[test]
fn a_container_change_reaches_each_session_at_its_own_slot_index() {
    let mut world = World::new(7);
    let mut plain = world.join();
    let mut mirrored = world.join_with(mirrored_def(), true);

    // In `plain`, chest cell 0 is menu slot 0. In `mirrored` the chest is
    // `InventoryRef` 1 and its cells are menu slots 4..8, so cell 0 is slot 4.
    assert_eq!(plain.slot(0), stack(STONE, 64));
    assert_eq!(mirrored.slot(4), stack(STONE, 64));

    assert!(plain.click(left(0)));
    world.settle(&mut [&mut plain, &mut mirrored]);

    assert_eq!(world.container_cell(0), None);
    assert_eq!(
        mirrored.slot(4),
        None,
        "the mirrored session was told about slot 4, not slot 0"
    );
    assert_eq!(
        mirrored.slot(0),
        None,
        "and its own slot 0 is its player inventory, which nothing touched"
    );
    assert_eq!(mirrored.inventories[MenuDef::PLAYER_MAIN].get(0), None);
}

/// A property belongs to the block, not to the viewer, so setting one reaches
/// every session bound to that container and seeds the next one to open.
#[test]
fn a_property_reaches_every_session_bound_to_the_container() {
    let mut world = World::new(8);
    let mut a = world.join_with(furnace_def(), false);
    let mut b = world.join_with(furnace_def(), false);

    world
        .server
        .set_property(&world.end, world.container, BURN, 42)
        .unwrap();
    world.settle(&mut [&mut a, &mut b]);

    assert_eq!(a.properties, vec![(BURN, 42)]);
    assert_eq!(b.properties, vec![(BURN, 42)]);

    let c = world.join_with(furnace_def(), false);
    assert_eq!(
        world.server.snapshot(c.menu).unwrap().state.properties,
        vec![42],
        "a session opened later starts at the real burn time"
    );
}

/// A host writing straight into the store is a first-class way to change the
/// world, and every session bound to what it wrote hears about it.
#[test]
fn a_host_write_into_the_store_reaches_every_bound_session() {
    let mut world = World::new(9);
    let mut a = world.join();
    let mut b = world.join();

    world
        .server
        .store_mut()
        .inventory_mut(world.container)
        .unwrap()
        .set(2, stack(EGG, 3));
    world.settle(&mut [&mut a, &mut b]);

    assert_eq!(a.slot(2), stack(EGG, 3));
    assert_eq!(b.slot(2), stack(EGG, 3));
}

/// A player leaving takes their sessions and their pockets with them.
#[test]
fn dropping_a_peer_closes_its_sessions_and_frees_its_private_inventories() {
    let mut world = World::new(10);
    let a = world.join();
    let b = world.join();
    let before = world.server.store().len();

    world.server.drop_peer(a.peer);

    assert!(world.server.sessions_of(a.peer).is_empty());
    assert_eq!(world.server.sessions_of(b.peer), vec![b.menu]);
    assert_eq!(
        world.server.store().len(),
        before - 1,
        "A's own inventory went with it, the shared chest did not"
    );
    assert!(world.server.store().inventory(world.container).is_some());
}

// --------------------------------------------------- 3. answering it twice

/// The finding, exactly: a correction that never arrives, then the client
/// retries, and the retry must be answered with the correction again.
///
/// Answering an ordinary ack instead is what left a client permanently
/// divergent: the ack retires the submission, so the snapshot is never asked
/// for again and the wrong slot stays wrong.
#[test]
fn a_lost_correction_is_repeated_as_a_correction_on_the_retry() {
    let mut world = World::new(11);
    let mut client = world.join();

    // A prediction that cannot be right: the client claims the click changed
    // nothing at all. The server disagrees and answers with the container.
    let action = left(0);
    apply_click(
        &client.def,
        &mut client.inventories,
        &mut client.state,
        action,
        &Actor::SURVIVAL,
        &Items,
    )
    .unwrap();
    client
        .authority
        .submit(client.menu, action, &Delta::default())
        .unwrap();

    world.link.tick();
    assert_eq!(world.pump(), vec![Outcome::Corrected]);
    world.link.tick();
    let lost = lose(&client.end());
    assert!(
        matches!(
            lost.as_slice(),
            [ServerMessage::SetContent {
                answers: Some(0),
                ..
            }]
        ),
        "the first answer was a correction naming the click it corrects: {lost:?}"
    );

    // The client never saw it, so it retries. The server has already applied
    // this sequence number and must repeat the *kind* of answer it gave.
    force_retransmit(&mut world, &client);
    assert!(client.authority.retransmits() >= 1);
    let outcomes = world.pump();
    assert!(
        !outcomes.is_empty() && outcomes.iter().all(|o| *o == Outcome::Duplicate),
        "a retransmission is a duplicate, not a second click: {outcomes:?}"
    );
    world.link.tick();
    let again = lose(&client.end());
    assert!(
        !again.is_empty()
            && again.iter().all(|m| matches!(
                m,
                ServerMessage::SetContent {
                    answers: Some(0),
                    ..
                }
            )),
        "the retry must be corrected again, not acked: {again:?}"
    );
    assert_eq!(
        world.server.state_id(client.menu),
        Some(1),
        "and the click was applied exactly once"
    );
}

/// The other half: an ack that goes missing is repeated as an ack, not
/// escalated into a whole container.
#[test]
fn a_lost_ack_is_repeated_as_an_ack_on_the_retry() {
    let mut world = World::new(12);
    let mut client = world.join();

    assert!(client.click(left(0)));
    world.link.tick();
    assert_eq!(world.pump(), vec![Outcome::Acked]);
    world.link.tick();
    let lost = lose(&client.end());
    assert!(matches!(
        lost.as_slice(),
        [ServerMessage::Ack { seq: 0, .. }]
    ));

    force_retransmit(&mut world, &client);
    let outcomes = world.pump();
    assert!(
        !outcomes.is_empty() && outcomes.iter().all(|o| *o == Outcome::Duplicate),
        "{outcomes:?}"
    );
    world.link.tick();
    let again = lose(&client.end());
    assert!(
        !again.is_empty()
            && again.iter().all(|m| matches!(
                m,
                ServerMessage::Ack {
                    seq: 0,
                    state_id: 1,
                    ..
                }
            )),
        "a repeated ack stays an ack: {again:?}"
    );
}

/// A snapshot says which submission it answers, so it retires that one and
/// everything older, and never "whichever is oldest".
///
/// Three clicks go out. The second is the one the server corrects. Delivered
/// out of order, the client must still end with nothing in flight, one event
/// per submission, and the server's items.
#[test]
fn an_out_of_order_snapshot_retires_exactly_what_it_answers() {
    let mut world = World::new(13);
    let mut client = world.join();

    // Click 0 is honest, click 1 lies about its outcome, click 2 is honest.
    assert!(client.click(left(0)));
    let action = left(4);
    apply_click(
        &client.def,
        &mut client.inventories,
        &mut client.state,
        action,
        &Actor::SURVIVAL,
        &Items,
    )
    .unwrap();
    client
        .authority
        .submit(client.menu, action, &Delta::default())
        .unwrap();
    assert!(client.click(left(6)));
    assert_eq!(client.authority.in_flight(), 3);

    world.link.tick();
    let outcomes = world.pump();
    assert_eq!(
        outcomes,
        vec![Outcome::Acked, Outcome::Corrected, Outcome::Corrected],
        "the lie is corrected, and the click after it inherits the correction"
    );

    // Shuffle the answers on their way back.
    world.link.set_conditions(Conditions::reordering(6));
    world.link.advance(10);
    let events = client.reconcile();

    assert_eq!(
        events.len(),
        3,
        "one event per submission, so the caller's round-trip count balances: {events:?}"
    );
    assert_eq!(
        client.authority.in_flight(),
        0,
        "and nothing is left waiting"
    );
    let snapshot = world.server.snapshot(client.menu).unwrap();
    assert_eq!(client.all_slots(), world.all_slots(client.menu));
    assert_eq!(client.state.carried, snapshot.state.carried);
    assert_eq!(client.state.state_id, snapshot.state.state_id);
}

/// A retransmission of a click older than everything the server remembers is
/// answered with the container rather than a stale ack.
#[test]
fn a_click_older_than_the_remembered_window_is_corrected() {
    let mut world = World::with_server(14, MenuServer::new().window(1));
    let mut client = world.join();

    assert!(client.click(left(0)));
    assert!(client.click(left(4)));
    assert_eq!(
        world.settle(&mut [&mut client]),
        vec![Outcome::Acked, Outcome::Acked]
    );

    // Sequence 0 has fallen out of a one-answer window. Replaying it must not
    // reapply the click and must not invent an ack.
    let before = world.server.snapshot(client.menu).unwrap();
    client
        .end()
        .send(
            PeerId::SERVER,
            ClientMessage::ClickContainer {
                menu: client.menu,
                state_id: 0,
                seq: 0,
                action: left(0),
                predicted: Delta::default(),
            },
        )
        .unwrap();
    world.link.tick();
    assert_eq!(world.pump(), vec![Outcome::Corrected]);
    assert_eq!(world.server.snapshot(client.menu).unwrap(), before);
}

// ------------------------------------------------------- 4. the long random

/// A thousand random clicks from two players on one container, over a link
/// that loses one message in ten.
///
/// The question is the one the whole crate exists to answer: after all that,
/// does each client's picture of the shared container match the server's, and
/// has the container invented anything? `Throw` sends items out of the menu
/// and into the world, so the count may fall; it may never rise.
#[test]
#[allow(clippy::too_many_lines)]
fn two_peers_on_one_container_converge_across_a_thousand_lossy_actions() {
    let mut world = World::new(20_260_906);
    let mut a = world.join();
    let mut b = world.join();
    world.link.set_conditions(Conditions::lossy(10));

    let start: u64 = world
        .server
        .snapshot(a.menu)
        .unwrap()
        .inventories
        .iter()
        .map(|(_, i)| {
            i.slots()
                .iter()
                .flatten()
                .map(|s| u64::from(s.count))
                .sum::<u64>()
        })
        .sum();

    let mut rng: u64 = 0x9E37_79B9_7F4A_7C15;
    let mut random = move || {
        rng ^= rng >> 12;
        rng ^= rng << 25;
        rng ^= rng >> 27;
        rng.wrapping_mul(0x2545_F491_4F6C_DD1D)
    };

    for step in 0..1000_u32 {
        let slot = u16::try_from(random() % 8).unwrap();
        let action = match random() % 6 {
            0 => ClickAction::QuickMove { slot: SlotIx(slot) },
            1 => ClickAction::Pickup {
                slot: SlotIx(slot),
                button: Button::Right,
            },
            2 => ClickAction::Throw {
                slot: SlotIx(slot),
                all: random() % 2 == 0,
            },
            3 => ClickAction::PickupAll {
                slot: SlotIx(slot),
                reverse: false,
            },
            4 => ClickAction::Toolbar(ToolbarAction::QuickStack {
                from: MenuDef::PLAYER_MAIN,
                to: MenuDef::CONTAINER,
            }),
            _ => left(slot),
        };
        if step % 2 == 0 { &mut a } else { &mut b }.click(action);
        if step % 3 == 0 {
            world.settle(&mut [&mut a, &mut b]);
        }
    }

    world.link.set_conditions(Conditions::PERFECT);
    for _ in 0..60 {
        world.settle(&mut [&mut a, &mut b]);
    }

    assert!(
        world.link.dropped() > 0,
        "the link really did lose messages"
    );
    assert_eq!(a.authority.in_flight(), 0);
    assert_eq!(b.authority.in_flight(), 0);

    for client in [&a, &b] {
        let snapshot = world.server.snapshot(client.menu).unwrap();
        assert_eq!(
            client.all_slots(),
            world.all_slots(client.menu),
            "a client ended on different items from the server's"
        );
        assert_eq!(client.state.carried, snapshot.state.carried);
        assert_eq!(
            client.tally(),
            tally(&snapshot.inventories, snapshot.state.carried.as_ref())
        );
    }

    // The two clients see one container, so they agree about it slot by slot.
    for cell in 0..4 {
        assert_eq!(
            a.slot(cell),
            b.slot(cell),
            "the two clients disagree about container cell {cell}"
        );
        assert_eq!(a.slot(cell), world.container_cell(usize::from(cell)));
    }

    let end: u64 = world
        .server
        .store()
        .iter()
        .map(|(_, c)| {
            c.inventory
                .slots()
                .iter()
                .flatten()
                .map(|s| u64::from(s.count))
                .sum::<u64>()
        })
        .sum::<u64>()
        + [&a, &b]
            .iter()
            .map(|c| {
                world
                    .server
                    .snapshot(c.menu)
                    .unwrap()
                    .state
                    .carried
                    .map_or(0, |s| u64::from(s.count))
            })
            .sum::<u64>();
    let started = start
        + starting_player()
            .slots()
            .iter()
            .flatten()
            .map(|s| u64::from(s.count))
            .sum::<u64>();
    assert!(
        end <= started,
        "the server minted items: {end} from {started}"
    );
}
