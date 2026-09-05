//! The prediction loop: interpret, queue, predict, submit, reconcile.

use std::collections::VecDeque;

use bevy::prelude::*;
use slotted_model::{ClickAction, SlotIx};

use crate::events::{MenuAction, SlotClicked};

/// Actions collected by observers during a frame, applied in order by
/// `SlottedEcsSet::Predict`. Observers never touch inventories directly, so
/// the model sees one deterministic sequence per frame.
#[derive(Resource, Debug, Default)]
pub struct ActionQueue(pub VecDeque<MenuAction>);

/// Turns raw [`SlotClicked`] gestures into [`ClickAction`]s.
///
/// Owns the double-click window (virtual time) and the current drag paint
/// state. The mapping is vanilla's: left/right = `Pickup`, shift+left =
/// `QuickMove`, middle = `Clone`, a second left click on the same slot within
/// `double_click_window` = `PickupAll`.
#[derive(Resource, Debug)]
pub struct ClickInterpreter {
    /// Two clicks closer than this on the same slot become `PickupAll`.
    pub double_click_window: std::time::Duration,
    /// Last left click: slot and virtual time.
    pub last_click: Option<(Entity, SlotIx, std::time::Duration)>,
}

impl Default for ClickInterpreter {
    fn default() -> Self {
        Self {
            double_click_window: std::time::Duration::from_millis(250),
            last_click: None,
        }
    }
}

impl ClickInterpreter {
    /// Pure mapping from a gesture to an action, without the double-click
    /// state. `slot` is the clicked slot's index.
    pub fn interpret(&self, slot: SlotIx, click: &SlotClicked) -> ClickAction {
        use slotted_model::Button;
        match (click.button, click.modifiers.shift) {
            (Button::Left, true) => ClickAction::QuickMove { slot },
            (Button::Middle, _) => ClickAction::Clone { slot },
            (button, _) => ClickAction::Pickup { slot, button },
        }
    }
}

/// Observer: [`SlotClicked`] on a `SlotRef` entity becomes a [`MenuAction`]
/// on its menu.
// PHASE2-IMPL: agent A. Resolve SlotRef, apply the double-click window from
// Time<Virtual>, handle drag paint, then `commands.trigger(MenuAction { .. })`.
pub fn interpret_slot_click(click: On<SlotClicked>, _commands: Commands) {
    tracing::warn!(?click, "interpret_slot_click is not implemented yet");
}

/// Observer: [`MenuAction`] is pushed onto the [`ActionQueue`].
pub fn enqueue_menu_action(action: On<MenuAction>, mut queue: ResMut<ActionQueue>) {
    queue.0.push_back(*action.event());
}

/// `SlottedEcsSet::Input`: nothing yet beyond what observers collected. Kept
/// as a system so games can order against the set.
pub fn gather_input() {}

/// `SlottedEcsSet::Predict`: drain [`ActionQueue`], assemble `Inventories`
/// from the menu's inventory entities, run `apply_click`, write changed
/// inventories back, update `Carried`, write `SlotSync` messages and trigger
/// `SlotChanged` on registered slot entities, then record the delta for
/// `Submit`.
// PHASE2-IMPL: agent A.
pub fn predict(mut queue: ResMut<ActionQueue>) {
    if !queue.0.is_empty() {
        tracing::warn!(
            count = queue.0.len(),
            "predict is not implemented yet; dropping actions"
        );
        queue.0.clear();
    }
}

/// `SlottedEcsSet::Submit`: hand each predicted delta to the `Authority`
/// resource and bump `PendingRoundTrips`.
// PHASE2-IMPL: agent A.
pub fn submit() {}

/// `SlottedEcsSet::Reconcile`: poll the authority. `Ack` decrements
/// `PendingRoundTrips`; `Resync` overwrites the menu state and inventories
/// and re-emits `SlotSync`/`SlotChanged` for every slot; `Property` updates
/// the `MenuProperty` child and triggers `PropertyChanged`. Also mirrors
/// `Favorite` onto slot entities.
// PHASE2-IMPL: agent A.
pub fn reconcile() {}

/// Keeps [`SlotEntities`](crate::SlotEntities) on each menu in step with
/// `Added<SlotRef>`.
pub fn register_slot_refs(
    added: Query<(Entity, &SlotRef), Added<SlotRef>>,
    mut menus: Query<&mut crate::SlotEntities>,
) {
    for (entity, slot_ref) in &added {
        if let Ok(mut slots) = menus.get_mut(slot_ref.menu) {
            slots.0.insert(slot_ref.slot, entity);
        } else {
            tracing::warn!(
                ?entity,
                ?slot_ref,
                "SlotRef points at an entity without OpenMenu"
            );
        }
    }
}

use crate::SlotRef;
