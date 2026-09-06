//! [`MenuServer`]: the copy of a menu that gets the final say.
//!
//! The server runs the same [`apply_click`](slotted_model::apply_click) the
//! client ran, against its own inventories, at
//! [`slotted_model::ValidationLevel::Always`]. A
//! client that agrees gets an ack and nothing else on the wire. A client that
//! disagrees gets the container back.

use std::collections::BTreeMap;

use slotted_model::{
    Actor, ClickError, Delta, Inventories, ItemStack, LookupCtx, MenuDef, MenuId, MenuSnapshot,
    MenuState, PropertyId, SlotIx, ValidationLevel, apply_click_validated, slot_view,
};

use crate::message::{ClientMessage, PeerId, ServerMessage};
use crate::transport::{ServerTransport, TransportError};

/// One menu as the server holds it.
#[derive(Debug, Clone)]
struct ServerMenu {
    def: MenuDef,
    inventories: Inventories,
    state: MenuState,
    actor: Actor,
    /// Clients watching this menu. All of them see every slot that changes.
    viewers: Vec<PeerId>,
    /// The last sequence number accepted from each viewer, and the ack that
    /// answered it. A retransmission repeats the ack instead of the action.
    last_seq: BTreeMap<PeerId, (u32, u32)>,
}

impl ServerMenu {
    fn snapshot(&self) -> MenuSnapshot {
        MenuSnapshot {
            inventories: self.inventories.clone(),
            state: self.state.clone(),
        }
    }
}

/// What one pumped message did, for a caller that wants to log or assert.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// The click was applied and the client's prediction matched.
    Acked,
    /// The click was applied but the prediction was wrong; the client was
    /// sent the whole container.
    Corrected,
    /// The click was refused; the client was sent the whole container.
    Refused(ClickError),
    /// The message was a retransmission of a click already applied. The ack
    /// was repeated and nothing else happened.
    Duplicate,
    /// The client asked for the container and got it.
    Resynced,
    /// The message named a menu this server has not got.
    UnknownMenu,
}

/// Sends a whole container to one peer.
fn send_content<T: ServerTransport>(
    transport: &T,
    peer: PeerId,
    id: MenuId,
    snapshot: MenuSnapshot,
) {
    let _ = transport.send(
        peer,
        ServerMessage::SetContent {
            menu: id,
            snapshot: Box::new(snapshot),
        },
    );
}

/// The menu slots whose content changed, read off the inventories' dirty
/// masks, plus the ghost slots the delta named.
///
/// The masks are the cheap part: one bit per slot, set by every write the
/// model made, so a `Sort` over 27 slots reports 27 changes without comparing
/// anything. This server is the only reader of those masks, so it clears them
/// here and every click starts from a clean one. Hints are not in an
/// inventory and have no mask, so they come from the delta instead.
fn changed_slots(menu: &mut ServerMenu, delta: &Delta) -> Vec<(SlotIx, Option<ItemStack>)> {
    let mut cells: Vec<(usize, usize)> = Vec::new();
    for (index, (_, inventory)) in menu.inventories.iter().enumerate() {
        for cell in inventory.changed().iter() {
            cells.push((index, cell));
        }
    }
    for inventory in menu.inventories.iter_mut() {
        inventory.clear_changed();
    }

    let mut out: Vec<(SlotIx, Option<ItemStack>)> = Vec::new();
    for (index, slot) in menu.def.slots.iter().enumerate() {
        let ix = SlotIx(u16::try_from(index).unwrap_or(u16::MAX));
        let touched = if slot.behaviour.is_ghost() {
            delta.slots.iter().any(|(s, _)| *s == ix)
        } else {
            cells.contains(&(slot.source.index(), usize::from(slot.index)))
        };
        if touched {
            out.push((
                ix,
                slot_view(&menu.def, &menu.inventories, &menu.state, ix).cloned(),
            ));
        }
    }
    out
}

/// The authoritative side of a set of open menus.
///
/// It owns the truth: the definitions, the inventories and the state. It
/// answers [`ClientMessage`]s and pushes [`ServerMessage`]s, and it knows
/// nothing about how they travel.
#[derive(Debug, Default)]
pub struct MenuServer {
    menus: BTreeMap<u32, ServerMenu>,
}

impl MenuServer {
    /// An empty server.
    pub fn new() -> Self {
        Self::default()
    }

