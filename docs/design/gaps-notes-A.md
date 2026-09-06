# Gaps notes A: authority, validation and the networked adapter

Package A of the gap-closing pass. It finishes the story `docs/PLAN.md` 4.1
started and left at "a networked adapter would do this": hints out of the
inventory, conservation as a runtime choice, a resync a client can ask for,
component ids that survive a reload, and `slotted-net`.

## 1. Ghost and filter hints live on `MenuState`

**Was.** A `Ghost` or `Filter` slot stored its hint as a real count-one stack
in the inventory cell its `SlotDef` pointed at. `Inventories::count_of` saw
it, so a recipe filter showing cobblestone made the game think the player had
one more cobblestone than they had. The conservation check papered over this
by listing every ghost cell and skipping it, which meant the check needed the
`MenuDef` to know what an item was.

**Is.** `MenuState::hints: BTreeMap<SlotIx, ItemStack>`, read and written
through `MenuState::hint` and `MenuState::set_hint`. Nothing else changed
about how a ghost slot behaves: an empty hand clears it, a full hand sets it
without consuming, a quick-move clears it, bulk moves skip it.

**Why the state and not the inventory.** A hint is keyed by *menu slot*, not
by inventory cell, and two menus over one inventory should not share one. It
is also per-open-menu by nature: closing the screen should not leave a
phantom in the chest. `MenuState` already travels inside `MenuSnapshot`, so
hints replicate with no new message.

**What reads it.**

- `slotted_model::slot_view(def, inv, state, ix)` is the new "what does this
  slot show" accessor and is what everything drawing a menu should call.
  `apply_click`'s `Delta` already carries hint content for ghost slots, so
  the ordinary render path needed no change.
- `slotted_ecs`: `read_slots`, `register_slot_refs` (the seeding of a freshly
  spawned slot entity), `apply_set_slot` (a host write to a ghost slot sets
  the hint) and the new `apply_slot_push`.
- `slotted_test::Queries::stack_at`.
- `slotted-ui` needed nothing: its phantom preview already went through
  `preview_drag(&menu.state, ..)`, and its `SlotHint` glyph is derived from
  the `SlotDef` alone.

**One behaviour did change, deliberately.** `SlotFanout` no longer fans a
ghost slot out to the other menus that draw the same inventory cell, because
there is no longer a cell involved. A ghost slot reaches its own menu's slot
entity and nothing else.

## 2. `ValidationLevel`

`apply_click` keeps its signature and its behaviour. `apply_click_validated`
takes one more argument:

| Level | Debug build | Release build | On violation |
| --- | --- | --- | --- |
| `Off` | no check | no check | — |
| `Debug` (default) | checks | no check | panics |
| `Always` | checks | checks | `Err(ClickError::Conservation)` |

`Always` reports the failure *after* the action has run, because that is when
the tally can be compared. A caller at that level must apply to a scratch
copy and commit only on `Ok`; `MenuServer::click` does exactly that, and the
doc comment on `ClickError::Conservation` says so.

The level is not a global. `Authority::validation()` reports it and
`slotted_ecs::predict` asks the authority resource, so wiring in a
server-backed authority also turns on the checking that server expects.
`LocalAuthority::with_validation` lets a single-player game opt into `Always`
in its shipped build.

The conservation tally itself got simpler rather than more complicated: with
hints out of the inventories it no longer needs the `MenuDef` to know which
cells to skip.

## 3. A refused submission can ask for the truth

`Authority` gained two defaulted methods, so no existing implementation
broke:

```rust
fn request_resync(&self, menu: MenuId) -> Result<ResyncRequest, AuthorityError>;
fn validation(&self) -> ValidationLevel;
```

`ResyncRequest::Pending` means "an `AuthorityEvent::Resync` for this menu is
coming"; `Unsupported` means "I am your own state, there is nothing to
resynchronise from" and is the default, which is right for `LocalAuthority`.

`slotted_ecs::submit` now asks on a refusal, and only falls back to
re-emitting the menu from local state when the answer is `Unsupported`. The
round-trip accounting stays exact because `Pending` counts as one round trip
and the `Resync` that answers it settles one.

`AuthorityEvent` also gained a `Slot { menu, slot, stack }` variant: the
cheap half of a resync, for an authority that knows exactly what changed.
Nobody asked for it, so it settles no round trip. `slotted_ecs::reconcile`
handles it through `apply_slot_push`.

