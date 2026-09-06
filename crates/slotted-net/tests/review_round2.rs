//! The seams between the external review's findings, rather than the findings
//! themselves.
//!
//! `sessions.rs` proves each of findings 1 to 3 on its own. Each of these
//! puts two of them in one scenario, which is where a fix that is right in
//! isolation stops being right:
//!
//! * authorisation (1) against the recorded-answer window (3): a peer whose
//!   access was revoked mid-session retransmits an old sequence number. The
//!   refusal must neither answer with state nor disturb what the window
//!   remembers.
//! * sessions (2) against closing (1): one of two peers on one container
//!   closes mid-drag. The container must not keep a phantom, and the stack on
//!   the closing player's cursor must not evaporate.
//! * correlated answers (3) against reordering: a snapshot answering sequence
//!   *n* arrives before the ack for *n - 1*.
//! * properties (4) against sessions (2): one server-side change to a shared
//!   furnace reaches both sessions and seeds a third.
//! * all of it at once: two peers, a lossy link, property traffic and a
//!   session that closes and reopens underneath.

#![allow(clippy::unwrap_used)]

mod common;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use pretty_assertions::assert_eq;
use slotted_model::{
    Actor, Authority, AuthorityEvent, Button, ClickAction, DragKind, DragStage, ItemStack, SlotIx,
    ToolbarAction, apply_click,
};
use slotted_net::{
    ClientMessage, Conditions, MenuServer, Outcome, PeerId, Refusal, ServerMessage, Transport,
};

use common::{BURN, EGG, Items, Revocable, STONE, Shared, World, furnace_def, left, lose, stack};

// ------------------------------------ 1 meets 3: refusal and the window

/// A peer opens a session, has its access taken away, and then retransmits a
/// click the server already answered.
///
/// Two things must hold at once. The retransmission is refused, and the
/// refusal carries no state: not the snapshot the retransmission path would
/// otherwise have produced, not an ack, not a slot. And the refusal must not
/// touch the recorded-answer window, because a refusal is not an answer to
/// the click. Recording one, or consuming the entry that was there, would
/// leave the session unable to replay its own history the moment access came
/// back -- and access coming back is the ordinary case, not the exotic one:
/// the player walked away from the chest and walked back.
#[test]
fn a_revoked_peer_retransmitting_an_old_sequence_is_refused_and_leaks_nothing() {
    let allowed = Arc::new(Revocable(AtomicBool::new(true)));
    let mut world = World::with_server(101, MenuServer::with_access(Shared(Arc::clone(&allowed))));
    let mut client = world.join();

    // Sequence 0: a click the server accepts and acks.
    let action = left(0);
    assert!(client.click(action));
    assert_eq!(world.settle(&mut [&mut client]), vec![Outcome::Acked]);
    let predicted = client.state.carried.clone();
    assert_eq!(predicted, stack(STONE, 64), "the stone is on the cursor");
    assert_eq!(world.container_cell(0), None, "and out of the container");

    let after_the_click = world.all_slots(client.menu);
    let carried_after_the_click = world.server.snapshot(client.menu).unwrap().state.carried;

    // The player walks away: the same session, no longer actable.
    allowed.0.store(false, Ordering::Relaxed);

    // The link ate the ack, so the client sends sequence 0 again. Forged
    // rather than driven through `retransmit`, because the client retired the
    // submission when the first ack arrived; what is under test is the
    // server's answer to a sequence it has already dealt with.
    let end = client.end();
    let replay = ClientMessage::ClickContainer {
        menu: client.menu,
        state_id: 0,
        seq: 0,
        action,
        predicted: slotted_model::Delta::default(),
    };
    end.send(PeerId::SERVER, replay.clone()).unwrap();
    world.link.tick();
    assert_eq!(
        world.pump(),
        vec![Outcome::Denied(Refusal::Denied)],
        "a revoked session refuses a retransmission like any other message"
    );
    world.link.tick();

    let answered = lose(&end);
    assert_eq!(
        answered,
        vec![ServerMessage::Refused {
            menu: client.menu,
            seq: Some(0),
            reason: Refusal::Denied,
        }],
        "the refusal is the whole answer: no snapshot, no ack, no slot"
    );
    assert_eq!(
        world.all_slots(client.menu),
        after_the_click,
        "and nothing moved"
    );

    // Access comes back. The window must still hold the ack it recorded, and
    // replay it -- not reapply the click, and not answer with a correction it
    // never gave the first time.
    allowed.0.store(true, Ordering::Relaxed);
    end.send(PeerId::SERVER, replay).unwrap();
    world.link.tick();
    assert_eq!(
        world.pump(),
        vec![Outcome::Duplicate],
        "the refusal did not consume or overwrite the recorded answer"
    );
    world.link.tick();
    assert_eq!(
        lose(&end),
        vec![ServerMessage::Ack {
            menu: client.menu,
            state_id: 1,
            seq: 0,
        }],
        "an ack repeats as an ack, across a refusal"
    );
    assert_eq!(
        world.all_slots(client.menu),
        after_the_click,
        "the click was not applied a second time"
    );
    assert_eq!(
        world.server.snapshot(client.menu).unwrap().state.carried,
        carried_after_the_click,
        "and the cursor is where the one application left it"
    );
}

