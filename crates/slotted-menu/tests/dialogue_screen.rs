//! The `slotted:dialogue` screen, headless (menus M3 contract 4.6): the
//! template, presenting a node, the typewriter on virtual time, the choices,
//! the history page, the hint entries and the tree snapshots, in three
//! themes. The runner itself is tested in `dialogue.rs` (B's file); the
//! helpers at the top are copied from there.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::float_cmp, dead_code)]

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use bevy::asset::AssetPlugin;
use bevy::input::gamepad::GamepadButton;
use bevy::prelude::*;
use pretty_assertions::assert_eq;
use slotted_menu::dialogue_screen::{DialogueChoicesPending, option_test_id, transcript};
use slotted_menu::{
    ActiveDialogue, Dialogue, DialogueChoice, DialogueConfig, DialogueEnded, DialogueId,
    DialogueLine, DialogueNodeEntered, DialogueOption, DialogueScreen, Dialogues, EndReason,
    HintEntry, HistoryLine, MenuConfig, MenuPlugin, NodeId, OPTION_TAG, kinds, start_dialogue,
};
use slotted_test::prelude::*;
use slotted_theme::Motion;
use slotted_ui::{RichReveal, SemanticRole, UiAction, Value};

const THEMES: [&str; 3] = ["glass", "paper", "neon"];

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
    harness_with(theme, None, |_| {})
}

