//! Scene 8: the Lua test runner and a recorded session replayed.
//!
//! `docs/design/showcase-contract.md` section 3.7. Two halves that make the
//! same point from opposite ends. On the left, the mods' `tests/*.lua` run
//! against the game the visitor is looking at, one action per frame, through
//! the same `LiveTestRunner` the Mods scene's Tests tab uses. On the right, a
//! session recorded in the windowed chest example plays back through the real
//! pointer and key paths, scrubbable frame by frame with [`ReplayCursor`].
//!
//! The two halves want different screens open, and the scene does not pretend
//! otherwise. It opens `copper_chest:chest`, which is what the bundled Lua
//! tests are written against. Pressing load on the recording swaps the canvas
//! to `demo:chest`, which is the chest the recording was made over: a
//! recording carries pointer positions and no locators, so it can only be
//! replayed over the screen it was taken from.

use std::sync::Arc;

use bevy::prelude::*;
use slotted::prelude::*;
use slotted_model::Actor;
use slotted_test::ReplayCursor;

use crate::bus::Bus;
use crate::scenes;
use crate::showcase::SceneHandler;

/// The session recorded with `cargo run -p chest -- --record`, compiled in.
///
/// A file rather than a build-script table because it is one file and it is
/// read as one string; `include_str!` keeps the checked-in RON the one copy.
pub const RECORDING_RON: &str = include_str!("../../../showcase/recordings/chest.rec.ron");

/// Scene 8.
pub struct TestingScene;

impl SceneHandler for TestingScene {
    fn enter(&self, world: &mut World) {
        // The Lua half. Same screen the Mods scene opens, because that is what
        // the bundled `tests/*.lua` say `open_screen` should find.
        crate::scenes::mods::ModsScene.enter(world);
    }

    fn leave(&self, world: &mut World) {
        world.remove_resource::<Replay>();
        scenes::teardown(world);
    }
}

/// The loaded recording and where the scrubber is.
#[derive(Resource)]
pub struct Replay {
    /// The cursor over the bundled recording.
    pub cursor: ReplayCursor,
    /// The screen the recording is replayed over, respawned on a rewind.
    pub screen: Arc<ScreenDef>,
    /// Where a seek is heading, while it is still getting there.
    pub target: Option<usize>,
}

/// `PostUpdate`: one recorded frame, when the visitor pressed play or dragged
/// the scrubber.
///
/// One frame per *app* frame, and never more, which is the whole reason a seek
/// is not a loop. Every recorded input is a message the picking backend, the
/// hover pass and the click handlers read on the next schedule run; feeding
/// twenty of them into one frame would leave the pointer at the last position
/// and every click in between would have landed on whatever was under that one.
/// A scrub of twenty frames therefore takes twenty frames, which at sixty a
/// second is a third of one and looks instant.
pub fn advance_replay(world: &mut World) {
    let Some(mut replay) = world.remove_resource::<Replay>() else {
        return;
    };
    match replay.target {
        Some(target) if replay.cursor.frame() < target => {
            if !replay.cursor.step(world) {
                replay.target = None;
            }
        }
        Some(_) => replay.target = None,
        None => {
            replay.cursor.tick(world);
        }
    }
    world.insert_resource(replay);
}

/// Whether a seek is still walking towards its target.
pub fn is_seeking(world: &World) -> bool {
    world
        .get_resource::<Replay>()
        .is_some_and(|replay| replay.target.is_some())
}