// --------------------------------- 2 meets 1: closing out of two sessions

/// Two peers on one container, and one of them closes mid-drag.
///
/// The container is shared, the drag is not. `Drag { Start | Add }` moves no
/// items, so a drag interrupted by a close must leave the container exactly
/// as the last committed click left it: no reservation, no half-distributed
/// stack, and nothing at all in the other peer's session, which never saw the
/// drag in the first place.
///
/// The stack on the closing player's cursor is the part that used to go
/// wrong. It has already been taken out of a container, so a close that just
/// forgets the session destroys it: an item sink any client can trigger by
/// pressing escape. It goes back into that player's own pockets.
#[test]
fn a_peer_closing_mid_drag_leaves_no_phantom_and_destroys_nothing() {
    let mut world = World::new(102);
    let mut a = world.join();
    let mut b = world.join();
    let before = world.world_tally();

    // A picks the stone up out of the shared container: both peers agree it
    // has left the chest, and only A is carrying it.
    assert!(a.click(left(0)));
    world.settle(&mut [&mut a, &mut b]);
    assert_eq!(a.state.carried, stack(STONE, 64));
    assert_eq!(b.state.carried, None, "a cursor is not shared");
    assert_eq!(b.slot(0), None, "but the container is");

    // A starts painting the stone over two container slots and stops halfway.
    for action in [
        ClickAction::Drag {
            stage: DragStage::Start,
            kind: DragKind::Left,
            slot: None,
        },
        ClickAction::Drag {
            stage: DragStage::Add,
            kind: DragKind::Left,
            slot: Some(SlotIx(2)),
        },
    ] {
        assert!(a.click(action));
    }
    world.settle(&mut [&mut a, &mut b]);
    assert!(
        world.server.snapshot(a.menu).unwrap().state.drag.is_some(),
        "the server is holding an unfinished drag"
    );
    let container_mid_drag: Vec<Option<ItemStack>> =
        (0..4).map(|cell| world.container_cell(cell)).collect();

    // Escape. The session goes; the drag goes with it.
    a.authority.close(a.menu).unwrap();
    let closing = a.menu;
    world.settle(&mut [&mut a, &mut b]);

    assert_eq!(
        world.server.owner_of(closing),
        None,
        "the session is gone from the server"
    );
    assert_eq!(a.authority.take_closed(), vec![closing]);
    assert_eq!(
        world.server.owner_of(b.menu),
        Some(b.peer),
        "and the other peer's session is untouched"
    );

    let container_after: Vec<Option<ItemStack>> =
        (0..4).map(|cell| world.container_cell(cell)).collect();
    assert_eq!(
        container_after, container_mid_drag,
        "an unfinished drag moved nothing, so closing it moves nothing back"
    );
    assert_eq!(
        world.world_tally(),
        before,
        "the carried stack came back rather than evaporating"
    );

    // It came back into A's own pockets, not into the chest the other peer is
    // standing at.
    let pockets = world.server.store().inventory(a.player).unwrap();
    assert_eq!(
        pockets.count_of(&ItemStack::new(STONE, 1)),
        69,
        "the five the player started with plus the sixty-four off the cursor"
    );

    // B's picture is the server's picture, and carries no trace of a drag it
    // was never part of.
    b.reconcile();
    assert_eq!(b.all_slots(), world.all_slots(b.menu));
    assert_eq!(b.state.drag, None);
    assert!(
        b.state.hints.is_empty(),
        "a hint is session state and never travels between sessions"
    );
}

