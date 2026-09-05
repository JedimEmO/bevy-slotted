//! Hotkeys and the keyboard-focus guard. Package B.

use bevy::input_focus::InputFocus;
use bevy::prelude::*;
use bevy::text::EditableText;

use crate::events::{BookmarkToggled, OpenRecipes, OpenUses, RecipeNav};
use crate::runtime::BrowserRuntime;

/// `BrowserSet::Input`: `has_keyboard_focus` is true while `InputFocus` is on
/// an entity with `EditableText`. Runs before every hotkey system.
pub fn track_keyboard_focus(
    focus: Option<Res<InputFocus>>,
    fields: Query<(), With<EditableText>>,
    mut runtime: ResMut<BrowserRuntime>,
) {
    let focused = focus
        .and_then(|f| f.get())
        .is_some_and(|e| fields.contains(e));
    if runtime.has_keyboard_focus != focused {
        runtime.has_keyboard_focus = focused;
    }
}

/// `BrowserSet::Input`: R, U, A, Ctrl+F, Backspace, Esc, PgUp/PgDn from
/// `BrowserRuntime::keys`, guarded by `has_keyboard_focus`.
pub fn browser_hotkeys(
    _keys: Res<ButtonInput<KeyCode>>,
    _runtime: Res<BrowserRuntime>,
    _recipes: MessageWriter<OpenRecipes>,
    _uses: MessageWriter<OpenUses>,
    _bookmarks: MessageWriter<BookmarkToggled>,
    _nav: MessageWriter<RecipeNav>,
) {
    // PHASE3-IMPL: B — resolve the hovered ingredient (Card, RecipeSlot,
    // SlotRef, then `handler.stack_under_cursor`).
}
