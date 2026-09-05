//! The theme's motion presets, applied to real widgets.
//!
//! `slotted-theme` owns how long an animation takes and how it eases;
//! this module owns *what* animates. Four of the presets are wired here:
//!
//! - [`MotionPreset::Hover`] scales a slot to [`HOVER_SCALE`] and back,
//! - [`MotionPreset::Press`] to [`PRESS_SCALE`] while a button is held,
//! - [`MotionPreset::DropSquash`] pops a slot that just received a stack,
//! - [`MotionPreset::FlyToSlot`] sends a transient icon from the slot a
//!   quick-move left to the slot it landed in.
//!
//! Every one of them reads `Time<Virtual>` through
//! [`advance_tweens`](slotted_theme::advance_tweens), so a harness stepping
//! frames by hand sees exactly what a player does, and every one of them
//! collapses to a single frame under [`Motion::reduced`].
//!
//! A slot's scale is state, not a sequence: [`MotionTarget`] records where it
//! is heading and [`slot_motion`] retargets from wherever the current tween
//! got to. That is what stops a pointer leaving mid-tween from parking a slot
//! at 1.02 forever.

use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use bevy::ui::ui_transform::{UiGlobalTransform, UiTransform};
use slotted_ecs::{MenuAction, SlotChanged, SlotEntities, SlotRef};
use slotted_model::{ClickAction, SlotIx};
use slotted_theme::{Motion, MotionPreset, Tween, TweenTarget};

use crate::item::{ItemView, spawn_item_view_children};
use crate::layers::CarriedLayer;
use crate::tooltip::ThemeTokens;
use crate::widgets::SLOT_SIZE;

/// A slot at rest.
pub const REST_SCALE: f32 = 1.0;
/// A hovered slot.
pub const HOVER_SCALE: f32 = 1.04;
/// A slot with a pointer button held on it.
pub const PRESS_SCALE: f32 = 0.96;
/// How far a slot squashes when a stack lands in it, before springing back to
/// whatever [`MotionTarget`] it was heading for.
pub const SQUASH_SCALE: f32 = 0.88;

/// The scale a slot is heading for. Written by [`slot_motion`]; a tween is
/// started only when this changes, so a retarget mid-flight is one insert and
/// never a queue of animations.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct MotionTarget(pub f32);

/// On a slot with a pointer button held down on it.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SlotPressed;

/// The transient icon a quick-move flies from one slot to another. Despawned
/// by [`despawn_finished_flights`] once its tween is done.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FlyingItem;

/// The slot an action named this frame, so [`drop_squash`] can tell a stack
/// the player put somewhere from one that arrived by any other route.
///
/// Set by [`record_gesture_target`] from the `MenuAction` every input path
/// converges on, which is why the squash works for a pointer click and for
/// the harness's semantic `click_slot` alike. Cleared at the end of the
/// frame by [`clear_gesture_target`].
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq)]
pub struct GestureTarget {
    /// Menu and slot the action named, if it named one.
    pub slot: Option<(Entity, SlotIx)>,
    /// Source slot of a quick-move waiting for its destination.
    pub quick_move: Option<(Entity, SlotIx)>,
}

/// Observer: remember which slot this frame's action was aimed at.
pub fn record_gesture_target(action: On<MenuAction>, mut target: ResMut<GestureTarget>) {
    let menu = action.entity;
    let named = match action.action {
        ClickAction::Pickup { slot, .. }
        | ClickAction::Swap { slot, .. }
        | ClickAction::Clone { slot }
        | ClickAction::Throw { slot, .. }
        | ClickAction::PickupAll { slot, .. }
        | ClickAction::QuickMove { slot } => Some(slot),
        ClickAction::Drag { slot, .. } => slot,
        ClickAction::Toolbar(_) => None,
    };
    target.slot = named.map(|slot| (menu, slot));
    if let ClickAction::QuickMove { slot } = action.action {
        target.quick_move = Some((menu, slot));
    }
}

/// `SlottedUiSet::Render`, last: the gesture target lasts one frame.
pub fn clear_gesture_target(mut target: ResMut<GestureTarget>) {
    *target = GestureTarget::default();
}

/// The tween a scale change becomes, starting from wherever the node is now.
fn retarget(
    commands: &mut Commands,
    entity: Entity,
    from: f32,
    to: f32,
    preset: MotionPreset,
    motion: Motion,
    durations: &slotted_theme::Durations,
) {
    commands.entity(entity).insert((
        MotionTarget(to),
        motion.tween(preset, TweenTarget::Scale { from, to }, durations),
    ));
}

fn current_scale(transform: Option<&UiTransform>) -> f32 {
    transform.map_or(REST_SCALE, |t| t.scale.x)
}

fn near(a: f32, b: f32) -> bool {
    (a - b).abs() < 1e-4
}

/// `SlottedUiSet::Render`: hover and press scaling on every slot.
///
/// Press wins over hover, and both are expressed as a target rather than as
/// an event, so a pointer that leaves halfway through the hover-in tween just
/// retargets to [`REST_SCALE`] from the scale reached so far.
pub fn slot_motion(
    motion: Res<Motion>,
    tokens: ThemeTokens,
    mut commands: Commands,
    slots: Query<
        (
            Entity,
            &Hovered,
            Has<SlotPressed>,
            Option<&MotionTarget>,
            Option<&UiTransform>,
        ),
        With<SlotRef>,
    >,
) {
    let durations = tokens.get().durations;
    for (entity, hovered, pressed, target, transform) in &slots {
        let wanted = if pressed {
            PRESS_SCALE
        } else if hovered.get() {
            HOVER_SCALE
        } else {
            REST_SCALE
        };
        match target {
            Some(target) if near(target.0, wanted) => continue,
            // A slot that has never moved and does not need to: record where
            // it is without spending a tween on it.
            None if near(wanted, REST_SCALE) => {
                commands.entity(entity).insert(MotionTarget(REST_SCALE));
                continue;
            }
            _ => {}
        }
        let leaving_press = target.is_some_and(|t| near(t.0, PRESS_SCALE));
        let preset = if pressed || leaving_press {
            MotionPreset::Press
        } else {
            MotionPreset::Hover
        };
        retarget(
            &mut commands,
            entity,
            current_scale(transform),
            wanted,
            preset,
            *motion,
            &durations,
        );
    }
}