// ------------------------------------------- 3: answers out of order

/// A snapshot answering sequence 1 arrives before the ack for sequence 0.
///
/// Both answers are correct and the link delivered them backwards, which is a
/// thing links do. The snapshot settles sequence 1 and every older submission
/// on the menu, so by the time the ack for sequence 0 turns up there is
/// nothing left for it to retire. What must not happen is the client counting
/// the round trip twice, leaving `in_flight` stuck above zero, or letting the
/// late ack undo the newer snapshot it already applied.
#[test]
fn a_snapshot_answering_the_newer_click_arrives_before_the_older_ack() {
    let mut world = World::new(103);
    let mut client = world.join();

    // Sequence 0: a click the server will agree with.
    assert!(client.click(left(0)));
    // Sequence 1: the same click applied locally, submitted with a prediction
    // that cannot be right, so the server answers it with the container.
    let action = ClickAction::Pickup {
        slot: SlotIx(1),
        button: Button::Left,
    };
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
        .submit(client.menu, action, &slotted_model::Delta::default())
        .unwrap();
    assert_eq!(client.authority.in_flight(), 2);

    world.link.tick();
    assert_eq!(world.pump(), vec![Outcome::Acked, Outcome::Corrected]);
    world.link.tick();

    // Take both answers off the wire before the client sees either, then hand
    // them back in the other order.
    let end = client.end();
    let answers = lose(&end);
    let ack = answers
        .iter()
        .find(|m| matches!(m, ServerMessage::Ack { seq: 0, .. }))
        .expect("an ack for sequence 0")
        .clone();
    let content = answers
        .iter()
        .find(|m| {
            matches!(
                m,
                ServerMessage::SetContent {
                    answers: Some(1),
                    ..
                }
            )
        })
        .expect("a correction answering sequence 1")
        .clone();

    world.end.send(client.peer, content).unwrap();
    world.link.tick();
    let first = client.reconcile();
    assert_eq!(
        client.authority.in_flight(),
        0,
        "the snapshot settles the click it answers and every older one"
    );
    assert_eq!(
        first.len(),
        2,
        "one resync for the click it answers, one ack for the older one it \
         also settles, or the caller waits for ever: {first:?}"
    );
    assert!(matches!(first[0], AuthorityEvent::Ack { .. }));
    assert!(matches!(first[1], AuthorityEvent::Resync { .. }));
    let settled = client.all_slots();
    assert_eq!(settled, world.all_slots(client.menu));

    // Now the ack that was overtaken. It names a submission the client no
    // longer has, so it asks for the truth rather than trusting itself -- and
    // it must not walk the applied snapshot backwards.
    world.end.send(client.peer, ack).unwrap();
    world.link.tick();
    let late = client.reconcile();
    assert_eq!(
        late.len(),
        1,
        "a late ack reports the one round trip it names and nothing else: {late:?}"
    );
    assert_eq!(
        client.all_slots(),
        settled,
        "and does not undo the newer snapshot"
    );

    world.settle(&mut [&mut client]);
    assert_eq!(client.authority.in_flight(), 0);
    assert_eq!(client.all_slots(), world.all_slots(client.menu));
    assert_eq!(
        client.state.carried,
        world.server.snapshot(client.menu).unwrap().state.carried
    );
}

// ------------------------------- 4 meets 2: one property, two sessions

