//! The focus ring (menus contract 2.3): one entity that follows Bevy's
//! `InputFocus` and shows whenever the player is not on the mouse.
//!
//! The ring is two nodes. The [`FocusRing`] entity is an absolute wrapper
//! that carries the state, the z band and the visibility, and slides with a
//! `Translate` tween; its one child, the [`FocusRingFrame`], is the themed
//! border and grows or shrinks with a `Size` tween. Two nodes because a
//! node holds one [`Tween`] at a time, and a move between a slot and a
//! button needs both.

use bevy::input_focus::InputFocus;
use bevy::prelude::*;
use bevy::ui::ui_transform::{UiGlobalTransform, UiTransform};
use slotted_theme::{Easing, Motion, MotionPreset, Themed, Tween, TweenTarget, roles};

use crate::actions::InputMode;
use crate::layers::zbands;
use crate::semantic::ScreenRoot;

/// Marks a node the ring may sit on. Inserted by every interactive widget;
/// `TabIndex` alone is not enough.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Focusable;

/// The ring entity.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FocusRing;

/// The ring's visible border, the one child of the [`FocusRing`] entity.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FocusRingFrame;

/// What the ring is doing, for tests and the hint bar.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FocusRingState {
    /// The focused `Focusable`, if any.
    pub target: Option<Entity>,
    /// Whether the ring is shown.
    pub visible: bool,
}

/// Where the ring last sat, in UI units, so the next move can start from it.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq)]
pub struct FocusRingRect(pub Option<Rect>);

/// Border width of the ring, in UI units. The theme colours it.
pub const RING_BORDER: f32 = 2.0;

/// `Startup`, when `SlottedUiConfig::spawn_layers`: spawns the ring hidden.
pub fn spawn_focus_ring(mut commands: Commands, config: Res<crate::plugin::SlottedUiConfig>) {
    if !config.spawn_layers {
        return;
    }
    let ring = commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: px(0),
                top: px(0),
                ..default()
            },
            GlobalZIndex(zbands::FOCUS),
            Pickable::IGNORE,
            Visibility::Hidden,
            FocusRing,
            FocusRingState::default(),
            FocusRingRect::default(),
        ))
        .id();
    commands.spawn((
        Node {
            width: px(0),
            height: px(0),
            border: UiRect::all(px(RING_BORDER)),
            ..default()
        },
        Themed(roles::FOCUS_RING),
        Pickable::IGNORE,
        FocusRingFrame,
        ChildOf(ring),
    ));
}

/// The screen root above `entity`, if it sits under one.
fn screen_root_of(
    entity: Entity,
    parents: &Query<&ChildOf>,
    roots: &Query<(), With<ScreenRoot>>,
) -> Option<Entity> {
    let mut current = entity;
    loop {
        if roots.contains(current) {
            return Some(current);
        }
        current = parents.get(current).ok()?.parent();
    }
}

