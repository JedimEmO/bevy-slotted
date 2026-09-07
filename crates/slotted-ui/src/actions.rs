//! The one UI action vocabulary (menus contract, sections 1.1, 2.1, 2.2).
//!
//! Keyboard, mouse buttons and gamepads all map onto [`UiAction`] through
//! [`UiBindings`]; every navigation decision in the workspace reads
//! [`UiActionEvent`] rather than a raw key. [`InputMode`] remembers the last
//! device the player touched, and exactly two things branch on it: the focus
//! ring and, later, the hint bar.

use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

use bevy::input::gamepad::GamepadButton;
use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// What the player asked the UI to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UiAction {
    /// Activate the focused widget.
    Accept,
    /// Cancel, close, go back.
    Back,
    /// The widget's secondary action (history, details).
    Secondary,
    /// Move focus up.
    Up,
    /// Move focus down.
    Down,
    /// Move focus left.
    Left,
    /// Move focus right.
    Right,
    /// Previous tab.
    TabPrev,
    /// Next tab.
    TabNext,
    /// Previous page of a list.
    PagePrev,
    /// Next page of a list.
    PageNext,
    /// Open the menu (pause, system).
    Menu,
}

impl UiAction {
    /// Every action, in declaration order.
    pub const ALL: [Self; 12] = [
        Self::Accept,
        Self::Back,
        Self::Secondary,
        Self::Up,
        Self::Down,
        Self::Left,
        Self::Right,
        Self::TabPrev,
        Self::TabNext,
        Self::PagePrev,
        Self::PageNext,
        Self::Menu,
    ];

    /// The four that repeat while held.
    pub const fn is_directional(self) -> bool {
        matches!(self, Self::Up | Self::Down | Self::Left | Self::Right)
    }

    /// The data-file spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Accept => "accept",
            Self::Back => "back",
            Self::Secondary => "secondary",
            Self::Up => "up",
            Self::Down => "down",
            Self::Left => "left",
            Self::Right => "right",
            Self::TabPrev => "tab_prev",
            Self::TabNext => "tab_next",
            Self::PagePrev => "page_prev",
            Self::PageNext => "page_next",
            Self::Menu => "menu",
        }
    }
}

impl core::str::FromStr for UiAction {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, String> {
        Self::ALL
            .into_iter()
            .find(|a| a.as_str() == s)
            .ok_or_else(|| format!("unknown UiAction `{s}`"))
    }
}

/// Which device produced an action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InputDevice {
    /// Mouse or touch.
    Pointer,
    /// Keyboard.
    Keyboard,
    /// Any gamepad.
    Gamepad,
}

/// One action this frame. Written by [`emit_ui_actions`], at most once per
/// action per frame.
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct UiActionEvent {
    /// The action.
    pub action: UiAction,
    /// Who pressed it.
    pub device: InputDevice,
    /// `true` for a held-key repeat rather than a fresh press.
    pub repeat: bool,
}

/// Which keys and buttons mean which action.
#[derive(Resource, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UiBindings {
    /// Keyboard bindings.
    pub keys: BTreeMap<UiAction, Vec<KeyCode>>,
    /// Gamepad button bindings.
    pub buttons: BTreeMap<UiAction, Vec<GamepadButton>>,
    /// Left-stick magnitude that counts as a press; release at half of it.
    pub stick_deadzone: f32,
    /// Delay before a held directional action repeats. `None` reads the
    /// theme's `durations.hover_delay`.
    pub repeat_delay: Option<Duration>,
    /// Interval between repeats. `None` reads the theme's `durations.fast`.
    pub repeat_every: Option<Duration>,
}

impl Default for UiBindings {
    fn default() -> Self {
        use GamepadButton as G;
        use KeyCode as K;
        let keys: BTreeMap<UiAction, Vec<KeyCode>> = [
            (UiAction::Accept, vec![K::Enter, K::Space]),
            (UiAction::Back, vec![K::Escape]),
            (UiAction::Secondary, vec![K::KeyX]),
            (UiAction::Up, vec![K::ArrowUp, K::KeyW]),
            (UiAction::Down, vec![K::ArrowDown, K::KeyS]),
            (UiAction::Left, vec![K::ArrowLeft, K::KeyA]),
            (UiAction::Right, vec![K::ArrowRight, K::KeyD]),
            (UiAction::TabPrev, vec![K::KeyQ]),
            (UiAction::TabNext, vec![K::KeyE]),
            (UiAction::PagePrev, vec![K::PageUp]),
            (UiAction::PageNext, vec![K::PageDown]),
            (UiAction::Menu, vec![K::Tab]),
        ]
        .into_iter()
        .collect();
        let buttons: BTreeMap<UiAction, Vec<GamepadButton>> = [
            (UiAction::Accept, vec![G::South]),
            (UiAction::Back, vec![G::East]),
            (UiAction::Secondary, vec![G::West]),
            (UiAction::Up, vec![G::DPadUp]),
            (UiAction::Down, vec![G::DPadDown]),
            (UiAction::Left, vec![G::DPadLeft]),
            (UiAction::Right, vec![G::DPadRight]),
            (UiAction::TabPrev, vec![G::LeftTrigger]),
            (UiAction::TabNext, vec![G::RightTrigger]),
            (UiAction::PagePrev, vec![G::LeftTrigger2]),
            (UiAction::PageNext, vec![G::RightTrigger2]),
            (UiAction::Menu, vec![G::Start]),
        ]
        .into_iter()
        .collect();
        Self {
            keys,
            buttons,
            stick_deadzone: 0.5,
            repeat_delay: None,
            repeat_every: None,
        }
    }
}