/// One furnace, two players watching it, and one server-side change.
///
/// The property belongs to the container, not to the viewer, which is what
/// finding 2 separated and what finding 4's single write path depends on. So
/// one `set_property` reaches every session bound to the container, at each
/// session's own property index, and a session opened afterwards starts at
/// the burn time the furnace actually has rather than at the definition's
/// initial value.
#[test]
fn one_property_change_on_a_shared_furnace_reaches_both_sessions_and_seeds_a_third() {
    let mut world = World::new(104);
    let mut a = world.join_with(furnace_def(), false);
    let mut b = world.join_with(furnace_def(), false);

    let container = world.container;
    world
        .server
        .set_property(&world.end, container, BURN, 42)
        .unwrap();
    world.settle(&mut [&mut a, &mut b]);

    for (name, client) in [("a", &a), ("b", &b)] {
        assert_eq!(
            client.properties,
            vec![(BURN, 42)],
            "session {name} heard the change exactly once"
        );
    }
    assert_eq!(
        world.server.store().get(container).unwrap().properties[&BURN],
        42,
        "the value lives on the container, which is the single path"
    );
    for menu in [a.menu, b.menu] {
        assert_eq!(
            world.server.snapshot(menu).unwrap().state.properties,
            vec![42],
            "and in every bound session's own state"
        );
    }

    // A third player opens the same furnace after the fact.
    let c = world.join_with(furnace_def(), false);
    assert_eq!(
        world.server.snapshot(c.menu).unwrap().state.properties,
        vec![42],
        "a session opened later starts at the furnace's real burn time"
    );
    assert_eq!(
        c.state.properties,
        vec![42],
        "and so does the client seeded from it"
    );

    // A change after the third session opened reaches all three.
    world
        .server
        .set_property(&world.end, container, BURN, 7)
        .unwrap();
    let mut c = c;
    world.settle(&mut [&mut a, &mut b, &mut c]);
    assert_eq!(a.properties, vec![(BURN, 42), (BURN, 7)]);
    assert_eq!(b.properties, vec![(BURN, 42), (BURN, 7)]);
    assert_eq!(c.properties, vec![(BURN, 7)]);
}

// ------------------------------------------------- everything at once

