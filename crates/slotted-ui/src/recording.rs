//! Input recording: the real input streams with frame numbers, as RON, so a
//! session becomes a `UiHarness::replay` test. Phase 6 contract section 2.3.
//! The types are always compiled; the recorder needs the `dev` feature.

use bevy::input::gamepad::{GamepadAxis, GamepadButton};
use bevy::input::keyboard::{Key, KeyCode};
use bevy::picking::pointer::PointerButton;
use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// File format version.
pub const RECORDING_VERSION: u32 = 1;

/// A pointer button, spelled out because `bevy_picking`'s enum has no serde.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RecordedButton {
    /// Left.
    Primary,
    /// Right.
    Secondary,
    /// Middle.
    Middle,
}

impl From<PointerButton> for RecordedButton {
    fn from(b: PointerButton) -> Self {
        match b {
            PointerButton::Primary => Self::Primary,
            PointerButton::Secondary => Self::Secondary,
            PointerButton::Middle => Self::Middle,
        }
    }
}

impl From<RecordedButton> for PointerButton {
    fn from(b: RecordedButton) -> Self {
        match b {
            RecordedButton::Primary => Self::Primary,
            RecordedButton::Secondary => Self::Secondary,
            RecordedButton::Middle => Self::Middle,
        }
    }
}

/// One recorded input.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum RecordedInput {
    /// Pointer moved to logical window coordinates.
    PointerMove {
        /// Position.
        pos: Vec2,
    },
    /// Pointer button pressed.
    PointerPress(RecordedButton),
    /// Pointer button released.
    PointerRelease(RecordedButton),
    /// Wheel.
    Scroll {
        /// Delta in lines.
        delta: Vec2,
    },
    /// Key state change.
    Key {
        /// Physical key.
        key_code: KeyCode,
        /// Logical key.
        logical: Key,
        /// Pressed or released.
        pressed: bool,
    },
    /// Gamepad button state change.
    GamepadButton {
        /// Which.
        button: GamepadButton,
        /// Pressed or released.
        pressed: bool,
    },
    /// Gamepad axis value.
    GamepadAxis {
        /// Which.
        axis: GamepadAxis,
        /// Value in -1..=1.
        value: f32,
    },
}

/// The inputs of one frame.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct RecordedFrame {
    /// Frame number since recording started.
    pub frame: u64,
    /// Inputs in arrival order.
    pub inputs: Vec<RecordedInput>,
}

/// A whole session.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Recording {
    /// [`RECORDING_VERSION`].
    pub version: u32,
    /// Logical window size the session ran at.
    pub resolution: Vec2,
    /// Window scale factor.
    pub scale_factor: f32,
    /// Frame delta the replay should use, in microseconds.
    pub frame_delta_us: u64,
    /// Frames with at least one input.
    pub frames: Vec<RecordedFrame>,
}

impl Recording {
    /// Parses RON.
    pub fn from_ron(text: &str) -> Result<Self, ron::error::SpannedError> {
        ron::from_str(text)
    }

    /// Serialises to pretty RON.
    pub fn to_ron(&self) -> Result<String, ron::Error> {
        ron::ser::to_string_pretty(self, ron::ser::PrettyConfig::default())
    }
}

/// The live recorder. Insert it (with a path) to record; flushed on `AppExit`.
#[cfg(feature = "dev")]
#[derive(Resource, Debug, Clone)]
pub struct InputRecorder {
    /// Where the RON goes.
    pub path: std::path::PathBuf,
    /// What has been captured so far.
    pub recording: Recording,
    /// Frames since insertion.
    pub frame: u64,
}

#[cfg(feature = "dev")]
impl InputRecorder {
    /// A recorder writing to `path`, for a window of `resolution` logical
    /// pixels at `scale_factor`, stepping `frame_delta`.
    ///
    /// The resolution is stored so a replay can say when it is being fed into
    /// a differently sized window, where a recorded click lands somewhere
    /// else than it did.
    pub fn new(
        path: impl Into<std::path::PathBuf>,
        resolution: Vec2,
        scale_factor: f32,
        frame_delta: std::time::Duration,
    ) -> Self {
        Self {
            path: path.into(),
            recording: Recording {
                version: RECORDING_VERSION,
                resolution,
                scale_factor,
                frame_delta_us: u64::try_from(frame_delta.as_micros()).unwrap_or(16_667),
                frames: Vec::new(),
            },
            frame: 0,
        }
    }
}

