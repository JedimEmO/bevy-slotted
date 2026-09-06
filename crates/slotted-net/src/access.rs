//! The [`Access`] port: may this peer touch this?
//!
//! Two questions, asked at two moments.
//!
//! * [`may_open`](Access::may_open) is asked once per bound container when a
//!   session opens. It is where a game says "only a player standing next to
//!   the chest, and only if it is not locked".
//! * [`may_act`](Access::may_act) is asked on *every* message that names a
//!   session, before the server reads or writes anything. It is where a game
//!   revokes access that was granted a moment ago: the player walked away,
//!   the block was broken, the trade was cancelled.
//!
//! The server also enforces two rules the port cannot switch off, because they
//! are invariants rather than policy: a session belongs to exactly one peer
//! and answers nobody else, and an inventory marked
//! [`Owner::Private`](crate::Owner::Private) can only be bound into its own
//! player's sessions. A permissive `Access` cannot let one player reach into
//! another's pockets.

use slotted_model::{InventoryId, MenuId};

use crate::message::PeerId;

/// What the server can tell an [`Access`] adapter about a session.
#[derive(Debug, Clone, Copy)]
pub struct SessionInfo<'a> {
    /// The session's id, which is also the menu id its owner uses.
    pub menu: MenuId,
    /// The one peer this session answers to.
    pub owner: PeerId,
    /// The containers it is bound to, in [`InventoryRef`](slotted_model::InventoryRef)
    /// order.
    pub containers: &'a [InventoryId],
}

/// Who may open what, and who may act on it afterwards.
///
/// The default methods are [`OwnerOnly`]: any container may be opened, and
/// only the session's own peer may act on it. Override `may_open` to add a
/// distance or lock check; override `may_act` to be able to take access away.
pub trait Access: Send + Sync + std::fmt::Debug {
    /// May `peer` bind `container` into a new session?
    ///
    /// Asked once per container at open time. The default allows every
    /// container; the private-inventory rule is enforced by the server
    /// regardless of the answer.
    fn may_open(&self, peer: PeerId, container: InventoryId) -> bool {
        let _ = (peer, container);
        true
    }

    /// May `peer` read or write through `session` right now?
    ///
    /// Asked on every message that names a session, after the server has
    /// already established that `peer` owns it, and before anything is read.
    /// The default allows it; a game overrides this to take access away again
    /// when the player walks off or the block is broken.
    fn may_act(&self, peer: PeerId, session: SessionInfo<'_>) -> bool {
        let _ = (peer, session);
        true
    }
}

/// The default policy: a session answers its own peer and nobody else, and
/// any container may be opened.
///
/// The "owner only" half is not implemented here, because it is not a policy
/// the server would let an adapter switch off: see the [module docs](self).
/// This type adds nothing on top of it, which is what makes it the default.
#[derive(Debug, Clone, Copy, Default)]
pub struct OwnerOnly;

impl Access for OwnerOnly {}

/// A policy that says no to every open and every action. For testing the
/// refusal paths.
#[derive(Debug, Clone, Copy, Default)]
pub struct DenyAll;

impl Access for DenyAll {
    fn may_open(&self, _peer: PeerId, _container: InventoryId) -> bool {
        false
    }

    fn may_act(&self, _peer: PeerId, _session: SessionInfo<'_>) -> bool {
        false
    }
}
