//! The dialogue runner and its screen, headless (menus M3 contract 3.3 and
//! 4.6). B owns the runner half (`// M3-TEST: B`), C the screen half
//! (`// M3-TEST: C`); both share the helpers at the top.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::float_cmp, dead_code)]

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use bevy::asset::AssetPlugin;
use bevy::input::gamepad::GamepadButton;
use bevy::prelude::*;
use pretty_assertions::assert_eq;
use slotted_menu::{
    ActiveDialogue, Condition, Dialogue, DialogueAssets, DialogueChoice, DialogueConfig,
    DialogueEnded, DialogueError, DialogueId, DialogueNode, DialogueNodeEntered, DialogueScreen,
    Dialogues, EndReason, HistoryLine, MenuChoice, MenuConfig, MenuPlugin, NodeId,
    advance_dialogue, choose_dialogue, end_dialogue, jump_dialogue, kinds, start_dialogue,
    start_dialogue_with,
};
use slotted_test::prelude::*;
use slotted_ui::{LocArgs, LocKey, UiAction, Value, ValueStore};

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
    harness_with(theme, |_| {})
}

/// A harness with `extra` applied before the plugin group, so it can
/// register an asset source or add a system of the game's.
fn harness_with(theme: &str, extra: impl Fn(&mut App) + Send + Sync + 'static) -> UiHarness {
    let mut h = UiHarness::builder()
        .plugins((
            extra,
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

// ---------------------------------------------------------------------------
// B: the model
// ---------------------------------------------------------------------------

fn node(id: &str) -> NodeId {
    NodeId::new(id)
}

fn greeting() -> Dialogue {
    Dialogue::from_ron(GREETING).expect("the sample parses")
}

#[test]
fn from_ron_parses_the_sample() {
    let d = greeting();
    assert_eq!(d.id, DialogueId::new("demo:greeting"));
    assert_eq!(d.start, node("hello"));
    assert_eq!(d.nodes.len(), 5);
    let Some(DialogueNode::Say {
        speaker,
        portrait,
        text,
        args,
        next,
    }) = d.node(&node("hello"))
    else {
        panic!("hello is a line");
    };
    assert_eq!(speaker.as_ref().map(|k| k.0.as_str()), Some("demo-elder"));
    assert_eq!(
        portrait,
        &Some(slotted_ui::IconDef::Image("portraits/elder.png".to_owned()))
    );
    assert_eq!(text.0, "demo-greeting-hello");
    assert!(args.is_empty());
    assert_eq!(next, &Some(node("ask")));
    let Some(DialogueNode::Choice { prompt, options }) = d.node(&node("ask")) else {
        panic!("ask is a choice");
    };
    assert_eq!(
        prompt.as_ref().map(|k| k.0.as_str()),
        Some("demo-greeting-ask")
    );
    assert_eq!(options.len(), 3);
    assert_eq!(options[0].enabled_if, None);
    assert_eq!(
        options[2].enabled_if,
        Some(Condition {
            key: "found_key".to_owned(),
            negate: false
        })
    );
    let Some(DialogueNode::Say { args, .. }) = d.node(&node("yes")) else {
        panic!("yes is a line");
    };
    assert_eq!(args.get("name"), Some(&Value::Text("Traveller".to_owned())));
    assert_eq!(d.node(&node("bye")), Some(&DialogueNode::End));
    // Without the file-level attribute the loader's default still allows
    // the bare forms.
    let bare = GREETING.replace("#![enable(implicit_some)]", "");
    assert_eq!(Dialogue::from_ron(&bare).expect("implicit_some is on"), d);
}

#[test]
fn from_ron_rejects_a_dangling_next() {
    let text = r#"(id: "t", start: "a", nodes: {"a": say(text: "x", next: "nowhere")})"#;
    match Dialogue::from_ron(text) {
        Err(DialogueError::MissingNode { from, to }) => {
            assert_eq!(from, node("a"));
            assert_eq!(to, node("nowhere"));
        }
        other => panic!("expected MissingNode, got {other:?}"),
    }
    let text = r#"(id: "t", start: "a", nodes: {
        "a": choice(options: [(id: "x", text: "x", next: "gone")]),
    })"#;
    assert!(matches!(
        Dialogue::from_ron(text),
        Err(DialogueError::MissingNode { from, to }) if from == node("a") && to == node("gone")
    ));
}

#[test]
fn from_ron_rejects_a_missing_start() {
    let text = r#"(id: "t", start: "nope", nodes: {"a": end})"#;
    assert!(matches!(
        Dialogue::from_ron(text),
        Err(DialogueError::MissingStart(id)) if id == node("nope")
    ));
}

#[test]
fn from_ron_rejects_an_empty_choice() {
    let text = r#"(id: "t", start: "a", nodes: {"a": choice(options: [])})"#;
    assert!(matches!(
        Dialogue::from_ron(text),
        Err(DialogueError::EmptyChoice(id)) if id == node("a")
    ));
}

#[test]
fn from_ron_rejects_a_duplicate_option_id() {
    let text = r#"(id: "t", start: "a", nodes: {
        "a": choice(options: [(id: "x", text: "x"), (id: "x", text: "y")]),
    })"#;
    assert!(matches!(
        Dialogue::from_ron(text),
        Err(DialogueError::DuplicateOption { node: n, id }) if n == node("a") && id == "x"
    ));
}

