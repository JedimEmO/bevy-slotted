//! Keyboard directional navigation. Bevy ships `AutoDirectionalNavigator` as
//! a `SystemParam` and wires no keys to it (ADR 0002); this is the system
//! the spike proved.

use bevy::input::ButtonInput;
use bevy::math::CompassOctant;
use bevy::prelude::*;
use bevy::ui::auto_directional_navigation::AutoDirectionalNavigator;

/// Which keys move focus. Defaults to the arrow keys.
#[derive(Resource, Debug, Clone, PartialEq, Eq)]
pub struct NavKeys {
    /// Move focus up.
    pub up: Vec<KeyCode>,
    /// Move focus down.
    pub down: Vec<KeyCode>,
    /// Move focus left.
    pub left: Vec<KeyCode>,
    /// Move focus right.
    pub right: Vec<KeyCode>,
}

impl Default for NavKeys {
    fn default() -> Self {
        Self {
            up: vec![KeyCode::ArrowUp],
            down: vec![KeyCode::ArrowDown],
            left: vec![KeyCode::ArrowLeft],
            right: vec![KeyCode::ArrowRight],
        }
    }
}

/// `SlottedUiSet::Input`: arrow keys drive `AutoDirectionalNavigator`.
/// Gamepad d-pad joins in Phase 6.
pub fn directional_nav_keys(
    keys: Res<ButtonInput<KeyCode>>,
    nav_keys: Res<NavKeys>,
    mut nav: AutoDirectionalNavigator,
) {
    let pressed = |set: &[KeyCode]| set.iter().any(|k| keys.just_pressed(*k));
    let dir = if pressed(&nav_keys.right) {
        CompassOctant::East
    } else if pressed(&nav_keys.left) {
        CompassOctant::West
    } else if pressed(&nav_keys.up) {
        CompassOctant::North
    } else if pressed(&nav_keys.down) {
        CompassOctant::South
    } else {
        return;
    };
    if let Err(e) = nav.navigate(dir) {
        tracing::trace!(?e, "directional navigation found no target");
    }
}
