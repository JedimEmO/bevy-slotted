# slotted-net

A networked [`Authority`](https://docs.rs/slotted-model) for
[slotted](https://github.com/mathiasmyrland/bevy-slotted): the client predicts,
the server decides, and neither one knows what carries the bytes between them.

* `RemoteAuthority` is the client half. It implements `slotted_model::Authority`,
  so dropping it in place of `LocalAuthority` is the whole change a game makes.
* `MenuServer` is the server half. It validates every click with the same
  `apply_click` the client ran, at `ValidationLevel::Always`, against its own
  copy of the menu.
* `Transport` is the port between them. `Loopback` is the in-process adapter,
  and it can add latency, reorder and drop messages so a test can see what a
  bad connection does.
* `Access` is the port that decides who may open a container and who may act
  through a session. `OwnerOnly` is the default.

The server holds a `ContainerStore` of inventories and one *session* per open
screen. A session belongs to exactly one peer, owns that player's cursor, drag,
hints and properties, and binds one `InventoryId` per inventory its definition
addresses. Two players at one chest are two sessions binding one container:
they share the items and nothing else, and each one's slot indices are its own.
Inventories can be marked private to a player, and no access policy can bind
one of those into somebody else's session.

Every message that names a session is authorised before anything is read, and a
peer naming another peer's session is answered `Refused` and told nothing else.
A refusal is not an answer to the click it refused, so it neither records nor
consumes anything in the window below: access taken away and given back leaves
the session able to replay its own history.

Every click's answer is recorded per sequence number, so a retransmission
replays the kind of answer it first got: a lost correction repeats as a
correction rather than becoming an ack.

Closing is reliable in both directions. `CloseMenu` returns whatever the
session was carrying to that player's own pockets before the session goes -- a
cursor holds items that have already left a container, so forgetting it would
be an item sink any client could trigger by pressing escape. And the request
itself is repeated until the server answers, because a lost one would leave a
session open on the server, bound to a player's private inventories, with
nobody left who would ever close it.

See `docs/PLAN.md` section 4.13, `docs/design/gaps-notes-A.md` and
`docs/design/review-notes-A.md`.
