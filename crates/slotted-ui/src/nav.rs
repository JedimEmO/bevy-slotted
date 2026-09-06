//! Keyboard directional navigation. Bevy ships `AutoDirectionalNavigator` as
//! a `SystemParam` and wires no keys to it (ADR 0002); this is the system
//! the spike proved.

use bevy::input::ButtonInput;
use bevy::input_focus::InputFocus;
use bevy::math::CompassOctant;
use bevy::prelude::*;
use bevy::text::EditableText;
use bevy::ui::auto_directional_navigation::AutoDirectionalNavigator;

use crate::semantic::SemanticRole;

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

/// Whether a text-entry node owns the keyboard, as of the last
/// [`track_text_entry_focus`] run.
///
/// Every keyboard system in the crate asks this first. A focused search field
/// has to see its own arrow keys and its own digits, so a UI-level shortcut
/// must not act on a key the field is about to consume: without this, typing
/// "3" into the browser's search box also swaps a hovered slot with hotbar
/// slot 3, and the arrow keys walk the focus out of the field.
///
/// It is a resource rather than a `SystemParam` reading `InputFocus` because
/// `AutoDirectionalNavigator` already takes `ResMut<InputFocus>`, and a
/// system cannot hold both.
#[derive(Resource, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct TextEntryFocused(pub bool);

/// `SlottedUiSet::Input`, before every keyboard system: is the focused entity
/// a text field?
///
/// A node counts as text entry if it is a [`SemanticRole::TextField`] or
/// carries Bevy's [`EditableText`], so a game's own field is covered whether
/// it uses this crate's vocabulary or Bevy's widget.
pub fn track_text_entry_focus(
    focus: Option<Res<InputFocus>>,
    fields: Query<(Option<&SemanticRole>, Option<&EditableText>)>,
    mut focused: ResMut<TextEntryFocused>,
) {
    let active = focus
        .and_then(|f| f.get())
        .and_then(|e| fields.get(e).ok())
        .is_some_and(|(role, editable)| {
            editable.is_some() || role == Some(&SemanticRole::TextField)
        });
    if focused.0 != active {
        focused.0 = active;
    }
}

/// `SlottedUiSet::Input`: arrow keys drive `AutoDirectionalNavigator`.
/// Gamepad d-pad joins in Phase 6.
///
/// Inert while a text field has the keyboard: the arrow keys move the caret,
/// not the focus.
pub fn directional_nav_keys(
    keys: Res<ButtonInput<KeyCode>>,
    nav_keys: Res<NavKeys>,
    text_entry: Res<TextEntryFocused>,
    mut nav: AutoDirectionalNavigator,
) {
    if text_entry.0 {
        return;
    }
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