/// Observer: a stack landing in the slot the player aimed at squashes it.
///
/// The tween runs from [`SQUASH_SCALE`] back to the slot's current
/// [`MotionTarget`], so it springs out to rest (or to the hover scale, if the
/// pointer is still over it) without [`slot_motion`] fighting it: the target
/// itself never changes.
pub fn drop_squash(
    changed: On<SlotChanged>,
    target: Res<GestureTarget>,
    motion: Res<Motion>,
    tokens: ThemeTokens,
    slots: Query<Option<&MotionTarget>, With<SlotRef>>,
    mut commands: Commands,
) {
    if changed.stack.is_none() {
        return;
    }
    if target.slot != Some((changed.menu, changed.slot)) {
        return;
    }
    let Ok(motion_target) = slots.get(changed.entity) else {
        return;
    };
    let rest = motion_target.map_or(REST_SCALE, |t| t.0);
    let durations = tokens.get().durations;
    commands.entity(changed.entity).insert(motion.tween(
        MotionPreset::DropSquash,
        TweenTarget::Scale {
            from: SQUASH_SCALE,
            to: rest,
        },
        &durations,
    ));
}

/// Observer: a quick-move flies a transient icon from the slot it left to the
/// slot it landed in.
// Bevy systems declare their world access as parameters; splitting this one
// would only move the same access into a `SystemParam` struct.
#[allow(clippy::too_many_arguments)]
///
/// Skipped entirely under [`Motion::reduced`]: a flight that completes on its
/// first frame is a node that appears and disappears for no reason.
pub fn fly_to_slot(
    changed: On<SlotChanged>,
    mut target: ResMut<GestureTarget>,
    motion: Res<Motion>,
    tokens: ThemeTokens,
    menus: Query<&SlotEntities>,
    rects: Query<(&ComputedNode, &UiGlobalTransform)>,
    layers: Query<Entity, With<CarriedLayer>>,
    mut commands: Commands,
) {
    if motion.reduced {
        return;
    }
    let Some((menu, source_slot)) = target.quick_move else {
        return;
    };
    let Some(stack) = changed.stack.clone() else {
        return;
    };
    if changed.menu != menu || changed.slot == source_slot {
        return;
    }
    let Ok(slot_entities) = menus.get(menu) else {
        return;
    };
    let Some(source) = slot_entities.0.get(&source_slot).copied() else {
        return;
    };
    let (Some(from), Some(to)) = (rect_of(&rects, source), rect_of(&rects, changed.entity)) else {
        return;
    };
    target.quick_move = None;

    let Some(layer) = layers.iter().next() else {
        return;
    };
    let duration = motion.duration(MotionPreset::FlyToSlot, &tokens.get().durations);
    let travel = to.center() - from.center();
    let corner = from.center() - Vec2::splat(SLOT_SIZE * 0.5);
    commands.queue(move |world: &mut World| {
        let item = world
            .spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(corner.x),
                    top: Val::Px(corner.y),
                    width: Val::Px(SLOT_SIZE),
                    height: Val::Px(SLOT_SIZE),
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::Center,
                    ..default()
                },
                ItemView::new(Some(stack)),
                FlyingItem,
                Pickable::IGNORE,
                Tween::new(
                    TweenTarget::Translate {
                        from: Vec2::ZERO,
                        to: travel,
                    },
                    duration,
                ),
                ChildOf(layer),
            ))
            .id();
        spawn_item_view_children(world, item);
    });
}

fn rect_of(rects: &Query<(&ComputedNode, &UiGlobalTransform)>, entity: Entity) -> Option<Rect> {
    let (node, transform) = rects.get(entity).ok()?;
    let scale = node.inverse_scale_factor();
    let size = node.size() * scale;
    if size.x <= 0.0 || size.y <= 0.0 {
        return None;
    }
    Some(Rect::from_center_size(transform.translation * scale, size))
}

/// `SlottedUiSet::Render`: a flight whose tween has finished is over.
pub fn despawn_finished_flights(
    finished: Query<Entity, (With<FlyingItem>, Without<Tween>)>,
    mut commands: Commands,
) {
    for entity in &finished {
        commands.entity(entity).despawn();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The four scales are an ordering, and rest has to be identity or a slot
    /// that finishes a tween is not where layout put it.
    #[test]
    fn the_scales_are_ordered_around_identity() {
        const _: () = assert!(SQUASH_SCALE < PRESS_SCALE);
        const _: () = assert!(PRESS_SCALE < REST_SCALE);
        const _: () = assert!(REST_SCALE < HOVER_SCALE);
        assert!(near(REST_SCALE, 1.0), "rest must be identity");
    }

    #[test]
    fn near_tolerates_tween_rounding_but_not_a_visible_gap() {
        assert!(near(1.0, 1.000_01));
        assert!(!near(REST_SCALE, HOVER_SCALE));
    }
}
