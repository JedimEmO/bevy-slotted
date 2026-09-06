//! Plugin and system sets.

use bevy::prelude::*;

use crate::authority::{Authority, PendingRoundTrips};
use crate::events::SlotSync;
use crate::menu::{Dropped, MenuIdAllocator, PlayerInventories};
use crate::systems::{
    ActionQueue, ClickInterpreter, PendingSubmissions, clear_dirty_masks, enqueue_menu_action,
    gather_input, interpret_slot_click, mirror_favorites, predict, reconcile, register_slot_refs,
    submit,
};

/// The prediction loop, in `Update`, chained in this order.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SlottedEcsSet {
    /// Collect actions. Observers have already filled [`ActionQueue`].
    Input,
    /// `apply_click` locally, emit slot events.
    Predict,
    /// Send deltas to the [`Authority`].
    Submit,
    /// Poll the authority; apply acks, resyncs and property updates.
    Reconcile,
}

/// Adds the components, events, observers and the four system sets.
///
/// Does not insert [`Authority`] or [`crate::Registries`]: the app (or the
/// facade) does, before or after `add_plugins`. An `Authority` is defaulted
/// to [`crate::LocalAuthority`] at startup if none is present.
#[derive(Default)]
pub struct SlottedEcsPlugin;

impl Plugin for SlottedEcsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ActionQueue>()
            .init_resource::<ClickInterpreter>()
            .init_resource::<MenuIdAllocator>()
            .init_resource::<PendingRoundTrips>()
            .init_resource::<PendingSubmissions>()
            .init_resource::<PlayerInventories>()
            .init_resource::<Dropped>()
            .add_message::<SlotSync>()
            .add_observer(interpret_slot_click)
            .add_observer(enqueue_menu_action)
            .add_observer(crate::systems::apply_set_property)
            .add_observer(crate::systems::apply_set_slot)
            .configure_sets(
                Update,
                (
                    SlottedEcsSet::Input,
                    SlottedEcsSet::Predict,
                    SlottedEcsSet::Submit,
                    SlottedEcsSet::Reconcile,
                )
                    .chain(),
            )
            .add_systems(Startup, default_authority)
            .add_systems(
                Update,
                (
                    (clear_dirty_masks, gather_input, register_slot_refs)
                        .chain()
                        .in_set(SlottedEcsSet::Input),
                    predict.in_set(SlottedEcsSet::Predict),
                    submit.in_set(SlottedEcsSet::Submit),
                    (reconcile, mirror_favorites)
                        .chain()
                        .in_set(SlottedEcsSet::Reconcile),
                ),
            );
    }
}

fn default_authority(mut commands: Commands, existing: Option<Res<Authority>>) {
    if existing.is_none() {
        commands.insert_resource(Authority::local());
    }
}