/// The lossy two-peer fuzz, with property traffic and a session that closes
/// and reopens while the link is dropping one message in ten.
///
/// The question is unchanged and the pressure is not: after a thousand random
/// actions, does every client's picture match the server's, do the two agree
/// about the one container they share, and has the world invented anything?
/// A close in the middle is the interesting addition, because it is the one
/// event that ends a session while submissions for it are still in flight.
#[test]
#[allow(clippy::too_many_lines)]
fn two_peers_survive_a_lossy_link_with_properties_and_a_session_that_reopens() {
    let mut world = World::new(20_260_907);
    let mut a = world.join_with(furnace_def(), false);
    let mut b = world.join_with(furnace_def(), false);
    let container = world.container;
    world.link.set_conditions(Conditions::lossy(10));

    let start = world.world_tally();
    let mut closes = 0_u32;
    let mut properties = 0_i32;

    let mut rng: u64 = 0x243F_6A88_85A3_08D3;
    let mut random = move || {
        rng ^= rng >> 12;
        rng ^= rng << 25;
        rng ^= rng >> 27;
        rng.wrapping_mul(0x2545_F491_4F6C_DD1D)
    };

    for step in 0..1000_u32 {
        let slot = u16::try_from(random() % 8).unwrap();
        // Nothing in this mix leaves the menu and nothing in it mints, so the
        // world's item count is conserved exactly rather than merely bounded:
        // a much sharper question to ask of a link that is losing a tenth of
        // everything and a session that keeps closing under it. `Throw` and a
        // middle drag are left out for that reason, not because they are
        // uninteresting; `sessions.rs` covers `Throw`.
        let action = match random() % 6 {
            0 => ClickAction::QuickMove { slot: SlotIx(slot) },
            1 => ClickAction::Pickup {
                slot: SlotIx(slot),
                button: Button::Right,
            },
            2 => ClickAction::Drag {
                stage: DragStage::Start,
                kind: DragKind::Left,
                slot: None,
            },
            3 => ClickAction::PickupAll {
                slot: SlotIx(slot),
                reverse: false,
            },
            4 => ClickAction::Toolbar(ToolbarAction::QuickStack {
                from: slotted_model::MenuDef::PLAYER_MAIN,
                to: slotted_model::MenuDef::CONTAINER,
            }),
            _ => left(slot),
        };
        let who = if step % 2 == 0 { &mut a } else { &mut b };
        who.click(action);
        // A drag that is started is painted and finished, so the mix leaves a
        // drag half-open only where the close below interrupts one.
        if matches!(action, ClickAction::Drag { .. }) {
            who.click(ClickAction::Drag {
                stage: DragStage::Add,
                kind: DragKind::Left,
                slot: Some(SlotIx(slot)),
            });
            if random() % 3 > 0 {
                who.click(ClickAction::Drag {
                    stage: DragStage::End,
                    kind: DragKind::Left,
                    slot: None,
                });
            }
        }

        // The furnace keeps burning while the players rummage.
        if step % 7 == 0 {
            properties += 1;
            world
                .server
                .set_property(&world.end, container, BURN, properties)
                .unwrap();
        }

        // Every so often A closes the screen and opens it again, with
        // whatever it had in flight still on the wire.
        if step % 250 == 249 {
            a.authority.close(a.menu).unwrap();
            world.settle(&mut [&mut a, &mut b]);
            assert_eq!(
                world.server.owner_of(a.menu),
                None,
                "the close really did end the session"
            );
            world.reopen(&mut a);
            closes += 1;
        }

        if step % 3 == 0 {
            world.settle(&mut [&mut a, &mut b]);
        }
    }

    world.link.set_conditions(Conditions::PERFECT);
    for _ in 0..60 {
        world.settle(&mut [&mut a, &mut b]);
    }

    // A `SetProperty` is fire-and-forget: unlike a click it is not
    // retransmitted, and unlike a slot it is not re-derived from a dirty
    // mask, so a lost one is repaired only by the next snapshot. With the
    // furnace stopped, that snapshot has to be asked for -- which is what a
    // client does when it suspects it has drifted, and what this proves the
    // snapshot path still carries. See FOLLOWUPS: "a lost property update".
    for client in [&a, &b] {
        client.authority.request_resync(client.menu).unwrap();
    }
    world.settle(&mut [&mut a, &mut b]);

    assert!(
        world.link.dropped() > 0,
        "the link really did lose messages"
    );
    assert_eq!(closes, 4, "the session closed and reopened four times");
    assert_eq!(a.authority.in_flight(), 0);
    assert_eq!(b.authority.in_flight(), 0);

    for (name, client) in [("a", &a), ("b", &b)] {
        assert_eq!(
            client.all_slots(),
            world.all_slots(client.menu),
            "client {name} ended on different items from the server's"
        );
        assert_eq!(
            client.state.carried,
            world.server.snapshot(client.menu).unwrap().state.carried,
            "client {name} disagrees about its own cursor"
        );
        assert_eq!(
            client.state.properties,
            world.server.snapshot(client.menu).unwrap().state.properties,
            "client {name} missed a property update"
        );
        assert_eq!(
            client.state.properties,
            vec![properties],
            "client {name} did not end on the furnace's last burn time"
        );
    }

    for cell in 0..4 {
        assert_eq!(
            a.slot(cell),
            b.slot(cell),
            "the two clients disagree about container cell {cell}"
        );
    }

    // Nothing in the mix leaves the menu, so the world holds exactly what it
    // started with: a close mid-flight neither minted nor sank an item, over
    // four closes and a link that dropped a tenth of everything.
    assert_eq!(
        world.world_tally(),
        start,
        "the world did not end with what it started with"
    );
    assert!(
        start.get(&STONE.0).is_some_and(|n| *n > 0) && start.get(&EGG.0).is_some_and(|n| *n > 0),
        "the conservation check is only worth anything over a non-empty world: {start:?}"
    );
}
