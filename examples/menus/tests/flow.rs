//! The menus example, headless: the whole flow by gamepad, the About button
//! a mod-style injection added, and the tree snapshots of the three
//! templates the example opens (menus M2 contract 5).
//!
//! The theme is found because `examples/menus/assets` is a symlink to the
//! workspace `assets/`, which is where Bevy's `AssetPlugin` looks when it
//! resolves against `CARGO_MANIFEST_DIR`.
#![allow(clippy::unwrap_used, clippy::float_cmp)]

use bevy::input::gamepad::GamepadButton;
use bevy::prelude::*;
use menus::{MenusDemoPlugin, main_kind};
use pretty_assertions::assert_eq;
use slotted::menu::{MemorySettings, SettingsStorage, kinds};
use slotted::ui::{SliderState, Value};
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
    assert!(
        h.try_find(&by::test_id("accept_danger").within(dialog))
            .is_some()
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
        Some(h.find(&by::test_id("accept_danger").within(dialog)))
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
