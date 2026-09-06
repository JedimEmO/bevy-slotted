//! [`RemoteAuthority`]: the client half, and the only thing a game swaps.
//!
//! It implements [`slotted_model::Authority`], so replacing `LocalAuthority`
//! with one of these is the whole change from a single-player game to a
//! client of a [`MenuServer`](crate::MenuServer). Prediction, reconciliation
//! and the rest of the loop are already written against the port.

use std::collections::BTreeMap;
use std::sync::{Mutex, MutexGuard};

use slotted_model::{
    Authority, AuthorityError, AuthorityEvent, ClickAction, Delta, MenuId, ResyncRequest,
    ValidationLevel,
};

use crate::message::{ClientMessage, PeerId, ServerMessage};
use crate::transport::ClientTransport;

/// A click that has been sent and not yet answered.
#[derive(Debug, Clone)]
struct InFlight {
    menu: MenuId,
    seq: u32,
    state_id: u32,
    action: ClickAction,
    predicted: Delta,
    /// The poll at which this was last put on the wire.
    sent_at: u64,
}

#[derive(Debug, Default)]
struct ClientState {
    /// Polls since this authority was built. The retry timer counts these
    /// rather than wall-clock time, so a test is not a race.
    now: u64,
    /// The client's own state id per menu, as the last submission left it.
    state_id: BTreeMap<u32, u32>,
    /// The next sequence number per menu.
    next_seq: BTreeMap<u32, u32>,
    /// Submissions awaiting an ack, oldest first.
    in_flight: Vec<InFlight>,
    /// Events `poll` will hand to the prediction loop.
    outbox: Vec<AuthorityEvent>,
    /// Menus a resync has been asked for and not yet answered, with the poll
    /// the request was last put on the wire at. A transport reports `Ok` for
    /// a message it then loses, so an unanswered request has to be repeated
    /// on the same timer a click is; without that, one lost request leaves the
    /// client waiting for a container that is never coming.
    awaiting_resync: Vec<(u32, u64)>,
    /// Retransmissions sent so far, for tests and metrics.
    retransmits: u64,
}

/// The client's authority: predicts locally, defers to a server.
///
/// It is deliberately dumb about time. [`poll`](Authority::poll) is the clock:
/// every call advances one tick, delivers what arrived, and retransmits any
/// click that has gone unanswered for [`retry_after`](Self::retry_after)
/// ticks. The prediction loop already calls it once a frame.
///
/// Retransmission is safe because a click carries a sequence number: the
/// server re-acks a sequence it has already applied rather than applying it
/// twice. See [`ClientMessage::ClickContainer`].
#[derive(Debug)]
pub struct RemoteAuthority<T> {
    transport: T,
    state: Mutex<ClientState>,
    retry_after: u64,
    validation: ValidationLevel,
}

impl<T: ClientTransport> RemoteAuthority<T> {
    /// A client over `transport`, retransmitting an unanswered click after
    /// eight polls.
    pub fn new(transport: T) -> Self {
        Self {
            transport,
            state: Mutex::new(ClientState::default()),
            retry_after: 8,
            validation: ValidationLevel::Debug,
        }
    }

    /// Sets how many polls an unanswered click waits before it is sent again.
    /// Zero retransmits on the next poll.
    #[must_use]
    pub fn retry_after(mut self, polls: u64) -> Self {
        self.retry_after = polls;
        self
    }

    /// Sets the validation level this authority asks its clients for.
    ///
    /// A server that runs at
    /// [`slotted_model::ValidationLevel::Always`]
    /// usually wants its clients checking too: a client that predicts an
    /// impossible outcome then gets corrected on every single click, which
    /// looks like lag and is really a bug.
    #[must_use]
    pub fn validation(mut self, level: ValidationLevel) -> Self {
        self.validation = level;
        self
    }

    /// The transport underneath, for a caller that also drives it.
    pub fn transport(&self) -> &T {
        &self.transport
    }

    /// How many clicks have been put back on the wire after going unanswered.
    pub fn retransmits(&self) -> u64 {
        self.lock().retransmits
    }

    /// How many clicks are still waiting for an answer.
    pub fn in_flight(&self) -> usize {
        self.lock().in_flight.len()
    }

    fn lock(&self) -> MutexGuard<'_, ClientState> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Turns one server message into what the prediction loop understands.
    fn receive(&self, state: &mut ClientState, message: ServerMessage) {
        match message {
            ServerMessage::Ack {
                menu,
                state_id,
                seq,
            } => {
                let known = state
                    .in_flight
                    .iter()
                    .position(|f| f.menu == menu && f.seq == seq);
                if let Some(index) = known {
                    state.in_flight.remove(index);
                } else {
                    // An ack for something this client never sent, or sent so
                    // long ago it has forgotten. Either way its copy is no
                    // longer trustworthy, so the truth is asked for as well.
                    tracing::debug!(?menu, seq, "ack for an unknown submission");
                    self.ask_for_content(state, menu);
                }
                // Reported either way, so that the round trip it settles is
                // not counted for ever.
                state.outbox.push(AuthorityEvent::Ack { menu, state_id });
            }
            ServerMessage::SetSlot { menu, slot, stack } => {
                state
                    .outbox
                    .push(AuthorityEvent::Slot { menu, slot, stack });
            }
            ServerMessage::SetContent { menu, snapshot } => {
                state.awaiting_resync.retain(|(m, _)| *m != menu.0);
                // A snapshot answers exactly one thing: the oldest click still
                // waiting on this menu, or, when there is none, the resync
                // this client asked for. Retiring more than one would leave
                // the caller's round-trip count stuck above zero for ever.
                if let Some(index) = state.in_flight.iter().position(|f| f.menu == menu) {
                    state.in_flight.remove(index);
                }
                state.state_id.insert(menu.0, snapshot.state.state_id);
                state.outbox.push(AuthorityEvent::Resync {
                    menu,
                    snapshot: *snapshot,
                });
            }
            ServerMessage::SetProperty { menu, id, value } => {
                state
                    .outbox
                    .push(AuthorityEvent::Property { menu, id, value });
            }
        }
    }