#[test]
fn from_ron_rejects_a_bad_condition() {
    let text = r#"(id: "t", start: "a", nodes: {
        "a": choice(options: [(id: "x", text: "x", enabled_if: "!")]),
    })"#;
    // A `try_from` failure surfaces as RON's error, with the message inside.
    let err = Dialogue::from_ron(text).expect_err("bad condition");
    assert!(matches!(err, DialogueError::Ron(_)), "got {err:?}");
    assert!(
        err.to_string().contains("bad condition `!`"),
        "the message names the condition: {err}"
    );
}

#[test]
fn condition_parses_both_forms_and_reads_truthiness() {
    assert_eq!(
        Condition::parse("met_elder").unwrap(),
        Condition {
            key: "met_elder".to_owned(),
            negate: false
        }
    );
    assert_eq!(
        Condition::parse("!met_elder").unwrap(),
        Condition {
            key: "met_elder".to_owned(),
            negate: true
        }
    );
    assert_eq!(
        Condition::parse(" ! spaced ").unwrap(),
        Condition {
            key: "spaced".to_owned(),
            negate: true
        }
    );
    for bad in ["", "!", "  ", "two words", "!!x"] {
        assert!(
            matches!(Condition::parse(bad), Err(DialogueError::BadCondition(_))),
            "{bad:?} is not a condition"
        );
    }
    assert_eq!(String::from(Condition::parse("!k").unwrap()), "!k");

    let mut values = ValueStore::default();
    values.insert("yes", true);
    values.insert("no", false);
    values.insert("one", 1i64);
    values.insert("zero", 0i64);
    values.insert("half", 0.5f64);
    values.insert("nil", 0.0f64);
    values.insert("word", "high");
    values.insert("blank", "");
    let holds = |c: &str| Condition::parse(c).unwrap().holds(&values);
    assert!(holds("yes"));
    assert!(!holds("no"));
    assert!(holds("one"));
    assert!(!holds("zero"));
    assert!(holds("half"));
    assert!(!holds("nil"));
    assert!(holds("word"));
    assert!(!holds("blank"));
    assert!(!holds("missing"), "a missing key is false");
    assert!(holds("!missing"));
    assert!(holds("!no"));
    assert!(!holds("!yes"));
}

// ---------------------------------------------------------------------------
// B: the runner
// ---------------------------------------------------------------------------

fn active(h: &UiHarness) -> Option<ActiveDialogue> {
    h.world().get_resource::<ActiveDialogue>().cloned()
}

fn current_node(h: &UiHarness) -> Option<NodeId> {
    active(h).map(|a| a.node)
}

/// Queues `f` as a command and applies it.
fn run(h: &mut UiHarness, f: impl FnOnce(&mut Commands) + Send + 'static) {
    let world = h.world_mut();
    let mut commands = world.commands();
    f(&mut commands);
    world.flush();
}