    /// Opens `id` over `def` and `inventories`, with no viewers yet.
    ///
    /// # Panics
    /// When `inventories` does not cover every slot `def` references, which
    /// would make every click on the menu fail with `MenuMismatch`.
    pub fn open(&mut self, id: MenuId, def: MenuDef, inventories: Inventories, actor: Actor) {
        let sizes = def.inventory_sizes();
        assert!(
            sizes.iter().enumerate().all(|(k, want)| inventories
                .get(slotted_model::InventoryRef::new(
                    u16::try_from(k).unwrap_or(u16::MAX)
                ))
                .is_some_and(|inv| inv.len() >= *want)),
            "the inventories do not cover every slot of the menu definition"
        );
        let state = MenuState::new(&def);
        self.menus.insert(
            id.0,
            ServerMenu {
                def,
                inventories,
                state,
                actor,
                viewers: Vec::new(),
                last_seq: BTreeMap::new(),
            },
        );
    }

    /// Closes `id`. Messages about it are answered [`Outcome::UnknownMenu`].
    pub fn close(&mut self, id: MenuId) {
        self.menus.remove(&id.0);
    }

    /// Adds `peer` to the viewers of `id`. A viewer sees every slot of the
    /// menu that changes, whoever changed it.
    pub fn add_viewer(&mut self, id: MenuId, peer: PeerId) {
        if let Some(menu) = self.menus.get_mut(&id.0)
            && !menu.viewers.contains(&peer)
        {
            menu.viewers.push(peer);
        }
    }

    /// Removes `peer` from the viewers of `id`.
    pub fn remove_viewer(&mut self, id: MenuId, peer: PeerId) {
        if let Some(menu) = self.menus.get_mut(&id.0) {
            menu.viewers.retain(|p| *p != peer);
        }
    }

    /// The server's own view of `id`, for tests and for seeding a client that
    /// has just opened the menu.
    pub fn snapshot(&self, id: MenuId) -> Option<MenuSnapshot> {
        self.menus.get(&id.0).map(ServerMenu::snapshot)
    }

    /// What slot `slot` of menu `id` holds, hints included.
    pub fn slot(&self, id: MenuId, slot: SlotIx) -> Option<ItemStack> {
        let menu = self.menus.get(&id.0)?;
        slot_view(&menu.def, &menu.inventories, &menu.state, slot).cloned()
    }

    /// The server's state id for `id`.
    pub fn state_id(&self, id: MenuId) -> Option<u32> {
        self.menus.get(&id.0).map(|m| m.state.state_id)
    }

    /// Sends the whole container to `peer`.
    pub fn send_content<T: ServerTransport>(
        &self,
        transport: &T,
        peer: PeerId,
        id: MenuId,
    ) -> Result<(), TransportError> {
        let Some(menu) = self.menus.get(&id.0) else {
            return Ok(());
        };
        transport.send(
            peer,
            ServerMessage::SetContent {
                menu: id,
                snapshot: Box::new(menu.snapshot()),
            },
        )
    }

    /// Sets a synced property and tells every viewer.
    pub fn set_property<T: ServerTransport>(
        &mut self,
        transport: &T,
        id: MenuId,
        property: PropertyId,
        value: i32,
    ) -> Result<(), TransportError> {
        let Some(menu) = self.menus.get_mut(&id.0) else {
            return Ok(());
        };
        if let Some(position) = menu.def.properties.iter().position(|p| p.id == property)
            && let Some(slot) = menu.state.properties.get_mut(position)
        {
            *slot = value;
        }
        let viewers = menu.viewers.clone();
        let mut first = None;
        for peer in viewers {
            if let Err(error) = transport.send(
                peer,
                ServerMessage::SetProperty {
                    menu: id,
                    id: property,
                    value,
                },
            ) && first.is_none()
            {
                first = Some(error);
            }
        }
        first.map_or(Ok(()), Err)
    }

    /// Handles everything waiting on `transport` and answers it.
    ///
    /// Call once per server tick. `ctx` is the same registry lookup the
    /// clients run against; a disagreement about stack sizes would otherwise
    /// show up as a stream of corrections.
    pub fn pump<T: ServerTransport>(&mut self, transport: &T, ctx: &dyn LookupCtx) -> Vec<Outcome> {
        transport
            .poll()
            .into_iter()
            .map(|(peer, message)| self.handle(transport, ctx, peer, message))
            .collect()
    }

