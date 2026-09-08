//! Confirm dialogs (menus M2 contract 3.3).

use bevy::prelude::*;
use slotted_ui::{LocArgs, LocKey};

/// What a confirm dialog asks.
#[derive(Debug, Clone, PartialEq)]
pub struct ConfirmSpec {
    /// Comes back in the [`ConfirmResult`].
    pub id: String,
    /// The title.
    pub title: LocKey,
    /// The message, rich text, with `args`.
    pub message: LocKey,
    /// Arguments for the message.
    pub args: LocArgs,
    /// The accept button's label.
    pub accept: LocKey,
    /// The cancel button's label.
    pub cancel: LocKey,
    /// Draw the accept button as destructive.
    pub danger: bool,
}

impl ConfirmSpec {
    /// A dialog with the default title and button labels.
    pub fn new(id: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            title: LocKey("slotted.menu.confirm_title".to_owned()),
            message: LocKey(message.into()),
            args: LocArgs::new(),
            accept: LocKey("slotted.menu.ok".to_owned()),
            cancel: LocKey("slotted.menu.cancel".to_owned()),
            danger: false,
        }
    }
}

/// On a confirm dialog's root: which spec it is answering.
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub struct PendingConfirm(pub String);

/// The answer, exactly once per dialog.
#[derive(Message, Debug, Clone, PartialEq, Eq)]
pub struct ConfirmResult {
    /// The spec's id.
    pub id: String,
    /// Accept, or cancel / `Back`.
    pub accepted: bool,
}

/// Pushes a confirm dialog.
pub fn confirm(commands: &mut Commands, spec: ConfirmSpec) {
    // M2-IMPL: B
    let _ = (commands, spec);
}

/// Observer on `ScreenClosed`: a dialog closed by `Back` answers `false`
/// unless a button already answered.
pub fn on_confirm_closed(
    _closed: On<slotted_ui::ScreenClosed>,
    _pending: Query<&PendingConfirm>,
    _results: MessageWriter<ConfirmResult>,
) {
    // M2-IMPL: B
}

/// Registers the confirm systems.
pub fn build(app: &mut App) {
    app.add_observer(on_confirm_closed);
}
