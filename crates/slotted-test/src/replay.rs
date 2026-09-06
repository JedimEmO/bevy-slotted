//! Replay a recorded input session (`slotted_ui::Recording`) through the
//! harness. Phase 6 contract section 2.3. A replay is indistinguishable from a
//! test: every recorded input goes through the same `pointer_input` and key
//! paths the actions use.

use std::path::Path;

use slotted_ui::Recording;

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
}

impl UiHarness {
    /// Replays the RON recording at `path`. Warns (does not fail) when the
    /// recording's resolution or scale factor differ from the harness's.
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
    pub fn replay_recording(&mut self, recording: &Recording) -> Result<ReplayReport, ReplayError> {
        if recording.version > slotted_ui::RECORDING_VERSION {
            return Err(ReplayError::Version(recording.version));
        }
        // PHASE6-IMPL: B. For each frame in order: step until the harness's
        // frame counter reaches `frame.frame`, then feed each input through
        // `pointer_move_to` / `pointer_press` / `pointer_release` / `scroll` /
        // `key_with` / gamepad; count.
        let _ = recording;
        Ok(ReplayReport::default())
    }
}