## 4. `ComponentPatch` across a reload

`slotted_packs::remap_inventories` looked up every live stack's item by name
and renumbered it, and left the `ComponentPatch` keys alone. Those keys are
interned exactly like item ids and are renumbered exactly as freely, so a
reload could turn `slotted:damage = 40` into whatever component now holds
that dense id. The patch is now rebuilt by name alongside the item; a key the
new registry has lost is dropped and reported as `ModError::ComponentVanished`.
The same pass now also walks `OpenMenu::state` so ghost hints and the carried
stack are remapped, not just inventories.

## 5. `slotted-net`

```
ClientMessage  ::= ClickContainer { menu, state_id, seq, action, predicted } | RequestResync { menu }
ServerMessage  ::= Ack { menu, state_id, seq } | SetSlot { menu, slot, stack }
                 | SetContent { menu, snapshot } | SetProperty { menu, id, value }
```

Vanilla's shape, with two additions.

- **`predicted: Delta`.** The server can then answer "you were right" with an
  ack of a few bytes instead of the whole container. Vanilla sends the
  container back on every click; this does not.
- **`seq`.** Vanilla's `state_id` cannot identify a click, because
  `Drag { Start }` and `Drag { Add }` leave it unchanged, so two different
  clicks can carry the same one. Without a per-click identity a retransmitted
  click is indistinguishable from a new one, and the retry after a dropped
  packet would apply the action twice. `state_id` is still sent and still
  used for what it is good at: noticing a client whose copy has drifted.

**Client.** `RemoteAuthority<T>` implements `slotted_model::Authority`, so
swapping it in for `LocalAuthority` is the whole change a game makes.
`poll` is its clock: each call delivers what arrived and retransmits any
click unanswered for `retry_after` polls. A snapshot retires exactly one
in-flight click, and an ack for a submission it does not recognise is still
reported before a resync is asked for, both so that the caller's round-trip
count cannot get stuck above zero.

**Server.** `MenuServer` holds def, inventories, state, actor and viewers per
menu. Every click is applied to a scratch copy at `ValidationLevel::Always`
and committed only on `Ok`. The clicking client gets an ack when its
prediction matched exactly *and* its `state_id` agreed; otherwise it gets the
container. Other viewers get `SetSlot` per changed slot, and the changed set
comes off the inventories' dirty masks rather than a diff, so a 27-slot sort
costs one pass over a bitset. Hints have no mask and come from the delta.

**Transport.** Two methods, `send` and `poll`, plus `peers`. `Loopback` is
the in-process adapter: a tick-driven clock, per-message latency, jitter that
reorders a stream, and a deterministic drop percentage. Nothing runs on its
own, so a test decides when the network gets to be slow.

Tests in `crates/slotted-net/tests/loopback.rs` cover the happy path, a
server refusing a move and the client repainting only the slots that
differed, reordered acks, a dropped click retried and applied once, two
clients on one container, and a thousand random actions over a five percent
lossy link ending with both sides on the same items and the same slots.

## 6. `bevy_replicon`, not done

The `replicon` feature is not in the crate. The adapter would be small and it
is written down here rather than guessed at:

```rust
// A `Transport` over replicon's own channels.
struct RepliconClient<'w> { events: EventWriter<'w, ToServer<ClientMessage>>, .. }
```

- Server to client: one `ServerEvent` per `ServerMessage`, sent with
  `ToClients { mode: SendMode::Direct(entity), event }`. `SendMode::Direct`
  is what makes the per-viewer `SetSlot` fan-out land on the right client.
- Client to server: one `ClientEvent` for `ClientMessage`, read on the server
  as `FromClient { client_entity, event }`, which is the `PeerId`.
- Channels: `SetContent` on a reliable ordered channel, `SetSlot` and `Ack`
  on a reliable unordered one. The protocol already tolerates reordering, so
  ordering guarantees buy nothing but head-of-line blocking.
- `Transport::poll` becomes a drain of a buffer that a replicon system fills,
  because replicon delivers through systems and this port is pull-shaped.
  That buffer is the only real work in the adapter.

It was left out because the messages, the client, the server and the
conditions harness are all testable without it, and a dependency on a
replication crate is not something to add on a guess about its Bevy 0.19
support.
