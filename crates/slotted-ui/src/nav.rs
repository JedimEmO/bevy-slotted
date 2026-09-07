//! Directional focus navigation over `UiActionEvent`s. Bevy ships
//! `AutoDirectionalNavigator` as a `SystemParam` and wires no input to it
//! (ADR 0002); this is what drives it, plus the explicit nav graph.

use bevy::input_focus::InputFocus;
use bevy::prelude::*;
use bevy::text::EditableText;
use bevy::ui::auto_directional_navigation::AutoDirectionalNavigator;

use crate::semantic::SemanticRole;

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

/// `SlottedUiSet::Input`, after `UiActionEmit`: directional actions drive
/// focus (menus contract 2.4).
///
/// An explicit `NavLinks` link on the focused node or one of its ancestors
/// wins; otherwise Bevy's `AutoDirectionalNavigator` picks the target.
/// Keyboard-sourced actions are already suppressed while a text field has
/// the keyboard (`emit_ui_actions`), so this needs no guard of its own.
#[allow(clippy::too_many_arguments)]
pub fn directional_nav_actions(
    _events: MessageReader<crate::actions::UiActionEvent>,
    _links: Query<&crate::def::NavLinks>,
    _parents: Query<&ChildOf>,
    _ids: Query<(Entity, &crate::semantic::TestId)>,
    _focusables: Query<(), With<crate::focus_ring::Focusable>>,
    _children: Query<&Children>,
    _roots: Query<(), With<crate::semantic::ScreenRoot>>,
    _nav: AutoDirectionalNavigator,
) {
    // M0-IMPL: A
}

/// Observer on `ScreenSpawned`: gives the new screen its initial focus
/// (menus contract 2.4).
#[allow(clippy::too_many_arguments)]
pub fn focus_on_spawn(
    _spawned: On<crate::screen::ScreenSpawned>,
    _hints: Query<(
        &crate::semantic::ScreenRoot,
        &crate::semantic::ScreenFocusHint,
    )>,
    _ids: Query<(Entity, &crate::semantic::TestId)>,
    _focusables: Query<(), With<crate::focus_ring::Focusable>>,
    _children: Query<&Children>,
    _stack: Res<crate::stack::ScreenStack>,
    _focus: Option<ResMut<InputFocus>>,
    _commands: Commands,
) {
    // M0-IMPL: A
}
