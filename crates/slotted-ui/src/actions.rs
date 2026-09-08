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

/// What the player asked the UI to do. Written as a lowercase string in data
/// (`"accept"`), like every other data enum, so it survives the untyped
/// `Value` path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
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
    /// Open the menu (pause, system). On the keyboard it shares `Escape`
    /// with [`Back`](Self::Back): with a screen open the stack claims the
    /// press as `Back` and pops; with nothing open `Menu` is what is left,
    /// and `slotted-menu` pauses on it. `Tab` stays Bevy's tab-navigation
    /// key.
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

impl core::fmt::Display for UiAction {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl Serialize for UiAction {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for UiAction {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        String::deserialize(d)?
            .parse()
            .map_err(serde::de::Error::custom)
    }
}

/// Which device produced an action. A lowercase string in data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InputDevice {
    /// Mouse or touch.
    Pointer,
    /// Keyboard.
    Keyboard,
    /// Any gamepad.
    Gamepad,
}

impl InputDevice {
    /// The data-file spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pointer => "pointer",
            Self::Keyboard => "keyboard",
            Self::Gamepad => "gamepad",
        }
    }
}

impl core::str::FromStr for InputDevice {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, String> {
        match s {
            "pointer" => Ok(Self::Pointer),
            "keyboard" => Ok(Self::Keyboard),
            "gamepad" => Ok(Self::Gamepad),
            other => Err(format!("unknown InputDevice `{other}`")),
        }
    }
}

impl core::fmt::Display for InputDevice {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl Serialize for InputDevice {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for InputDevice {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        String::deserialize(d)?
            .parse()
            .map_err(serde::de::Error::custom)
    }
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
            (UiAction::Menu, vec![K::Escape]),
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

/// What [`track_input_mode`] remembers between frames.
#[derive(Resource, Debug, Default, Clone, Copy, PartialEq)]
pub struct InputModeTracker {
    /// The last `CursorMoved` position sampled, in logical window px.
    pub last_cursor: Option<Vec2>,
    /// Whether a stick was past the deadzone last frame, so a held stick
    /// switches the mode once when it crosses and not every frame after.
    pub stick_over: bool,
}

/// Distance a `CursorMoved` has to cover, in logical px, before it counts as
/// the player reaching for the mouse (menus contract 2.2).
pub const POINTER_MOVE_THRESHOLD: f32 = 2.0;

/// Whether either stick on `gamepad` is past `deadzone`.
fn any_stick_past(gamepad: &Gamepad, deadzone: f32) -> bool {
    gamepad.left_stick().length() > deadzone || gamepad.right_stick().length() > deadzone
}

/// `SlottedUiSet::Input`, before [`UiActionEmit`]: sets [`InputMode`] from the
/// last device that did anything (menus contract 2.2).
///
/// The pointer is read from `CursorMoved`, which the window writes, and from
/// the mouse's `PointerInput` moves and presses, which is all a headless
/// harness or a replay writes. When two devices act in the same frame the
/// pad wins over the keyboard, which wins over the pointer: a player who
/// touches a key or a button wants the ring, and a mouse that jogs at the
/// same time does not take it away.
#[allow(clippy::too_many_arguments)]
pub fn track_input_mode(
    mut cursor: MessageReader<bevy::window::CursorMoved>,
    mut pointer: MessageReader<bevy::picking::pointer::PointerInput>,
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    gamepads: Query<&Gamepad>,
    bindings: Res<UiBindings>,
    mut tracker: ResMut<InputModeTracker>,
    mut mode: ResMut<InputMode>,
    mut changed: MessageWriter<InputModeChanged>,
) {
    let mut wanted = None;

    let mut moved = false;
    for event in cursor.read() {
        let far_enough = tracker
            .last_cursor
            .is_none_or(|last| last.distance(event.position) > POINTER_MOVE_THRESHOLD);
        if far_enough {
            moved = true;
            tracker.last_cursor = Some(event.position);
        }
    }
    for event in pointer.read() {
        if event.pointer_id != bevy::picking::pointer::PointerId::Mouse {
            continue;
        }
        match event.action {
            bevy::picking::pointer::PointerAction::Move { delta }
                if delta.length() > POINTER_MOVE_THRESHOLD =>
            {
                moved = true;
            }
            bevy::picking::pointer::PointerAction::Press(_) => moved = true,
            _ => {}
        }
    }
    if moved || mouse.get_just_pressed().next().is_some() {
        wanted = Some(InputMode::Pointer);
    }

    if keys.get_just_pressed().next().is_some() {
        wanted = Some(InputMode::Keyboard);
    }

    let button = gamepads
        .iter()
        .any(|g| g.get_just_pressed().next().is_some());
    let stick_over = gamepads
        .iter()
        .any(|g| any_stick_past(g, bindings.stick_deadzone));
    let crossed = stick_over && !tracker.stick_over;
    if tracker.stick_over != stick_over {
        tracker.stick_over = stick_over;
    }
    if button || crossed {
        wanted = Some(InputMode::Gamepad);
    }

    if let Some(to) = wanted
        && *mode != to
    {
        let from = *mode;
        *mode = to;
        changed.write(InputModeChanged { from, to });
    }
}

/// What one device says about an action this frame.
#[derive(Default, Clone, Copy)]
struct Press {
    just: bool,
    held: bool,
}

/// One frame's worth of evidence for an action, from every device.
#[derive(Default, Clone, Copy)]
struct Evidence {
    key: Press,
    pad: Press,
}

impl Evidence {
    fn just(self) -> bool {
        self.key.just || self.pad.just
    }