/// `First`: mirrors `PointerInput` (mouse pointer), `KeyboardInput` and
/// `GamepadEvent` into the current frame.
#[cfg(feature = "dev")]
pub fn record_inputs(
    recorder: Option<ResMut<InputRecorder>>,
    mut pointer: MessageReader<bevy::picking::pointer::PointerInput>,
    mut keys: MessageReader<bevy::input::keyboard::KeyboardInput>,
    mut gamepad: MessageReader<bevy::input::gamepad::GamepadEvent>,
) {
    use bevy::picking::pointer::{PointerAction, PointerId};
    let Some(mut recorder) = recorder else {
        // Leave the readers where they are: without a recorder there is
        // nothing to catch up on when one is inserted mid-session either.
        return;
    };
    let mut inputs = Vec::new();
    for input in pointer.read() {
        if input.pointer_id != PointerId::Mouse {
            continue;
        }
        match input.action {
            PointerAction::Move { .. } => inputs.push(RecordedInput::PointerMove {
                pos: input.location.position,
            }),
            PointerAction::Press(button) => {
                inputs.push(RecordedInput::PointerPress(button.into()));
            }
            PointerAction::Release(button) => {
                inputs.push(RecordedInput::PointerRelease(button.into()));
            }
            PointerAction::Scroll { x, y, .. } => {
                inputs.push(RecordedInput::Scroll {
                    delta: Vec2::new(x, y),
                });
            }
            PointerAction::Cancel => {}
        }
    }
    for key in keys.read() {
        if key.repeat {
            continue;
        }
        inputs.push(RecordedInput::Key {
            key_code: key.key_code,
            logical: key.logical_key.clone(),
            pressed: key.state.is_pressed(),
        });
    }
    for event in gamepad.read() {
        match event {
            bevy::input::gamepad::GamepadEvent::Button(button) => {
                inputs.push(RecordedInput::GamepadButton {
                    button: button.button,
                    pressed: button.state.is_pressed(),
                });
            }
            bevy::input::gamepad::GamepadEvent::Axis(axis) => {
                inputs.push(RecordedInput::GamepadAxis {
                    axis: axis.axis,
                    value: axis.value,
                });
            }
            bevy::input::gamepad::GamepadEvent::Connection(_) => {}
        }
    }
    let frame = recorder.frame;
    if !inputs.is_empty() {
        recorder
            .recording
            .frames
            .push(RecordedFrame { frame, inputs });
    }
    recorder.frame += 1;
}

/// On `AppExit`: writes the recording.
#[cfg(feature = "dev")]
pub fn flush_recording(recorder: Option<Res<InputRecorder>>, mut exit: MessageReader<AppExit>) {
    if exit.read().next().is_none() {
        return;
    }
    let Some(recorder) = recorder else {
        return;
    };
    write_recording(&recorder.path, &recorder.recording);
}

/// Writes one recording as RON, logging what went wrong instead of failing:
/// losing a recording must not take the session with it.
#[cfg(feature = "dev")]
pub fn write_recording(path: &std::path::Path, recording: &Recording) {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let text = match recording.to_ron() {
            Ok(text) => text,
            Err(e) => {
                tracing::warn!(%e, "serialising the recording");
                return;
            }
        };
        if let Some(dir) = path.parent()
            && !dir.as_os_str().is_empty()
            && let Err(e) = std::fs::create_dir_all(dir)
        {
            tracing::warn!(path = %dir.display(), %e, "creating the recording directory");
            return;
        }
        match std::fs::write(path, text) {
            Ok(()) => tracing::info!(
                path = %path.display(),
                frames = recording.frames.len(),
                "recording written"
            ),
            Err(e) => tracing::warn!(path = %path.display(), %e, "writing the recording"),
        }
    }
    #[cfg(target_arch = "wasm32")]
    let _ = (path, recording);
}
