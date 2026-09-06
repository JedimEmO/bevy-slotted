//! The `Authority` port: who gets the final say on a click.
//!
//! The client applies every action locally through
//! [`apply_click`](crate::click::apply_click) for instant feedback, then
//! submits the action and its predicted [`Delta`] to an `Authority`. A local
//! authority applies and acks immediately. A networked one serialises the
//! action, exactly like vanilla's click packet, and answers with an ack or a
//! full resync when its own `state_id` disagrees.

use serde::{Deserialize, Serialize};

use crate::click::{ClickAction, ClickError, Delta};
use crate::inventory::Inventories;
use crate::menu::{MenuState, PropertyId};

/// Identifies one open menu to the authority (vanilla's window id).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MenuId(pub u32);

/// Everything needed to overwrite a client's view of a menu.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MenuSnapshot {
    /// The inventories the menu addresses.
    pub inventories: Inventories,
    /// Carried stack, state id and properties.
    pub state: MenuState,
}

/// What an authority reports back.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AuthorityEvent {
    /// The action was accepted; the authority is now at `state_id`.
    Ack {
        /// Which menu.
        menu: MenuId,
        /// Authority state id after the action.
        state_id: u32,
    },
    /// The prediction diverged; replace everything with `snapshot`.
    Resync {
        /// Which menu.
        menu: MenuId,
        /// The authoritative content.
        snapshot: MenuSnapshot,
    },
    /// One slot of the menu now holds something else.
    ///
    /// The cheap half of a resync: an authority that knows exactly what
    /// changed sends these rather than a whole snapshot. Nobody asked for it,
    /// so it settles no round trip. A `Ghost` or `Filter` slot carries its
    /// hint here; every other slot carries a real stack.
    Slot {
        /// Which menu.
        menu: MenuId,
        /// Which slot of it.
        slot: crate::menu::SlotIx,
        /// The new content.
        stack: Option<crate::stack::ItemStack>,
    },
    /// A synced property changed.
    Property {
        /// Which menu.
        menu: MenuId,
        /// Which property.
        id: PropertyId,
        /// New value.
        value: i32,
    },
}

/// Why a submission failed outright (before any ack or resync).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, thiserror::Error)]
pub enum AuthorityError {
    /// The authority evaluated the action and refused it.
    #[error("authority rejected the action: {0}")]
    Rejected(ClickError),
    /// The menu is not open on the authority.
    #[error("menu {0:?} is not open")]
    UnknownMenu(MenuId),
    /// The authority is unreachable.
    #[error("authority disconnected")]
    Disconnected,
}

/// What came of asking an authority for a full snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ResyncRequest {
    /// The request is on its way. An
    /// [`AuthorityEvent::Resync`] for this menu will arrive from a later
    /// [`Authority::poll`], so the caller counts it as a round trip in
    /// flight.
    Pending,
    /// This authority cannot produce a snapshot, and no event will follow.
    /// A single-player [`LocalAuthority`](crate::Authority) is the case: it
    /// holds no second copy of the world to resynchronise from, so the
    /// caller falls back to redrawing from its own state.
    Unsupported,
}

/// The boundary between prediction and truth.
pub trait Authority: Send + Sync {
    /// Submits an action with the client's predicted outcome.
    fn submit(
        &self,
        menu: MenuId,
        action: ClickAction,
        predicted: &Delta,
    ) -> Result<(), AuthorityError>;

    /// Drains pending events.
    fn poll(&self) -> Vec<AuthorityEvent>;

    /// Asks for a full snapshot of `menu`.
    ///
    /// The client calls this when it can no longer trust its own copy: a
    /// [`submit`](Self::submit) came back `Err`, or an ack arrived for a
    /// state id the client never predicted. The answer, when there is one,
    /// comes back through [`poll`](Self::poll) as
    /// [`AuthorityEvent::Resync`] rather than from this call, so a transport
    /// with latency behaves the same as one without.
    ///
    /// The default implementation answers
    /// [`ResyncRequest::Unsupported`], which is right for any authority that
    /// is itself the client's state.
    fn request_resync(&self, menu: MenuId) -> Result<ResyncRequest, AuthorityError> {
        let _ = menu;
        Ok(ResyncRequest::Unsupported)
    }

    /// How hard this authority wants clicks validated before they are
    /// believed. See [`ValidationLevel`](crate::ValidationLevel).
    ///
    /// A client asks its authority rather than deciding for itself, so that
    /// wiring in a server-backed authority also turns on the checking that
    /// server expects. The default is
    /// [`ValidationLevel::Debug`](crate::ValidationLevel::Debug).
    fn validation(&self) -> crate::ValidationLevel {
        crate::ValidationLevel::Debug
    }
}
