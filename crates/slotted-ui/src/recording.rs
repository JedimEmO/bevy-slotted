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

/// `First`: mirrors `PointerInput` (mouse pointer), `KeyboardInput` and
/// `GamepadEvent` into the current frame.
#[cfg(feature = "dev")]
pub fn record_inputs(
    recorder: Option<ResMut<InputRecorder>>,
    pointer: MessageReader<bevy::picking::pointer::PointerInput>,
    keys: MessageReader<bevy::input::keyboard::KeyboardInput>,
    gamepad: MessageReader<bevy::input::gamepad::GamepadEvent>,
) {
    // PHASE6-IMPL: B.
    let _ = (recorder, pointer, keys, gamepad);
}

/// On `AppExit`: writes the recording.
#[cfg(feature = "dev")]
pub fn flush_recording(recorder: Option<Res<InputRecorder>>, exit: MessageReader<AppExit>) {
    // PHASE6-IMPL: B.
    let _ = (recorder, exit);
}
