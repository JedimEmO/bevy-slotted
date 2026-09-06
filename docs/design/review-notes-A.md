# Review notes A: authorisation, sessions and corrections in `slotted-net`

Package A of the external review pass. Three findings, all P1, all in
`crates/slotted-net`, plus one additive change in `slotted-model`.

## 1. The server acted on any message that named a menu it had

**Was.** `MenuServer::handle` checked that the menu existed and then acted.
The viewer list decided who was *notified*, not who was *allowed*: any peer
could click, resync or otherwise drive any open menu on the server, and would
be sent its whole contents on a mismatch.

**Is.** A menu id names a *session*, and a session belongs to exactly one
peer. Every message goes through one gate, `MenuServer::authorize`, before
anything is read:

1. the session exists, else `Refusal::UnknownMenu`;
2. the sending peer owns it, else `Refusal::NotYours`;
3. `Access::may_act(peer, session)` says yes, else `Refusal::Denied`.

The answer to a failed check is `ServerMessage::Refused { menu, seq, reason }`
and a `warn` log. It carries no state, not a snapshot and not a slot: a peer
must not learn what is in a container it has no business with. The client
counts refusals, retires whatever it had in flight for that menu, and stops
asking, so a refusal does not turn into a retransmission loop.

**The `Access` port.**

```rust
pub trait Access: Send + Sync + Debug {
    fn may_open(&self, peer: PeerId, container: InventoryId) -> bool;
    fn may_act(&self, peer: PeerId, session: SessionInfo<'_>) -> bool;
}
```

`may_open` is asked once per bound container when a session opens; it is where
a game puts a distance or lock check. `may_act` is asked on every message, so
access can be taken away again mid-session. `OwnerOnly` is the default and
adds nothing on top of the server's own rules, which is the point: ownership
and privacy are invariants the port cannot switch off.

## 2. Viewers shared state that belongs to a player

**Was.** `ServerMenu` owned one `MenuState`, one `Actor` and one `Inventories`
for every viewer. Two players at one chest shared a cursor, a drag and a
player inventory. Two menus over one chest, opened separately, each got a
private *copy* of the chest, so their edits never met.

**Is.** The two halves are modelled separately.

- `ContainerStore` holds inventories keyed by `InventoryId`, each `Shared` or
  `Private(peer)`, along with the synced properties of whatever they are part
  of. It hands out ids and never reuses one.
- A session holds the peer, the `MenuDef`, the actor, its own `MenuState`
  (carried stack, drag, ghost hints, property values, state id) and a
  `Vec<InventoryId>`: one binding per `InventoryRef` the definition addresses.

`MenuServer::open(menu, peer, def, bindings, actor)` checks every binding for
existence, size, privacy and `may_open` before it creates anything, so a
refused open leaves nothing behind. Binding an inventory that is
`Private` to another peer is `OpenError::NotYours` regardless of policy.

A click gathers the bound inventories into an `Inventories`, applies to that
scratch copy at `ValidationLevel::Always`, and on `Ok` writes them back into
the store. The dirty masks then say which `(InventoryId, cell)` pairs changed,
and every session bound to one of them is sent a `SetSlot` **at its own slot
index**: one container is `InventoryRef::new(0)` in one session and
`new(1)` in another, and a raw slot index means nothing across the two.
Hints never travel this way, because a hint is session state and has no cell.

Properties moved with them. `set_property` is addressed by `InventoryId`
rather than by menu: two players watching one furnace see one burn time, and a
session opened later is seeded from the container's stored values rather than
from the definition's initial ones.

A host writing straight into the store through `store_mut()` is a supported
way to change the world; `MenuServer::pump` flushes the masks before it
handles any message, so those writes reach the sessions without the host doing
anything else.

**`InventoryId`** is the one addition to `slotted-model`. `InventoryRef` says
*which of the inventories this menu addresses*; `InventoryId` says *which
inventory in the world*. Both questions are real and a server needs both.

## 3. A lost correction could not be recovered

**Was.** The server kept `(last_seq, acked_state_id)` per viewer. A
retransmitted click was answered with an ack whatever the original answer had
been, so a correction that the link ate was replaced by an ack on the retry.
The client accepted the ack, retired the submission, and never asked for the
snapshot again: permanently divergent, from one dropped packet. Separately, a
snapshot arriving at the client retired *the oldest* in-flight submission on
that menu, which is the wrong one as soon as two are outstanding.

**Is.** Both halves are correlated.

- The server records `Response::Ack(state_id)` or `Response::Correction` per
  `(session, seq)` in a bounded window (`MenuServer::DEFAULT_WINDOW`, 64,
  settable with `window()`). A retransmission replays the *kind*: an ack
  repeats as an ack, a correction repeats as the container as it is now. A
  sequence number older than the window gets the container, which is always
  safe.
- `ServerMessage::SetContent` carries `answers: Option<u32>`. `Some(seq)`
  means it corrects that click; the client retires that submission and every
  older one on the menu. `None` means it answers a `RequestResync` and retires
  nothing.

Retiring several submissions with one snapshot would leave the caller's
`PendingRoundTrips` stuck above zero, so the client reports the extra ones as
`AuthorityEvent::Ack` at the authoritative state id. That is the vocabulary
the `Authority` port has for "this is settled"; no event variant was added, so
`slotted-ecs` needed no change.

## What else changed on the wire

- `ClientMessage::CloseMenu { menu }`, so a client can end its own session,
  answered `ServerMessage::Closed`. A peer closing another peer's session gets
  a refusal and closes nothing.
- `ServerMessage::Refused` and `ServerMessage::Closed`, described above.
- `Outcome::UnknownMenu` became `Outcome::Denied(Refusal)`, and
  `Outcome::Closed` joined it.

`Transport` and `Loopback` are unchanged: `Loopback` was already multi-peer,
with a queue per client and a deterministic tick clock.

## Tests

`crates/slotted-net/tests/sessions.rs`, fifteen of them.

- Every client message kind sent by a peer that does not own the session:
  refused, no state in the answer, nothing moved, the session still open.
- An unknown session id, and a policy that revokes access mid-session.
- A private inventory bound into another peer's session: `OpenError::NotYours`
  and no half-built session.
- Two sessions on one container: edits meet, cursors and player inventories do
  not. A container change reaching two sessions at two different slot indices.
  A property reaching both and seeding a third. A host write reaching both.
  Dropping a peer taking its sessions and its pockets.
- A correction lost and retried, answered with the correction again. An ack
  lost and retried, answered with an ack. A snapshot delivered out of order
  retiring exactly what it answers, with one event per submission. A click
  older than the remembered window.
- A thousand random actions from two peers on one container over a link losing
  one message in ten: both clients end on the server's slots, agree with each
  other about the container, and no items were minted.

`loopback.rs` and `review_adversarial.rs` keep their scenarios and were ported
to sessions; the stale-client case now converges on the prediction mismatch
rather than on a shared state id, because a state id belongs to a session.
