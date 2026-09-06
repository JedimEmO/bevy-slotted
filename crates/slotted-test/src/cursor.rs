//! A scrubbable position in a recorded session.
//!
//! [`UiHarness::replay`](crate::UiHarness::replay) plays a recording start to
//! finish and tells you what it did. That is the right shape for a test and
//! the wrong one for a person: the showcase's Testing scene
//! (`docs/design/showcase-contract.md` section 3.7) hands the visitor a
//! scrubber, a play button and a frame counter over the same file, and none of
//! those can be built on a call that only returns when it is over.
//!
//! [`ReplayCursor`] is that same replay, one recorded frame at a time, with an
//! index you can move backwards. Moving backwards is the whole difficulty: an
//! input stream is not reversible, so a seek to an earlier frame puts the world
//! back to where the recording opened and feeds `0..frame` again. Putting the
//! world back is the caller's job, because the cursor knows about inputs and
//! nothing else -- not which menu is open, not what was in it.
//!
//! Nothing here needs a [`UiHarness`](crate::UiHarness): it drives a plain
//! `&mut World`, which is what the web playground has.

use bevy::camera::NormalizedRenderTarget;
use bevy::input::ButtonState;
use bevy::input::keyboard::KeyboardInput;
use bevy::picking::pointer::{Location, PointerAction, PointerId, PointerInput};
use bevy::prelude::*;
use bevy::window::{PrimaryWindow, WindowRef};
use slotted_ui::{RecordedInput, Recording};

use crate::replay::ReplayError;

/// Where a replay has got to, and whether it is running.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReplayStatus {
    /// Recorded frames fed so far: `0` is "nothing yet", `frames` is "done".
    pub frame: usize,
    /// How many recorded frames the file holds.
    pub frames: usize,
    /// Whether [`ReplayCursor::tick`] advances.
    pub playing: bool,
}

/// A position in a [`Recording`], movable in both directions.
///
/// A "frame" here is one entry of `Recording::frames`, which is one *recorded*
/// frame: the file only stores frames that carried an input, so the scrubber's
/// numbers are dense and the empty frames between them are not shown. That is
/// what a person scrubbing wants; a test that cares about wall-clock pacing
/// wants [`UiHarness::replay_recording`](crate::UiHarness::replay_recording)
/// instead.
#[derive(Debug, Clone)]
pub struct ReplayCursor {
    recording: Recording,
    frame: usize,
    playing: bool,
}

impl ReplayCursor {
    /// A cursor over `recording`, parked before its first frame.
    ///
    /// # Errors
    ///
    /// The recording is a newer format than this crate knows.
    pub fn new(recording: Recording) -> Result<Self, ReplayError> {
        if recording.version > slotted_ui::RECORDING_VERSION {
            return Err(ReplayError::Version(recording.version));
        }
        Ok(Self {
            recording,
            frame: 0,
            playing: false,
        })
    }

    /// A cursor over a recording in RON, which is how one arrives compiled
    /// into a wasm module.
    ///
    /// # Errors
    ///
    /// The text is not a recording, or is a newer format.
    pub fn from_ron(text: &str) -> Result<Self, ReplayError> {
        let recording = Recording::from_ron(text).map_err(|source| ReplayError::Parse {
            path: "<bundled>".to_owned(),
            source: Box::new(source),
        })?;
        Self::new(recording)
    }

    /// The recording behind the cursor.
    pub fn recording(&self) -> &Recording {
        &self.recording
    }

    /// Frames fed so far.
    pub fn frame(&self) -> usize {
        self.frame
    }

    /// Frames the recording holds.
    pub fn frames(&self) -> usize {
        self.recording.frames.len()
    }

    /// Whether [`tick`](Self::tick) advances.
    pub fn playing(&self) -> bool {
        self.playing
    }

    /// Starts or stops playback. Pressing play at the end rewinds first, which
    /// is what a play button on a finished recording is asking for.
    pub fn set_playing(&mut self, on: bool) {
        if on && self.frame >= self.frames() {
            self.frame = 0;
        }
        self.playing = on;
    }

    /// Frame, total and playing, for a scrubber to render.
    pub fn status(&self) -> ReplayStatus {
        ReplayStatus {
            frame: self.frame,
            frames: self.frames(),
            playing: self.playing,
        }
    }

