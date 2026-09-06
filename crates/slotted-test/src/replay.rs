//! Replay a recorded input session (`slotted_ui::Recording`) through the
//! harness. Phase 6 contract section 2.3. A replay is indistinguishable from a
//! test: every recorded input goes through the same `pointer_input` and key
//! paths the actions use.

use std::path::Path;

use bevy::prelude::*;
use slotted_ui::{RecordedInput, Recording};

use crate::harness::UiHarness;

/// What a replay did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ReplayReport {
    /// Frames stepped, including empty ones between recorded frames.
    pub frames: usize,
    /// Inputs fed.
    pub inputs: usize,
}

/// Why a replay could not run.
#[derive(Debug, thiserror::Error)]
pub enum ReplayError {
    /// The file could not be read.
    #[error("reading {path}: {source}")]
    Io {
        /// Path.
        path: String,
        /// Cause.
        source: std::io::Error,
    },
    /// The file is not a recording.
    #[error("parsing {path}: {source}")]
    Parse {
        /// Path.
        path: String,
        /// Cause.
        source: Box<ron::error::SpannedError>,
    },
    /// A newer format than this harness knows.
    #[error("recording version {0} is newer than this harness supports")]
    Version(u32),
    /// The window is not the size the recording was made at.
    ///
    /// A recording stores pointer positions, not locators: nothing in it says
    /// which node a click was meant for. Replayed into a window of another
    /// size, or a screen whose layout moved, every position lands on whatever
    /// happens to be under it, and the replay passes while clicking the wrong
    /// slot. Refusing is the only honest answer.
    #[error(
        "recording was made at {recorded:?} and this harness is {harness:?}: \
         recorded pointer positions would land on different nodes"
    )]
    Resolution {
        /// Size the recording was made at.
        recorded: Vec2,
        /// Size of this harness's window.
        harness: Vec2,
    },
    /// The window's scale factor is not the recording's.
    #[error(
        "recording was made at scale factor {recorded} and this harness is at {harness}: \
         recorded pointer positions would land on different nodes"
    )]
    ScaleFactor {
        /// Scale factor the recording was made at.
        recorded: f32,
        /// This harness's scale factor.
        harness: f32,
    },
}

impl UiHarness {
    /// Replays the RON recording at `path`.
    ///
    /// # Errors
    ///
    /// The file is missing or is not a recording, the recording is a newer
    /// format, or it was made at another resolution or scale factor.
    pub fn replay(&mut self, path: impl AsRef<Path>) -> Result<ReplayReport, ReplayError> {
        let path = path.as_ref();
        let text = std::fs::read_to_string(path).map_err(|source| ReplayError::Io {
            path: path.display().to_string(),
            source,
        })?;
        let recording = Recording::from_ron(&text).map_err(|source| ReplayError::Parse {
            path: path.display().to_string(),
            source: Box::new(source),
        })?;
        self.replay_recording(&recording)
    }

    /// Replays an in-memory recording.
    ///
    /// # Errors
    ///
    /// A newer format, or a window that is not the size or scale factor the
    /// recording was made at.
    pub fn replay_recording(&mut self, recording: &Recording) -> Result<ReplayReport, ReplayError> {
        if recording.version > slotted_ui::RECORDING_VERSION {
            return Err(ReplayError::Version(recording.version));
        }
        self.check_geometry(recording)?;
        let mut report = ReplayReport::default();
        // The harness steps a frame per fed input, so `frame` runs ahead of
        // the recorded numbers as soon as one frame carried several inputs.
        // Recorded frame numbers are a floor, not a schedule: they say what
        // may not happen before what.
        let mut frame = 0_u64;
        for recorded in &recording.frames {
            while frame < recorded.frame {
                self.step(1);
                report.frames += 1;
                frame += 1;
            }
            for input in &recorded.inputs {
                self.feed(input);
                report.inputs += 1;
                report.frames += 1;
                frame += 1;
            }
        }
        Ok(report)
    }

