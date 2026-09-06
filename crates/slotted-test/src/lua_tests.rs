//! Run a mod's `tests/*.lua` through the harness. Phase 6 contract section
//! 3.2. The Lua side is `slotted_script::TEST_PRELUDE`; each yielded
//! [`TestOp`] is performed by [`ops`] against the harness's world.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use slotted_model::Value;
use slotted_script::TestOp;

use crate::fixture::MenuFixture;
use crate::harness::UiHarness;

/// One test's outcome.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LuaTestResult {
    /// Test name.
    pub name: String,
    /// Passed.
    pub passed: bool,
    /// Failure message.
    pub message: Option<String>,
    /// Ops performed.
    pub steps: usize,
}

/// One file's outcomes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LuaTestReport {
    /// Which mod.
    pub mod_id: String,
    /// Which file.
    pub file: PathBuf,
    /// In registration order.
    pub results: Vec<LuaTestResult>,
}

impl LuaTestReport {
    /// Every test passed.
    pub fn all_passed(&self) -> bool {
        self.results.iter().all(|r| r.passed)
    }
}

/// What performing one op produced.
#[derive(Debug, Clone, PartialEq)]
pub enum StepOutcome {
    /// Finished, with the value to hand back or the error to raise.
    Done(Result<Value, String>),
    /// Needs more frames (live driver only).
    Pending,
}

/// Something that can perform test ops against a world. The harness is one;
/// the playground's `LiveDriver` is the other.
pub trait TestDriver {
    /// Perform `op`.
    fn perform(&mut self, world: &mut World, op: &TestOp) -> StepOutcome;
}

/// Fixture aliases a host registered for `open_screen(kind, "alias")`.
#[derive(Resource, Default)]
pub struct LuaFixtures(
    pub std::collections::BTreeMap<String, std::sync::Arc<dyn MenuFixture + Send + Sync>>,
);

/// A fixture built from a Lua table: `{ slots = {27, 27, 9}, fill = { ["0:0"]
/// = { item = "ns:id", count = 8 } } }`.
#[derive(Debug, Clone, PartialEq)]
pub struct LuaFixture {
    /// Inventory sizes in order.
    pub slots: Vec<u16>,
    /// `(inventory, slot) -> (item, count)`.
    pub fill: Vec<((u16, u16), (String, u32))>,
}

impl LuaFixture {
    /// Parses the table form; `Err` names what was wrong.
    pub fn from_value(value: &Value) -> Result<Self, String> {
        // PHASE6-IMPL: C.
        let _ = value;
        Err("fixture tables are not implemented yet".to_owned())
    }
}

/// The `World`-level implementation of every [`TestOp`], shared by the
/// harness driver and the playground's live driver.
pub mod ops {
    use bevy::prelude::*;
    use slotted_model::Value;
    use slotted_script::{TestLocator, TestOp};

    use crate::locator::Locator;

    /// Maps a Lua locator onto a harness [`Locator`].
    pub fn to_locator(loc: &TestLocator) -> Result<Locator, String> {
        // PHASE6-IMPL: C. `role` by snake-case `SemanticRole` name; `tag`,
        // `test_id`, `text`, `widget`, `item`, `index` onto `by::*` and the
        // builder methods.
        let _ = loc;
        Err("locators are not implemented yet".to_owned())
    }

    /// Resolves to exactly one entity or a message naming near misses.
    pub fn resolve_one(world: &World, loc: &TestLocator) -> Result<Entity, String> {
        let locator = to_locator(loc)?;
        let found = locator.resolve(world);
        match found.as_slice() {
            [one] => Ok(*one),
            [] => Err(format!("no node matches {locator}")),
            many => Err(format!("{} nodes match {locator}", many.len())),
        }
    }

    /// A query op that needs no frames: `StackAt`, `TextOf`, `PropertyOf`,
    /// `TankFill`, `IsVisible`, `LogContains`. Returns `None` for action ops.
    pub fn query(world: &mut World, op: &TestOp) -> Option<Result<Value, String>> {
        // PHASE6-IMPL: C.
        let _ = (world, op);
        None
    }
}

impl UiHarness {
    /// Registers a fixture alias for Lua `open_screen(kind, "alias")`.
    pub fn register_fixture(
        &mut self,
        alias: &str,
        fixture: impl MenuFixture + Send + Sync + 'static,
    ) {
        let world = self.world_mut();
        world.init_resource::<LuaFixtures>();
        world
            .resource_mut::<LuaFixtures>()
            .0
            .insert(alias.to_owned(), std::sync::Arc::new(fixture));
    }

    /// Loads `source` as a test-stage script of `mod_id` and runs every test
    /// it registers, performing each op through this harness.
    pub fn run_lua_tests(&mut self, mod_id: &str, file: &Path, source: &str) -> LuaTestReport {
        // PHASE6-IMPL: C. Load through `ScriptHost` with `Stage::Test`, send
        // `TestList`, then per name loop `TestRun` / `TestResume`, performing
        // each `TestStep` (`OpenScreen` -> `open_mod_screen`, `Settle` ->
        // `settle`, pointer ops through the actions, queries through
        // `ops::query`). Unload the script afterwards.
        let _ = source;
        LuaTestReport {
            mod_id: mod_id.to_owned(),
            file: file.to_path_buf(),
            results: Vec::new(),
        }
    }

    /// Runs `tests/*.lua` (sorted) of `mod_id` under the mods directory the
    /// harness was built with.
    pub fn run_mod_tests(&mut self, mod_id: &str) -> Vec<LuaTestReport> {
        // PHASE6-IMPL: C. `ModsUnderTest::dir()/<mod_id>/tests/*.lua`.
        let _ = mod_id;
        Vec::new()
    }
}

impl TestDriver for UiHarness {
    fn perform(&mut self, _world: &mut World, op: &TestOp) -> StepOutcome {
        // PHASE6-IMPL: C. The harness owns its world; `_world` is unused here
        // and the op is performed through `self`.
        let _ = op;
        StepOutcome::Done(Err("the harness driver is not implemented yet".to_owned()))
    }
}
