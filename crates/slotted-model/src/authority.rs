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
}