    /// Sends a resync request, at most one per menu at a time.
    ///
    /// A request already outstanding is not sent again here: it is repeated by
    /// [`Self::retransmit`] once it has gone unanswered for
    /// [`retry_after`](Self::retry_after) polls, so a caller asking every
    /// frame does not flood the link.
    fn ask_for_content(&self, state: &mut ClientState, menu: MenuId) -> bool {
        if state.awaiting_resync.iter().any(|(m, _)| *m == menu.0) {
            return true;
        }
        match self
            .transport
            .send(PeerId::SERVER, ClientMessage::RequestResync { menu })
        {
            Ok(()) => {
                let now = state.now;
                state.awaiting_resync.push((menu.0, now));
                true
            }
            Err(error) => {
                tracing::warn!(?menu, %error, "could not ask for a resync");
                false
            }
        }
    }

    /// Puts every click, and every resync request, that has waited too long
    /// back on the wire.
    fn retransmit(&self, state: &mut ClientState) {
        self.retransmit_resyncs(state);
        let now = state.now;
        let deadline = self.retry_after;
        let mut due: Vec<usize> = Vec::new();
        for (index, flight) in state.in_flight.iter().enumerate() {
            if now.saturating_sub(flight.sent_at) >= deadline {
                due.push(index);
            }
        }
        for index in due {
            let message = {
                let flight = &state.in_flight[index];
                ClientMessage::ClickContainer {
                    menu: flight.menu,
                    state_id: flight.state_id,
                    seq: flight.seq,
                    action: flight.action,
                    predicted: flight.predicted.clone(),
                }
            };
            match self.transport.send(PeerId::SERVER, message) {
                Ok(()) => {
                    state.in_flight[index].sent_at = now;
                    state.retransmits += 1;
                }
                Err(error) => {
                    tracing::warn!(%error, "could not retransmit a click");
                }
            }
        }
    }

    /// Repeats any resync request that has gone unanswered too long.
    ///
    /// `Transport::send` says `Ok` for a message the link then drops, which is
    /// what an unreliable transport does and what
    /// [`Loopback`](crate::Loopback) models. So "the request was sent" is not
    /// "the request arrived", and the only thing that distinguishes the two is
    /// a `SetContent` coming back. Until one does, keep asking.
    fn retransmit_resyncs(&self, state: &mut ClientState) {
        let now = state.now;
        let deadline = self.retry_after;
        let due: Vec<u32> = state
            .awaiting_resync
            .iter()
            .filter(|(_, sent_at)| now.saturating_sub(*sent_at) >= deadline)
            .map(|(menu, _)| *menu)
            .collect();
        for menu in due {
            let message = ClientMessage::RequestResync { menu: MenuId(menu) };
            match self.transport.send(PeerId::SERVER, message) {
                Ok(()) => {
                    if let Some(entry) = state.awaiting_resync.iter_mut().find(|(m, _)| *m == menu)
                    {
                        entry.1 = now;
                    }
                    state.retransmits += 1;
                }
                Err(error) => {
                    tracing::warn!(menu, %error, "could not repeat a resync request");
                }
            }
        }
    }
}

impl<T: ClientTransport> Authority for RemoteAuthority<T> {
    fn submit(
        &self,
        menu: MenuId,
        action: ClickAction,
        predicted: &Delta,
    ) -> Result<(), AuthorityError> {
        let mut state = self.lock();
        let before = state.state_id.get(&menu.0).copied().unwrap_or_default();
        let seq = {
            let next = state.next_seq.entry(menu.0).or_insert(0);
            let seq = *next;
            *next = next.wrapping_add(1);
            seq
        };
        let message = ClientMessage::ClickContainer {
            menu,
            state_id: before,
            seq,
            action,
            predicted: predicted.clone(),
        };
        self.transport
            .send(PeerId::SERVER, message)
            .map_err(|error| {
                tracing::warn!(?menu, %error, "could not send a click");
                AuthorityError::Disconnected
            })?;
        state.state_id.insert(menu.0, predicted.state_id);
        let now = state.now;
        state.in_flight.push(InFlight {
            menu,
            seq,
            state_id: before,
            action,
            predicted: predicted.clone(),
            sent_at: now,
        });
        Ok(())
    }

    fn poll(&self) -> Vec<AuthorityEvent> {
        let arrived = self.transport.poll();
        let mut state = self.lock();
        state.now += 1;
        for (_, message) in arrived {
            self.receive(&mut state, message);
        }
        self.retransmit(&mut state);
        std::mem::take(&mut state.outbox)
    }

    fn request_resync(&self, menu: MenuId) -> Result<ResyncRequest, AuthorityError> {
        let mut state = self.lock();
        if self.ask_for_content(&mut state, menu) {
            Ok(ResyncRequest::Pending)
        } else {
            Err(AuthorityError::Disconnected)
        }
    }

    fn validation(&self) -> ValidationLevel {
        self.validation
    }
}