    /// Refuses a recording this window cannot reproduce.
    ///
    /// A recording stores pointer positions and no locators, so replayed into
    /// a window of another size every click lands on whatever happens to be
    /// under it and the replay passes while touching the wrong slots. Same
    /// reasoning as [`UiHarness::replay_recording`](crate::UiHarness::replay_recording),
    /// and the same refusal.
    ///
    /// # Errors
    ///
    /// The window is not the resolution or the scale factor the recording was
    /// made at. A world with no primary window is accepted: there is nothing
    /// to disagree with.
    pub fn check_geometry(&self, world: &mut World) -> Result<(), ReplayError> {
        let Some(window) = primary_window_component(world) else {
            return Ok(());
        };
        if window.0 != self.recording.resolution {
            return Err(ReplayError::Resolution {
                recorded: self.recording.resolution,
                harness: window.0,
            });
        }
        if (window.1 - self.recording.scale_factor).abs() > f32::EPSILON {
            return Err(ReplayError::ScaleFactor {
                recorded: self.recording.scale_factor,
                harness: window.1,
            });
        }
        Ok(())
    }

    /// Feeds the next recorded frame's inputs, if there is one.
    ///
    /// Returns `false` at the end, having done nothing. It does not run the
    /// schedule: the caller is inside one.
    pub fn step(&mut self, world: &mut World) -> bool {
        let Some(recorded) = self.recording.frames.get(self.frame) else {
            return false;
        };
        for input in &recorded.inputs {
            feed(world, input);
        }
        self.frame += 1;
        true
    }

    /// One frame of playback: [`step`](Self::step) while playing, and stop at
    /// the end.
    ///
    /// Returns whether anything was fed.
    pub fn tick(&mut self, world: &mut World) -> bool {
        if !self.playing {
            return false;
        }
        if self.step(world) {
            return true;
        }
        self.playing = false;
        false
    }

    /// Moves to `frame`, feeding whatever has to be fed to get there.
    ///
    /// Forwards is cheap: the frames between here and there are fed in order.
    /// Backwards is not, because an input stream does not run in reverse, so
    /// `rewind` is called to put the world back to the state the recording
    /// opened on and every frame from the start is fed again. `rewind` is the
    /// caller's, because the cursor knows nothing about the menu the recording
    /// was made over.
    ///
    /// `frame` is clamped to `0..=frames()`.
    pub fn seek(&mut self, world: &mut World, frame: usize, rewind: impl FnOnce(&mut World)) {
        let target = frame.min(self.frames());
        if target < self.frame {
            rewind(world);
            self.frame = 0;
        }
        while self.frame < target {
            if !self.step(world) {
                break;
            }
        }
    }
}

/// One recorded input into `world`, through the same messages a real device
/// writes.
///
/// Pointer positions are logical window coordinates, which is what the
/// recorder stored and what `bevy_picking` reads.
pub fn feed(world: &mut World, input: &RecordedInput) {
    match input {
        RecordedInput::PointerMove { pos } => {
            send_pointer(world, *pos, PointerAction::Move { delta: Vec2::ZERO });
        }
        RecordedInput::PointerPress(button) => {
            let pos = pointer_pos(world);
            send_pointer(world, pos, PointerAction::Press((*button).into()));
        }
        RecordedInput::PointerRelease(button) => {
            let pos = pointer_pos(world);
            send_pointer(world, pos, PointerAction::Release((*button).into()));
        }
        RecordedInput::Scroll { delta } => {
            let pos = pointer_pos(world);
            send_pointer(
                world,
                pos,
                PointerAction::Scroll {
                    unit: bevy::input::mouse::MouseScrollUnit::Line,
                    x: delta.x,
                    y: delta.y,
                    phase: bevy::input::touch::TouchPhase::Moved,
                },
            );
        }
        RecordedInput::Key {
            key_code,
            logical,
            pressed,
        } => {
            let Some(window) = primary_window(world) else {
                return;
            };
            let state = if *pressed {
                ButtonState::Pressed
            } else {
                ButtonState::Released
            };
            let text = match (logical, state) {
                (bevy::input::keyboard::Key::Character(c), ButtonState::Pressed) => Some(c.clone()),
                _ => None,
            };
            world.write_message(KeyboardInput {
                key_code: *key_code,
                logical_key: logical.clone(),
                state,
                text,
                repeat: false,
                window,
            });
        }
        RecordedInput::GamepadButton { button, pressed } => {
            let Some(entity) = first_gamepad(world) else {
                return;
            };
            let state = if *pressed {
                ButtonState::Pressed
            } else {
                ButtonState::Released
            };
            let value = if *pressed { 1.0 } else { 0.0 };
            world.write_message(bevy::input::gamepad::GamepadEvent::Button(
                bevy::input::gamepad::GamepadButtonChangedEvent::new(entity, *button, state, value),
            ));
        }
        RecordedInput::GamepadAxis { axis, value } => {
            let Some(entity) = first_gamepad(world) else {
                return;
            };
            world.write_message(bevy::input::gamepad::GamepadEvent::Axis(
                bevy::input::gamepad::GamepadAxisChangedEvent::new(entity, *axis, *value),
            ));
        }
    }
}