fn register_greeting(h: &mut UiHarness) -> Arc<Dialogue> {
    h.world_mut()
        .resource_mut::<Dialogues>()
        .register(greeting())
}

/// Registers the sample and starts it; one frame so the messages are seen.
fn start_greeting(h: &mut UiHarness) -> ActiveDialogue {
    register_greeting(h);
    run(h, |c| start_dialogue(c, DialogueId::new("demo:greeting")));
    h.settle();
    active(h).expect("the dialogue runs")
}

fn entered(h: &UiHarness) -> Vec<String> {
    h.world()
        .resource::<Seen>()
        .entered
        .iter()
        .map(|e| e.node.0.clone())
        .collect()
}

fn ended(h: &UiHarness) -> Vec<(String, EndReason)> {
    h.world()
        .resource::<Seen>()
        .ended
        .iter()
        .map(|e| (e.node.0.clone(), e.reason))
        .collect()
}

#[test]
fn a_registered_dialogue_starts_as_an_overlay_and_enters_its_nodes() {
    let mut h = harness();
    let a = start_greeting(&mut h);
    assert_eq!(a.dialogue.id, DialogueId::new("demo:greeting"));
    assert_eq!(a.node, node("hello"));
    assert!(!a.revealed);
    assert_eq!(
        a.history,
        vec![HistoryLine {
            speaker: Some("demo-elder".to_owned()),
            text: "demo-greeting-hello".to_owned(),
        }],
        "the line is resolved through the catalogue when it is entered"
    );
    assert_eq!(h.stack(), vec![kinds::dialogue()]);
    assert_eq!(h.stack_top(), None, "an overlay is never the plain top");
    let world = h.world();
    let stack = world.resource::<slotted_ui::ScreenStack>();
    assert_eq!(
        stack.focus_top().map(|e| e.root),
        Some(a.root),
        "the dialogue takes focus"
    );
    assert!(
        world.get::<DialogueScreen>(a.root).is_some(),
        "the root carries DialogueScreen"
    );
    assert_eq!(entered(&h), vec!["hello"]);
    assert!(h.world().resource::<Seen>().ended.is_empty());

    // Every node fires `DialogueNodeEntered` as it is walked.
    run(&mut h, advance_dialogue);
    run(&mut h, advance_dialogue);
    run(&mut h, |c| choose_dialogue(c, "no"));
    h.settle();
    assert_eq!(entered(&h), vec!["hello", "ask", "bye"]);
}

#[test]
fn starting_an_unknown_dialogue_does_nothing() {
    let mut h = harness();
    run(&mut h, |c| {
        start_dialogue(c, DialogueId::new("demo:missing"));
    });
    h.settle();
    assert_eq!(active(&h), None);
    assert_eq!(h.stack(), vec![]);
}

#[test]
fn start_dialogue_with_runs_an_unregistered_dialogue() {
    let mut h = harness();
    run(&mut h, |c| start_dialogue_with(c, Arc::new(greeting())));
    h.settle();
    assert_eq!(current_node(&h), Some(node("hello")));
    assert_eq!(h.stack(), vec![kinds::dialogue()]);
}

#[test]
fn advance_reveals_then_advances() {
    let mut h = harness();
    start_greeting(&mut h);
    assert!(!active(&h).unwrap().revealed);
    run(&mut h, advance_dialogue);
    let a = active(&h).unwrap();
    assert!(a.revealed, "the first advance reveals");
    assert_eq!(a.node, node("hello"), "and stays on the line");
    run(&mut h, advance_dialogue);
    let a = active(&h).unwrap();
    assert_eq!(a.node, node("ask"), "the second advance moves on");
    assert!(a.revealed, "a choice needs no reveal");
    assert_eq!(a.history.len(), 1, "a choice is not a history line");
    // On a choice `advance` is a no-op.
    run(&mut h, advance_dialogue);
    assert_eq!(current_node(&h), Some(node("ask")));
    h.settle();
    assert_eq!(entered(&h), vec!["hello", "ask"]);
}

