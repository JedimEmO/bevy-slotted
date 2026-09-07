//! The focus ring (menus contract 2.3): one entity that follows Bevy's
//! `InputFocus` and shows whenever the player is not on the mouse.

use bevy::input_focus::InputFocus;
use bevy::prelude::*;

use crate::actions::InputMode;

/// Marks a node the ring may sit on. Inserted by every interactive widget;
/// `TabIndex` alone is not enough.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Focusable;

/// The ring entity.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FocusRing;

/// What the ring is doing, for tests and the hint bar.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FocusRingState {
    /// The focused `Focusable`, if any.
    pub target: Option<Entity>,
    /// Whether the ring is shown.
    pub visible: bool,
}

/// `Startup`, when `SlottedUiConfig::spawn_layers`: spawns the ring hidden.
pub fn spawn_focus_ring(_commands: Commands, _config: Res<crate::plugin::SlottedUiConfig>) {
    // M0-IMPL: A
}

/// `SlottedUiSet::Render`: moves, sizes and shows or hides the ring.
pub fn update_focus_ring(
    _focus: Option<Res<InputFocus>>,
    _mode: Res<InputMode>,
    _focusables: Query<(&ComputedNode, &UiGlobalTransform), With<Focusable>>,
    _ring: Query<(Entity, &mut Node, &mut Visibility, &mut FocusRingState), With<FocusRing>>,
    _units: crate::scale::UiUnits,
) {
    // M0-IMPL: A
}

/// Registers the ring's systems.
pub fn build(app: &mut App) {
    app.add_systems(Startup, spawn_focus_ring).add_systems(
        Update,
        update_focus_ring.in_set(crate::plugin::SlottedUiSet::Render),
    );
}
