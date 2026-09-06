//! Perform test ops against a running app, one per frame. Phase 6 contract
//! section 3.2: the playground's Tests tab drives this from a resource. It
//! shares [`ops`](crate::lua_tests::ops) with the harness driver.

use bevy::prelude::*;
use slotted_script::TestOp;

use crate::lua_tests::{StepOutcome, TestDriver};

/// A driver over the live world. `Settle` is pending until `ActiveMotions ==
/// 0 && PendingRoundTrips == 0` for two consecutive frames; `Step` counts
/// frames; `OpenScreen` uses the screen that is open and fails when its kind
/// differs.
#[derive(Debug, Default)]
pub struct LiveDriver {
    /// Frames the current op has waited.
    pub waited: u32,
    /// Quiet frames seen in a row while settling.
    pub quiet: u32,
}

impl TestDriver for LiveDriver {
    fn perform(&mut self, world: &mut World, op: &TestOp) -> StepOutcome {
        // PHASE6-IMPL: C. Pointer ops write `PointerInput` on the `Mouse`
        // pointer of the primary window; keys write `KeyboardInput`.
        let _ = (world, &mut self.waited, &mut self.quiet);
        StepOutcome::Done(Err(format!("live driver cannot perform {op:?} yet")))
    }
}