#[test]
fn accept_advances_the_line_and_claims_while_the_dialogue_is_the_focus_top() {
    let mut h = harness();
    start_greeting(&mut h);
    h.action(UiAction::Accept);
    h.settle();
    let a = active(&h).unwrap();
    assert!(a.revealed, "Accept reveals");
    assert_eq!(a.node, node("hello"));
    h.action(UiAction::Accept);
    h.settle();
    assert_eq!(current_node(&h), Some(node("ask")), "Accept advances");
    // On a choice the runner leaves Accept to the option buttons.
    h.action(UiAction::Accept);
    h.settle();
    assert_eq!(current_node(&h), Some(node("ask")));
    assert_eq!(h.stack(), vec![kinds::dialogue()]);
}

#[test]
fn choose_writes_the_choice_and_follows_next() {
    let mut h = harness();
    start_greeting(&mut h);
    run(&mut h, |c| jump_dialogue(c, node("ask")));
    let a = active(&h).unwrap();
    let options: Vec<(usize, String, bool)> = a
        .options(h.world().get_resource::<ValueStore>())
        .into_iter()
        .map(|(i, o, enabled)| (i, o.id.clone(), enabled))
        .collect();
    assert_eq!(
        options,
        vec![
            (0, "yes".to_owned(), true),
            (1, "no".to_owned(), true),
            (2, "secret".to_owned(), false),
        ]
    );
    run(&mut h, |c| choose_dialogue(c, "yes"));
    let a = active(&h).unwrap();
    assert_eq!(a.node, node("yes"));
    assert!(!a.revealed);
    assert_eq!(
        a.history.last(),
        Some(&HistoryLine {
            speaker: Some("demo-elder".to_owned()),
            text: "demo-greeting-yes".to_owned(),
        })
    );
    h.settle();
    assert_eq!(
        h.world().resource::<Seen>().chosen,
        vec![DialogueChoice {
            dialogue: DialogueId::new("demo:greeting"),
            node: node("ask"),
            option: "yes".to_owned(),
            index: 0,
        }]
    );
    assert_eq!(entered(&h), vec!["hello", "ask", "yes"]);
}

#[test]
fn a_disabled_option_is_a_no_op_until_its_condition_holds() {
    let mut h = harness();
    start_greeting(&mut h);
    run(&mut h, |c| jump_dialogue(c, node("ask")));
    run(&mut h, |c| choose_dialogue(c, "secret"));
    assert_eq!(
        current_node(&h),
        Some(node("ask")),
        "disabled: nothing happens"
    );
    run(&mut h, |c| choose_dialogue(c, "nope"));
    assert_eq!(
        current_node(&h),
        Some(node("ask")),
        "unknown: nothing happens"
    );
    h.settle();
    assert!(h.world().resource::<Seen>().chosen.is_empty());

    h.world_mut()
        .resource_mut::<ValueStore>()
        .insert("found_key", true);
    let a = active(&h).unwrap();
    assert!(
        a.options(h.world().get_resource::<ValueStore>())[2].2,
        "the store enables it"
    );
    run(&mut h, |c| choose_dialogue(c, "secret"));
    let a = active(&h).unwrap();
    assert_eq!(a.node, node("secret"));
    assert_eq!(
        a.history.last(),
        Some(&HistoryLine {
            speaker: None,
            text: "demo-greeting-secret".to_owned(),
        }),
        "narration has no speaker"
    );
    h.settle();
    assert_eq!(h.world().resource::<Seen>().chosen.len(), 1);
}

#[test]
fn choose_on_a_line_is_a_no_op() {
    let mut h = harness();
    start_greeting(&mut h);
    run(&mut h, |c| choose_dialogue(c, "yes"));
    assert_eq!(current_node(&h), Some(node("hello")));
    h.settle();
    assert!(h.world().resource::<Seen>().chosen.is_empty());
}

/// A game answering `DialogueNodeEntered` on `ask` by jumping to `secret`.
fn answer_ask_with_secret(mut entered: MessageReader<DialogueNodeEntered>, mut commands: Commands) {
    for event in entered.read() {
        if event.node == NodeId::new("ask") {
            jump_dialogue(&mut commands, NodeId::new("secret"));
        }
    }
}

