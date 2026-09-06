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
//! section 4.1.
//!
//! # Example
//!
//! One client, one server, one chest, over a perfect in-process link.
//!
//! ```
//! use slotted_model::{
//!     Actor, Authority, Button, ClickAction, Inventories, ItemId, ItemStack, LookupCtx, MenuDef,
//!     MenuId, MenuState, Namespaced, SlotIx, apply_click,
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
//! let mut inventories = Inventories::for_menu(&def);
//! inventories[MenuDef::CONTAINER].set(0, Some(ItemStack::new(ItemId(1), 32)));
//!
//! let link = Loopback::new(1);
//! let client_end = link.client(PeerId(1));
//! let server_end = link.server();
//!
//! let mut server = MenuServer::new();
//! server.open(MenuId(1), def.clone(), inventories.clone(), Actor::SURVIVAL);
//! server.add_viewer(MenuId(1), PeerId(1));
//!
//! let authority = RemoteAuthority::new(client_end);
//!
//! // The client predicts picking the stack up, then submits.
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

pub mod client;
pub mod message;
pub mod server;
pub mod transport;

pub use client::RemoteAuthority;
pub use message::{ClientMessage, PeerId, ServerMessage};
pub use server::{MenuServer, Outcome};
pub use transport::{
    BoxedClientTransport, BoxedServerTransport, ClientEnd, ClientTransport, Conditions, Loopback,
    ServerEnd, ServerTransport, Transport, TransportError,
};

// Re-exported so a consumer of this crate can name a menu without also
// depending on the model crate directly.
pub use slotted_model::{Authority, AuthorityEvent, MenuId, MenuSnapshot};
