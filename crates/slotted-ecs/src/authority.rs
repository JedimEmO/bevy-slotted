//! The [`Authority`] resource and the local adapter.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use bevy::prelude::*;
use slotted_model::{AuthorityError, AuthorityEvent, ClickAction, Delta, MenuId, ValidationLevel};

/// Who has the final say on a click. Holds any [`slotted_model::Authority`]
/// adapter; the facade inserts a [`LocalAuthority`] unless the app replaced it
/// before `add_plugins`.
#[derive(Resource, Clone)]
pub struct Authority(pub Arc<dyn slotted_model::Authority>);

impl Authority {
    /// Wraps an adapter.
    pub fn new(adapter: impl slotted_model::Authority + 'static) -> Self {
        Self(Arc::new(adapter))
    }

    /// The local adapter, which acks everything immediately.
    pub fn local() -> Self {
        Self::new(LocalAuthority::default())
    }

    /// The local adapter, validating every click at `level`.
    pub fn local_validated(level: ValidationLevel) -> Self {
        Self::new(LocalAuthority::with_validation(level))
    }
}

impl Default for Authority {
    fn default() -> Self {
        Self::local()
    }
}

/// Actions submitted but not yet acked or resynced, per open menu.
///
/// `slotted-test`'s `settle()` waits for this to reach zero. A local authority
/// keeps it at zero across frames; a networked one does not.
#[derive(Resource, Default, Debug, Clone, PartialEq, Eq)]
pub struct PendingRoundTrips(pub u32);

impl PendingRoundTrips {
    /// `true` when nothing is in flight.
    pub const fn is_idle(&self) -> bool {
        self.0 == 0
    }
}

/// The single-player authority: the prediction is the truth.
///
/// `submit` records an `Ack` at the predicted `state_id`; `poll` drains them.
/// Never resyncs, never rejects, and answers
/// [`ResyncRequest::Unsupported`](slotted_model::ResyncRequest::Unsupported)
/// because it holds no second copy of the world to resynchronise from.
#[derive(Default, Debug)]
pub struct LocalAuthority {
    events: Mutex<VecDeque<AuthorityEvent>>,
    validation: ValidationLevel,
}

impl LocalAuthority {
    /// A local authority that has `predict` validate every click at `level`.
    ///
    /// A single-player game that wants the conservation check in its shipped
    /// build, rather than only in tests, asks for
    /// [`ValidationLevel::Always`] here.
    pub fn with_validation(validation: ValidationLevel) -> Self {
        Self {
            events: Mutex::default(),
            validation,
        }
    }
}

impl slotted_model::Authority for LocalAuthority {
    fn submit(
        &self,
        menu: MenuId,
        _action: ClickAction,
        predicted: &Delta,
    ) -> Result<(), AuthorityError> {
        let mut events = self
            .events
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        events.push_back(AuthorityEvent::Ack {
            menu,
            state_id: predicted.state_id,
        });
        Ok(())
    }

    fn poll(&self) -> Vec<AuthorityEvent> {
        let mut events = self
            .events
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        events.drain(..).collect()
    }

    fn validation(&self) -> ValidationLevel {
        self.validation
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use slotted_model::Authority as _;

    #[test]
    fn local_authority_acks_at_the_predicted_state_id() {
        let auth = LocalAuthority::default();
        let delta = Delta {
            state_id: 7,
            ..Delta::default()
        };
        auth.submit(
            MenuId(1),
            ClickAction::QuickMove {
                slot: slotted_model::SlotIx(0),
            },
            &delta,
        )
        .expect("local authority never fails");
        assert_eq!(
            auth.poll(),
            vec![AuthorityEvent::Ack {
                menu: MenuId(1),
                state_id: 7
            }]
        );
        assert!(auth.poll().is_empty());
    }
}