/// Loads the bundled recording and opens the chest it was made over.
///
/// The geometry check is reported rather than enforced. A recording stores
/// pointer positions, so replaying it into a window of another size lands the
/// clicks on whatever happens to be under them, and in a test that is a reason
/// to refuse outright (`UiHarness::replay_recording` does). In a browser the
/// canvas is whatever size the visitor's window makes it, and refusing would
/// mean the scrubber never worked for anybody; so the page is told the replay
/// will not land where it was recorded, and plays it anyway. The guarantee
/// lives in `tests/showcase.rs`, which replays at the recorded 1600x900 and
/// asserts the inventory it ends on.
///
/// # Errors
///
/// The bundled recording does not parse, or the mods have not loaded.
pub fn load(world: &mut World) -> Result<(), String> {
    let cursor = ReplayCursor::from_ron(RECORDING_RON)
        .map_err(|e| format!("the bundled recording did not load: {e}"))?;
    let bus = world.resource::<Bus>().clone();
    if let Err(e) = cursor.check_geometry(world) {
        bus.log("warn", "replay", e.to_string());
    }
    let screen = scenes::register_screen(world, showcase::chest::screen());
    let replay = Replay {
        cursor,
        screen: screen.clone(),
        target: None,
    };
    rewind(world, &screen)?;
    bus.log(
        "info",
        "replay",
        format!("loaded {} recorded frames", replay.cursor.frames()),
    );
    world.insert_resource(replay);
    Ok(())
}

/// Aims the scrubber at `frame`. [`advance_replay`] walks there, one recorded
/// frame per app frame.
///
/// Going backwards is not a walk: an input stream does not run in reverse, so
/// the chest is reopened as it was when the recording started and the cursor
/// starts again from zero. That happens here, at once, because it is a despawn
/// and a respawn rather than anything the picking backend has to see.
///
/// # Errors
///
/// Nothing is loaded.
pub fn seek(world: &mut World, frame: usize) -> Result<(), String> {
    let Some(mut replay) = world.remove_resource::<Replay>() else {
        return Err("no recording is loaded; press load first".to_owned());
    };
    let target = frame.min(replay.cursor.frames());
    let screen = replay.screen.clone();
    let mut failed = None;
    if target < replay.cursor.frame() {
        replay.cursor.seek(world, 0, |world| {
            failed = rewind(world, &screen).err();
        });
    }
    // Playing and seeking at once would race for the same cursor, and the
    // scrubber is the one the visitor is holding.
    replay.cursor.set_playing(false);
    replay.target = (target > replay.cursor.frame()).then_some(target);
    world.insert_resource(replay);
    failed.map_or(Ok(()), Err)
}

/// Starts or stops playback.
///
/// # Errors
///
/// Nothing is loaded.
pub fn play(world: &mut World, on: bool) -> Result<(), String> {
    let Some(mut replay) = world.get_resource_mut::<Replay>() else {
        return Err("no recording is loaded; press load first".to_owned());
    };
    replay.target = None;
    replay.cursor.set_playing(on);
    Ok(())
}

/// `{"frame":n,"frames":n,"playing":bool}` for the scrubber.
///
/// Nothing loaded reads as a zero-length recording, which is what a scrubber
/// with nothing to scrub should show.
pub fn status(world: &World) -> String {
    let Some(replay) = world.get_resource::<Replay>() else {
        return "{\"frame\":0,\"frames\":0,\"playing\":false}".to_owned();
    };
    let status = replay.cursor.status();
    // A seek in progress reads as playing, because that is what it looks like
    // and what a page should keep polling for.
    let busy = status.playing || replay.target.is_some();
    format!(
        "{{\"frame\":{},\"frames\":{},\"playing\":{busy}}}",
        status.frame, status.frames
    )
}

/// Puts the canvas back to the state the recording opened on: a fresh
/// `demo:chest` over fresh inventories.
fn rewind(world: &mut World, screen: &Arc<ScreenDef>) -> Result<(), String> {
    let registries =
        scenes::registries(world).ok_or_else(|| "the mods have not loaded yet".to_owned())?;
    scenes::teardown(world);
    let entities: Vec<Entity> = showcase::chest::inventories(&registries)
        .into_iter()
        .map(|inventory| world.spawn(slotted::ecs::menu::Inventory(inventory)).id())
        .collect();
    let mut ids = world
        .remove_resource::<slotted::ecs::MenuIdAllocator>()
        .unwrap_or_default();
    {
        let mut commands = world.commands();
        let menu = open_menu(
            &mut commands,
            &mut ids,
            showcase::chest::menu_def(),
            entities,
            Actor::SURVIVAL,
        );
        spawn_screen(&mut commands, screen.clone(), Some(menu));
    }
    world.insert_resource(ids);
    world.flush();
    Ok(())
}
