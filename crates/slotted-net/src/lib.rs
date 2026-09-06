//! A networked [`slotted_model::Authority`] for slotted: the
//! client predicts, the server decides, and neither knows what carries the
//! bytes.
//!
//! Three pieces, in the order a game meets them.
//!
//! * [`RemoteAuthority`] is the client. It implements
//!   `slotted_model::Authority`, so putting one where a `LocalAuthority` was
//!   is the whole change from single-player to client-server: prediction,
//!   reconciliation and the round-trip accounting are already written against
//!   the port.
//! * [`MenuServer`] is the server. It runs the same
//!   [`apply_click`](slotted_model::apply_click) the client ran, against its
//!   own copy, at
//!   [`slotted_model::ValidationLevel::Always`], and
//!   answers with an ack, a slot write or the whole container.
//! * [`Transport`] is the port between them, two methods wide.
//!   [`Loopback`] is the in-process adapter, and it can add latency, reorder
//!   and drop messages, which is how the tests here see what a bad connection
//!   does without opening a socket.
//!
//! The protocol mirrors vanilla's: one packet per click carrying the action
//! and the state id the client believed it was at, answered by an ack, a
//! `SetSlot`, a `SetContent` or a `SetProperty`. See
//! `docs/research/research-mc-anatomy.md` section 3 and `docs/PLAN.md`
//! section 4.13.
//!
//! # Sessions, containers and who may touch them
//!
//! The server does not hold "a menu". It holds a [`ContainerStore`] of
//! inventories, each either shared or private to one player, and a *session*
//! per open screen. A session belongs to exactly one peer, owns that player's
//! cursor, drag, hints and properties, and binds one [`InventoryId`] per
//! inventory its definition addresses. Two players at one chest are two
//! sessions binding one container id: they see each other's items and not
//! each other's cursors, and each writes into one inventory rather than into
//! a private copy of it.
//!
//! Every message that names a session is authorised before anything is read.
//! The server checks the peer owns it, then asks the [`Access`] port whether
//! it may act through it right now; a peer naming somebody else's session is
//! answered [`ServerMessage::Refused`] and told nothing else. [`OwnerOnly`]
//! is the default policy, and even a permissive one cannot bind an inventory
//! private to one player into another player's session.
//!
//! # Example
//!
//! One client, one server, one chest, over a perfect in-process link.
//!
//! ```
//! use slotted_model::{
//!     Actor, Authority, Button, ClickAction, Inventories, Inventory, ItemId, ItemStack, LookupCtx,
//!     MenuDef, MenuId, MenuState, Namespaced, SlotIx, apply_click,
//! };
//! use slotted_net::{Loopback, MenuServer, PeerId, RemoteAuthority};
//!
//! struct Items;
//! impl LookupCtx for Items {
//!     fn max_stack(&self, _id: ItemId) -> u32 { 64 }
//!     fn has_tag(&self, _id: ItemId, _tag: &Namespaced) -> bool { false }
//! }
//!
//! let def = MenuDef::chest(1);
//! let sizes = def.inventory_sizes();
//! let alice = PeerId(1);
//!
//! // The chest is shared; the player's own inventories are not.
//! let mut server = MenuServer::new();
//! // `from_slots` seeds without marking anything dirty; a `set` here would
//! // have the server open the session by telling the client about the slot.
//! let chest = Inventory::from_slots(
//!     (0..sizes[0]).map(|i| (i == 0).then(|| ItemStack::new(ItemId(1), 32))),
//! );
//! let container = server.add_shared(chest);
//! let bindings: Vec<_> = std::iter::once(container)
//!     .chain(sizes[1..].iter().map(|n| server.add_private(alice, Inventory::new(*n))))
//!     .collect();
//! server.open(MenuId(1), alice, def.clone(), bindings, Actor::SURVIVAL).unwrap();
//!
//! let link = Loopback::new(1);
//! let server_end = link.server();
//! let authority = RemoteAuthority::new(link.client(alice));
//!
//! // The client predicts picking the stack up, then submits.
//! let mut inventories = server.snapshot(MenuId(1)).unwrap().inventories;
//! let mut state = MenuState::new(&def);
//! let action = ClickAction::Pickup { slot: SlotIx(0), button: Button::Left };
//! let delta = apply_click(&def, &mut inventories, &mut state, action, &Actor::SURVIVAL, &Items)?;
//! authority.submit(MenuId(1), action, &delta).unwrap();
//!
//! // The server agrees, so all that comes back is an ack.
//! server.pump(&server_end, &Items);
//! assert_eq!(server.slot(MenuId(1), SlotIx(0)), None);
//! assert!(matches!(
//!     authority.poll().as_slice(),
//!     [slotted_model::AuthorityEvent::Ack { state_id: 1, .. }]
//! ));
//! # Ok::<(), slotted_model::ClickError>(())
//! ```

pub mod access;
pub mod client;
pub mod message;
pub mod server;
pub mod store;
pub mod transport;

pub use access::{Access, DenyAll, OwnerOnly, SessionInfo};
pub use client::RemoteAuthority;
pub use message::{ClientMessage, PeerId, Refusal, ServerMessage};
pub use server::{MenuServer, OpenError, Outcome};
pub use store::{Container, ContainerStore, Owner};
pub use transport::{
    BoxedClientTransport, BoxedServerTransport, ClientEnd, ClientTransport, Conditions, Loopback,
    ServerEnd, ServerTransport, Transport, TransportError,
};

// Re-exported so a consumer of this crate can name a menu without also
// depending on the model crate directly.
pub use slotted_model::{Authority, AuthorityEvent, InventoryId, MenuId, MenuSnapshot};