#[test]
fn jump_from_a_message_answer_redirects() {
    let mut h = harness_with("glass", |app| {
        app.add_systems(Update, answer_ask_with_secret);
    });
    start_greeting(&mut h);
    run(&mut h, advance_dialogue);
    run(&mut h, advance_dialogue);
    h.settle();
    let a = active(&h).unwrap();
    assert_eq!(
        a.node,
        node("secret"),
        "the game's answer redirected the choice"
    );
    assert_eq!(entered(&h), vec!["hello", "ask", "secret"]);
    assert_eq!(a.history.len(), 2);
    // A jump to a node that does not exist is a warning, not a move.
    run(&mut h, |c| jump_dialogue(c, node("nowhere")));
    assert_eq!(current_node(&h), Some(node("secret")));
}

#[test]
fn end_cancels() {
    let mut h = harness();
    start_greeting(&mut h);
    run(&mut h, end_dialogue);
    h.settle();
    assert_eq!(active(&h), None);
    assert_eq!(h.stack(), vec![], "the screen closed");
    assert_eq!(ended(&h), vec![("hello".to_owned(), EndReason::Cancelled)]);
    // Ending nothing is a no-op.
    run(&mut h, end_dialogue);
    h.settle();
    assert_eq!(ended(&h).len(), 1);
}

#[test]
fn starting_over_a_running_dialogue_replaces_it() {
    let mut h = harness();
    start_greeting(&mut h);
    let first_root = active(&h).unwrap().root;
    let mut other = greeting();
    other.id = DialogueId::new("demo:other");
    other.start = node("secret");
    run(&mut h, move |c| start_dialogue_with(c, Arc::new(other)));
    h.settle();
    let a = active(&h).unwrap();
    assert_eq!(a.dialogue.id, DialogueId::new("demo:other"));
    assert_eq!(a.node, node("secret"));
    assert_ne!(a.root, first_root, "a fresh screen");
    assert!(
        h.world().get_entity(first_root).is_err(),
        "the old screen is gone"
    );
    assert_eq!(h.stack(), vec![kinds::dialogue()], "one dialogue at a time");
    assert_eq!(
        h.world().resource::<Seen>().ended,
        vec![DialogueEnded {
            dialogue: DialogueId::new("demo:greeting"),
            node: node("hello"),
            reason: EndReason::Replaced,
        }]
    );
}

#[test]
fn an_end_node_and_a_missing_next_both_finish_and_close_the_screen() {
    // Through the `end` node.
    let mut h = harness();
    start_greeting(&mut h);
    run(&mut h, |c| jump_dialogue(c, node("ask")));
    run(&mut h, |c| choose_dialogue(c, "no"));
    h.settle();
    assert_eq!(active(&h), None);
    assert_eq!(h.stack(), vec![]);
    assert_eq!(
        entered(&h),
        vec!["hello", "ask", "bye"],
        "the end node is entered"
    );
    assert_eq!(ended(&h), vec![("bye".to_owned(), EndReason::Finished)]);

    // Through `next: None` on a line.
    let mut h = harness();
    let mut short = greeting();
    short.nodes.insert(
        node("hello"),
        DialogueNode::Say {
            speaker: None,
            portrait: None,
            text: LocKey("demo-greeting-hello".to_owned()),
            args: LocArgs::new(),
            next: None,
        },
    );
    run(&mut h, move |c| start_dialogue_with(c, Arc::new(short)));
    run(&mut h, advance_dialogue);
    assert_eq!(
        current_node(&h),
        Some(node("hello")),
        "revealed, still here"
    );
    run(&mut h, advance_dialogue);
    h.settle();
    assert_eq!(active(&h), None);
    assert_eq!(h.stack(), vec![]);
    assert_eq!(ended(&h), vec![("hello".to_owned(), EndReason::Finished)]);

    // Through an option with no `next`.
    let mut h = harness();
    let mut short = greeting();
    if let Some(DialogueNode::Choice { options, .. }) = short.nodes.get_mut(&node("ask")) {
        options[1].next = None;
    }
    run(&mut h, move |c| start_dialogue_with(c, Arc::new(short)));
    run(&mut h, |c| jump_dialogue(c, node("ask")));
    run(&mut h, |c| choose_dialogue(c, "no"));
    h.settle();
    assert_eq!(active(&h), None);
    assert_eq!(ended(&h), vec![("ask".to_owned(), EndReason::Finished)]);
    assert_eq!(h.world().resource::<Seen>().chosen.len(), 1);
}

