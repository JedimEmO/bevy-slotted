//! [`MenuServer`]: the copy of a menu that gets the final say.
//!
//! The server runs the same [`apply_click`](slotted_model::apply_click) the
//! client ran, against its own inventories, at
//! [`slotted_model::ValidationLevel::Always`]. A client that agrees gets an
//! ack and nothing else on the wire. A client that disagrees gets the
//! container back.
//!
//! # Sessions over a store
//!
//! What the server holds is not "a menu". It is a [`ContainerStore`] of
//! inventories and a set of *sessions*, one per open screen, each belonging to
//! exactly one peer. A session owns the things that are the player's own — the
//! carried stack, the in-progress drag, the ghost hints, the property values
//! it has been told about — and *binds* the things that are not: one
//! [`InventoryId`] per [`InventoryRef`](slotted_model::InventoryRef) its
//! definition addresses.
//!
//! Two players at one chest therefore have two sessions binding one container
//! id and their own private player inventories. Neither can see the other's
//! cursor, because a cursor is session state; both see the other's edits to
//! the chest, because the chest is one inventory and a write to it marks slots
//! dirty for every session bound to it. Vanilla arrives at the same answer
//! from the other direction: see `docs/research/research-mc-anatomy.md`
//! section 3.
//!
//! # Who may do what
//!
//! Every message that names a session is checked twice before anything is
//! read: the server checks the peer owns the session, and the [`Access`] port
//! is asked whether it may act through it right now. A peer naming somebody
//! else's session gets [`ServerMessage::Refused`] and a log line, never state.
//!
//! # Answering the same click twice
//!
//! A click that needed a correction and a click that got an ack are not
//! interchangeable, so the server records which answer it gave per
//! `(session, seq)` for a bounded window. A retransmission replays the
//! recorded *kind* of answer: an ack repeats as an ack, a correction repeats
//! as the container. A correction that goes missing therefore cannot be turned
//! into an ack by the retry, which is how a client used to end up permanently
//! divergent.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use slotted_model::{
    Actor, ClickError, Delta, Inventories, InventoryId, ItemStack, LookupCtx, MenuDef, MenuId,
    MenuSnapshot, MenuState, PropertyId, SlotIx, ValidationLevel, apply_click_validated, slot_view,
};

use crate::access::{Access, OwnerOnly, SessionInfo};
use crate::message::{ClientMessage, PeerId, Refusal, ServerMessage};
use crate::store::ContainerStore;
use crate::transport::{ServerTransport, TransportError};

/// What the server answered a click with, remembered so a retransmission gets
/// the same *kind* of answer rather than whichever is cheapest to produce.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Response {
    /// An ack at this state id.
    Ack(u32),
    /// The whole container. Replaying this sends the container as it is
    /// *now*, which is what the client needs; what has to be preserved across
    /// the retry is that it is a correction at all.
    Correction,
}

/// One open screen, belonging to one peer.
#[derive(Debug, Clone)]
struct Session {
    peer: PeerId,
    def: MenuDef,
    /// One container per `InventoryRef` the definition addresses.
    bindings: Vec<InventoryId>,
    /// Carried stack, drag, hints, properties and state id. Never shared.
    state: MenuState,
    actor: Actor,
    /// The answer given per sequence number, oldest first, bounded.
    responses: VecDeque<(u32, Response)>,
    /// The highest sequence number ever accepted, so a click older than the
    /// remembered window is still recognisable as stale.
    highest_seq: Option<u32>,
}

impl Session {
    fn info(&self, menu: MenuId) -> SessionInfo<'_> {
        SessionInfo {
            menu,
            owner: self.peer,
            containers: &self.bindings,
        }
    }

    fn recall(&self, seq: u32) -> Option<Response> {
        self.responses
            .iter()
            .find(|(s, _)| *s == seq)
            .map(|(_, r)| *r)
    }

    fn record(&mut self, seq: u32, response: Response, window: usize) {
        self.responses.push_back((seq, response));
        while self.responses.len() > window {
            self.responses.pop_front();
        }
        self.highest_seq = Some(self.highest_seq.map_or(seq, |h| h.max(seq)));
    }
}

