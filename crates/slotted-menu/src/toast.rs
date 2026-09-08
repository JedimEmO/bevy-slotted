//! Toasts (menus M2 contract 3.4): transient notices in their own z band,
//! outside the screen stack.

use std::time::Duration;

use bevy::prelude::*;
use slotted_ui::{LocArgs, LocKey};

/// How loud a toast is; picks its role.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ToastLevel {
    /// `toast.info`.
    #[default]
    Info,
    /// `toast.success`.
    Success,
    /// `toast.warning`.
    Warning,
    /// `toast.error`.
    Error,
}

impl ToastLevel {
    /// The theme role.
    pub fn role(self) -> slotted_theme::Role {
        use slotted_theme::roles;
        match self {
            Self::Info => roles::TOAST_INFO,
            Self::Success => roles::TOAST_SUCCESS,
            Self::Warning => roles::TOAST_WARNING,
            Self::Error => roles::TOAST_ERROR,
        }
    }
}

/// What a toast says.
#[derive(Debug, Clone, PartialEq)]
pub struct ToastSpec {
    /// The text, rich, with `args`.
    pub key: LocKey,
    /// Arguments.
    pub args: LocArgs,
    /// The level.
    pub level: ToastLevel,
    /// How long it stays; `None` = ten `durations.slow`.
    pub duration: Option<Duration>,
}

impl ToastSpec {
    /// An info toast.
    pub fn new(key: impl Into<String>) -> Self {
        Self {
            key: LocKey(key.into()),
            args: LocArgs::new(),
            level: ToastLevel::Info,
            duration: None,
        }
    }
}

/// Toast policy.
#[derive(Resource, Debug, Clone, PartialEq, Eq)]
pub struct Toasts {
    /// How many show at once; the rest queue.
    pub max_visible: usize,
}

impl Default for Toasts {
    fn default() -> Self {
        Self { max_visible: 3 }
    }
}

/// One toast on screen.
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub struct Toast {
    /// Its level.
    pub level: ToastLevel,
    /// Virtual time left before it fades.
    pub remaining: Duration,
}

/// The full-window host the toasts stack in.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ToastHost;

/// A toast waiting for room.
#[derive(Resource, Debug, Default, Clone, PartialEq)]
pub struct ToastQueue(pub std::collections::VecDeque<ToastSpec>);

/// Shows a toast, or queues it.
pub fn toast(commands: &mut Commands, spec: ToastSpec) {
    // M2-IMPL: B
    let _ = (commands, spec);
}

/// `SlottedUiSet::Render`: counts down, fades out, despawns, and dequeues.
pub fn tick_toasts(
    _time: Res<Time<Virtual>>,
    _policy: Res<Toasts>,
    _queue: ResMut<ToastQueue>,
    _toasts: Query<(Entity, &mut Toast)>,
    _commands: Commands,
) {
    // M2-IMPL: B
}

/// Registers the toast systems.
pub fn build(app: &mut App) {
    app.init_resource::<ToastQueue>()
        .add_systems(Update, tick_toasts.in_set(slotted_ui::SlottedUiSet::Render));
}
