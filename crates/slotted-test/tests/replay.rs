//! Recorded input replays into the same world state. Phase 6 contract 2.3.
//!
//! The recording here is built in code rather than captured from a window, so
//! the test needs no `dev` feature and no session: what it pins is that the
//! replay path feeds the recorded stream through exactly the pointer and key
//! plumbing the actions use, and that a recorded gesture therefore lands the
//! same way twice.
#![allow(clippy::unwrap_used)]

use bevy::picking::pointer::PointerButton;
use bevy::prelude::*;
use pretty_assertions::assert_eq;
use slotted_test::prelude::*;
use slotted_ui::{
    Layout, RecordedButton, RecordedFrame, RecordedInput, Recording, ScreenDef, Screens, Tags,
    UiNodeDef,
};

const CHEST: &str = "demo:chest";
const WIDTH: f32 = 1280.0;
const HEIGHT: f32 = 720.0;

/// Three grids, the same shape the chest tests use.
fn chest_screen() -> ScreenDef {
    let grid = |inventory, rows, first, region: &str| UiNodeDef::SlotGrid {
        inventory,
        cols: 9,
        rows,
        first,
        tags: Tags::new().with("region", region),
    };
    ScreenDef {
        kind: ScreenKind::new(CHEST),
        initial_focus: None,
        presentation: slotted_ui::Presentation::default(),
        inherits: None,
        remove: vec![],
        root: UiNodeDef::Panel {
            role: slotted_theme::roles::PANEL,
            layout: Layout {
                gap: 1.0,
                padding: 1.0.into(),
                ..Layout::default()
            },
            children: vec![
                grid(MenuDef::CONTAINER, 3, 0, "chest"),
                grid(MenuDef::PLAYER_MAIN, 3, 27, "player"),
                grid(MenuDef::PLAYER_HOTBAR, 1, 54, "hotbar"),
            ],
            tags: Tags::new().with("test_id", "chest_panel"),
        },
        listring: vec![MenuDef::CONTAINER, MenuDef::PLAYER_MAIN],
    }
}

fn open_chest() -> (UiHarness, Opened) {
    let mut h = UiHarness::builder()
        .plugins(SlottedPlugins::headless())
        .registries(TestRegistries::basic())
        .resolution(WIDTH, HEIGHT)
        .theme("glass")
        .build();
    h.world_mut()
        .resource_mut::<Screens>()
        .register(chest_screen());
    let opened = h.open_screen(ScreenKind::new(CHEST), ChestFixture::filled());
    h.settle();
    (h, opened)
}

fn chest_slot(h: &UiHarness, n: usize) -> Entity {
    h.find(&by::role(SemanticRole::Slot).tag("region", "chest").index(n))
}

/// The stacks of every chest slot, as text: what "the same inventory state"
/// means without depending on entity ids.
fn contents(h: &UiHarness) -> Vec<Option<(u32, u32)>> {
    h.find_all(&by::role(SemanticRole::Slot).tag("region", "chest"))
        .into_iter()
        .map(|slot| h.stack_at(slot).map(|s| (s.id.0, s.count)))
        .collect()
}

/// Pick a stack up out of chest slot 0 and put it down in slot 3, once by
/// driving the harness and once by replaying a recording of the same clicks.
#[test]
fn a_recorded_click_replays_to_the_same_tree_and_inventory() {
    let (live, opened) = open_chest();
    let from = live.center_of(chest_slot(&live, 0));
    let to = live.center_of(chest_slot(&live, 3));
    let _ = opened;

    let mut live = live;
    live.pointer_move_to(from);
    live.pointer_press(PointerButton::Primary);
    live.pointer_release(PointerButton::Primary);
    live.pointer_move_to(to);
    live.pointer_press(PointerButton::Primary);
    live.pointer_release(PointerButton::Primary);
    live.settle();

    // The gesture did something: slot 0 emptied into slot 3.
    let after = contents(&live);
    assert!(after[0].is_none(), "the stack left slot 0");
    assert!(after[3].is_some(), "and landed in slot 3");

    let recording = Recording {
        version: slotted_ui::RECORDING_VERSION,
        resolution: Vec2::new(WIDTH, HEIGHT),
        scale_factor: 1.0,
        frame_delta_us: 16_667,
        frames: vec![
            frame(0, RecordedInput::PointerMove { pos: from }),
            frame(1, RecordedInput::PointerPress(RecordedButton::Primary)),
            frame(2, RecordedInput::PointerRelease(RecordedButton::Primary)),
            frame(3, RecordedInput::PointerMove { pos: to }),
            frame(4, RecordedInput::PointerPress(RecordedButton::Primary)),
            frame(5, RecordedInput::PointerRelease(RecordedButton::Primary)),
        ],
    };

    let (mut replayed, _) = open_chest();
    let report = replayed.replay_recording(&recording).unwrap();
    assert_eq!(report.inputs, 6);
    assert!(report.frames >= 6);
    replayed.settle();

    assert_eq!(contents(&replayed), after);
    assert_eq!(
        replayed.screen_tree().to_string(),
        live.screen_tree().to_string()
    );
    replayed.assert_conserved();
}

/// The same recording through the file path a `--record` session writes.
#[test]
fn a_recording_round_trips_through_ron_and_replays_from_disk() {
    let (live, _) = open_chest();
    let from = live.center_of(chest_slot(&live, 0));
    let recording = Recording {
        version: slotted_ui::RECORDING_VERSION,
        resolution: Vec2::new(WIDTH, HEIGHT),
        scale_factor: 1.0,
        frame_delta_us: 16_667,
        frames: vec![
            frame(0, RecordedInput::PointerMove { pos: from }),
            frame(1, RecordedInput::PointerPress(RecordedButton::Primary)),
            frame(2, RecordedInput::PointerRelease(RecordedButton::Primary)),
            RecordedFrame {
                frame: 4,
                inputs: vec![RecordedInput::Key {
                    key_code: KeyCode::Escape,
                    logical: bevy::input::keyboard::Key::Escape,
                    pressed: true,
                }],
            },
        ],
    };
    let text = recording.to_ron().unwrap();
    assert_eq!(Recording::from_ron(&text).unwrap(), recording);

    let path = std::env::temp_dir().join("slotted-replay-test.ron");
    std::fs::write(&path, text).unwrap();

    let (mut replayed, _) = open_chest();
    let report = replayed.replay(&path).unwrap();
    std::fs::remove_file(&path).ok();
    assert_eq!(report.inputs, 4);
    replayed.settle();
    // The click picked the stack up; `Esc` closed the screen with it, which is
    // the interpreter's business, not the replay's. What matters here is that
    // every input arrived.
    assert!(report.frames >= 5);
}

/// A recording from a newer format is refused rather than half-applied.
#[test]
fn a_newer_recording_is_refused() {
    let (mut h, _) = open_chest();
    let recording = Recording {
        version: slotted_ui::RECORDING_VERSION + 1,
        resolution: Vec2::new(WIDTH, HEIGHT),
        scale_factor: 1.0,
        frame_delta_us: 16_667,
        frames: Vec::new(),
    };
    assert!(matches!(
        h.replay_recording(&recording),
        Err(slotted_test::ReplayError::Version(_))
    ));
}

fn frame(n: u64, input: RecordedInput) -> RecordedFrame {
    RecordedFrame {
        frame: n,
        inputs: vec![input],
    }
}