/// Why a session could not be opened.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum OpenError {
    /// A session with that id is already open. Ids are server-wide, so this
    /// means the caller reused one, not that two players clashed.
    #[error("menu {0:?} is already open")]
    AlreadyOpen(MenuId),
    /// The definition addresses more inventories than were bound.
    #[error("the definition addresses {needs} inventories, {bound} were bound")]
    MissingBinding {
        /// How many the definition addresses.
        needs: usize,
        /// How many were bound.
        bound: usize,
    },
    /// One of the bound ids names nothing in the store.
    #[error("no container {0:?} in the store")]
    NoSuchContainer(InventoryId),
    /// A bound container is too small for the slots the definition puts on it.
    #[error("container {id:?} has {has} slots, the definition needs {needs}")]
    TooSmall {
        /// The container.
        id: InventoryId,
        /// How many slots it has.
        has: usize,
        /// How many the definition needs.
        needs: usize,
    },
    /// A bound container is private to a different player. This one is not
    /// negotiable: no [`Access`] policy can allow it.
    #[error("container {id:?} is private to another player")]
    NotYours {
        /// The container.
        id: InventoryId,
    },
    /// The [`Access`] port refused the open.
    #[error("access denied for container {id:?}")]
    Denied {
        /// The container it refused.
        id: InventoryId,
    },
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
    /// The message was a retransmission of a click already answered. The
    /// recorded answer was repeated and nothing else happened.
    Duplicate,
    /// The client asked for the container and got it.
    Resynced,
    /// The session was closed at the client's request.
    Closed,
    /// The peer may not touch the session it named. Nothing was read, nothing
    /// was written, and the answer carried no state.
    Denied(Refusal),
}

fn narrow(index: usize) -> u16 {
    u16::try_from(index).unwrap_or(u16::MAX)
}

/// The authoritative side of a set of open menus.
///
/// It owns the truth: the container store, the sessions over it, and the
/// access policy. It answers [`ClientMessage`]s and pushes [`ServerMessage`]s,
/// and it knows nothing about how they travel.
#[derive(Debug)]
pub struct MenuServer {
    store: ContainerStore,
    /// Sessions by menu id. Ids are server-wide: a peer naming one it does not
    /// own is a distinguishable event, and a refusable one.
    sessions: BTreeMap<u32, Session>,
    access: Box<dyn Access>,
    window: usize,
}

impl Default for MenuServer {
    fn default() -> Self {
        Self::new()
    }
}

impl MenuServer {
    /// How many answers a session remembers per peer, so a retransmission can
    /// be replayed rather than reapplied.
    pub const DEFAULT_WINDOW: usize = 64;

    /// An empty server under the default [`OwnerOnly`] policy.
    pub fn new() -> Self {
        Self::with_access(OwnerOnly)
    }

    /// An empty server under `access`.
    pub fn with_access(access: impl Access + 'static) -> Self {
        Self {
            store: ContainerStore::new(),
            sessions: BTreeMap::new(),
            access: Box::new(access),
            window: Self::DEFAULT_WINDOW,
        }
    }

    /// Sets how many answers each session remembers. A retransmission of a
    /// click older than this is answered with the container, which is always
    /// safe and sometimes more than was needed.
    #[must_use]
    pub fn window(mut self, answers: usize) -> Self {
        self.window = answers.max(1);
        self
    }

    // ------------------------------------------------------------ containers

    /// The container store, for a caller that wants to read the world.
    pub fn store(&self) -> &ContainerStore {
        &self.store
    }

    /// The container store, mutably. A caller writing through this should
    /// call [`flush`](Self::flush) afterwards, or let the next
    /// [`pump`](Self::pump) do it, so the sessions hear about the change.
    pub fn store_mut(&mut self) -> &mut ContainerStore {
        &mut self.store
    }