/// `SlottedUiSet::Render`: moves, sizes and shows or hides the ring.
///
/// The target is `InputFocus` when that entity is `Focusable`; the ring is
/// visible when there is a target and the player is not on the mouse. A move
/// slides over `durations.fast` with the standard ease; it snaps under
/// reduced motion, on the first placement and when the target is on another
/// screen root.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn update_focus_ring(
    focus: Option<Res<InputFocus>>,
    mode: Res<InputMode>,
    motion: Res<Motion>,
    tokens: crate::tooltip::ThemeTokens,
    units: crate::scale::UiUnits,
    focusables: Query<(&ComputedNode, &UiGlobalTransform), With<Focusable>>,
    masks: Query<(), With<crate::nav::FocusMask>>,
    parents: Query<&ChildOf>,
    roots: Query<(), With<ScreenRoot>>,
    mut ring: Query<
        (
            Entity,
            &mut Node,
            &mut Visibility,
            &mut FocusRingState,
            &mut FocusRingRect,
            &Children,
            Has<Tween>,
        ),
        With<FocusRing>,
    >,
    mut frames: Query<(&mut Node, Has<Tween>), (With<FocusRingFrame>, Without<FocusRing>)>,
    mut commands: Commands,
) {
    let Ok((ring_entity, mut node, mut visibility, mut state, mut last, children, ring_tweening)) =
        ring.single_mut()
    else {
        return;
    };
    // A focusable under a `FocusMask` (a hidden tab page, menus M1 4.4)
    // gets no ring.
    let target = focus
        .and_then(|f| f.get())
        .filter(|e| focusables.contains(*e))
        .filter(|e| crate::widgets::tabs::masked_ancestor(*e, &masks, &parents).is_none());
    let visible = target.is_some() && *mode != InputMode::Pointer;

    let tokens = tokens.get();
    let grow = tokens.spacing.xs;
    let rect = target.and_then(|e| {
        let (computed, transform) = focusables.get(e).ok()?;
        let scale = computed.inverse_scale_factor();
        let size = computed.size() * scale;
        let center = transform.translation * scale;
        let logical = Rect::from_center_size(center, size);
        let ui = units.rect(logical);
        Some(Rect {
            min: ui.min - Vec2::splat(grow),
            max: ui.max + Vec2::splat(grow),
        })
    });

    let Some(frame) = children.first().copied() else {
        return;
    };
    let Ok((mut frame_node, frame_tweening)) = frames.get_mut(frame) else {
        return;
    };

    if let Some(rect) = rect {
        let target_changed = state.target != target;
        let previous = last.0;
        let same_screen = match (state.target, target) {
            (Some(old), Some(new)) => {
                screen_root_of(old, &parents, &roots) == screen_root_of(new, &parents, &roots)
            }
            _ => false,
        };
        let snap = motion.reduced || previous.is_none() || !same_screen || !state.visible;
        if target_changed {
            node.left = px(rect.min.x);
            node.top = px(rect.min.y);
            match previous {
                Some(from) if !snap => {
                    // `durations.fast` with the standard ease (contract 2.3):
                    // the tier behind `Hover`, not a theme's per-preset
                    // override of it.
                    let fast = motion.duration(MotionPreset::Hover, &tokens.durations);
                    let easing = Easing::Standard;
                    commands.entity(ring_entity).insert(
                        Tween::new(
                            TweenTarget::Translate {
                                from: from.min - rect.min,
                                to: Vec2::ZERO,
                            },
                            fast,
                        )
                        .with_easing(easing),
                    );
                    commands.entity(frame).insert(
                        Tween::new(
                            TweenTarget::Size {
                                from: from.size(),
                                to: rect.size(),
                            },
                            fast,
                        )
                        .with_easing(easing),
                    );
                }
                _ => {
                    commands.entity(ring_entity).remove::<Tween>();
                    commands.entity(frame).remove::<Tween>();
                    commands.entity(ring_entity).insert(UiTransform::IDENTITY);
                    frame_node.width = px(rect.width());
                    frame_node.height = px(rect.height());
                }
            }
            last.0 = Some(rect);
        } else if previous != Some(rect) && !ring_tweening && !frame_tweening {
            // Same target, moved by layout: follow it without ceremony.
            node.left = px(rect.min.x);
            node.top = px(rect.min.y);
            frame_node.width = px(rect.width());
            frame_node.height = px(rect.height());
            last.0 = Some(rect);
        }
    }

    let wanted = if visible {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };
    if *visibility != wanted {
        *visibility = wanted;
    }
    let next = FocusRingState { target, visible };
    if *state != next {
        *state = next;
    }
}

/// Registers the ring's systems.
pub fn build(app: &mut App) {
    app.add_systems(Startup, spawn_focus_ring).add_systems(
        Update,
        update_focus_ring.in_set(crate::plugin::SlottedUiSet::Render),
    );
}
