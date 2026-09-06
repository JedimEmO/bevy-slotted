//! Pointer gestures on slots, translated into the ecs crate's events.
//!
//! Observers only ever *trigger*; nothing here touches an inventory. A press
//! and release on the same slot becomes `SlotClicked`, which the ecs crate's
//! interpreter turns into a `MenuAction`. A drag across slots is a paint, and
//! goes straight to `MenuAction(ClickAction::Drag)` because its three stages
//! carry state `SlotClicked` has no room for.
//!
//! # A moving click is still a click
//!
//! `bevy_picking` has no drag threshold: it sends `Pointer<DragStart>` the
//! first time the pointer moves at all while a button is held, so a hand that
//! shifts one pixel between press and release turns an ordinary click into a
//! drag. Taking that at face value meant the release was swallowed as the end
//! of a paint and nothing was picked up, which is the "picking up items is
//! unreliable" report.
//!
//! So `DragStart` only *arms* a paint here, in [`DragPaint::pending`]. The
//! paint begins when the pointer actually reaches a second slot, which is the
//! gesture a paint means; the origin slot is painted then, retroactively.
//! Until that happens the press is still a click, and a drag that ends where
//! it started releases as one.
//!
//! # Sweeps
//!
//! Shift held while the left button is dragged across slots is the Mouse
//! Tweaks sweep: every slot the pointer touches is quick-moved once, the
//! press included. It is not a paint, so it never arms one, and a slot
//! already swept is never swept twice however often the pointer crosses it.
//! See `docs/research/research-nei-overlays.md` section 6.

use bevy::picking::events::{DragEnd, DragEnter, DragStart, Pointer, Press, Release};
use bevy::prelude::*;
use slotted_ecs::{MenuAction, SlotClicked, SlotRef};
use slotted_model::{Button as ModelButton, ClickAction, DragKind, DragStage, SlotIx};

use crate::item::ItemView;
use crate::widgets::{model_button, modifiers_from};

/// A press that has begun to move but has not yet reached a second slot.
/// Not a paint: the gesture is still a click until it leaves its origin.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PendingDrag {
    /// The menu the origin slot belongs to.
    pub menu: Entity,
    /// The slot the press landed on.
    pub entity: Entity,
    /// Its index.
    pub slot: SlotIx,
    /// Which distribution the held button would paint.
    pub kind: DragKind,
}

/// State of an in-progress drag paint. One pointer at a time in Phase 2.
#[derive(Resource, Debug, Default, Clone, PartialEq, Eq)]
pub struct DragPaint {
    /// Which distribution, `None` when no paint is running.
    pub kind: Option<DragKind>,
    /// A press that has moved but not yet reached a second slot. Promoted to
    /// [`kind`](Self::kind) by [`on_slot_drag_enter`]; dropped without ever
    /// painting anything if the gesture ends on its origin.
    pub pending: Option<PendingDrag>,
    /// Set when a drag ended this frame, so the `Release` that follows does
    /// not also read as a click.
    pub suppress_release: bool,
}

impl DragPaint {
    /// `true` while a paint is actually running, as opposed to armed.
    pub const fn painting(&self) -> bool {
        self.kind.is_some()
    }
}

/// A shift-drag in progress: one quick-move per slot the pointer touches.
///
/// The slot the press landed on is swept immediately, which is the ordinary
/// shift-click; the sweep then continues into every further slot. `visited`
/// holds every slot already swept, empty ones included, so a pointer that
/// wanders back over a slot it has just emptied does not move the stack that
/// landed there back again.
#[derive(Resource, Debug, Default, Clone, PartialEq, Eq)]
pub struct SweepQuickMove {
    /// `true` between the shift-press and the release that ends it.
    pub active: bool,
    /// Slots already swept, in sweep order.
    pub visited: Vec<SlotIx>,
}

impl SweepQuickMove {
    /// Ends the sweep and forgets what it touched.
    fn end(&mut self) {
        self.active = false;
        self.visited.clear();
    }

    /// `true` when `slot` has not been swept yet, recording it either way.
    fn first_visit(&mut self, slot: SlotIx) -> bool {
        if self.visited.contains(&slot) {
            return false;
        }
        self.visited.push(slot);
        true
    }
}

/// Quick-moves `slot` unless the sweep has already been there or it is empty.
fn sweep_slot(
    sweep: &mut SweepQuickMove,
    slot: &SlotRef,
    view: Option<&ItemView>,
    commands: &mut Commands,
) {
    if !sweep.first_visit(slot.slot) {
        return;
    }
    // An empty slot has nothing to move. It still counts as visited: a
    // quick-move later in the same sweep may well fill it.
    if view.is_none_or(|v| v.stack.is_none()) {
        return;
    }
    commands.trigger(MenuAction {
        entity: slot.menu,
        action: ClickAction::QuickMove { slot: slot.slot },
    });
}

/// `SlottedUiSet::Input`: clears the one-frame release suppression.
pub fn clear_drag_suppression(mut drag: ResMut<DragPaint>) {
    if drag.suppress_release {
        drag.suppress_release = false;
    }
}

fn drag_kind(button: ModelButton) -> DragKind {
    match button {
        ModelButton::Left => DragKind::Left,
        ModelButton::Right => DragKind::Right,
        ModelButton::Middle => DragKind::Middle,
    }
}