/// A harness with `extra` applied before the plugin group and, with
/// `motion`, that motion policy.
fn harness_with(
    theme: &str,
    motion: Option<Motion>,
    extra: impl Fn(&mut App) + Send + Sync + 'static,
) -> UiHarness {
    let mut builder = UiHarness::builder()
        .plugins((
            extra,
            SlottedPlugins::headless().set(AssetPlugin {
                file_path: assets_dir(),
                ..default()
            }),
            add_menu,
        ))
        .resolution(1280.0, 720.0)
        .theme(theme);
    if let Some(motion) = motion {
        builder = builder.motion(motion);
    }
    let mut h = builder.build();
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

fn node(id: &str) -> NodeId {
    NodeId::new(id)
}

fn greeting() -> Dialogue {
    Dialogue::from_ron(GREETING).expect("the sample parses")
}

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

/// Registers the sample and starts it, then one frame: the screen is up and
/// the first line has been presented but not typed yet.
fn start_greeting(h: &mut UiHarness) -> ActiveDialogue {
    register_greeting(h);
    run(h, |c| start_dialogue(c, DialogueId::new("demo:greeting")));
    h.step(1);
    active(h).expect("the dialogue runs")
}

/// Starts the sample and walks it to the `ask` choice with its buttons up.
fn start_at_ask(h: &mut UiHarness) -> ActiveDialogue {
    start_greeting(h);
    h.advance(Duration::from_secs(1));
    assert!(active(h).unwrap().revealed);
    h.action(UiAction::Accept);
    h.step(1);
    assert_eq!(current_node(h), Some(node("ask")));
    h.advance(Duration::from_millis(400));
    h.settle();
    active(h).unwrap()
}

fn chosen(h: &UiHarness) -> Vec<(String, usize)> {
    h.world()
        .resource::<Seen>()
        .chosen
        .iter()
        .map(|c| (c.option.clone(), c.index))
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

fn dialogue_root(h: &UiHarness) -> Entity {
    active(h).expect("the dialogue runs").root
}

fn text_node(h: &UiHarness) -> Entity {
    h.find(&by::test_id("text").within(dialogue_root(h)))
}

fn reveal(h: &UiHarness) -> Option<RichReveal> {
    h.world().get::<RichReveal>(text_node(h)).copied()
}

fn line(h: &UiHarness) -> Option<DialogueLine> {
    h.world().get::<DialogueLine>(text_node(h)).copied()
}

/// The visible text of a rich node: the spans painted with any alpha.
fn shown(h: &UiHarness, entity: Entity) -> String {
    fn walk(world: &World, entity: Entity, out: &mut String) {
        if let (Some(span), Some(color)) = (
            world.get::<TextSpan>(entity),
            world.get::<TextColor>(entity),
        ) && color.0.alpha() > 0.0
        {
            out.push_str(&span.0);
        }
        if let Some(children) = world.get::<Children>(entity) {
            for child in children.iter() {
                walk(world, child, out);
            }
        }
    }
    let mut out = String::new();
    walk(h.world(), entity, &mut out);
    out
}

/// Every `Text` and `TextSpan` under `entity`, shown or not.
fn text_of(h: &UiHarness, entity: Entity) -> String {
    fn walk(world: &World, entity: Entity, out: &mut String) {
        if let Some(text) = world.get::<Text>(entity) {
            out.push_str(&text.0);
        }
        if let Some(span) = world.get::<TextSpan>(entity) {
            out.push_str(&span.0);
        }
        if let Some(children) = world.get::<Children>(entity) {
            for child in children.iter() {
                walk(world, child, out);
            }
        }
    }
    let mut out = String::new();
    walk(h.world(), entity, &mut out);
    out
}

/// The option buttons in column order, with their ids and whether each is
/// enabled.
fn options(h: &UiHarness) -> Vec<(String, bool)> {
    let root = dialogue_root(h);
    let column = h.find(&by::test_id("choices").within(root));
    let world = h.world();
    world
        .get::<Children>(column)
        .map(|children| {
            children
                .iter()
                .filter_map(|e| {
                    let option = world.get::<DialogueOption>(e)?;
                    let disabled = world.get::<slotted_ui::ButtonState>(e)?.disabled;
                    Some((option.id.clone(), !disabled))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn option(h: &UiHarness, id: &str) -> Entity {
    h.find(&by::tag(OPTION_TAG, id).within(dialogue_root(h)))
}

fn hints(h: &UiHarness) -> Vec<(UiAction, String)> {
    let bar = h.find(&by::test_id("hints").within(dialogue_root(h)));
    h.hint_entries(bar)
        .into_iter()
        .map(|HintEntry { action, label }| (action, label.0))
        .collect()
}

fn cps(h: &UiHarness) -> u32 {
    let world = h.world();
    let active = world.resource::<slotted_theme::ActiveTheme>().0.clone();
    world
        .resource::<Assets<slotted_theme::Theme>>()
        .get(&active)
        .unwrap()
        .tokens
        .dialogue
        .chars_per_second
}

fn choice_delay(h: &UiHarness) -> Duration {
    let world = h.world();
    let active = world.resource::<slotted_theme::ActiveTheme>().0.clone();
    let ms = world
        .resource::<Assets<slotted_theme::Theme>>()
        .get(&active)
        .unwrap()
        .tokens
        .dialogue
        .choice_delay;
    Duration::from_millis(u64::from(ms))
}

// ---------------------------------------------------------------------------
// M3-TEST: C
// ---------------------------------------------------------------------------

#[test]
fn the_template_opens_as_a_focusable_overlay_in_three_themes() {
    for theme in THEMES {
        let mut h = harness_in(theme);
        let a = start_greeting(&mut h);
        h.settle();
        assert_eq!(h.stack(), vec![kinds::dialogue()], "{theme}");
        assert_eq!(
            h.stack_top(),
            None,
            "{theme}: an overlay is not the non-overlay top"
        );
        let world = h.world();
        let stack = world.resource::<slotted_ui::ScreenStack>();
        let top = stack.focus_top().expect("the overlay takes focus");
        assert_eq!(top.root, a.root, "{theme}");
        assert!(top.presentation.takes_focus(), "{theme}");
        assert!(world.get::<DialogueScreen>(a.root).is_some(), "{theme}");
        let panel = h.find(&by::test_id("dialogue").within(a.root));
        assert!(h.is_visible(panel), "{theme}: the panel is laid out");
        for id in [
            "header", "portrait", "speaker", "text", "choices", "footer", "hints",
        ] {
            assert!(
                h.try_find(&by::test_id(id).within(a.root)).is_some(),
                "{theme}: the template has `{id}`"
            );
        }
    }
}

#[test]
fn a_say_shows_speaker_and_portrait_and_types_the_text_at_chars_per_second() {
    for theme in THEMES {
        let mut h = harness_in(theme);
        let a = start_greeting(&mut h);
        let root = a.root;
        let speaker = h.find(&by::test_id("speaker").within(root));
        let portrait = h.find(&by::test_id("portrait").within(root));
        assert_eq!(h.text_of(speaker).as_deref(), Some("demo-elder"), "{theme}");
        assert_eq!(
            h.world().get::<Visibility>(speaker),
            Some(&Visibility::Inherited),
            "{theme}"
        );
        assert_eq!(
            h.world().get::<Visibility>(portrait),
            Some(&Visibility::Inherited),
            "{theme}"
        );
        let world = h.world();
        let image = world
            .get::<Children>(portrait)
            .and_then(|c| c.iter().find(|e| world.get::<ImageNode>(*e).is_some()));
        assert!(image.is_some(), "{theme}: the portrait image is spawned");

        // Presented, not typed: nothing shows on the first frame.
        assert_eq!(reveal(&h), Some(RichReveal(Some(0))), "{theme}");
        let first = line(&h).expect("the line types");
        assert_eq!(first.units, 0);
        assert_eq!(first.total, "demo-greeting-hello".chars().count());
        assert_eq!(shown(&h, text_node(&h)), "", "{theme}");
        assert!(!active(&h).unwrap().revealed);

        h.advance(Duration::from_millis(250));
        let mid = line(&h).expect("still typing");
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let expected = (mid.elapsed.as_secs_f64() * f64::from(cps(&h))).floor() as usize;
        assert_eq!(
            mid.units,
            expected.min(mid.total),
            "{theme}: units follow the clock"
        );
        assert!(mid.units > 0 && mid.units < mid.total, "{theme}: {mid:?}");
        assert_eq!(reveal(&h), Some(RichReveal(Some(mid.units))), "{theme}");
        let prefix: String = "demo-greeting-hello".chars().take(mid.units).collect();
        assert_eq!(shown(&h, text_node(&h)), prefix, "{theme}");
        let caption = h.find(&by::test_id("continue").within(root));
        assert_eq!(
            h.world().get::<Visibility>(caption),
            Some(&Visibility::Hidden),
            "{theme}: no caption while typing"
        );

        h.advance(Duration::from_secs(2));
        h.step(1);
        assert_eq!(line(&h), None, "{theme}: the line is done");
        assert_eq!(reveal(&h), Some(RichReveal::ALL), "{theme}");
        assert!(active(&h).unwrap().revealed, "{theme}");
        assert_eq!(shown(&h, text_node(&h)), "demo-greeting-hello", "{theme}");
        assert_eq!(
            h.world().get::<Visibility>(caption),
            Some(&Visibility::Inherited),
            "{theme}: the caption shows once revealed"
        );
        assert!(
            text_of(&h, caption).contains("continue"),
            "{theme}: {}",
            text_of(&h, caption)
        );
    }
}

#[test]
fn accept_mid_reveal_shows_everything_and_the_next_accept_advances() {
    let mut h = harness();
    start_greeting(&mut h);
    h.advance(Duration::from_millis(100));
    let mid = line(&h).unwrap();
    assert!(mid.units < mid.total);
    h.action(UiAction::Accept);
    let a = active(&h).unwrap();
    assert!(a.revealed);
    assert_eq!(a.node, node("hello"));
    assert_eq!(line(&h), None, "the typewriter jumped to the end");
    assert_eq!(reveal(&h), Some(RichReveal::ALL));
    assert_eq!(shown(&h, text_node(&h)), "demo-greeting-hello");
    h.action(UiAction::Accept);
    assert_eq!(current_node(&h), Some(node("ask")));
}

#[test]
fn reduced_motion_reveals_at_once_and_shows_the_choices_at_once() {
    let mut h = harness_with(
        "glass",
        Some(Motion {
            reduced: true,
            ..default()
        }),
        |_| {},
    );
    let a = start_greeting(&mut h);
    assert!(a.revealed, "the runner is marked revealed");
    assert_eq!(line(&h), None);
    assert_eq!(reveal(&h), Some(RichReveal::ALL));
    assert_eq!(shown(&h, text_node(&h)), "demo-greeting-hello");
    let caption = h.find(&by::test_id("continue").within(a.root));
    assert_eq!(
        h.world().get::<Visibility>(caption),
        Some(&Visibility::Inherited)
    );
    h.action(UiAction::Accept);
    assert_eq!(current_node(&h), Some(node("ask")));
    assert_eq!(
        options(&h),
        vec![
            ("yes".to_owned(), true),
            ("no".to_owned(), true),
            ("secret".to_owned(), false)
        ],
        "no delay under reduced motion"
    );
}

#[test]
fn a_zero_chars_per_second_theme_reveals_at_once() {
    let mut h = harness();
    {
        let world = h.world_mut();
        let active = world.resource::<slotted_theme::ActiveTheme>().0.clone();
        let mut themes = world.resource_mut::<Assets<slotted_theme::Theme>>();
        themes
            .get_mut(&active)
            .unwrap()
            .tokens
            .dialogue
            .chars_per_second = 0;
    }
    h.settle();
    let a = start_greeting(&mut h);
    assert!(a.revealed);
    assert_eq!(line(&h), None);
    assert_eq!(reveal(&h), Some(RichReveal::ALL));
}

#[test]
fn a_choice_spawns_its_buttons_after_the_delay_and_focuses_the_first_enabled() {
    for theme in THEMES {
        let mut h = harness_in(theme);
        start_greeting(&mut h);
        h.action(UiAction::Accept);
        h.action(UiAction::Accept);
        assert_eq!(current_node(&h), Some(node("ask")), "{theme}");
        let root = dialogue_root(&h);
        let column = h.find(&by::test_id("choices").within(root));
        assert!(
            h.world().get::<DialogueChoicesPending>(column).is_some(),
            "{theme}: the choices wait for the delay"
        );
        assert_eq!(options(&h), vec![], "{theme}: nothing yet");
        assert!(
            h.focused()
                .is_none_or(|f| h.world().get::<DialogueOption>(f).is_none()),
            "{theme}: no option to focus yet"
        );
        // The prompt sits where the line would, fully shown; the header is
        // gone.
        assert_eq!(line(&h), None);
        assert_eq!(reveal(&h), Some(RichReveal::ALL));
        assert_eq!(shown(&h, text_node(&h)), "demo-greeting-ask", "{theme}");
        let speaker = h.find(&by::test_id("speaker").within(root));
        assert_eq!(
            h.world().get::<Visibility>(speaker),
            Some(&Visibility::Hidden),
            "{theme}"
        );

        h.advance(choice_delay(&h) + h.frame_delta());
        h.step(1);
        assert_eq!(
            options(&h),
            vec![
                ("yes".to_owned(), true),
                ("no".to_owned(), true),
                ("secret".to_owned(), false)
            ],
            "{theme}"
        );
        assert!(
            h.world().get::<DialogueChoicesPending>(column).is_none(),
            "{theme}"
        );
        let yes = option(&h, "yes");
        assert_eq!(
            h.focused(),
            Some(yes),
            "{theme}: focus on the first enabled"
        );
        assert_eq!(
            h.world()
                .get::<slotted_ui::TestId>(yes)
                .map(|t| t.0.clone()),
            Some(option_test_id("yes"))
        );
        assert_eq!(
            h.world().get::<DialogueOption>(yes),
            Some(&DialogueOption {
                id: "yes".to_owned(),
                index: 0
            })
        );
        h.settle();
        assert!(h.is_visible(yes), "{theme}: the buttons are laid out");
        assert_eq!(
            text_of(&h, yes),
            "demo-yes",
            "{theme}: the label is the option's key"
        );

        // The gamepad walk skips the disabled option and wraps.
        h.gamepad(GamepadButton::DPadDown);
        assert_eq!(h.focused(), Some(option(&h, "no")), "{theme}");
        h.gamepad(GamepadButton::DPadDown);
        assert_eq!(
            h.focused(),
            Some(yes),
            "{theme}: down from the last enabled wraps past the disabled one"
        );
        h.gamepad(GamepadButton::DPadUp);
        assert_eq!(h.focused(), Some(option(&h, "no")), "{theme}");
        h.gamepad(GamepadButton::DPadUp);
        assert_eq!(h.focused(), Some(yes), "{theme}");

        // Activating an option yields the choice and the next line.
        h.gamepad(GamepadButton::South);
        h.step(1);
        assert_eq!(chosen(&h), vec![("yes".to_owned(), 0)], "{theme}");
        assert_eq!(current_node(&h), Some(node("yes")), "{theme}");
        assert_eq!(options(&h), vec![], "{theme}: the buttons are gone");
        assert!(
            h.focused().is_none_or(|f| h.world().get_entity(f).is_err()
                || h.world().get::<DialogueOption>(f).is_none()),
            "{theme}: a line has nothing to focus"
        );
        assert_eq!(
            h.world().get::<Visibility>(speaker),
            Some(&Visibility::Inherited),
            "{theme}"
        );
        let first = line(&h).expect("the next line types");
        assert!(first.units < 3, "{theme}: a fresh line, {first:?}");
        h.advance(Duration::from_secs(2));
        assert_eq!(shown(&h, text_node(&h)), "demo-greeting-yes", "{theme}");
    }
}

#[test]
fn a_condition_that_holds_enables_the_option() {
    let mut h = harness();
    h.set_value("found_key", Value::Bool(true));
    start_at_ask(&mut h);
    assert_eq!(
        options(&h),
        vec![
            ("yes".to_owned(), true),
            ("no".to_owned(), true),
            ("secret".to_owned(), true)
        ]
    );
    let secret = option(&h, "secret");
    h.gamepad(GamepadButton::DPadUp);
    assert_eq!(
        h.focused(),
        Some(secret),
        "up from the first wraps to the last"
    );
    h.gamepad(GamepadButton::South);
    h.step(1);
    assert_eq!(chosen(&h), vec![("secret".to_owned(), 2)]);
    assert_eq!(current_node(&h), Some(node("secret")));
    let root = dialogue_root(&h);
    let speaker = h.find(&by::test_id("speaker").within(root));
    let portrait = h.find(&by::test_id("portrait").within(root));
    assert_eq!(
        h.world().get::<Visibility>(speaker),
        Some(&Visibility::Hidden),
        "narration has no speaker"
    );
    assert_eq!(
        h.world().get::<Visibility>(portrait),
        Some(&Visibility::Hidden),
        "nor a portrait"
    );
}

#[test]
fn secondary_opens_the_history_and_done_returns_to_the_dialogue_with_its_focus() {
    for theme in THEMES {
        let mut h = harness_in(theme);
        start_at_ask(&mut h);
        h.gamepad(GamepadButton::South);
        h.advance(Duration::from_secs(2));
        assert_eq!(current_node(&h), Some(node("yes")), "{theme}");
        h.action(UiAction::Accept);
        assert_eq!(
            ended(&h),
            vec![("bye".to_owned(), EndReason::Finished)],
            "{theme}"
        );
        // Same walk again, to a choice this time, so a focus can come back.
        run(&mut h, |c| {
            start_dialogue(c, DialogueId::new("demo:greeting"));
        });
        h.step(1);
        h.advance(Duration::from_secs(1));
        h.action(UiAction::Accept);
        h.advance(Duration::from_millis(400));
        h.settle();
        h.gamepad(GamepadButton::DPadDown);
        let no = option(&h, "no");
        assert_eq!(h.focused(), Some(no), "{theme}");
        let dialogue = dialogue_root(&h);

        h.action(UiAction::Secondary);
        h.settle();
        assert_eq!(h.stack(), vec![kinds::dialogue(), kinds::page()], "{theme}");
        let page = h
            .world()
            .resource::<slotted_ui::ScreenStack>()
            .top()
            .unwrap()
            .root;
        let title = h.find(&by::test_id("title").within(page));
        assert_eq!(h.text_of(title).as_deref(), Some("History"), "{theme}");
        let body = h.find(&by::test_id("body").within(page));
        let expected = transcript(&[HistoryLine {
            speaker: Some("demo-elder".to_owned()),
            text: "demo-greeting-hello".to_owned(),
        }]);
        assert_eq!(
            expected, "[b]demo-elder[/b]\ndemo-greeting-hello\n\n",
            "{theme}"
        );
        assert_eq!(
            text_of(&h, body),
            "demo-elder\ndemo-greeting-hello\n\n",
            "{theme}: the transcript, markup rendered"
        );
        let done = h.find(&by::test_id("done").within(page));
        assert_eq!(h.focused(), Some(done), "{theme}: the page took focus");
        assert_eq!(
            current_node(&h),
            Some(node("ask")),
            "{theme}: the runner waits"
        );

        h.gamepad(GamepadButton::South);
        h.settle();
        assert_eq!(h.stack(), vec![kinds::dialogue()], "{theme}");
        assert_eq!(
            h.world()
                .resource::<slotted_ui::ScreenStack>()
                .focus_top()
                .map(|e| e.root),
            Some(dialogue),
            "{theme}"
        );
        assert_eq!(
            h.focused(),
            Some(no),
            "{theme}: the option has its focus back"
        );
    }
}

#[test]
fn the_history_lists_every_line_said_so_far_in_order() {
    let mut h = harness();
    start_at_ask(&mut h);
    h.gamepad(GamepadButton::South);
    h.advance(Duration::from_secs(2));
    assert_eq!(current_node(&h), Some(node("yes")));
    h.action(UiAction::Secondary);
    h.settle();
    let page = h
        .world()
        .resource::<slotted_ui::ScreenStack>()
        .top()
        .unwrap()
        .root;
    let body = h.find(&by::test_id("body").within(page));
    assert_eq!(
        text_of(&h, body),
        "demo-elder\ndemo-greeting-hello\n\ndemo-elder\ndemo-greeting-yes\n\n"
    );
    // Narration and a speaker with markup characters.
    assert_eq!(
        transcript(&[
            HistoryLine {
                speaker: None,
                text: "A [b]dark[/b] night.".to_owned(),
            },
            HistoryLine {
                speaker: Some("El[der]".to_owned()),
                text: "Hm.".to_owned(),
            },
        ]),
        "A [b]dark[/b] night.\n\n[b]El[[der]][/b]\nHm.\n\n"
    );
}

#[test]
fn history_off_leaves_secondary_alone() {
    let mut h = harness();
    h.world_mut().resource_mut::<DialogueConfig>().history = false;
    start_greeting(&mut h);
    h.action(UiAction::Secondary);
    h.settle();
    assert_eq!(h.stack(), vec![kinds::dialogue()]);
}

#[test]
fn the_hint_entries_follow_the_state() {
    let accept = |key: &str| (UiAction::Accept, format!("slotted.menu.dialogue.{key}"));
    let history = (
        UiAction::Secondary,
        "slotted.menu.dialogue.history".to_owned(),
    );
    let leave = (UiAction::Back, "slotted.menu.dialogue.leave".to_owned());

    let mut h = harness();
    start_greeting(&mut h);
    h.step(1);
    assert_eq!(hints(&h), vec![accept("skip"), history.clone()], "typing");
    h.advance(Duration::from_secs(1));
    h.step(1);
    assert_eq!(hints(&h), vec![accept("next"), history.clone()], "revealed");
    h.action(UiAction::Accept);
    h.step(1);
    assert_eq!(
        hints(&h),
        vec![history.clone()],
        "a choice without its buttons yet"
    );
    h.advance(Duration::from_millis(400));
    h.step(1);
    assert_eq!(
        hints(&h),
        vec![
            (UiAction::Accept, "slotted.menu.select".to_owned()),
            history.clone()
        ],
        "the focused option's verb"
    );

    let mut h = harness();
    h.world_mut().resource_mut::<DialogueConfig>().back_cancels = true;
    h.world_mut().resource_mut::<DialogueConfig>().history = false;
    start_greeting(&mut h);
    h.step(1);
    assert_eq!(hints(&h), vec![accept("skip"), leave.clone()]);
    h.action(UiAction::Back);
    h.settle();
    assert_eq!(ended(&h), vec![("hello".to_owned(), EndReason::Cancelled)]);
}

#[test]
fn the_text_height_does_not_change_during_a_reveal() {
    for theme in THEMES {
        let mut h = harness_in(theme);
        start_greeting(&mut h);
        h.step(2);
        let text = text_node(&h);
        assert!(line(&h).unwrap().units < 3, "{theme}: barely started");
        let before = h.rect_of(text).height();
        assert!(before > 0.0, "{theme}");
        let panel = h.find(&by::test_id("dialogue").within(dialogue_root(&h)));
        let panel_before = h.rect_of(panel).height();
        h.advance(Duration::from_secs(2));
        h.settle();
        assert_eq!(line(&h), None, "{theme}");
        assert_eq!(
            h.rect_of(text).height(),
            before,
            "{theme}: the text kept its height"
        );
        assert_eq!(
            h.rect_of(panel).height(),
            panel_before,
            "{theme}: the panel kept its height"
        );
    }
}

#[test]
fn a_pause_pushed_over_the_dialogue_takes_focus_and_the_dialogue_resumes_after_it_pops() {
    let mut h = harness();
    start_at_ask(&mut h);
    let yes = option(&h, "yes");
    assert_eq!(h.focused(), Some(yes));
    let dialogue = dialogue_root(&h);

    h.action(UiAction::Menu);
    h.settle();
    assert_eq!(h.stack(), vec![kinds::dialogue(), kinds::pause()]);
    let pause = h
        .world()
        .resource::<slotted_ui::ScreenStack>()
        .top()
        .unwrap()
        .root;
    let focused = h.focused().expect("the pause has a focus");
    assert!(
        h.find_all(&by::role(SemanticRole::Button).within(pause))
            .contains(&focused),
        "the pause holds the focus"
    );
    assert_ne!(focused, yes);
    // An Accept under the pause reaches the pause, never the option.
    // Resume is the pause's first focus, so it pops it too.
    h.action(UiAction::Accept);
    h.settle();
    assert_eq!(chosen(&h), vec![], "the option heard nothing");
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
        Some(dialogue)
    );
    assert_eq!(h.focused(), Some(yes), "the option has its focus back");
    h.gamepad(GamepadButton::South);
    h.step(1);
    assert_eq!(chosen(&h), vec![("yes".to_owned(), 0)]);
    assert_eq!(current_node(&h), Some(node("yes")));

    // A paused clock freezes a half-typed line; unpausing resumes it.
    h.step(2);
    h.world_mut().resource_mut::<Time<Virtual>>().pause();
    let frozen = line(&h).unwrap();
    h.step(30);
    assert_eq!(line(&h), Some(frozen), "no virtual time, no typing");
    h.world_mut().resource_mut::<Time<Virtual>>().unpause();
    h.step(30);
    assert!(line(&h).is_none_or(|l| l.units > frozen.units));
}

#[test]
fn ending_the_dialogue_closes_the_screen_and_its_line() {
    let mut h = harness();
    start_greeting(&mut h);
    let text = text_node(&h);
    run(&mut h, slotted_menu::end_dialogue);
    h.settle();
    assert_eq!(h.stack(), vec![]);
    assert!(h.world().get_entity(text).is_err(), "the screen is gone");
    let world = h.world_mut();
    assert!(world.query::<&DialogueLine>().iter(world).next().is_none());
}

#[test]
fn a_say_and_a_choice_tree_match_in_three_themes() {
    for theme in THEMES {
        let mut h = harness_in(theme);
        start_greeting(&mut h);
        h.advance(Duration::from_secs(1));
        h.settle();
        assert!(active(&h).unwrap().revealed);
        assert_tree_snapshot!(format!("dialogue_say_tree_{theme}"), h.screen_tree());

        h.action(UiAction::Accept);
        h.advance(Duration::from_millis(400));
        h.settle();
        assert_eq!(current_node(&h), Some(node("ask")));
        assert_eq!(options(&h).len(), 3);
        assert_tree_snapshot!(format!("dialogue_choice_tree_{theme}"), h.screen_tree());
    }
}
