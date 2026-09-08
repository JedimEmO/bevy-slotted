//! The menus example, headless: the whole flow by gamepad, the About button
//! a mod-style injection added, and the tree snapshots of the three
//! templates the example opens (menus M2 contract 5).
//!
//! The theme is found because `examples/menus/assets` is a symlink to the
//! workspace `assets/`, which is where Bevy's `AssetPlugin` looks when it
//! resolves against `CARGO_MANIFEST_DIR`.
#![allow(clippy::unwrap_used, clippy::float_cmp)]

use std::time::Duration;

use bevy::input::gamepad::GamepadButton;
use bevy::prelude::*;
use menus::{MenusDemoPlugin, main_kind};
use pretty_assertions::assert_eq;
use slotted::menu::{
    DialogueChoice, DialogueEnded, DialogueNodeEntered, EndReason, MemorySettings, NodeId,
    SettingsStorage, kinds,
};
use slotted::ui::{SliderState, UiAction, Value};
use slotted_test::prelude::*;

/// The example under one theme, with a memory settings store, the main menu
/// open and the theme loaded.
fn open_main_in(theme: &str) -> (UiHarness, MemorySettings) {
    let store = MemorySettings::default();
    let storage = SettingsStorage::new(store.clone());
    let mut h = UiHarness::builder()
        .plugins(SlottedPlugins::headless())
        .plugins(MenusDemoPlugin)
        .plugins(move |app: &mut App| {
            app.insert_resource(storage.clone());
        })
        .registries(menus::registries())
        .resolution(1600.0, 900.0)
        .theme(theme)
        .build();
    wait_for_theme(&mut h);
    h.world_mut().commands().queue(|world: &mut World| {
        menus::open_main_menu(&mut world.commands());
    });
    h.settle();
    (h, store)
}

fn open_main() -> (UiHarness, MemorySettings) {
    open_main_in("glass")
}