    fn held(self) -> bool {
        self.key.held || self.pad.held
    }

    /// The device to report. A fresh press names the device that made it;
    /// a repeat names whichever is still holding.
    fn device(self, fresh: bool) -> InputDevice {
        let keyboard = if fresh { self.key.just } else { self.key.held };
        if keyboard {
            InputDevice::Keyboard
        } else {
            InputDevice::Gamepad
        }
    }
}

/// The directions the left stick counts as pressed, with hysteresis: a
/// direction engages past `deadzone` and releases at half of it.
fn stick_directions(
    gamepads: &Query<&Gamepad>,
    deadzone: f32,
    held: &BTreeSet<UiAction>,
) -> BTreeSet<UiAction> {
    let mut out = BTreeSet::new();
    let release = deadzone * 0.5;
    for gamepad in gamepads {
        let stick = gamepad.left_stick();
        let axes = [
            (UiAction::Right, stick.x),
            (UiAction::Left, -stick.x),
            (UiAction::Up, stick.y),
            (UiAction::Down, -stick.y),
        ];
        for (action, value) in axes {
            let threshold = if held.contains(&action) {
                release
            } else {
                deadzone
            };
            if value > threshold {
                out.insert(action);
            }
        }
    }
    out
}

/// [`UiActionEmit`]: turns keys, buttons and the left stick into
/// [`UiActionEvent`]s (menus contract 2.1). Clears [`UiActionClaims`] first.
///
/// One event per action per frame at most. A fresh press is `repeat: false`;
/// a held directional action fires again after `repeat_delay` and then every
/// `repeat_every`, on virtual time. Keyboard evidence for anything but `Back`
/// is ignored while a text field owns the keyboard.
#[allow(clippy::too_many_arguments)]
pub fn emit_ui_actions(
    keys: Res<ButtonInput<KeyCode>>,
    gamepads: Query<&Gamepad>,
    bindings: Res<UiBindings>,
    text_entry: Res<crate::nav::TextEntryFocused>,
    time: Res<Time<Virtual>>,
    tokens: crate::tooltip::ThemeTokens,
    mut repeat: ResMut<ActionRepeat>,
    mut claims: ResMut<UiActionClaims>,
    mut events: MessageWriter<UiActionEvent>,
) {
    claims.clear();

    let stick = stick_directions(&gamepads, bindings.stick_deadzone, &repeat.stick_held);
    let stick_just: BTreeSet<UiAction> = stick.difference(&repeat.stick_held).copied().collect();

    let mut evidence: BTreeMap<UiAction, Evidence> = BTreeMap::new();
    for action in UiAction::ALL {
        let mut e = Evidence::default();
        let keyboard_allowed = !text_entry.0 || action == UiAction::Back;
        if keyboard_allowed && let Some(codes) = bindings.keys.get(&action) {
            e.key.just = codes.iter().any(|k| keys.just_pressed(*k));
            e.key.held = codes.iter().any(|k| keys.pressed(*k));
        }
        if let Some(buttons) = bindings.buttons.get(&action) {
            e.pad.just = gamepads
                .iter()
                .any(|g| buttons.iter().any(|b| g.just_pressed(*b)));
            e.pad.held = gamepads
                .iter()
                .any(|g| buttons.iter().any(|b| g.pressed(*b)));
        }
        if action.is_directional() {
            e.pad.just |= stick_just.contains(&action);
            e.pad.held |= stick.contains(&action);
        }
        evidence.insert(action, e);
    }
    repeat.stick_held = stick;

    let durations = tokens.get().durations;
    let delay = bindings
        .repeat_delay
        .unwrap_or_else(|| Duration::from_millis(u64::from(durations.hover_delay)))
        .as_secs_f64();
    let every = bindings
        .repeat_every
        .unwrap_or_else(|| Duration::from_millis(u64::from(durations.fast)))
        .as_secs_f64();
    let now = time.elapsed_secs_f64();

    for (action, e) in evidence {
        if e.just() {
            events.write(UiActionEvent {
                action,
                device: e.device(true),
                repeat: false,
            });
            if action.is_directional() {
                repeat.next_fire.insert(action, now + delay);
            }
            continue;
        }
        if !action.is_directional() {
            continue;
        }
        if !e.held() {
            repeat.next_fire.remove(&action);
            continue;
        }
        let Some(next) = repeat.next_fire.get(&action).copied() else {
            // Held since before we started counting (a key held across a
            // text-entry focus change): treat this frame as the press.
            repeat.next_fire.insert(action, now + delay);
            continue;
        };
        if now + f64::EPSILON >= next {
            events.write(UiActionEvent {
                action,
                device: e.device(false),
                repeat: true,
            });
            // Keep the cadence anchored to the schedule, not to the frame it
            // was noticed on, so a slow frame does not drift the rhythm.
            let mut following = next + every;
            if following <= now {
                following = now + every;
            }
            repeat.next_fire.insert(action, following);
        }
    }
}

/// Registers the resources, messages and systems of this module.
pub fn build(app: &mut App) {
    app.init_resource::<UiBindings>()
        .init_resource::<InputMode>()
        .init_resource::<UiActionClaims>()
        .init_resource::<ActionRepeat>()
        .init_resource::<InputModeTracker>()
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
