//! The dialogue helpers (menus M3 contract 5) over a dialogue registered by
//! hand: no asset, no example, the `slotted:dialogue` template as shipped.
//! The menus example's `tests/flow.rs` walks the same helpers by gamepad
//! over its own conversation.

#![allow(clippy::unwrap_used)]

use std::path::Path;
use std::time::Duration;

use bevy::asset::AssetPlugin;
use bevy::prelude::*;
use pretty_assertions::assert_eq;
use slotted::menu::{
    Dialogue, DialogueChoice, DialogueEnded, DialogueId, DialogueNodeEntered, Dialogues, EndReason,
    NodeId, kinds,
};
use slotted::ui::Value;
use slotted_test::prelude::*;

/// Two lines, a gated choice, a narration line and an end. The keys have
/// no catalogue, so they draw as written.
const SMALL: &str = r#"
(
    id: "test:small",
    start: "one",
    nodes: {
        "one": say(speaker: "Guide", text: "First [b]line[/b]", next: "two"),
        "two": choice(prompt: "Well?", options: [
            (id: "go", text: "Go on", next: "three"),
            (id: "locked", text: "Locked", next: "three", enabled_if: "flag"),
            (id: "stop", text: "Stop", next: None),
        ]),
        "three": say(text: "Narration", next: None),
    },
)
"#;

fn assets_dir() -> String {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets")
        .canonicalize()
        .expect("the workspace assets directory exists")
        .to_string_lossy()
        .into_owned()
}

fn harness() -> UiHarness {
    let mut h = UiHarness::builder()
        .plugins(SlottedPlugins::headless().set(AssetPlugin {
            file_path: assets_dir(),
            ..default()
        }))
        .resolution(1280.0, 720.0)
        .theme("glass")
        .build();
    for _ in 0..600 {
        let world = h.world();
        let active = world.resource::<slotted::theme::ActiveTheme>().0.clone();
        if world
            .resource::<Assets<slotted::theme::Theme>>()
            .get(&active)
            .is_some()
        {
            break;
        }
        h.step(1);
    }
    h.settle();
    h.world_mut()
        .resource_mut::<Dialogues>()
        .register(Dialogue::from_ron(SMALL).unwrap());
    h
}

fn id() -> DialogueId {
    DialogueId::new("test:small")
}

fn node(s: &str) -> NodeId {
    NodeId::new(s)
}

#[test]
fn the_helpers_read_and_drive_a_dialogue_registered_by_hand() {
    let mut h = harness();
    assert_eq!(h.dialogue(), None);
    assert_eq!(h.dialogue_events(), vec![]);

    // An unknown id warns and starts nothing.
    h.start_dialogue("test:missing");
    assert_eq!(h.dialogue(), None);
    assert_eq!(h.stack(), vec![]);

    h.start_dialogue("test:small");
    assert_eq!(h.dialogue(), Some((id(), node("one"))));
    assert_eq!(h.stack(), vec![kinds::dialogue()]);
    assert_eq!(
        h.stack_top(),
        None,
        "an overlay is never the non-overlay top"
    );
    assert!(!h.dialogue_revealed());
    assert_eq!(h.dialogue_text(), "", "nothing is typed on the first frame");
    assert_eq!(
        h.dialogue_events(),
        vec![DialogueEvent::Entered(DialogueNodeEntered {
            dialogue: id(),
            node: node("one"),
        })]
    );
    assert_eq!(
        h.dialogue_history(),
        vec![(Some("Guide".to_owned()), "First [b]line[/b]".to_owned())],
        "the history records the resolved markup as the node is entered"
    );

    // The typewriter, then Accept shows the rest: the markup is rendered,
    // not shown.
    h.advance(Duration::from_millis(100));
    let partial = h.dialogue_text();
    assert!(
        !partial.is_empty() && partial.len() < "First line".len(),
        "typing: {partial:?}"
    );
    h.dialogue_advance();
    assert!(h.dialogue_revealed());
    assert_eq!(h.dialogue_text(), "First line");
    assert_eq!(h.dialogue(), Some((id(), node("one"))));
    assert_eq!(h.dialogue_options(), vec![]);

    // Accept again: the choice, its buttons after `choice_delay`.
    h.dialogue_advance();
    assert_eq!(h.dialogue(), Some((id(), node("two"))));
    assert!(h.dialogue_revealed(), "a prompt does not type");
    assert_eq!(h.dialogue_text(), "Well?");
    h.advance(Duration::from_millis(400));
    h.settle();
    assert_eq!(
        h.dialogue_options(),
        vec![
            ("go".to_owned(), true),
            ("locked".to_owned(), false),
            ("stop".to_owned(), true),
        ]
    );
    assert_eq!(
        h.dialogue_history().len(),
        1,
        "a prompt is not a line of the transcript"
    );

    // A disabled option is refused by the runner; the flag enables it for
    // the next choice, not this one (the buttons do not re-present).
    h.dialogue_choose("locked");
    assert_eq!(h.dialogue(), Some((id(), node("two"))));
    assert_eq!(h.dialogue_events().len(), 1, "only `two` was entered");
    h.set_value("flag", Value::Bool(true));
    h.settle();
    assert_eq!(h.dialogue_options()[1], ("locked".to_owned(), false));

    h.dialogue_choose("go");
    assert_eq!(h.dialogue(), Some((id(), node("three"))));
    assert_eq!(
        h.dialogue_events(),
        vec![
            DialogueEvent::Chosen(DialogueChoice {
                dialogue: id(),
                node: node("two"),
                option: "go".to_owned(),
                index: 0,
            }),
            DialogueEvent::Entered(DialogueNodeEntered {
                dialogue: id(),
                node: node("three"),
            }),
        ]
    );
    assert_eq!(h.dialogue_options(), vec![]);
    assert_eq!(
        h.dialogue_history().last(),
        Some(&(None, "Narration".to_owned()))
    );

    // Reveal, then `next: None` ends it and closes the screen.
    h.dialogue_advance();
    assert!(h.dialogue_revealed());
    h.dialogue_advance();
    assert_eq!(h.dialogue(), None);
    assert_eq!(h.stack(), vec![]);
    assert_eq!(
        h.dialogue_events(),
        vec![DialogueEvent::Ended(DialogueEnded {
            dialogue: id(),
            node: node("three"),
            reason: EndReason::Finished,
        })]
    );
}

#[test]
fn a_flag_set_before_the_choice_enables_its_option_and_stop_ends_early() {
    let mut h = harness();
    h.set_value("flag", Value::Bool(true));
    h.start_dialogue("test:small");
    h.dialogue_advance();
    h.dialogue_advance();
    h.advance(Duration::from_millis(400));
    h.settle();
    assert_eq!(
        h.dialogue_options(),
        vec![
            ("go".to_owned(), true),
            ("locked".to_owned(), true),
            ("stop".to_owned(), true),
        ]
    );
    h.dialogue_events();
    h.dialogue_choose("stop");
    assert_eq!(h.dialogue(), None, "`next: None` on an option finishes");
    let events = h.dialogue_events();
    assert!(matches!(
        events.last(),
        Some(DialogueEvent::Ended(DialogueEnded {
            reason: EndReason::Finished,
            ..
        }))
    ));
    assert!(matches!(
        events.first(),
        Some(DialogueEvent::Chosen(DialogueChoice { index: 2, .. }))
    ));
}