    /// Adds a shared inventory: a chest every player at it edits in place.
    pub fn add_shared(&mut self, inventory: slotted_model::Inventory) -> InventoryId {
        self.store.add_shared(inventory)
    }

    /// Adds an inventory private to `peer`: their own main, hotbar or armour.
    /// It can never be bound into another player's session.
    pub fn add_private(
        &mut self,
        peer: PeerId,
        inventory: slotted_model::Inventory,
    ) -> InventoryId {
        self.store.add_private(peer, inventory)
    }

    // -------------------------------------------------------------- sessions

    /// Opens `id` for `peer` over `def`, binding one container per
    /// [`InventoryRef`](slotted_model::InventoryRef) the definition addresses.
    ///
    /// `bindings[k]` backs `InventoryRef::new(k)`. Every binding is checked
    /// for existence, size, privacy and [`Access::may_open`] before anything
    /// is created, so a refused open leaves the server exactly as it was.
    ///
    /// # Errors
    /// See [`OpenError`].
    pub fn open(
        &mut self,
        id: MenuId,
        peer: PeerId,
        def: MenuDef,
        bindings: Vec<InventoryId>,
        actor: Actor,
    ) -> Result<(), OpenError> {
        if self.sessions.contains_key(&id.0) {
            return Err(OpenError::AlreadyOpen(id));
        }
        let sizes = def.inventory_sizes();
        if bindings.len() < sizes.len() {
            return Err(OpenError::MissingBinding {
                needs: sizes.len(),
                bound: bindings.len(),
            });
        }
        for (k, needs) in sizes.iter().enumerate() {
            let container = bindings[k];
            let Some(held) = self.store.get(container) else {
                return Err(OpenError::NoSuchContainer(container));
            };
            if held.inventory.len() < *needs {
                return Err(OpenError::TooSmall {
                    id: container,
                    has: held.inventory.len(),
                    needs: *needs,
                });
            }
            if !held.owner.allows(peer) {
                return Err(OpenError::NotYours { id: container });
            }
            if !self.access.may_open(peer, container) {
                return Err(OpenError::Denied { id: container });
            }
        }

        let mut state = MenuState::new(&def);
        // A furnace that has been burning for a while must open at its real
        // progress, not at the definition's initial value.
        for container in bindings.iter().take(sizes.len()) {
            let Some(held) = self.store.get(*container) else {
                continue;
            };
            for (property, value) in &held.properties {
                if let Some(position) = def.properties.iter().position(|p| p.id == *property)
                    && let Some(slot) = state.properties.get_mut(position)
                {
                    *slot = *value;
                }
            }
        }

        self.sessions.insert(
            id.0,
            Session {
                peer,
                def,
                bindings,
                state,
                actor,
                responses: VecDeque::new(),
                highest_seq: None,
            },
        );
        Ok(())
    }

    /// Closes `id`. Messages about it are answered
    /// [`Refusal::UnknownMenu`].
    ///
    /// Anything on the session's cursor is lost, because only the caller knows
    /// where a dropped stack should go; read [`snapshot`](Self::snapshot)
    /// first if it matters.
    pub fn close(&mut self, id: MenuId) -> bool {
        self.sessions.remove(&id.0).is_some()
    }

    /// Closes every session belonging to `peer` and drops the inventories
    /// private to them. What a disconnect does.
    pub fn drop_peer(&mut self, peer: PeerId) {
        self.sessions.retain(|_, s| s.peer != peer);
        self.store.remove_private(peer);
    }

    /// The sessions `peer` has open, ascending.
    pub fn sessions_of(&self, peer: PeerId) -> Vec<MenuId> {
        self.sessions
            .iter()
            .filter(|(_, s)| s.peer == peer)
            .map(|(id, _)| MenuId(*id))
            .collect()
    }

    /// Who owns session `id`.
    pub fn owner_of(&self, id: MenuId) -> Option<PeerId> {
        self.sessions.get(&id.0).map(|s| s.peer)
    }