/// Observer on a slot: a press is recorded but never acted on, so that a
/// gesture is only ever interpreted once, on release. The marker it leaves is
/// what `slot_motion` reads to hold the slot at the press scale.
pub fn on_slot_press(
    press: On<Pointer<Press>>,
    slots: Query<(&SlotRef, Option<&ItemView>)>,
    keys: Res<ButtonInput<KeyCode>>,
    mut sweep: ResMut<SweepQuickMove>,
    mut commands: Commands,
) {
    tracing::trace!(entity = ?press.entity, button = ?press.event.button, "slot pressed");
    commands
        .entity(press.entity)
        .insert(crate::motion::SlotPressed);
    if model_button(press.event.button) != ModelButton::Left || !modifiers_from(&keys).shift {
        return;
    }
    let Ok((slot, view)) = slots.get(press.entity) else {
        return;
    };
    // Shift-clicking is a quick-move whether or not the pointer goes on to
    // move, so the press acts at once and the release is swallowed below.
    sweep.active = true;
    sweep.visited.clear();
    sweep_slot(&mut sweep, slot, view, &mut commands);
}

/// Observer on a slot: `Pointer<Release>` becomes [`SlotClicked`] with the
/// modifiers held at that moment.
pub fn on_slot_release(
    release: On<Pointer<Release>>,
    slots: Query<&SlotRef>,
    keys: Res<ButtonInput<KeyCode>>,
    drag: Res<DragPaint>,
    mut sweep: ResMut<SweepQuickMove>,
    mut commands: Commands,
) {
    let entity = release.entity;
    commands
        .entity(entity)
        .try_remove::<crate::motion::SlotPressed>();
    if sweep.active {
        // The press already quick-moved; releasing only ends the sweep.
        sweep.end();
        return;
    }
    if drag.kind.is_some() || drag.suppress_release {
        return;
    }
    if slots.get(entity).is_err() {
        return;
    }
    commands.trigger(SlotClicked {
        entity,
        button: model_button(release.event.button),
        modifiers: modifiers_from(&keys),
    });
}

/// Observer on a slot: the pointer has moved while held, which *arms* a
/// paint without starting one.
///
/// Nothing is triggered here. `bevy_picking` sends this on the first pixel of
/// movement, and one pixel is a click with a shaky hand, not a drag. See the
/// module docs.
pub fn on_slot_drag_start(
    start: On<Pointer<DragStart>>,
    slots: Query<&SlotRef>,
    sweep: Res<SweepQuickMove>,
    mut drag: ResMut<DragPaint>,
) {
    // A sweep is not a paint and must never arm one.
    if drag.kind.is_some() || sweep.active {
        return;
    }
    let Ok(slot) = slots.get(start.entity) else {
        return;
    };
    drag.pending = Some(PendingDrag {
        menu: slot.menu,
        entity: start.entity,
        slot: slot.slot,
        kind: drag_kind(model_button(start.event.button)),
    });
}

/// Observer on a slot: the pointer entering a second slot is what makes the
/// gesture a paint. It starts one if none is running, painting the origin
/// slot first, then adds the slot just entered.
pub fn on_slot_drag_enter(
    enter: On<Pointer<DragEnter>>,
    slots: Query<(&SlotRef, Option<&ItemView>)>,
    mut sweep: ResMut<SweepQuickMove>,
    mut drag: ResMut<DragPaint>,
    mut commands: Commands,
) {
    let Ok((slot, view)) = slots.get(enter.entity) else {
        return;
    };
    if sweep.active {
        sweep_slot(&mut sweep, slot, view, &mut commands);
        return;
    }
    let kind = if let Some(kind) = drag.kind {
        kind
    } else {
        // Arming turns into a paint only on a *different* slot. A pointer
        // that wanders inside its origin slot has not drawn anything.
        let Some(pending) = drag.pending.filter(|p| p.entity != enter.entity) else {
            return;
        };
        drag.kind = Some(pending.kind);
        drag.pending = None;
        commands.trigger(MenuAction {
            entity: pending.menu,
            action: ClickAction::Drag {
                stage: DragStage::Start,
                kind: pending.kind,
                slot: None,
            },
        });
        commands.trigger(MenuAction {
            entity: pending.menu,
            action: ClickAction::Drag {
                stage: DragStage::Add,
                kind: pending.kind,
                slot: Some(pending.slot),
            },
        });
        pending.kind
    };
    commands.trigger(MenuAction {
        entity: slot.menu,
        action: ClickAction::Drag {
            stage: DragStage::Add,
            kind,
            slot: Some(slot.slot),
        },
    });
}

/// Observer on a slot: releasing ends the paint and distributes.
///
/// A gesture that never reached a second slot ends here with nothing to
/// distribute and, crucially, without suppressing the release: `Pointer<
/// Release>` has already run this frame and read it as the click it was.
pub fn on_slot_drag_end(
    end: On<Pointer<DragEnd>>,
    slots: Query<&SlotRef>,
    mut sweep: ResMut<SweepQuickMove>,
    mut drag: ResMut<DragPaint>,
    mut commands: Commands,
) {
    commands
        .entity(end.entity)
        .try_remove::<crate::motion::SlotPressed>();
    if sweep.active {
        // `Release` may not have run yet; suppression keeps it from reading
        // the end of a sweep as one more shift-click.
        sweep.end();
        drag.suppress_release = true;
        return;
    }
    drag.pending = None;
    let Some(kind) = drag.kind.take() else {
        return;
    };
    drag.suppress_release = true;
    let Ok(slot) = slots.get(end.entity) else {
        return;
    };
    commands.trigger(MenuAction {
        entity: slot.menu,
        action: ClickAction::Drag {
            stage: DragStage::End,
            kind,
            slot: None,
        },
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drag_kinds_follow_the_button() {
        assert_eq!(drag_kind(ModelButton::Left), DragKind::Left);
        assert_eq!(drag_kind(ModelButton::Right), DragKind::Right);
        assert_eq!(drag_kind(ModelButton::Middle), DragKind::Middle);
    }
}
