//! The wire vocabulary: what a client says to a server and back.
//!
//! The shape follows vanilla's container protocol. A client sends one packet
//! per click carrying the action and the state id it believed it was at; the
//! server answers with an ack, individual slot writes, a full container
//! content, or a property update. See `docs/research/research-mc-anatomy.md`
//! section 3.
//!
//! Every message is `Serialize + Deserialize`, and nothing in this module
//! knows how the bytes travel.

use serde::{Deserialize, Serialize};

use slotted_model::{ClickAction, Delta, ItemStack, MenuId, MenuSnapshot, PropertyId, SlotIx};

/// One connected client, as the transport names it.
///
/// The server addresses clients by this and nothing else. `PeerId::SERVER` is
/// the name a client uses for its one peer, so that both directions of a
/// [`Transport`](crate::Transport) share one signature.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PeerId(pub u64);

impl PeerId {
    /// The peer a client sends to: its server.
    pub const SERVER: Self = Self(0);
}

/// Client to server.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ClientMessage {
    /// A click the client has already applied to its own copy.
    ///
    /// Vanilla's `ServerboundContainerClickPacket`, plus the predicted
    /// [`Delta`], which lets the server answer "you were right" with an
    /// [`Ack`](ServerMessage::Ack) instead of a whole container.
    ClickContainer {
        /// Which menu.
        menu: MenuId,
        /// The client's state id *before* the action, as vanilla's packet
        /// carries. The server compares it with its own to notice a client
        /// that has drifted.
        state_id: u32,
        /// Strictly increasing per menu, per client. Two clicks never share a
        /// sequence number, but a *retransmission* of one click reuses its
        /// own, which is how the server tells a resend from a new action: a
        /// state id cannot do that job, because `Drag { Start }` and
        /// `Drag { Add }` both leave it alone.
        seq: u32,
        /// What the player did.
        action: ClickAction,
        /// What the client's own [`apply_click`](slotted_model::apply_click)
        /// made of it.
        predicted: Delta,
    },
    /// "I no longer trust my copy of this menu; send me all of it."
    ///
    /// Answered with [`ServerMessage::SetContent`].
    RequestResync {
        /// Which menu.
        menu: MenuId,
    },
}

impl ClientMessage {
    /// The menu this message is about.
    pub fn menu(&self) -> MenuId {
        match self {
            Self::ClickContainer { menu, .. } | Self::RequestResync { menu } => *menu,
        }
    }
}

/// Server to client.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ServerMessage {
    /// The click was accepted and the prediction was right. Nothing else
    /// needs to be sent, which is the case this whole design optimises for.
    Ack {
        /// Which menu.
        menu: MenuId,
        /// The server's state id after the action.
        state_id: u32,
        /// The [`ClickContainer`](ClientMessage::ClickContainer) sequence
        /// number this answers.
        seq: u32,
    },
    /// One slot of the menu changed. Vanilla's `ClientboundContainerSetSlot`.
    ///
    /// The server sends these to the *other* viewers of a container after a
    /// click, and to the clicking client only for the slots its prediction
    /// missed.
    SetSlot {
        /// Which menu.
        menu: MenuId,
        /// Which slot.
        slot: SlotIx,
        /// Its new content. For a `Ghost` or `Filter` slot this is the hint.
        stack: Option<ItemStack>,
    },
    /// The whole container. Vanilla's `ClientboundContainerSetContent`.
    ///
    /// The answer to a refused click, a state id mismatch or an explicit
    /// [`RequestResync`](ClientMessage::RequestResync).
    SetContent {
        /// Which menu.
        menu: MenuId,
        /// Everything the client should replace its copy with.
        snapshot: Box<MenuSnapshot>,
    },
    /// A synced integer property changed. Vanilla's
    /// `ClientboundContainerSetData`.
    SetProperty {
        /// Which menu.
        menu: MenuId,
        /// Which property.
        id: PropertyId,
        /// Its new value.
        value: i32,
    },
}

impl ServerMessage {
    /// The menu this message is about.
    pub fn menu(&self) -> MenuId {
        match self {
            Self::Ack { menu, .. }
            | Self::SetSlot { menu, .. }
            | Self::SetContent { menu, .. }
            | Self::SetProperty { menu, .. } => *menu,
        }
    }
}