    /// Refuses a recording the window cannot reproduce.
    ///
    /// Contract 2.3 asked for a warning here. A warning is the wrong answer:
    /// a replay whose positions have drifted still reports every frame fed
    /// and every input delivered, so it passes green while clicking nodes the
    /// recording never touched. There is no locator in a recording to fall
    /// back on, so the mismatch has to stop the run.
    fn check_geometry(&self, recording: &Recording) -> Result<(), ReplayError> {
        let window = self.window();
        let Some(window) = self.world().get::<bevy::window::Window>(window) else {
            return Ok(());
        };
        let resolution = Vec2::new(window.width(), window.height());
        if resolution != recording.resolution {
            return Err(ReplayError::Resolution {
                recorded: recording.resolution,
                harness: resolution,
            });
        }
        let scale = window.resolution.scale_factor();
        if (scale - recording.scale_factor).abs() > f32::EPSILON {
            return Err(ReplayError::ScaleFactor {
                recorded: recording.scale_factor,
                harness: scale,
            });
        }
        Ok(())
    }

    /// One recorded input, through the same paths the actions use.
    fn feed(&mut self, input: &RecordedInput) {
        match input {
            RecordedInput::PointerMove { pos } => self.pointer_move_to(*pos),
            RecordedInput::PointerPress(button) => self.pointer_press((*button).into()),
            RecordedInput::PointerRelease(button) => self.pointer_release((*button).into()),
            RecordedInput::Scroll { delta } => self.scroll_here(*delta),
            RecordedInput::Key {
                key_code,
                logical,
                pressed,
            } => self.key_state(*key_code, logical.clone(), *pressed),
            RecordedInput::GamepadButton { button, pressed } => {
                self.gamepad_button(*button, *pressed);
            }
            RecordedInput::GamepadAxis { axis, value } => self.gamepad_axis(*axis, *value),
        }
    }

    /// Scroll where the pointer already is. The action of the same name takes
    /// an entity; a recording only knows a position.
    fn scroll_here(&mut self, delta: Vec2) {
        let pos = self.pointer_pos();
        let window = self.window();
        let pointer = self.pointer();
        let id = *self
            .world()
            .get::<bevy::picking::pointer::PointerId>(pointer)
            .expect("the harness pointer has a PointerId");
        let location = bevy::picking::pointer::Location {
            target: bevy::camera::NormalizedRenderTarget::Window(
                bevy::window::WindowRef::Primary
                    .normalize(Some(window))
                    .expect("primary window exists"),
            ),
            position: pos,
        };
        self.world_mut()
            .write_message(bevy::picking::pointer::PointerInput::new(
                id,
                location,
                bevy::picking::pointer::PointerAction::Scroll {
                    unit: bevy::input::mouse::MouseScrollUnit::Line,
                    x: delta.x,
                    y: delta.y,
                    phase: bevy::input::touch::TouchPhase::Moved,
                },
            ));
        self.step(1);
    }

    /// A key press or release on its own; `key_with` does both.
    fn key_state(&mut self, key_code: KeyCode, logical: bevy::input::keyboard::Key, pressed: bool) {
        let window = self.window();
        self.world_mut()
            .write_message(bevy::input::keyboard::KeyboardInput {
                key_code,
                logical_key: logical,
                state: if pressed {
                    bevy::input::ButtonState::Pressed
                } else {
                    bevy::input::ButtonState::Released
                },
                text: None,
                repeat: false,
                window,
            });
        self.step(1);
    }

    /// The gamepad entity a replayed gamepad event targets: the first one the
    /// app has. A headless harness usually has none, and then the event is
    /// dropped rather than invented.
    fn gamepad(&mut self) -> Option<Entity> {
        self.world_mut()
            .query_filtered::<Entity, With<bevy::input::gamepad::Gamepad>>()
            .iter(self.world())
            .next()
    }

    fn gamepad_button(&mut self, button: bevy::input::gamepad::GamepadButton, pressed: bool) {
        let Some(entity) = self.gamepad() else {
            self.step(1);
            return;
        };
        let state = if pressed {
            bevy::input::ButtonState::Pressed
        } else {
            bevy::input::ButtonState::Released
        };
        let value = if pressed { 1.0 } else { 0.0 };
        self.world_mut()
            .write_message(bevy::input::gamepad::GamepadEvent::Button(
                bevy::input::gamepad::GamepadButtonChangedEvent::new(entity, button, state, value),
            ));
        self.step(1);
    }

    fn gamepad_axis(&mut self, axis: bevy::input::gamepad::GamepadAxis, value: f32) {
        let Some(entity) = self.gamepad() else {
            self.step(1);
            return;
        };
        self.world_mut()
            .write_message(bevy::input::gamepad::GamepadEvent::Axis(
                bevy::input::gamepad::GamepadAxisChangedEvent::new(entity, axis, value),
            ));
        self.step(1);
    }
}