/// Where the mouse pointer is, so a press lands where the last move put it.
/// A recording always moves before it presses, so this is only the fallback
/// for a file that opens with a press.
fn pointer_pos(world: &mut World) -> Vec2 {
    let mut query = world.query::<(&PointerId, &bevy::picking::pointer::PointerLocation)>();
    for (id, location) in query.iter(world) {
        if *id == PointerId::Mouse
            && let Some(location) = location.location()
        {
            return location.position;
        }
    }
    Vec2::ZERO
}

fn send_pointer(world: &mut World, position: Vec2, action: PointerAction) {
    let Some(window) = primary_window(world) else {
        return;
    };
    let Some(target) = WindowRef::Primary.normalize(Some(window)) else {
        return;
    };
    world.write_message(PointerInput::new(
        PointerId::Mouse,
        Location {
            target: NormalizedRenderTarget::Window(target),
            position,
        },
        action,
    ));
}

fn primary_window(world: &mut World) -> Option<Entity> {
    let mut query = world.query_filtered::<Entity, With<PrimaryWindow>>();
    query.iter(world).next()
}

/// `(logical size, scale factor)` of the primary window.
fn primary_window_component(world: &mut World) -> Option<(Vec2, f32)> {
    let mut query = world.query_filtered::<&Window, With<PrimaryWindow>>();
    query.iter(world).next().map(|w| {
        (
            Vec2::new(w.width(), w.height()),
            w.resolution.scale_factor(),
        )
    })
}

fn first_gamepad(world: &mut World) -> Option<Entity> {
    let mut query = world.query_filtered::<Entity, With<bevy::input::gamepad::Gamepad>>();
    query.iter(world).next()
}

#[cfg(test)]
mod tests {
    use super::*;
    use slotted_ui::RecordedFrame;

    #[allow(clippy::cast_precision_loss)]
    fn recording(frames: usize) -> Recording {
        Recording {
            version: slotted_ui::RECORDING_VERSION,
            resolution: Vec2::new(1600.0, 900.0),
            scale_factor: 1.0,
            frame_delta_us: 16_666,
            frames: (0..frames)
                .map(|i| RecordedFrame {
                    frame: i as u64,
                    inputs: vec![RecordedInput::PointerMove {
                        pos: Vec2::splat(i as f32),
                    }],
                })
                .collect(),
        }
    }

    #[test]
    fn a_cursor_counts_the_recorded_frames_and_starts_parked() {
        let cursor = ReplayCursor::new(recording(4)).expect("this version");
        assert_eq!(
            cursor.status(),
            ReplayStatus {
                frame: 0,
                frames: 4,
                playing: false
            }
        );
    }

    #[test]
    fn seeking_forwards_feeds_and_seeking_back_rewinds_once() {
        let mut world = World::new();
        world.init_resource::<Messages<PointerInput>>();
        let mut cursor = ReplayCursor::new(recording(5)).expect("this version");

        let mut rewinds = 0;
        cursor.seek(&mut world, 3, |_| rewinds += 1);
        assert_eq!(cursor.frame(), 3);
        assert_eq!(rewinds, 0, "forwards needs no rewind");

        cursor.seek(&mut world, 1, |_| rewinds += 1);
        assert_eq!(cursor.frame(), 1);
        assert_eq!(rewinds, 1, "backwards rewinds exactly once");
    }

    #[test]
    fn a_seek_past_the_end_lands_on_the_last_frame() {
        let mut world = World::new();
        world.init_resource::<Messages<PointerInput>>();
        let mut cursor = ReplayCursor::new(recording(2)).expect("this version");
        cursor.seek(&mut world, 99, |_| panic!("no rewind going forwards"));
        assert_eq!(cursor.frame(), 2);
    }

    #[test]
    fn playing_stops_at_the_end_and_play_again_rewinds() {
        let mut world = World::new();
        world.init_resource::<Messages<PointerInput>>();
        let mut cursor = ReplayCursor::new(recording(2)).expect("this version");
        cursor.set_playing(true);
        assert!(cursor.tick(&mut world));
        assert!(cursor.tick(&mut world));
        assert!(!cursor.tick(&mut world), "nothing left to feed");
        assert!(!cursor.playing(), "playback stopped itself");

        cursor.set_playing(true);
        assert_eq!(cursor.frame(), 0, "play at the end starts over");
    }

    #[test]
    fn a_newer_recording_is_refused() {
        let mut newer = recording(1);
        newer.version = slotted_ui::RECORDING_VERSION + 1;
        assert!(matches!(
            ReplayCursor::new(newer),
            Err(ReplayError::Version(_))
        ));
    }
}
