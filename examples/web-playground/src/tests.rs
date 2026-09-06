//! The Tests tab's half of the world: run a mod's bundled `tests/*.lua`
//! against the live app, one op per frame. Phase 6 contract section 3.2.
//!
//! The alternative was a second headless harness inside the page. That would
//! double the world, need its own window and camera fakes beside the real
//! ones, and show the player nothing. Against the live app a test drives the
//! canvas they are looking at: `open_screen` means "the screen you have
//! open", which is what a modder iterating in the editor wants. The cost is
//! stated and real: these tests are not isolated, and a fixture the open
//! screen does not match fails saying so.

use std::collections::VecDeque;

use bevy::prelude::*;
use slotted_packs::ScriptHost;
use slotted_script::{ModId, ScriptCommand, ScriptEvent, ScriptId, Stage, TestOp};
use slotted_test::live::LiveDriver;
use slotted_test::lua_tests::{StepOutcome, TestDriver};

use crate::bus::Bus;

/// Ops one test body may yield before the runner calls it a runaway.
const MAX_STEPS: usize = 2000;

/// A run in flight: one file's tests, one op at a time.
#[derive(Resource)]
pub struct LiveTestRunner {
    /// Which mod.
    mod_id: String,
    /// Which file.
    file: String,
    /// The loaded test script.
    script: ScriptId,
    /// Tests not started yet.
    queued: VecDeque<String>,
    /// The test running now, and the op it is waiting on.
    current: Option<Running>,
    /// Passed and failed so far.
    tally: (usize, usize),
    /// The op driver.
    driver: LiveDriver,
}

struct Running {
    name: String,
    op: TestOp,
    steps: usize,
}

impl LiveTestRunner {
    /// Loads `mod_id`'s bundled tests and asks the file for its test names.
    ///
    /// # Errors
    ///
    /// No such mod, no bundled tests, no script runtime, or a file that does
    /// not compile: all of them things the page shows on its console rather
    /// than panics, because a panic on wasm is a dead canvas.
    pub fn start(world: &mut World, mod_id: &str) -> Result<Self, String> {
        let files = crate::bundle::test_files(mod_id);
        let Some((file, _)) = files.first().cloned() else {
            return Err(format!("`{mod_id}` bundles no tests/*.lua"));
        };
        let source = crate::bundle::read_test_file(mod_id, &file)?;
        let host = world
            .get_resource::<ScriptHost>()
            .cloned()
            .ok_or_else(|| "no script runtime".to_owned())?;
        let id = ModId::new(mod_id).map_err(|e| e.to_string())?;
        let script = host
            .lock()
            .load(&id, &file, &source, Stage::Test)
            .map_err(|e| e.to_string())?;
        let names = match call(&host, script, ScriptEvent::TestList)? {
            ScriptCommand::TestList { names } => names,
            other => return Err(format!("expected a test list, got {other:?}")),
        };
        Ok(Self {
            mod_id: mod_id.to_owned(),
            file,
            script,
            queued: names.into(),
            current: None,
            tally: (0, 0),
            driver: LiveDriver::default(),
        })
    }
}

/// One call into the test script, keeping only the command that answers it.
fn call(host: &ScriptHost, script: ScriptId, event: ScriptEvent) -> Result<ScriptCommand, String> {
    let commands = host
        .lock()
        .call(script, &event)
        .map_err(|e| e.to_string())?;
    commands
        .into_iter()
        .find(|c| {
            matches!(
                c,
                ScriptCommand::TestList { .. }
                    | ScriptCommand::TestStep { .. }
                    | ScriptCommand::TestDone { .. }
            )
        })
        .ok_or_else(|| "the test file answered with no test command".to_owned())
}

/// Stops a live test run, if one is in flight.
///
/// The runner drives a real screen one op per frame and holds the entities it
/// found on the way in. Anything that despawns that screen leaves the next op
/// pointing at an entity that is gone, and on `wasm32-unknown-unknown` that is
/// a panic, which is an aborted module and a dead canvas rather than an error
/// line the page can report. The Testing scene hit it by itself: pressing Load
/// recording rewinds the canvas to the recording's opening chest, and a visitor
/// who pressed Run a moment earlier still had a runner walking the screen that
/// rewind tore down.
///
/// So the rule is that whatever takes the screen away stops the run first, and
/// says so. `scenes::teardown` is the one place that happens, which is every
/// scene switch as well as the rewind.
pub fn cancel_live_tests(world: &mut World) {
    let Some(runner) = world.remove_resource::<LiveTestRunner>() else {
        return;
    };
    if let Some(host) = world.get_resource::<ScriptHost>().cloned() {
        host.lock().unload(runner.script);
    }
    if let Some(bus) = world.get_resource::<Bus>() {
        bus.log(
            "warn",
            "test",
            format!(
                "{}/{}: the screen it was driving closed, so the run was stopped",
                runner.mod_id, runner.file
            ),
        );
    }
}