    /// The containers session `id` is bound to, in `InventoryRef` order.
    pub fn bindings_of(&self, id: MenuId) -> Option<&[InventoryId]> {
        self.sessions.get(&id.0).map(|s| s.bindings.as_slice())
    }

    // ------------------------------------------------------------------ reads

    /// The inventories session `id` addresses, gathered out of the store.
    ///
    /// `None` when there is no such session or one of its containers has been
    /// removed from under it.
    fn gather(&self, id: MenuId) -> Option<(Inventories, &Session)> {
        let session = self.sessions.get(&id.0)?;
        let mut inventories = Inventories::new();
        for container in &session.bindings {
            inventories.push(self.store.inventory(*container)?.clone());
        }
        Some((inventories, session))
    }

    /// The server's own view of session `id`, for tests and for seeding a
    /// client that has just opened the menu.
    pub fn snapshot(&self, id: MenuId) -> Option<MenuSnapshot> {
        let (inventories, session) = self.gather(id)?;
        Some(MenuSnapshot {
            inventories,
            state: session.state.clone(),
        })
    }

    /// What slot `slot` of session `id` holds, hints included.
    pub fn slot(&self, id: MenuId, slot: SlotIx) -> Option<ItemStack> {
        let (inventories, session) = self.gather(id)?;
        slot_view(&session.def, &inventories, &session.state, slot).cloned()
    }

    /// The server's state id for session `id`.
    pub fn state_id(&self, id: MenuId) -> Option<u32> {
        self.sessions.get(&id.0).map(|s| s.state.state_id)
    }

    // ------------------------------------------------------------------ sends

    /// Sends the whole container of session `id` to the peer that owns it.
    ///
    /// There is no peer argument on purpose: a snapshot goes to the session's
    /// owner or to nobody.
    pub fn send_content<T: ServerTransport>(
        &self,
        transport: &T,
        id: MenuId,
    ) -> Result<(), TransportError> {
        self.send_content_answering(transport, id, None)
    }

    fn send_content_answering<T: ServerTransport>(
        &self,
        transport: &T,
        id: MenuId,
        answers: Option<u32>,
    ) -> Result<(), TransportError> {
        let Some(snapshot) = self.snapshot(id) else {
            return Ok(());
        };
        let Some(session) = self.sessions.get(&id.0) else {
            return Ok(());
        };
        transport.send(
            session.peer,
            ServerMessage::SetContent {
                menu: id,
                answers,
                snapshot: Box::new(snapshot),
            },
        )
    }

