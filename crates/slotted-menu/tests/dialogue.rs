//! The dialogue runner and its screen, headless (menus M3 contract 3.3 and
//! 4.6). B owns the runner half (`// M3-TEST: B`), C the screen half
//! (`// M3-TEST: C`); both share the helpers at the top.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::float_cmp, dead_code)]

use std::path::Path;

use bevy::asset::AssetPlugin;
use bevy::prelude::*;
use pretty_assertions::assert_eq;
use slotted_menu::{
    DialogueChoice, DialogueConfig, DialogueEnded, DialogueNodeEntered, MenuConfig, MenuPlugin,
    kinds,
};
use slotted_test::prelude::*;

/// The sample of contract 3.1.
const GREETING: &str = r#"
#![enable(implicit_some)]
(
    id: "demo:greeting",
    start: "hello",
    nodes: {
        "hello": say(speaker: "demo-elder", portrait: (image: "portraits/elder.png"),
                     text: "demo-greeting-hello", next: "ask"),
        "ask": choice(prompt: "demo-greeting-ask", options: [
            (id: "yes", text: "demo-yes", next: "yes"),
            (id: "no", text: "demo-no", next: "bye"),
            (id: "secret", text: "demo-secret", next: "secret", enabled_if: "found_key"),
        ]),
        "yes": say(speaker: "demo-elder", text: "demo-greeting-yes", args: {"name": "Traveller"},
                   next: "bye"),
        "secret": say(text: "demo-greeting-secret", next: "bye"),
        "bye": end,
    },
)
"#;

/// The messages the runner wrote, for the tests that count.
#[derive(Resource, Default)]
struct Seen {
    entered: Vec<DialogueNodeEntered>,
    chosen: Vec<DialogueChoice>,
    ended: Vec<DialogueEnded>,
}

fn record(
    mut seen: ResMut<Seen>,
    mut entered: MessageReader<DialogueNodeEntered>,
    mut chosen: MessageReader<DialogueChoice>,
    mut ended: MessageReader<DialogueEnded>,
) {
    seen.entered.extend(entered.read().cloned());
    seen.chosen.extend(chosen.read().cloned());
    seen.ended.extend(ended.read().cloned());
}

fn add_menu(app: &mut App) {
    if !app.is_plugin_added::<MenuPlugin>() {
        app.add_plugins(MenuPlugin);
    }
    app.insert_resource(MenuConfig::default())
        .init_resource::<Seen>()
        .add_systems(Last, record);
}

/// The workspace `assets/` directory.
fn assets_dir() -> String {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets")
        .canonicalize()
        .expect("the workspace assets directory exists")
        .to_string_lossy()
        .into_owned()
}

fn harness_in(theme: &str) -> UiHarness {
    let mut h = UiHarness::builder()
        .plugins((
            SlottedPlugins::headless().set(AssetPlugin {
                file_path: assets_dir(),
                ..default()
            }),
            add_menu,
        ))
        .resolution(1280.0, 720.0)
        .theme(theme)
        .build();
    wait_for_theme(&mut h);
    h
}

fn harness() -> UiHarness {
    harness_in("glass")
}

fn wait_for_theme(h: &mut UiHarness) {
    for _ in 0..600 {
        let world = h.world();
        let active = world.resource::<slotted_theme::ActiveTheme>().0.clone();
        if world
            .resource::<Assets<slotted_theme::Theme>>()
            .get(&active)
            .is_some()
        {
            h.settle();
            return;
        }
        h.step(1);
    }
    panic!("the theme never loaded; is the asset root right?");
}

// ---------------------------------------------------------------------------
// Skeleton
// ---------------------------------------------------------------------------

#[test]
fn the_dialogue_template_registers_and_the_config_defaults_to_it() {
    let h = harness();
    let world = h.world();
    let screens = world.resource::<slotted_ui::Screens>();
    let def = screens.get(&kinds::dialogue()).expect("registered");
    assert_eq!(def.presentation.mode, slotted_ui::PresentationMode::Overlay);
    assert!(def.presentation.takes_focus());
    let config = world.resource::<DialogueConfig>();
    assert_eq!(config.kind, kinds::dialogue());
    assert!(!config.back_cancels);
    assert!(config.history);
}

// M3-TEST: B (the runner half, contract 3.3)

// M3-TEST: C (the screen half, contract 4.6)