/// `Update`, exclusive: advance the run by one op, or by one frame of one.
///
/// The runner is taken out of the world for the duration, because performing
/// an op needs the whole `World` and the driver keeps state between frames.
pub fn run_live_tests(world: &mut World) {
    let Some(mut runner) = world.remove_resource::<LiveTestRunner>() else {
        return;
    };
    let bus = world.resource::<Bus>().clone();
    let Some(host) = world.get_resource::<ScriptHost>().cloned() else {
        return;
    };

    match step(&mut runner, world, &host, &bus) {
        Ok(true) => world.insert_resource(runner),
        Ok(false) => {
            host.lock().unload(runner.script);
            let (passed, failed) = runner.tally;
            bus.log(
                if failed == 0 { "info" } else { "error" },
                "test",
                format!(
                    "{}/{}: {passed} passed, {failed} failed",
                    runner.mod_id, runner.file
                ),
            );
        }
        Err(message) => {
            host.lock().unload(runner.script);
            bus.log("error", "test", format!("{}: {message}", runner.mod_id));
        }
    }
}

/// One frame of the run. `Ok(true)` means there is more to do.
fn step(
    runner: &mut LiveTestRunner,
    world: &mut World,
    host: &ScriptHost,
    bus: &Bus,
) -> Result<bool, String> {
    // Nothing running: start the next test, if there is one.
    let Some(mut running) = runner.current.take() else {
        let Some(name) = runner.queued.pop_front() else {
            return Ok(false);
        };
        let command = call(
            host,
            runner.script,
            ScriptEvent::TestRun { name: name.clone() },
        )?;
        runner.current = advance(runner, bus, name, 0, command);
        return Ok(true);
    };

    // A test is running: give its op one more frame.
    let outcome = runner.driver.perform(world, &running.op);
    let StepOutcome::Done(answer) = outcome else {
        runner.current = Some(running);
        return Ok(true);
    };
    running.steps += 1;
    if running.steps > MAX_STEPS {
        finish(
            runner,
            bus,
            &running.name,
            false,
            Some(format!("more than {MAX_STEPS} steps: runaway test")),
        );
        return Ok(true);
    }
    let (value, error) = match answer {
        Ok(value) => (value, None),
        Err(message) => (slotted_model::Value::Null, Some(message)),
    };
    let command = call(
        host,
        runner.script,
        ScriptEvent::TestResume { value, error },
    )?;
    runner.current = advance(runner, bus, running.name, running.steps, command);
    Ok(true)
}

/// A step or a result: either the runner waits on another op, or the test is
/// over and its line goes on the console.
fn advance(
    runner: &mut LiveTestRunner,
    bus: &Bus,
    name: String,
    steps: usize,
    command: ScriptCommand,
) -> Option<Running> {
    match command {
        ScriptCommand::TestStep { op } => Some(Running { name, op, steps }),
        ScriptCommand::TestDone {
            name,
            passed,
            message,
        } => {
            finish(runner, bus, &name, passed, message);
            None
        }
        other => {
            finish(
                runner,
                bus,
                &name,
                false,
                Some(format!("expected a test step, got {other:?}")),
            );
            None
        }
    }
}

/// Counts one result and writes its `ok` or `FAIL` line.
fn finish(
    runner: &mut LiveTestRunner,
    bus: &Bus,
    name: &str,
    passed: bool,
    message: Option<String>,
) {
    let file = runner.file.clone();
    let mod_id = runner.mod_id.clone();
    if passed {
        runner.tally.0 += 1;
        bus.log("info", "test", format!("ok {mod_id}/{file}: {name}"));
    } else {
        runner.tally.1 += 1;
        bus.log(
            "error",
            "test",
            format!(
                "FAIL {mod_id}/{file}: {name}: {}",
                message.as_deref().unwrap_or("no message")
            ),
        );
    }
}
