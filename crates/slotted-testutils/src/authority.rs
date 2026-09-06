//! Fake [`Authority`] adapters.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex, MutexGuard};

use slotted_model::{
    Authority, AuthorityError, AuthorityEvent, ClickAction, Delta, MenuId, MenuSnapshot,
    ResyncRequest, ValidationLevel,
};

/// One call to [`Authority::submit`], as the fakes recorded it.
#[derive(Debug, Clone, PartialEq)]
pub struct Submitted {
    /// The menu the action ran on.
    pub menu: MenuId,
    /// The action itself.
    pub action: ClickAction,
    /// The client's predicted outcome.
    pub delta: Delta,
}

#[derive(Debug, Default)]
struct Recording {
    submitted: Vec<Submitted>,
    /// Acks the caller has not released yet, oldest first.
    held: VecDeque<AuthorityEvent>,
    /// Events the next `poll` will return.
    outbox: VecDeque<AuthorityEvent>,
    auto_ack: bool,
    error: Option<AuthorityError>,
    /// Menus a caller has asked for a full snapshot of, in order.
    resync_requests: Vec<MenuId>,
    /// What a resync request answers with. `None` reports the request
    /// unsupported, which is what a single-player authority does.
    resync_answer: Option<MenuSnapshot>,
    validation: ValidationLevel,
}

/// An authority that remembers everything submitted to it.
///
/// [`new`](Self::new) acks each submission on the next poll, like
/// [`LocalAuthority`](slotted_ecs::LocalAuthority). [`manual`](Self::manual)
/// holds the acks back until [`ack_next`](Self::ack_next) or
/// [`ack_all`](Self::ack_all) releases them, which is how a test keeps
/// `PendingRoundTrips` above zero for a frame.
#[derive(Debug, Default)]
pub struct RecordingAuthority {
    state: Mutex<Recording>,
}

impl RecordingAuthority {
    /// Records and acks immediately.
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new(Recording {
                auto_ack: true,
                ..Recording::default()
            }),
        })
    }

    /// Records and holds every ack until it is released.
    pub fn manual() -> Arc<Self> {
        Arc::new(Self::default())
    }

    fn lock(&self) -> MutexGuard<'_, Recording> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Everything submitted so far, in order.
    pub fn submitted(&self) -> Vec<Submitted> {
        self.lock().submitted.clone()
    }

    /// Number of submissions so far.
    pub fn len(&self) -> usize {
        self.lock().submitted.len()
    }

    /// `true` when nothing has been submitted.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The most recent submission.
    pub fn last(&self) -> Option<Submitted> {
        self.lock().submitted.last().cloned()
    }

    /// Releases the oldest held ack. `true` when there was one.
    pub fn ack_next(&self) -> bool {
        let mut state = self.lock();
        match state.held.pop_front() {
            Some(event) => {
                state.outbox.push_back(event);
                true
            }
            None => false,
        }
    }

    /// Releases every held ack, oldest first. Returns how many.
    pub fn ack_all(&self) -> usize {
        let mut state = self.lock();
        let held: Vec<_> = state.held.drain(..).collect();
        let n = held.len();
        state.outbox.extend(held);
        n
    }

    /// Queues an event the next poll will return. Use it to inject a
    /// `Resync` or a `Property` update at a chosen frame.
    pub fn push_event(&self, event: AuthorityEvent) {
        self.lock().outbox.push_back(event);
    }

    /// Queues a `Resync` of `menu` to `snapshot`.
    pub fn resync(&self, menu: MenuId, snapshot: MenuSnapshot) {
        self.push_event(AuthorityEvent::Resync { menu, snapshot });
    }

    /// Queues a `Property` update.
    pub fn property(&self, menu: MenuId, id: slotted_model::PropertyId, value: i32) {
        self.push_event(AuthorityEvent::Property { menu, id, value });
    }

    /// Makes every following `submit` fail with `error`. Pass `None` to stop.
    pub fn fail_with(&self, error: Option<AuthorityError>) {
        self.lock().error = error;
    }

    /// Answers every [`Authority::request_resync`] with `snapshot`, as a
    /// networked authority does. Without this the fake reports the request
    /// unsupported and the client redraws from its own state.
    pub fn answer_resync_with(&self, snapshot: MenuSnapshot) {
        self.lock().resync_answer = Some(snapshot);
    }

    /// Menus a caller has asked for a full snapshot of, in order.
    pub fn resync_requests(&self) -> Vec<MenuId> {
        self.lock().resync_requests.clone()
    }

    /// Reports `level` as the validation this authority wants.
    pub fn require_validation(&self, level: ValidationLevel) {
        self.lock().validation = level;
    }
}

impl Authority for RecordingAuthority {
    fn submit(
        &self,
        menu: MenuId,
        action: ClickAction,
        predicted: &Delta,
    ) -> Result<(), AuthorityError> {
        let mut state = self.lock();
        state.submitted.push(Submitted {
            menu,
            action,
            delta: predicted.clone(),
        });
        if let Some(error) = state.error {
            return Err(error);
        }
        let ack = AuthorityEvent::Ack {
            menu,
            state_id: predicted.state_id,
        };
        if state.auto_ack {
            state.outbox.push_back(ack);
        } else {
            state.held.push_back(ack);
        }
        Ok(())
    }

    fn poll(&self) -> Vec<AuthorityEvent> {
        self.lock().outbox.drain(..).collect()
    }

    fn request_resync(&self, menu: MenuId) -> Result<ResyncRequest, AuthorityError> {
        let mut state = self.lock();
        state.resync_requests.push(menu);
        let Some(snapshot) = state.resync_answer.clone() else {
            return Ok(ResyncRequest::Unsupported);
        };
        state
            .outbox
            .push_back(AuthorityEvent::Resync { menu, snapshot });
        Ok(ResyncRequest::Pending)
    }

    fn validation(&self) -> ValidationLevel {
        self.lock().validation
    }
}

/// An authority that never agrees: every submission comes back as a `Resync`
/// to a fixed snapshot.
///
/// It records submissions the same way [`RecordingAuthority`] does, so a test
/// can still assert on the predicted delta that was refused.
#[derive(Debug)]
pub struct RejectingAuthority {
    snapshot: MenuSnapshot,
    state: Mutex<Recording>,
}

impl RejectingAuthority {
    /// Resyncs every menu to `snapshot`.
    pub fn new(snapshot: MenuSnapshot) -> Arc<Self> {
        Arc::new(Self {
            snapshot,
            state: Mutex::new(Recording::default()),
        })
    }

    fn lock(&self) -> MutexGuard<'_, Recording> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Everything submitted so far, in order.
    pub fn submitted(&self) -> Vec<Submitted> {
        self.lock().submitted.clone()
    }

    /// The snapshot every menu is forced back to.
    pub fn snapshot(&self) -> &MenuSnapshot {
        &self.snapshot
    }
}

impl Authority for RejectingAuthority {
    fn submit(
        &self,
        menu: MenuId,
        action: ClickAction,
        predicted: &Delta,
    ) -> Result<(), AuthorityError> {
        let mut state = self.lock();
        state.submitted.push(Submitted {
            menu,
            action,
            delta: predicted.clone(),
        });
        state.outbox.push_back(AuthorityEvent::Resync {
            menu,
            snapshot: self.snapshot.clone(),
        });
        Ok(())
    }

    fn poll(&self) -> Vec<AuthorityEvent> {
        self.lock().outbox.drain(..).collect()
    }
}