    /// Sets a synced property of `container` and tells every session bound to
    /// it that has that property.
    ///
    /// A property belongs to the thing being viewed, not to the viewer, so
    /// this is addressed by container: two players watching one furnace both
    /// see the same burn time.
    pub fn set_property<T: ServerTransport>(
        &mut self,
        transport: &T,
        container: InventoryId,
        property: PropertyId,
        value: i32,
    ) -> Result<(), TransportError> {
        let Some(held) = self.store.get_mut(container) else {
            return Ok(());
        };
        held.properties.insert(property, value);

        let mut first = None;
        for (id, session) in &mut self.sessions {
            if !session.bindings.contains(&container) {
                continue;
            }
            let Some(position) = session.def.properties.iter().position(|p| p.id == property)
            else {
                continue;
            };
            if let Some(slot) = session.state.properties.get_mut(position) {
                *slot = value;
            }
            if let Err(error) = transport.send(
                session.peer,
                ServerMessage::SetProperty {
                    menu: MenuId(*id),
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

    /// Tells every session about every container slot that has changed since
    /// the last flush, then clears the marks.
    ///
    /// [`pump`](Self::pump) calls this first, so a host that wrote straight
    /// into the store between ticks does not have to. `except` is the session
    /// that caused the change and has already been answered.
    pub fn flush<T: ServerTransport>(&mut self, transport: &T) {
        self.broadcast_dirty(transport, None);
    }

    fn broadcast_dirty<T: ServerTransport>(&mut self, transport: &T, except: Option<MenuId>) {
        let dirty: BTreeSet<(InventoryId, usize)> = self
            .store
            .iter()
            .flat_map(|(id, container)| {
                container
                    .inventory
                    .changed()
                    .iter()
                    .map(move |cell| (id, cell))
            })
            .collect();
        if dirty.is_empty() {
            return;
        }

        for (menu, session) in &self.sessions {
            let menu = MenuId(*menu);
            if Some(menu) == except {
                continue;
            }
            for (index, slot) in session.def.slots.iter().enumerate() {
                // A hint is session state, not an inventory cell, so it never
                // travels this way. The session that changed one either
                // predicted it or was sent the whole container.
                if slot.behaviour.is_ghost() {
                    continue;
                }
                let Some(container) = session.bindings.get(slot.source.index()).copied() else {
                    continue;
                };
                let cell = usize::from(slot.index);
                if !dirty.contains(&(container, cell)) {
                    continue;
                }
                let stack = self
                    .store
                    .inventory(container)
                    .and_then(|inventory| inventory.get(cell))
                    .cloned();
                let _ = transport.send(
                    session.peer,
                    ServerMessage::SetSlot {
                        menu,
                        slot: SlotIx(narrow(index)),
                        stack,
                    },
                );
            }
        }

        self.store.clear_changed();
    }

    // ------------------------------------------------------------------ pump

    /// Handles everything waiting on `transport` and answers it.
    ///
    /// Call once per server tick. `ctx` is the same registry lookup the
    /// clients run against; a disagreement about stack sizes would otherwise
    /// show up as a stream of corrections.
    pub fn pump<T: ServerTransport>(&mut self, transport: &T, ctx: &dyn LookupCtx) -> Vec<Outcome> {
        // Anything a host wrote straight into the store since the last tick
        // goes out before the clicks, so the marks are clean and a click's
        // fan-out is exactly its own.
        self.flush(transport);
        transport
            .poll()
            .into_iter()
            .map(|(peer, message)| self.handle(transport, ctx, peer, message))
            .collect()
    }

    /// Puts whatever a session was carrying back into that player's own
    /// inventory, before the session goes.
    ///
    /// A carried stack has already been taken out of a container, so dropping
    /// the session without this destroys it: an item sink any client can
    /// trigger by closing its screen mid-click. [`ClientMessage::CloseMenu`]
    /// is documented to return the stack, and this is where that happens.
    ///
    /// The destination is a binding the store marks
    /// [`Owner::Private`](crate::Owner::Private) to the session's own peer,
    /// in `InventoryRef` order: a player's own pockets are the only honest
    /// place to put it, and putting it in a shared chest would hand it to
    /// whoever is standing there. Anything that does not fit is dropped, the
    /// same answer the model gives a full inventory.
    fn return_carried(&mut self, id: MenuId, ctx: &dyn LookupCtx) {
        let Some(session) = self.sessions.get_mut(&id.0) else {
            return;
        };
        let Some(mut carried) = session.state.carried.take() else {
            return;
        };
        let peer = session.peer;
        let bindings = session.bindings.clone();
        let max = ctx.max_stack(carried.id);

        for container in bindings {
            if self.store.owner(container) != Some(crate::store::Owner::Private(peer)) {
                continue;
            }
            let Some(inventory) = self.store.inventory_mut(container) else {
                continue;
            };
            while carried.count > 0
                && let Some(cell) = inventory.find_mergeable(&carried, max)
            {
                // `find_mergeable` only names an occupied cell, so the `else`
                // is unreachable. It breaks rather than falling back to a
                // clone of the carried stack: a fallback that duplicated
                // items would be a worse bug than the one this method fixes.
                let Some(mut held) = inventory.replace(cell, None) else {
                    break;
                };
                carried.merge_into(&mut held, max);
                inventory.set(cell, Some(held));
            }
            while carried.count > 0
                && let Some(cell) = inventory.first_empty()
            {
                let moved = carried.split(max.min(carried.count));
                match moved {
                    Some(stack) => inventory.set(cell, Some(stack)),
                    None => break,
                }
            }
            if carried.count == 0 {
                return;
            }
        }

        if carried.count > 0 {
            tracing::warn!(
                ?peer,
                ?id,
                count = carried.count,
                "no room in the player's own inventories for the carried stack on close"
            );
        }
    }

    /// Answers a peer that named a session it may not touch, and tells it
    /// nothing else.
    fn refuse<T: ServerTransport>(
        transport: &T,
        peer: PeerId,
        menu: MenuId,
        seq: Option<u32>,
        reason: Refusal,
    ) -> Outcome {
        tracing::warn!(?peer, ?menu, ?seq, ?reason, "refused a message");
        let _ = transport.send(peer, ServerMessage::Refused { menu, seq, reason });
        Outcome::Denied(reason)
    }

    /// The one gate every message goes through: does this peer own the
    /// session it named, and may it act through it right now?
    fn authorize(&self, peer: PeerId, menu: MenuId) -> Result<(), Refusal> {
        let Some(session) = self.sessions.get(&menu.0) else {
            return Err(Refusal::UnknownMenu);
        };
        if session.peer != peer {
            return Err(Refusal::NotYours);
        }
        if !self.access.may_act(peer, session.info(menu)) {
            return Err(Refusal::Denied);
        }
        Ok(())
    }

    fn handle<T: ServerTransport>(
        &mut self,
        transport: &T,
        ctx: &dyn LookupCtx,
        peer: PeerId,
        message: ClientMessage,
    ) -> Outcome {
        let menu = message.menu();
        let seq = match &message {
            ClientMessage::ClickContainer { seq, .. } => Some(*seq),
            _ => None,
        };
        if let Err(reason) = self.authorize(peer, menu) {
            return Self::refuse(transport, peer, menu, seq, reason);
        }
        match message {
            ClientMessage::RequestResync { menu } => {
                let _ = self.send_content(transport, menu);
                Outcome::Resynced
            }
            ClientMessage::CloseMenu { menu } => {
                self.return_carried(menu, ctx);
                self.sessions.remove(&menu.0);
                // The session is gone before the fan-out, so the closing peer
                // is not told about slots in a menu it no longer has; any
                // other session of theirs showing the same pockets is.
                self.broadcast_dirty(transport, None);
                let _ = transport.send(peer, ServerMessage::Closed { menu });
                Outcome::Closed
            }
            ClientMessage::ClickContainer {
                menu,
                state_id,
                seq,
                action,
                predicted,
            } => self.click(transport, ctx, menu, state_id, seq, action, &predicted),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn click<T: ServerTransport>(
        &mut self,
        transport: &T,
        ctx: &dyn LookupCtx,
        id: MenuId,
        state_id: u32,
        seq: u32,
        action: slotted_model::ClickAction,
        predicted: &Delta,
    ) -> Outcome {
        if let Some(outcome) = self.answer_again(transport, id, seq) {
            return outcome;
        }

        let Some(session) = self.sessions.get(&id.0) else {
            // Unreachable: `handle` authorised this message, which proves the
            // session exists. Answering nothing is still better than a panic.
            return Outcome::Denied(Refusal::UnknownMenu);
        };
        let def = session.def.clone();
        let actor = session.actor;
        let peer = session.peer;
        let bindings = session.bindings.clone();
        let mut state = session.state.clone();

        let mut inventories = Inventories::new();
        for container in &bindings {
            let Some(held) = self.store.inventory(*container) else {
                // A container was removed from under an open session. There is
                // nothing honest left to answer with, so the session goes.
                tracing::error!(?id, ?container, "session bound to a container that is gone");
                self.sessions.remove(&id.0);
                let _ = transport.send(peer, ServerMessage::Closed { menu: id });
                return Outcome::Closed;
            };
            inventories.push(held.clone());
        }

        // Apply to a scratch copy: `ValidationLevel::Always` reports a
        // conservation failure only after the action has run, so the real
        // inventories must not be the ones it ran on.
        for inventory in inventories.iter_mut() {
            inventory.clear_changed();
        }
        let outcome = apply_click_validated(
            &def,
            &mut inventories,
            &mut state,
            action,
            &actor,
            ctx,
            ValidationLevel::Always,
        );

        let window = self.window;
        let delta = match outcome {
            Ok(delta) => delta,
            Err(error) => {
                tracing::debug!(?peer, ?id, ?action, %error, "click refused");
                if let Some(session) = self.sessions.get_mut(&id.0) {
                    session.record(seq, Response::Correction, window);
                }
                let _ = self.send_content_answering(transport, id, Some(seq));
                return Outcome::Refused(error);
            }
        };

        // The client said where it thought it was. If that is not where the
        // server was, the two were looking at different containers and the
        // prediction cannot be trusted however plausible it looks.
        let drifted = state_id != self.sessions[&id.0].state.state_id;

        // Commit: the scratch inventories go back into the store, dirty marks
        // and all, so the fan-out below sees exactly what this click touched.
        for (position, container) in bindings.iter().enumerate() {
            let Some(scratch) = inventories.get(slotted_model::InventoryRef::new(narrow(position)))
            else {
                continue;
            };
            if let Some(held) = self.store.get_mut(*container) {
                held.inventory = scratch.clone();
            }
        }
        let agreed = !drifted && Self::agrees(predicted, &delta);
        if let Some(session) = self.sessions.get_mut(&id.0) {
            session.state = state;
            session.record(
                seq,
                if agreed {
                    Response::Ack(session.state.state_id)
                } else {
                    Response::Correction
                },
                window,
            );
        }

        if agreed {
            let state_id = self.sessions[&id.0].state.state_id;
            let _ = transport.send(
                peer,
                ServerMessage::Ack {
                    menu: id,
                    state_id,
                    seq,
                },
            );
        } else {
            let _ = self.send_content_answering(transport, id, Some(seq));
        }

        // Everyone else finds out slot by slot, in their own slot numbering:
        // one container is `InventoryRef::new(0)` in one session and
        // `new(3)` in another, and a slot index means nothing across the two.
        // The clicking session already has the whole container if it was
        // wrong, and needs nothing if it was right.
        self.broadcast_dirty(transport, Some(id));

        if agreed {
            Outcome::Acked
        } else {
            Outcome::Corrected
        }
    }

    /// Answers a click the server has already dealt with, if it has.
    ///
    /// Two cases, and neither applies the action a second time. A sequence
    /// number the session remembers gets the *kind* of answer it got the first
    /// time: an ack repeats as an ack, a correction repeats as the container.
    /// One older than anything remembered gets the container, because the
    /// world has moved on and there is no honest way to apply it now.
    fn answer_again<T: ServerTransport>(
        &mut self,
        transport: &T,
        id: MenuId,
        seq: u32,
    ) -> Option<Outcome> {
        let session = self.sessions.get(&id.0)?;
        if let Some(response) = session.recall(seq) {
            self.replay(transport, id, seq, response);
            return Some(Outcome::Duplicate);
        }
        if session.highest_seq.is_none_or(|highest| seq >= highest) {
            return None;
        }
        let _ = self.send_content_answering(transport, id, Some(seq));
        let window = self.window;
        if let Some(session) = self.sessions.get_mut(&id.0) {
            session.record(seq, Response::Correction, window);
        }
        Some(Outcome::Corrected)
    }

    /// Repeats a recorded answer for a retransmitted click.
    fn replay<T: ServerTransport>(&self, transport: &T, id: MenuId, seq: u32, response: Response) {
        let Some(session) = self.sessions.get(&id.0) else {
            return;
        };
        match response {
            Response::Ack(state_id) => {
                let _ = transport.send(
                    session.peer,
                    ServerMessage::Ack {
                        menu: id,
                        state_id,
                        seq,
                    },
                );
            }
            Response::Correction => {
                let _ = self.send_content_answering(transport, id, Some(seq));
            }
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