#[test]
fn clear_screens_cancels() {
    let mut h = harness();
    start_greeting(&mut h);
    run(&mut h, slotted_ui::clear_screens);
    h.settle();
    assert_eq!(active(&h), None);
    assert_eq!(h.stack(), vec![]);
    assert_eq!(ended(&h), vec![("hello".to_owned(), EndReason::Cancelled)]);
    // Closing the root directly does the same.
    let a = start_greeting(&mut h);
    let root = a.root;
    run(&mut h, move |c| slotted_ui::close_screen(c, root));
    h.settle();
    assert_eq!(active(&h), None);
    assert_eq!(ended(&h).len(), 2);
    assert_eq!(ended(&h)[1], ("hello".to_owned(), EndReason::Cancelled));
}

#[test]
fn back_leaves_an_overlay_alone_unless_it_cancels() {
    let mut h = harness();
    start_greeting(&mut h);
    // A pure Back (B on the pad) is nobody's: the runner leaves it alone by
    // default, and nothing pops an overlay.
    h.gamepad(GamepadButton::East);
    h.settle();
    assert_eq!(current_node(&h), Some(node("hello")));
    assert_eq!(h.stack(), vec![kinds::dialogue()]);
    // Escape is Back and Menu: with Back unowned, Menu over an overlay
    // pauses (contract 2.1), and the dialogue stays underneath.
    h.action(UiAction::Back);
    h.settle();
    assert_eq!(h.stack(), vec![kinds::dialogue(), kinds::pause()]);
    assert_eq!(current_node(&h), Some(node("hello")));

    let mut h = harness();
    h.world_mut().resource_mut::<DialogueConfig>().back_cancels = true;
    start_greeting(&mut h);
    h.action(UiAction::Back);
    h.settle();
    assert_eq!(active(&h), None, "Back cancels when configured");
    assert_eq!(h.stack(), vec![], "and the claim keeps Escape from pausing");
    assert_eq!(ended(&h), vec![("hello".to_owned(), EndReason::Cancelled)]);
}

#[test]
fn a_modal_pushed_above_silences_accept_and_popping_it_restores() {
    let mut h = harness();
    start_greeting(&mut h);
    run(&mut h, |c| {
        c.queue(|world: &mut World| {
            let def = world
                .resource::<slotted_ui::Screens>()
                .get(&kinds::pause())
                .expect("the pause template is registered")
                .clone();
            slotted_ui::push_screen(&mut world.commands(), def, None);
        });
    });
    h.settle();
    assert_eq!(h.stack(), vec![kinds::dialogue(), kinds::pause()]);
    let dialogue_root = active(&h).unwrap().root;
    assert_ne!(
        h.world()
            .resource::<slotted_ui::ScreenStack>()
            .focus_top()
            .map(|e| e.root),
        Some(dialogue_root),
        "the modal took the focus top"
    );
    // The typewriter (C) reveals the line on its own with time, so the
    // proof that the runner ignored Accept is the node, not `revealed`: a
    // revealed line that heard Accept would have moved on to `ask`.
    h.advance(Duration::from_secs(1));
    assert!(active(&h).unwrap().revealed, "the line typed out meanwhile");
    h.action(UiAction::Accept);
    h.settle();
    let a = active(&h).unwrap();
    assert_eq!(
        a.node,
        node("hello"),
        "the runner ignored Accept under a modal"
    );
    // The pause may have answered that Accept with its focused button; make
    // sure it is gone either way, then the runner listens again.
    if h.stack().contains(&kinds::pause()) {
        run(&mut h, slotted_ui::pop_screen);
        h.settle();
    }
    assert_eq!(h.stack(), vec![kinds::dialogue()]);
    assert_eq!(
        h.world()
            .resource::<slotted_ui::ScreenStack>()
            .focus_top()
            .map(|e| e.root),
        Some(dialogue_root),
        "the dialogue is the focus top again"
    );
    h.action(UiAction::Accept);
    h.settle();
    assert_eq!(
        current_node(&h),
        Some(node("ask")),
        "Accept reaches the runner again"
    );
}