    fn handle<T: ServerTransport>(
        &mut self,
        transport: &T,
        ctx: &dyn LookupCtx,
        peer: PeerId,
        message: ClientMessage,
    ) -> Outcome {
        let id = message.menu();
        if !self.menus.contains_key(&id.0) {
            tracing::debug!(?peer, ?id, "message about a menu this server has not got");
            return Outcome::UnknownMenu;
        }
        match message {
            ClientMessage::RequestResync { menu } => {
                let _ = self.send_content(transport, peer, menu);
                Outcome::Resynced
            }
            ClientMessage::ClickContainer {
                menu,
                state_id,
                seq,
                action,
                predicted,
            } => self.click(
                transport, ctx, peer, menu, state_id, seq, action, &predicted,
            ),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn click<T: ServerTransport>(
        &mut self,
        transport: &T,
        ctx: &dyn LookupCtx,
        peer: PeerId,
        id: MenuId,
        state_id: u32,
        seq: u32,
        action: slotted_model::ClickAction,
        predicted: &Delta,
    ) -> Outcome {
        let Some(menu) = self.menus.get_mut(&id.0) else {
            return Outcome::UnknownMenu;
        };

        // A retransmission: the client never heard the ack, not a second
        // click. Repeat the answer and touch nothing.
        if let Some((last, acked)) = menu.last_seq.get(&peer).copied() {
            if seq == last {
                let _ = transport.send(
                    peer,
                    ServerMessage::Ack {
                        menu: id,
                        state_id: acked,
                        seq,
                    },
                );
                return Outcome::Duplicate;
            }
            if seq < last {
                // An older click arriving after a newer one. The world has
                // moved on and there is no honest way to apply it.
                let snapshot = menu.snapshot();
                send_content(transport, peer, id, snapshot);
                return Outcome::Corrected;
            }
        }

        // Apply to a scratch copy: `ValidationLevel::Always` reports a
        // conservation failure only after the action has run, so the real
        // inventories must not be the ones it ran on.
        let mut inventories = menu.inventories.clone();
        let mut state = menu.state.clone();
        let outcome = apply_click_validated(
            &menu.def,
            &mut inventories,
            &mut state,
            action,
            &menu.actor,
            ctx,
            ValidationLevel::Always,
        );

        let delta = match outcome {
            Ok(delta) => delta,
            Err(error) => {
                tracing::debug!(?peer, ?id, ?action, %error, "click refused");
                let snapshot = menu.snapshot();
                send_content(transport, peer, id, snapshot);
                return Outcome::Refused(error);
            }
        };

        // The client said where it thought it was. If that is not where the
        // server was, the two were looking at different containers and the
        // prediction cannot be trusted however plausible it looks.
        let drifted = state_id != menu.state.state_id;

        menu.inventories = inventories;
        menu.state = state;
        menu.last_seq.insert(peer, (seq, menu.state.state_id));

        let changed = changed_slots(menu, &delta);
        let agreed = !drifted && Self::agrees(predicted, &delta);
        let viewers = menu.viewers.clone();
        let server_state_id = menu.state.state_id;
        let snapshot = (!agreed).then(|| menu.snapshot());

        if agreed {
            let _ = transport.send(
                peer,
                ServerMessage::Ack {
                    menu: id,
                    state_id: server_state_id,
                    seq,
                },
            );
        } else if let Some(snapshot) = snapshot {
            send_content(transport, peer, id, snapshot);
        }

        // Everyone else finds out slot by slot. The clicking client already
        // has the whole container if it was wrong, and needs nothing if it
        // was right.
        for viewer in viewers.iter().filter(|v| **v != peer) {
            for (slot, stack) in &changed {
                let _ = transport.send(
                    *viewer,
                    ServerMessage::SetSlot {
                        menu: id,
                        slot: *slot,
                        stack: stack.clone(),
                    },
                );
            }
        }

        if agreed {
            Outcome::Acked
        } else {
            Outcome::Corrected
        }
    }

    /// `true` when the client's prediction says exactly what the server's own
    /// application said.
    ///
    /// Slot order is already canonical (`apply_click` sorts and dedupes), so
    /// this is a plain comparison rather than a set comparison.
    fn agrees(predicted: &Delta, actual: &Delta) -> bool {
        predicted.slots == actual.slots
            && predicted.carried == actual.carried
            && predicted.state_id == actual.state_id
            && predicted.dropped == actual.dropped
    }
}
