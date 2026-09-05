//! Pointer gestures on slots, translated into the ecs crate's events.
//!
//! Observers only ever *trigger*; nothing here touches an inventory. A press
//! and release on the same slot becomes `SlotClicked`, which the ecs crate's
//! interpreter turns into a `MenuAction`. A drag across slots is a paint, and
//! goes straight to `MenuAction(ClickAction::Drag)` because its three stages
//! carry state `SlotClicked` has no room for.

use bevy::picking::events::{DragEnd, DragEnter, DragStart, Pointer, Press, Release};
use bevy::prelude::*;
use slotted_ecs::{MenuAction, SlotClicked, SlotRef};
use slotted_model::{Button as ModelButton, ClickAction, DragKind, DragStage};

use crate::widgets::{model_button, modifiers_from};

/// State of an in-progress drag paint. One pointer at a time in Phase 2.
#[derive(Resource, Debug, Default, Clone, PartialEq, Eq)]
pub struct DragPaint {
    /// Which distribution, `None` when no drag is running.
    pub kind: Option<DragKind>,
    /// Set when a drag ended this frame, so the `Release` that follows does
    /// not also read as a click.
    pub suppress_release: bool,
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
/// gesture is only ever interpreted once, on release.
pub fn on_slot_press(press: On<Pointer<Press>>) {
    tracing::trace!(entity = ?press.entity, button = ?press.event.button, "slot pressed");
}

/// Observer on a slot: `Pointer<Release>` becomes [`SlotClicked`] with the
/// modifiers held at that moment.
pub fn on_slot_release(
    release: On<Pointer<Release>>,
    slots: Query<&SlotRef>,
    keys: Res<ButtonInput<KeyCode>>,
    drag: Res<DragPaint>,
    mut commands: Commands,
) {
    let entity = release.entity;
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

/// Observer on a slot: dragging out of it starts a paint and immediately
/// paints the origin slot.
pub fn on_slot_drag_start(
    start: On<Pointer<DragStart>>,
    slots: Query<&SlotRef>,
    mut drag: ResMut<DragPaint>,
    mut commands: Commands,
) {
    let Ok(slot) = slots.get(start.entity) else {
        return;
    };
    let kind = drag_kind(model_button(start.event.button));
    drag.kind = Some(kind);
    commands.trigger(MenuAction {
        entity: slot.menu,
        action: ClickAction::Drag {
            stage: DragStage::Start,
            kind,
            slot: None,
        },
    });
    commands.trigger(MenuAction {
        entity: slot.menu,
        action: ClickAction::Drag {
            stage: DragStage::Add,
            kind,
            slot: Some(slot.slot),
        },
    });
}

/// Observer on a slot: the pointer entering while a paint runs adds it.
pub fn on_slot_drag_enter(
    enter: On<Pointer<DragEnter>>,
    slots: Query<&SlotRef>,
    drag: Res<DragPaint>,
    mut commands: Commands,
) {
    let Some(kind) = drag.kind else {
        return;
    };
    let Ok(slot) = slots.get(enter.entity) else {
        return;
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
pub fn on_slot_drag_end(
    end: On<Pointer<DragEnd>>,
    slots: Query<&SlotRef>,
    mut drag: ResMut<DragPaint>,
    mut commands: Commands,
) {
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