/// A second asset source rooted at `tests/fixtures`, so the theme still
/// loads from the workspace `assets/` while the dialogue file comes from
/// the crate's own directory.
fn fixtures_source(app: &mut App) {
    use bevy::asset::AssetApp;
    use bevy::asset::io::{AssetSource, AssetSourceBuilder};
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .canonicalize()
        .expect("the fixtures directory exists")
        .to_string_lossy()
        .into_owned();
    app.register_asset_source(
        "fixtures",
        AssetSourceBuilder::new(AssetSource::get_default_reader(dir)),
    );
}

#[test]
fn the_asset_loader_round_trips_and_apply_dialogue_assets_registers() {
    let mut h = harness_with("glass", fixtures_source);
    let id = DialogueId::new("demo:greeting");
    assert_eq!(h.world().resource::<Dialogues>().get(&id), None);
    run(&mut h, |c| {
        c.queue(|world: &mut World| {
            let server = world.resource::<AssetServer>().clone();
            world
                .resource_mut::<DialogueAssets>()
                .load(&server, "fixtures://dialogue/greeting.dialogue.ron");
        });
    });
    assert_eq!(
        h.world().resource::<DialogueAssets>().0.len(),
        1,
        "the handle is kept"
    );
    let mut loaded = None;
    for _ in 0..600 {
        if let Some(d) = h.world().resource::<Dialogues>().get(&id) {
            loaded = Some(d);
            break;
        }
        h.step(1);
    }
    let loaded = loaded.expect("the dialogue asset loads and registers");
    assert_eq!(
        *loaded,
        greeting(),
        "the file is the sample of contract 3.1"
    );
    assert_eq!(h.world().resource::<Dialogues>().ids(), vec![id.clone()]);

    // The registered dialogue runs like a hand-registered one.
    run(&mut h, move |c| start_dialogue(c, id));
    h.settle();
    assert_eq!(current_node(&h), Some(node("hello")));
    assert!(Arc::ptr_eq(&active(&h).unwrap().dialogue, &loaded));

    // A modified asset registers anew, and the running dialogue keeps its
    // old `Arc` until it starts again.
    let handle = h.world().resource::<DialogueAssets>().0[0].clone();
    {
        let mut assets = h.world_mut().resource_mut::<Assets<Dialogue>>();
        let mut d = assets.get_mut(&handle).expect("loaded");
        d.start = NodeId::new("secret");
    }
    // `Modified` is written at the end of the frame and read the next.
    let mut registered = None;
    for _ in 0..10 {
        h.step(1);
        let d = h
            .world()
            .resource::<Dialogues>()
            .get(&DialogueId::new("demo:greeting"))
            .unwrap();
        if d.start == node("secret") {
            registered = Some(d);
            break;
        }
    }
    let registered = registered.expect("the modified asset registers");
    assert!(!Arc::ptr_eq(&registered, &loaded), "a new Arc");
    let running = active(&h).unwrap();
    assert!(
        Arc::ptr_eq(&running.dialogue, &loaded),
        "the run keeps its Arc"
    );
    assert_eq!(running.dialogue.start, node("hello"));
    run(&mut h, |c| {
        start_dialogue(c, DialogueId::new("demo:greeting"));
    });
    h.settle();
    assert_eq!(
        current_node(&h),
        Some(node("secret")),
        "the next start takes the edit"
    );
}

// M3-TEST: C (the screen half, contract 4.6)

// ---------------------------------------------------------------------------
// M3-TEST: D
// ---------------------------------------------------------------------------