fn wait_for_theme(h: &mut UiHarness) {
    for _ in 0..600 {
        let world = h.world();
        let active = world.resource::<slotted::theme::ActiveTheme>().0.clone();
        if world
            .resource::<Assets<slotted::theme::Theme>>()
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

fn find(h: &UiHarness, id: &str) -> Entity {
    h.find(&by::test_id(id))
}

fn pad(h: &mut UiHarness, button: GamepadButton) {
    h.gamepad(button);
    h.settle();
}

fn choices(h: &mut UiHarness) -> Vec<(String, ScreenKind)> {
    h.menu_choices()
        .into_iter()
        .map(|c| (c.id, c.screen))
        .collect()
}

fn exits(h: &UiHarness) -> Vec<AppExit> {
    bevy::ecs::message::MessageCursor::default()
        .read(h.world().resource::<Messages<AppExit>>())
        .cloned()
        .collect()
}

/// main → play → (chest) → back → pause → settings → adjust a slider → back
/// → quit → leave → main → quit → confirm → exit, every step on the pad.
#[test]
#[allow(clippy::too_many_lines)]
fn the_whole_flow_runs_by_gamepad() {
    let (mut h, store) = open_main();
    let main = main_kind();
    let chest = ScreenKind::new(chest::CHEST);
    let settings = ScreenKind::new(menus::SETTINGS);
    assert_eq!(h.stack(), vec![main.clone()]);
    assert_eq!(h.focused(), Some(find(&h, "play")));
    assert_eq!(
        h.text_of(find(&h, "title")).as_deref(),
        Some("Slotted"),
        "the title comes from `MenuConfig` through the catalogue"
    );
    assert_eq!(
        h.text_of(find(&h, "version")).as_deref(),
        Some(format!("Version {}", env!("CARGO_PKG_VERSION")).as_str())
    );
    assert!(
        h.try_find(&by::test_id("footer_note")).is_some(),
        "the inherited node"
    );
    assert!(store.get().is_none(), "opening saves nothing");

    // Play: the main menu goes, the chest comes.
    pad(&mut h, GamepadButton::South);
    assert_eq!(h.stack(), vec![chest.clone()]);
    assert_eq!(choices(&mut h), vec![("play".to_owned(), main.clone())]);
    assert_eq!(h.find_all(&by::role(SemanticRole::Slot)).len(), 63);

    // Back pops the chest; Start pauses over nothing.
    pad(&mut h, GamepadButton::East);
    assert_eq!(h.stack(), vec![]);
    pad(&mut h, GamepadButton::Start);
    assert_eq!(h.stack(), vec![kinds::pause()]);
    assert_eq!(h.focused(), Some(find(&h, "resume")));

    // Down to Settings, South opens the generated settings screen.
    pad(&mut h, GamepadButton::DPadDown);
    assert_eq!(h.focused(), Some(find(&h, "settings")));
    pad(&mut h, GamepadButton::South);
    assert_eq!(h.stack(), vec![kinds::pause(), settings.clone()]);
    assert_eq!(
        choices(&mut h),
        vec![("settings".to_owned(), kinds::pause())]
    );
    let tabs = find(&h, "settings.tabs");
    assert_eq!(
        h.focused(),
        Some(h.find(&by::role(SemanticRole::Tab).within(tabs).index(0)))
    );

    // Down past the resolution select to the UI scale slider; Right steps it.
    let slider = find(&h, "settings.ui_scale");
    pad(&mut h, GamepadButton::DPadDown);
    pad(&mut h, GamepadButton::DPadDown);
    assert_eq!(h.focused(), Some(slider));
    assert_eq!(h.value("settings.ui_scale"), Some(Value::Float(1.0)));
    let modal = h.find(&by::screen(settings.clone()));
    let hints = h.hint_entries(h.find(&by::test_id("hints").within(modal)));
    assert_eq!(
        hints.iter().map(|e| e.label.0.as_str()).collect::<Vec<_>>(),
        vec![
            "slotted.menu.adjust",
            "slotted.menu.adjust",
            "slotted.menu.close",
            "slotted.menu.prev_tab",
            "slotted.menu.next_tab"
        ],
        "the hint bar names Adjust, Close and the tabs on a slider"
    );
    pad(&mut h, GamepadButton::DPadRight);
    assert_eq!(h.value("settings.ui_scale"), Some(Value::Float(1.25)));
    assert_eq!(h.world().get::<SliderState>(slider).unwrap().value, 1.25);
    let saved = store.get().expect("the change was saved");
    assert_eq!(
        saved.values.get("settings.ui_scale"),
        Some(&Value::Float(1.25))
    );

    // Back to the pause, then Quit asks before leaving the game.
    pad(&mut h, GamepadButton::East);
    assert_eq!(h.stack(), vec![kinds::pause()]);
    assert_eq!(
        h.focused(),
        Some(find(&h, "settings")),
        "focus is back where it was"
    );
    pad(&mut h, GamepadButton::DPadDown);
    assert_eq!(h.focused(), Some(find(&h, "quit")));
    pad(&mut h, GamepadButton::South);
    assert_eq!(h.stack(), vec![kinds::pause(), kinds::confirm()]);
    assert_eq!(choices(&mut h), vec![("quit".to_owned(), kinds::pause())]);
    let dialog = h.find(&by::screen(kinds::confirm()));
    assert_eq!(
        h.text_of(h.find(&by::test_id("title").within(dialog)))
            .as_deref(),
        Some("Leave the game?")
    );
    assert_eq!(
        h.focused(),
        Some(h.find(&by::test_id("cancel").within(dialog)))
    );
    let accept = h.find(&by::test_id("accept").within(dialog));
    assert_eq!(
        h.world()
            .get::<slotted::ui::ButtonState>(accept)
            .expect("the accept button")
            .variant,
        slotted::ui::ButtonVariant::Danger
    );

    // Cancel first, by Back: the pause is still there.
    pad(&mut h, GamepadButton::East);
    assert_eq!(h.stack(), vec![kinds::pause()]);
    pad(&mut h, GamepadButton::South);
    assert_eq!(h.stack(), vec![kinds::pause(), kinds::confirm()]);
    let dialog = h.find(&by::screen(kinds::confirm()));
    pad(&mut h, GamepadButton::DPadRight);
    assert_eq!(
        h.focused(),
        Some(h.find(&by::test_id("accept").within(dialog)))
    );
    pad(&mut h, GamepadButton::South);
    assert_eq!(
        h.stack(),
        vec![main.clone()],
        "Leave popped the pause and brought the title back"
    );
    assert_eq!(
        choices(&mut h),
        vec![
            ("quit".to_owned(), kinds::pause()),
            ("accept".to_owned(), kinds::confirm())
        ],
        "the second Quit and the accept both reached the game"
    );
    assert!(exits(&h).is_empty());

    // Quit on the title: a confirm, then the app exits.
    assert_eq!(h.focused(), Some(find(&h, "play")));
    pad(&mut h, GamepadButton::DPadDown);
    pad(&mut h, GamepadButton::DPadDown);
    assert_eq!(h.focused(), Some(find(&h, "quit")));
    pad(&mut h, GamepadButton::South);
    assert_eq!(h.stack(), vec![main.clone(), kinds::confirm()]);
    assert_eq!(choices(&mut h), vec![("quit".to_owned(), main.clone())]);
    let dialog = h.find(&by::screen(kinds::confirm()));
    assert_eq!(
        h.text_of(h.find(&by::test_id("title").within(dialog)))
            .as_deref(),
        Some("Quit?")
    );
    h.confirm_accept();
    assert_eq!(exits(&h), vec![AppExit::Success]);
}

/// The About button is not in the template: the example injects it at the
/// template's `buttons_end` anchor. A pad walks down to it and it opens the
/// About page; Back returns to the title.
#[test]
fn the_injected_about_button_is_reachable_by_gamepad_and_opens_a_page() {
    let (mut h, _) = open_main();
    let main = main_kind();
    let about = find(&h, "about");
    let buttons = find(&h, "buttons");
    let mut ancestor = h.world().get::<ChildOf>(about).map(ChildOf::parent);
    let mut under_buttons = false;
    while let Some(e) = ancestor {
        if e == buttons {
            under_buttons = true;
            break;
        }
        ancestor = h.world().get::<ChildOf>(e).map(ChildOf::parent);
    }
    assert!(
        under_buttons,
        "the injection landed inside the button column"
    );
    let play = h.rect_of(find(&h, "play"));
    let about_rect = h.rect_of(about);
    assert!(
        about_rect.min.y > play.max.y,
        "About sits below the template's buttons"
    );
    assert!(
        (about_rect.min.x - play.min.x).abs() <= 1.0
            && (about_rect.width() - play.width()).abs() <= 2.0,
        "About lines up with the column: {about_rect:?} vs {play:?}"
    );

    for id in ["settings", "quit", "about"] {
        pad(&mut h, GamepadButton::DPadDown);
        assert_eq!(h.focused(), Some(find(&h, id)), "Down reaches {id}");
    }
    pad(&mut h, GamepadButton::South);
    assert_eq!(h.stack(), vec![main.clone(), kinds::page()]);
    assert_eq!(choices(&mut h), vec![("about".to_owned(), main.clone())]);
    let page = h.find(&by::screen(kinds::page()));
    assert_eq!(
        h.text_of(h.find(&by::test_id("title").within(page)))
            .as_deref(),
        Some("About this example")
    );
    assert_eq!(h.focused(), Some(h.find(&by::test_id("done").within(page))));
    pad(&mut h, GamepadButton::East);
    assert_eq!(h.stack(), vec![main]);
    assert_eq!(h.focused(), Some(about), "focus is back on About");
}

/// A quick stack on the chest's rail shows a toast, over the chest, with
/// no stack entry and no focus change.
#[test]
fn a_quick_stack_on_the_chest_shows_a_toast() {
    let (mut h, _) = open_main();
    pad(&mut h, GamepadButton::South);
    assert_eq!(h.stack(), vec![ScreenKind::new(chest::CHEST)]);
    h.rebaseline();
    assert!(h.toasts().is_empty());
    let focused = h.focused();
    let rail = h.find(&by::role(SemanticRole::Button).tag("action", "quick_stack"));
    h.activate(rail);
    h.settle();
    let toasts = h.toasts();
    assert_eq!(toasts.len(), 1);
    assert_eq!(
        h.world()
            .get::<slotted::menu::Toast>(toasts[0])
            .map(|t| t.level),
        Some(slotted::menu::ToastLevel::Success)
    );
    assert_eq!(h.stack(), vec![ScreenKind::new(chest::CHEST)]);
    assert_eq!(h.focused(), focused, "a toast moves no focus");
    h.assert_conserved();
}

/// The trees of the three templates the example opens, per theme. Stable
/// across themes by design: a theme changes materials, not the tree.
#[test]
fn the_main_pause_and_confirm_trees_match_in_three_themes() {
    for theme in ["glass", "paper", "neon"] {
        let (mut h, _) = open_main_in(theme);
        assert_tree_snapshot!(format!("main_menu_tree_{theme}"), h.screen_tree());

        pad(&mut h, GamepadButton::South);
        pad(&mut h, GamepadButton::East);
        pad(&mut h, GamepadButton::Start);
        assert_eq!(h.stack(), vec![kinds::pause()]);
        assert_tree_snapshot!(format!("pause_tree_{theme}"), h.screen_tree());

        pad(&mut h, GamepadButton::DPadDown);
        pad(&mut h, GamepadButton::DPadDown);
        pad(&mut h, GamepadButton::South);
        assert_eq!(h.stack(), vec![kinds::pause(), kinds::confirm()]);
        assert_tree_snapshot!(format!("confirm_tree_{theme}"), h.screen_tree());
    }
}

/// Waits for the greeting to land in `Dialogues` (the asset loads through
/// the server) and returns.
fn wait_for_dialogue(h: &mut UiHarness) {
    for _ in 0..600 {
        if h.world()
            .resource::<slotted::menu::Dialogues>()
            .get(&menus::greeting_id())
            .is_some()
        {
            return;
        }
        h.step(1);
    }
    panic!("dialogue/greeting.dialogue.ron never registered");
}

/// Down to the injected Talk button (below About) and South, then a few
/// frames rather than a settle: a settle would let the typewriter finish
/// the first line, and the test wants to watch it type.
fn talk(h: &mut UiHarness) {
    for id in ["settings", "quit", "about", "talk"] {
        pad(h, GamepadButton::DPadDown);
        assert_eq!(h.focused(), Some(find(h, id)), "Down reaches {id}");
    }
    h.gamepad(GamepadButton::South);
    h.step(3);
}

/// Talk on the title starts the elder's greeting as an overlay over the
/// main menu: South skips the typewriter, South again continues to the
/// choice, the d-pad skips the locked third answer, South on Yes earns the
/// game's toast, South ends it and the title has its focus back.
#[test]
#[allow(clippy::too_many_lines)]
fn talk_walks_the_greeting_by_gamepad() {
    let (mut h, _) = open_main();
    let main = main_kind();
    wait_for_dialogue(&mut h);
    assert_eq!(h.dialogue(), None);
    talk(&mut h);
    assert_eq!(choices(&mut h), vec![("talk".to_owned(), main.clone())]);
    assert_eq!(h.stack(), vec![main.clone(), kinds::dialogue()]);
    assert_eq!(
        h.stack_top(),
        Some(main.clone()),
        "an overlay is not the non-overlay top"
    );
    let hello = NodeId::new("hello");
    assert_eq!(h.dialogue(), Some((menus::greeting_id(), hello.clone())));
    assert_eq!(
        h.dialogue_events(),
        vec![DialogueEvent::Entered(DialogueNodeEntered {
            dialogue: menus::greeting_id(),
            node: hello.clone(),
        })]
    );
    assert!(!h.dialogue_revealed());
    let root = h.find(&by::screen(kinds::dialogue()));
    assert_eq!(
        h.text_of(h.find(&by::test_id("speaker").within(root)))
            .as_deref(),
        Some("Elder")
    );
    let portrait = h.find(&by::test_id("portrait").within(root));
    assert_eq!(
        h.world().get::<Visibility>(portrait),
        Some(&Visibility::Inherited),
        "the elder has a portrait"
    );
    assert!(
        h.try_find(&by::test_id("talk")).is_some(),
        "the title stays on the stack under the overlay"
    );
    assert_eq!(h.dialogue_options(), vec![]);

    // The typewriter: some of the line after a moment, all of it on South.
    h.advance(Duration::from_millis(300));
    let partial = h.dialogue_text();
    assert!(
        !partial.is_empty() && partial.chars().count() < 40,
        "typing: {partial:?}"
    );
    assert!(!h.dialogue_revealed());
    h.gamepad(GamepadButton::South);
    h.step(2);
    assert!(h.dialogue_revealed());
    let full = h.dialogue_text();
    assert!(
        full.starts_with("Ah, a visitor. Few come this far"),
        "the whole line, markup rendered: {full:?}"
    );
    assert_eq!(h.dialogue(), Some((menus::greeting_id(), hello.clone())));
    let hints = h.hint_entries(h.find(&by::test_id("hints").within(root)));
    assert_eq!(
        hints
            .iter()
            .map(|e| (e.action, e.label.0.as_str()))
            .collect::<Vec<_>>(),
        vec![
            (UiAction::Accept, "slotted.menu.dialogue.next"),
            (UiAction::Secondary, "slotted.menu.dialogue.history"),
        ]
    );

    // South again: the choice. Its buttons show after `choice_delay`.
    h.gamepad(GamepadButton::South);
    h.step(2);
    let ask = NodeId::new("ask");
    assert_eq!(h.dialogue(), Some((menus::greeting_id(), ask.clone())));
    assert_eq!(h.dialogue_text(), "Sit with the elder?");
    assert_eq!(h.dialogue_options(), vec![], "not before `choice_delay`");
    h.advance(Duration::from_millis(400));
    h.settle();
    assert_eq!(
        h.dialogue_options(),
        vec![
            ("yes".to_owned(), true),
            ("no".to_owned(), true),
            ("secret".to_owned(), false),
        ],
        "the third answer waits for `demo.found_key`"
    );
    assert_eq!(h.value(menus::FOUND_KEY), Some(Value::Bool(false)));
    let option = |h: &UiHarness, id: &str| {
        h.find(&by::test_id(&slotted::menu::dialogue_screen::option_test_id(id)).within(root))
    };
    assert_eq!(h.focused(), Some(option(&h, "yes")));
    pad(&mut h, GamepadButton::DPadDown);
    assert_eq!(h.focused(), Some(option(&h, "no")));
    pad(&mut h, GamepadButton::DPadDown);
    assert_eq!(
        h.focused(),
        Some(option(&h, "yes")),
        "the walk skips the locked answer and wraps"
    );
    assert!(h.toasts().is_empty());
    assert_eq!(h.dialogue_events().len(), 1, "`ask` was entered");

    // Yes: the game's toast, and the elder's next line.
    h.gamepad(GamepadButton::South);
    h.step(3);
    let yes = NodeId::new("yes");
    assert_eq!(
        h.dialogue_events(),
        vec![
            DialogueEvent::Chosen(DialogueChoice {
                dialogue: menus::greeting_id(),
                node: ask.clone(),
                option: "yes".to_owned(),
                index: 0,
            }),
            DialogueEvent::Entered(DialogueNodeEntered {
                dialogue: menus::greeting_id(),
                node: yes.clone(),
            }),
        ]
    );
    assert_eq!(h.toasts().len(), 1, "the game answered `yes` with a toast");
    assert_eq!(h.dialogue_options(), vec![]);
    pad(&mut h, GamepadButton::South);
    assert!(h.dialogue_revealed());
    assert_eq!(
        h.dialogue_text(),
        "Good, Traveller. Then listen: the chest below was sealed long before the village had a name.",
        "the Fluent argument came through"
    );
    assert_eq!(
        h.dialogue_history(),
        vec![
            (
                Some("Elder".to_owned()),
                "Ah, a visitor. [i]Few[/i] come this far up the mountain. Will you sit a while and hear what the old stones remember?".to_owned()
            ),
            (
                Some("Elder".to_owned()),
                "Good, Traveller. Then listen: the chest below was sealed long before the village had a name.".to_owned()
            ),
        ],
        "the transcript keeps the markup and skips the prompt"
    );

    // West opens the history page over the dialogue; East closes it.
    pad(&mut h, GamepadButton::West);
    assert_eq!(
        h.stack(),
        vec![main.clone(), kinds::dialogue(), kinds::page()]
    );
    let page = h.find(&by::screen(kinds::page()));
    assert_eq!(
        h.text_of(h.find(&by::test_id("title").within(page)))
            .as_deref(),
        Some("History")
    );
    assert_eq!(h.focused(), Some(h.find(&by::test_id("done").within(page))));
    pad(&mut h, GamepadButton::East);
    assert_eq!(h.stack(), vec![main.clone(), kinds::dialogue()]);
    assert_eq!(h.dialogue(), Some((menus::greeting_id(), yes.clone())));

    // South: `next: "bye"` is an end. The overlay closes and Talk has the
    // focus again.
    pad(&mut h, GamepadButton::South);
    assert_eq!(h.dialogue(), None);
    assert_eq!(h.stack(), vec![main.clone()]);
    assert_eq!(
        h.dialogue_events(),
        vec![
            DialogueEvent::Entered(DialogueNodeEntered {
                dialogue: menus::greeting_id(),
                node: NodeId::new("bye"),
            }),
            DialogueEvent::Ended(DialogueEnded {
                dialogue: menus::greeting_id(),
                node: NodeId::new("bye"),
                reason: EndReason::Finished,
            }),
        ]
    );
    assert_eq!(h.focused(), Some(find(&h, "talk")), "focus is back on Talk");
    assert_eq!(
        choices(&mut h),
        vec![],
        "no `MenuChoice` came out of the dialogue"
    );
}

/// The settings' Demo tab unlocks the third answer: with `demo.found_key`
/// on, `secret` is enabled and `dialogue_choose` follows it to a narration
/// line with no speaker and no portrait.
#[test]
fn the_found_key_toggle_unlocks_the_secret_answer() {
    let (mut h, store) = open_main();
    wait_for_dialogue(&mut h);
    h.set_value(menus::FOUND_KEY, true);
    h.settle();
    assert_eq!(h.value(menus::FOUND_KEY), Some(Value::Bool(true)));
    assert_eq!(
        store
            .get()
            .and_then(|s| s.values.get(menus::FOUND_KEY).cloned()),
        Some(Value::Bool(true)),
        "a settings key, so the change was saved"
    );
    h.start_dialogue(menus::GREETING);
    h.dialogue_advance();
    h.dialogue_advance();
    assert_eq!(h.dialogue().map(|(_, n)| n), Some(NodeId::new("ask")));
    h.advance(Duration::from_millis(400));
    h.settle();
    assert_eq!(
        h.dialogue_options(),
        vec![
            ("yes".to_owned(), true),
            ("no".to_owned(), true),
            ("secret".to_owned(), true),
        ]
    );
    h.dialogue_choose("secret");
    assert_eq!(h.dialogue().map(|(_, n)| n), Some(NodeId::new("secret")));
    let root = h.find(&by::screen(kinds::dialogue()));
    for id in ["speaker", "portrait"] {
        assert_eq!(
            h.world()
                .get::<Visibility>(h.find(&by::test_id(id).within(root))),
            Some(&Visibility::Hidden),
            "narration hides the {id}"
        );
    }
    h.dialogue_advance();
    assert!(h.dialogue_revealed());
    assert_eq!(
        h.dialogue_history().last().map(|(s, _)| s.clone()),
        Some(None),
        "a narration line has no speaker"
    );
    assert!(h.toasts().is_empty(), "only `yes` earns a toast");
    h.dialogue_advance();
    assert_eq!(h.dialogue(), None);
    assert!(matches!(
        h.dialogue_events().last(),
        Some(DialogueEvent::Ended(DialogueEnded {
            reason: EndReason::Finished,
            ..
        }))
    ));
}

/// The dialogue's say and choice trees over the title, per theme.
#[test]
fn the_dialogue_trees_match_in_three_themes() {
    for theme in ["glass", "paper", "neon"] {
        let (mut h, _) = open_main_in(theme);
        wait_for_dialogue(&mut h);
        h.start_dialogue(menus::GREETING);
        h.dialogue_advance();
        h.settle();
        assert!(h.dialogue_revealed());
        assert_tree_snapshot!(format!("dialogue_say_tree_{theme}"), h.screen_tree());
        h.dialogue_advance();
        h.advance(Duration::from_millis(400));
        h.settle();
        assert_eq!(h.dialogue_options().len(), 3);
        assert_tree_snapshot!(format!("dialogue_choice_tree_{theme}"), h.screen_tree());
    }
}