impl UiBindings {
    /// The first keyboard key bound to `action`, for the harness.
    pub fn first_key(&self, action: UiAction) -> Option<KeyCode> {
        self.keys.get(&action).and_then(|k| k.first().copied())
    }

    /// The first gamepad button bound to `action`.
    pub fn first_button(&self, action: UiAction) -> Option<GamepadButton> {
        self.buttons.get(&action).and_then(|b| b.first().copied())
    }
}

/// The last device the player used. Only the focus ring and the hint bar
/// branch on it.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InputMode {
    /// Mouse: the ring hides until a key or button is pressed.
    #[default]
    Pointer,
    /// Keyboard: the ring shows.
    Keyboard,
    /// Gamepad: the ring shows.
    Gamepad,
}

/// [`InputMode`] changed this frame.
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct InputModeChanged {
    /// Before.
    pub from: InputMode,
    /// After.
    pub to: InputMode,
}

/// Which actions a consumer has already acted on this frame. The stack's
/// `pop_on_back` pops only an unclaimed `Back`. Cleared by
/// [`emit_ui_actions`] at the top of every frame.
#[derive(Resource, Debug, Clone, Default, PartialEq, Eq)]
pub struct UiActionClaims(BTreeSet<UiAction>);

impl UiActionClaims {
    /// Marks `action` as handled for this frame.
    pub fn claim(&mut self, action: UiAction) {
        self.0.insert(action);
    }

    /// Whether something already handled `action` this frame.
    pub fn is_claimed(&self, action: UiAction) -> bool {
        self.0.contains(&action)
    }

    /// Forgets every claim.
    pub fn clear(&mut self) {
        self.0.clear();
    }
}

/// The system set [`emit_ui_actions`] runs in, first inside
/// `SlottedUiSet::Input`. Consumers order `.after(UiActionEmit)`.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct UiActionEmit;

/// Per-action repeat bookkeeping, on virtual time.
#[derive(Resource, Debug, Default, Clone)]
pub struct ActionRepeat {
    /// When each held directional action fires next, in virtual seconds.
    pub next_fire: BTreeMap<UiAction, f64>,
    /// Stick directions currently counted as pressed.
    pub stick_held: BTreeSet<UiAction>,
}

/// `SlottedUiSet::Input`, before [`UiActionEmit`]: sets [`InputMode`] from the
/// last device that did anything (menus contract 2.2).
pub fn track_input_mode(
    _cursor: MessageReader<bevy::window::CursorMoved>,
    _mouse: Res<ButtonInput<MouseButton>>,
    _keys: Res<ButtonInput<KeyCode>>,
    _gamepads: Query<&Gamepad>,
    _bindings: Res<UiBindings>,
    _mode: ResMut<InputMode>,
    _changed: MessageWriter<InputModeChanged>,
) {
    // M0-IMPL: A
}

/// [`UiActionEmit`]: turns keys, buttons and the left stick into
/// [`UiActionEvent`]s (menus contract 2.1). Clears [`UiActionClaims`] first.
#[allow(clippy::too_many_arguments)]
pub fn emit_ui_actions(
    _keys: Res<ButtonInput<KeyCode>>,
    _gamepads: Query<&Gamepad>,
    _bindings: Res<UiBindings>,
    _text_entry: Res<crate::nav::TextEntryFocused>,
    _time: Res<Time<Virtual>>,
    _repeat: ResMut<ActionRepeat>,
    mut claims: ResMut<UiActionClaims>,
    _events: MessageWriter<UiActionEvent>,
) {
    claims.clear();
    // M0-IMPL: A
}

/// Registers the resources, messages and systems of this module.
pub fn build(app: &mut App) {
    app.init_resource::<UiBindings>()
        .init_resource::<InputMode>()
        .init_resource::<UiActionClaims>()
        .init_resource::<ActionRepeat>()
        .add_message::<UiActionEvent>()
        .add_message::<InputModeChanged>()
        .configure_sets(
            Update,
            UiActionEmit.in_set(crate::plugin::SlottedUiSet::Input),
        )
        .add_systems(
            Update,
            (
                track_input_mode
                    .before(UiActionEmit)
                    .in_set(crate::plugin::SlottedUiSet::Input),
                emit_ui_actions.in_set(UiActionEmit),
            ),
        );
}