/// A game starting a dialogue from a `MenuChoice` (the menus example's Talk
/// button), a frame after the choice (`PreUpdate`, so the order is not left
/// to the set). The runner reads `UiActionEvent`s through a reader that
/// persists across frames, so the frame it starts listening it must not see
/// the previous frame's `Accept`, the one that activated the button, as
/// fresh.
fn talk_on_resume(mut choices: MessageReader<MenuChoice>, mut commands: Commands) {
    for choice in choices.read() {
        if choice.id == "resume" {
            start_dialogue(&mut commands, DialogueId::new("demo:greeting"));
        }
    }
}

#[test]
fn the_accept_that_started_the_dialogue_does_not_skip_its_first_line() {
    let mut h = harness_with("glass", |app| {
        app.add_systems(PreUpdate, talk_on_resume);
    });
    register_greeting(&mut h);
    run(&mut h, |c| {
        c.queue(|world: &mut World| {
            let def = world
                .resource::<slotted_ui::Screens>()
                .get(&kinds::pause())
                .expect("the pause template is registered")
                .clone();
            slotted_ui::push_screen(&mut world.commands(), def, None);
        });
    });
    h.settle();
    assert_eq!(h.stack(), vec![kinds::pause()]);
    assert!(h.focused().is_some(), "the pause focuses Resume");
    // A pad, since a keyboard Enter activates on its release, a frame
    // later; not `settle()`, since the typewriter would finish the line
    // inside it.
    h.gamepad(GamepadButton::South);
    h.step(2);
    assert_eq!(h.stack(), vec![kinds::dialogue()]);
    let a = active(&h).expect("Resume started the dialogue");
    assert_eq!(a.node, node("hello"));
    assert!(
        !a.revealed,
        "the Accept that pressed Resume did not reach the runner"
    );
}

// ---------------------------------------------------------------------------
// M3-TEST: review round
// ---------------------------------------------------------------------------

/// A `Dialogue` built in code, not through `from_ron`, whose `start` names
/// no node: the panic `ActiveDialogue::current` would raise the next frame
/// is what `start_with`'s validation prevents.
#[test]
fn a_dialogue_whose_start_is_missing_never_opens_a_screen() {
    let mut h = harness();
    let broken = Dialogue {
        id: DialogueId::new("demo:broken"),
        start: node("nowhere"),
        nodes: BTreeMap::new(),
    };
    run(&mut h, move |c| start_dialogue_with(c, Arc::new(broken)));
    h.settle();
    assert!(active(&h).is_none(), "nothing runs");
    assert!(h.stack().is_empty(), "and no screen was pushed");
}

/// A jump back to the choice already showing rebuilds its buttons, so a
/// game that sets the value a condition reads can re-ask and see the option
/// unlock. Before the entry counter, neither the node nor the history
/// length changed and the screen skipped the rewrite.
#[test]
fn a_jump_back_to_the_current_choice_rebuilds_its_options() {
    let mut h = harness();
    run(&mut h, move |c| {
        start_dialogue_with(c, Arc::new(greeting()));
    });
    h.settle();
    run(&mut h, |c| jump_dialogue(c, node("ask")));
    h.settle();
    h.advance(Duration::from_millis(400));
    let locked = h.find(&by::test_id("option.secret"));
    assert!(
        h.world()
            .get::<slotted_ui::ButtonState>(locked)
            .is_some_and(|b| b.disabled),
        "the secret answer is locked while the flag is unset"
    );
    let entered = active(&h).expect("running").entered;

    h.set_value("found_key", true);
    run(&mut h, |c| jump_dialogue(c, node("ask")));
    h.settle();
    h.advance(Duration::from_millis(400));
    assert_eq!(
        active(&h).expect("running").entered,
        entered + 1,
        "the re-entry counted"
    );
    let unlocked = h.find(&by::test_id("option.secret"));
    assert!(
        !h.world()
            .get::<slotted_ui::ButtonState>(unlocked)
            .is_some_and(|b| b.disabled),
        "and the rebuilt button is live"
    );
}
