//! Confirm dialogs (menus M2 contract 3.3).

use std::sync::Arc;

use bevy::prelude::*;
use slotted_ui::{LocArgs, LocKey, PushScreen, ScreenDef, Screens, UiNodeDef};

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
    #[must_use]
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

    /// The accept button drawn as destructive.
    #[must_use]
    pub fn danger(mut self) -> Self {
        self.danger = true;
        self
    }

    /// The title.
    #[must_use]
    pub fn title(mut self, key: impl Into<String>) -> Self {
        self.title = LocKey(key.into());
        self
    }

    /// The button labels.
    #[must_use]
    pub fn buttons(mut self, accept: impl Into<String>, cancel: impl Into<String>) -> Self {
        self.accept = LocKey(accept.into());
        self.cancel = LocKey(cancel.into());
        self
    }

    /// One message argument.
    #[must_use]
    pub fn arg(mut self, name: impl Into<String>, value: impl Into<slotted_ui::Value>) -> Self {
        self.args.insert(name.into(), value.into());
        self
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

/// Rewrites a clone of the registered `slotted:confirm` template for
/// `spec`: the title, the message with its arguments, the button labels,
/// and the accept button's variant (`danger` when asked). A game's own
/// `slotted:confirm` needs only the `title`, `message`, `accept` and
/// `cancel` ids for all of this to reach it.
pub fn rewritten(mut def: ScreenDef, spec: &ConfirmSpec) -> ScreenDef {
    def.set_text("title", spec.title.clone(), LocArgs::new());
    def.set_text("message", spec.message.clone(), spec.args.clone());
    set_button_label(&mut def, "cancel", &spec.cancel);
    set_button_label(&mut def, "accept", &spec.accept);
    if spec.danger
        && let Some(UiNodeDef::Button { opts, .. }) = def.root.find_mut("accept")
    {
        opts.variant = slotted_ui::ButtonVariant::Danger;
    }
    def
}

/// Rewrites the label of the node with id `id`: `ScreenDef::set_text`, which
/// reaches a `button` since menus M3 (contract 2.6). `false` when no such
/// node.
pub fn set_button_label(def: &mut ScreenDef, id: &str, label: &LocKey) -> bool {
    def.set_text(id, label.clone(), slotted_ui::LocArgs::default())
}

/// Pushes a confirm dialog: the registered `slotted:confirm` def, rewritten
/// for `spec`, with [`PendingConfirm`] on its root. A confirm pushed over a
/// confirm stacks. Nothing happens, with a warning, when no confirm screen
/// is registered.
pub fn confirm(commands: &mut Commands, spec: ConfirmSpec) {
    commands.queue(move |world: &mut World| {
        let kind = crate::kinds::confirm();
        let Some(def) = world
            .get_resource::<Screens>()
            .and_then(|screens| crate::templates::cloned(screens, &kind))
        else {
            tracing::warn!(id = %spec.id, "confirm: no `slotted:confirm` screen is registered");
            return;
        };
        let def = rewritten(def, &spec);
        let root = world.spawn_empty().id();
        PushScreen {
            root,
            def: Arc::new(def),
            menu: None,
        }
        .apply(world);
        world.entity_mut(root).insert(PendingConfirm(spec.id));
    });
}

/// Observer on `ScreenClosed`: a dialog closed by `Back` answers `false`
/// unless a button already answered (which removed the [`PendingConfirm`]).
pub fn on_confirm_closed(
    closed: On<slotted_ui::ScreenClosed>,
    pending: Query<&PendingConfirm>,
    mut results: MessageWriter<ConfirmResult>,
) {
    if let Ok(pending) = pending.get(closed.entity) {
        results.write(ConfirmResult {
            id: pending.0.clone(),
            accepted: false,
        });
    }
}

/// Registers the confirm systems.
pub fn build(app: &mut App) {
    app.add_observer(on_confirm_closed);
}
