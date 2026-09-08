//! The crate's templates, menu actions, pause, confirm, toast, page and hint
//! bar, headless (menus M2 contract 3.7). Every template opens through
//! `UiHarness::open` in the three themes with the theme asset loaded, and a
//! gamepad walks it from `initial_focus`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::float_cmp)]

use std::path::Path;
use std::time::Duration;

use bevy::asset::AssetPlugin;
use bevy::input::gamepad::GamepadButton;
use bevy::prelude::*;
use pretty_assertions::assert_eq;
use slotted_menu::hint_bar::{HintGlyph, HintLabel};
use slotted_menu::toast::{ToastHost, ToastQueue};
use slotted_menu::{
    ConfirmResult, ConfirmSpec, HintEntries, MenuChoice, MenuConfig, MenuPlugin, PageSpec,
    PendingConfirm, Toast, ToastLevel, ToastSpec, Toasts, confirm, kinds, open_page, templates,
    toast,
};
use slotted_test::prelude::*;
use slotted_theme::{Motion, roles};
use slotted_ui::{
    GlyphSet, InputMode, LocKey, LocText, ScreenDef, SemanticRole, UiAction, UiBindings, Value,
    key_glyph_text,
};

/// The messages the crate wrote, for the tests that count.
#[derive(Resource, Default)]
struct Seen {
    choices: Vec<MenuChoice>,
    confirms: Vec<ConfirmResult>,
}

fn record(
    mut seen: ResMut<Seen>,
    mut choices: MessageReader<MenuChoice>,
    mut confirms: MessageReader<ConfirmResult>,
) {
    seen.choices.extend(choices.read().cloned());
    seen.confirms.extend(confirms.read().cloned());
}

/// A slider screen for the hint bar, with a settings-style tabs node and a
/// button that overrides its verb.
const CONTROLS: &str = r#"
#![enable(implicit_some)]
(
    kind: "test:controls",
    presentation: (mode: "modal", back: "pop"),
    initial_focus: "volume",
    root: (
        type: "panel", role: "panel",
        layout: (direction: "column", gap: 2.0, padding: 3.0, width: 480),
        tags: {"test_id": "controls"},
        children: [
            (type: "slider", label: "test.volume", min: 0.0, max: 100.0, step: 5.0,
             bind: "test.volume", tags: {"test_id": "volume"}),
            (type: "button", label: "test.jump", tags: {"test_id": "jump", "hint.accept": "test.hint.jump"}),
            (type: "custom", kind: "slotted:hint_bar", tags: {"test_id": "hints"}),
            (type: "custom", kind: "slotted:hint_bar",
             tags: {"test_id": "always", "hint.always": "true"}),
        ],
    ),
)
"#;

/// Adds the plugin (unless the facade already did) and the config, before
/// the first frame: `install_strings` reads the config in `PostStartup`.
fn add_menu(config: MenuConfig) -> impl Fn(&mut App) + Send + Sync + 'static {
    move |app: &mut App| {
        if !app.is_plugin_added::<MenuPlugin>() {
            app.add_plugins(MenuPlugin);
        }
        app.insert_resource(config.clone())
            .init_resource::<Seen>()
            .add_systems(Last, record);
    }
}

/// The workspace `assets/` directory: this crate has none of its own and
/// Bevy resolves the asset root against the crate.
fn assets_dir() -> String {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets")
        .canonicalize()
        .expect("the workspace assets directory exists")
        .to_string_lossy()
        .into_owned()
}

fn harness_in(theme: &str, config: MenuConfig) -> UiHarness {
    let mut h = UiHarness::builder()
        .plugins((
            SlottedPlugins::headless().set(AssetPlugin {
                file_path: assets_dir(),
                ..default()
            }),
            add_menu(config),
        ))
        .resolution(1280.0, 720.0)
        .theme(theme)
        .build();
    wait_for_theme(&mut h);
    h
}

fn harness() -> UiHarness {
    harness_in("glass", MenuConfig::default())
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

fn find(h: &UiHarness, id: &str) -> Entity {
    h.find(&by::test_id(id))
}

fn seen(h: &mut UiHarness) -> Seen {
    std::mem::take(&mut h.world_mut().resource_mut::<Seen>())
}

fn choices(h: &mut UiHarness) -> Vec<String> {
    seen(h).choices.into_iter().map(|c| c.id).collect()
}

fn entries(h: &UiHarness, bar: Entity) -> Vec<(UiAction, String)> {
    h.world()
        .get::<HintEntries>(bar)
        .expect("a hint bar carries its entries")
        .0
        .iter()
        .map(|e| (e.action, e.label.0.clone()))
        .collect()
}

/// The glyph texts and the labels the bar renders, in tree order.
fn rendered(h: &mut UiHarness, bar: Entity) -> (Vec<String>, Vec<String>) {
    fn walk(world: &World, entity: Entity, glyphs: &mut Vec<String>, labels: &mut Vec<String>) {
        if let Some(text) = world.get::<Text>(entity) {
            if world.get::<HintGlyph>(entity).is_some() {
                glyphs.push(text.0.clone());
            } else if world.get::<HintLabel>(entity).is_some() {
                labels.push(text.0.clone());
            }
        }
        if let Some(children) = world.get::<Children>(entity) {
            for child in children.iter() {
                walk(world, child, glyphs, labels);
            }
        }
    }
    let mut glyphs = Vec::new();
    let mut labels = Vec::new();
    walk(h.world(), bar, &mut glyphs, &mut labels);
    (glyphs, labels)
}

/// The toasts on screen, oldest first (the column's child order).
fn toasts(h: &mut UiHarness) -> Vec<Entity> {
    let mut q = h
        .world_mut()
        .query_filtered::<&Children, With<slotted_menu::toast::ToastColumn>>();
    let Ok(children) = q.single(h.world()) else {
        return vec![];
    };
    children
        .iter()
        .filter(|c| h.world().get::<Toast>(*c).is_some())
        .collect()
}

fn queued(h: &UiHarness) -> usize {
    h.world().resource::<ToastQueue>().0.len()
}

/// The text a node and its descendants display: every `Text` and
/// `TextSpan` in tree order, joined. A button's label is a child; a rich
/// text's runs are spans.
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

fn args(pairs: &[(&str, Value)]) -> slotted_ui::LocArgs {
    pairs
        .iter()
        .map(|(k, v)| ((*k).to_owned(), v.clone()))
        .collect()
}

// ---------------------------------------------------------------------------
// Templates: parse, register, open in three themes, gamepad walk
// ---------------------------------------------------------------------------

#[test]
fn every_embedded_template_parses_and_names_its_kind() {
    let defs = templates::all();
    let kinds: Vec<_> = defs.iter().map(|d| d.kind.clone()).collect();
    assert_eq!(kinds, kinds::all().to_vec());
    let toast: slotted_ui::UiNodeDef =
        ron::from_str(templates::TOAST_NODE).expect("the toast snippet parses");
    assert_eq!(toast.id(), Some("toast"));
    for def in &defs {
        // The dialogue's focus is its option buttons, spawned per node
        // (menus M3 contract 4.2), so it names none.
        assert!(
            def.initial_focus.is_some() || def.kind == kinds::dialogue(),
            "{} names an initial focus",
            def.kind.0
        );
        let anchors = def.root.anchors();
        assert!(!anchors.is_empty(), "{} carries anchors", def.kind.0);
    }
}

#[test]
fn the_templates_register_at_startup_unless_a_game_registered_the_kind_first() {
    let h = harness();
    let screens = h.world().resource::<slotted_ui::Screens>();
    for kind in kinds::all() {
        assert!(screens.get(&kind).is_some(), "{} registered", kind.0);
    }
    // A game's own pause, registered before the plugin's `PostStartup`.
    let own = ScreenDef {
        kind: kinds::pause(),
        inherits: None,
        root: slotted_ui::UiNodeDef::Panel {
            role: roles::PANEL,
            layout: default(),
            children: vec![],
            tags: slotted_ui::Tags::new().with("test_id", "own_pause"),
        },
        listring: vec![],
        remove: vec![],
        initial_focus: None,
        presentation: default(),
    };
    let mut app = App::new();
    app.add_plugins(SlottedPlugins::headless());
    app.world_mut()
        .resource_mut::<slotted_ui::Screens>()
        .register(own);
    if !app.is_plugin_added::<MenuPlugin>() {
        app.add_plugins(MenuPlugin);
    }
    app.update();
    let screens = app.world().resource::<slotted_ui::Screens>();
    assert_eq!(
        screens.get(&kinds::pause()).unwrap().root.id(),
        Some("own_pause"),
        "the game's pause survives the template registration"
    );
    assert!(screens.get(&kinds::main_menu()).is_some());
}

/// Every button reachable by gamepad from `initial_focus`, in each theme.
fn walk_buttons(theme: &str, kind: slotted_ui::ScreenKind, expected: &[&str]) {
    let mut h = harness_in(theme, MenuConfig::default());
    let root = h.open(kind.clone());
    h.settle();
    assert_eq!(h.stack(), vec![kind.clone()], "{theme}: {} opens", kind.0);
    assert_eq!(h.find(&by::screen(kind.clone())), root);
    let first = find(&h, expected[0]);
    assert_eq!(
        h.focused(),
        Some(first),
        "{theme}: initial focus on `{}`",
        expected[0]
    );
    let mut reached = vec![expected[0].to_owned()];
    for step in expected.iter().skip(1) {
        // Down first (a column), then Right (a row) if focus did not move.
        let before = h.focused();
        h.gamepad(GamepadButton::DPadDown);
        h.settle();
        if h.focused() == before {
            h.gamepad(GamepadButton::DPadRight);
            h.settle();
        }
        let want = find(&h, step);
        assert_eq!(
            h.focused(),
            Some(want),
            "{theme}: {} walks to `{step}`",
            kind.0
        );
        assert!(h.focus_ring().visible, "{theme}: the ring shows on a pad");
        reached.push((*step).to_owned());
    }
    assert_eq!(
        reached,
        expected.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>()
    );
}

#[test]
fn the_main_menu_opens_in_three_themes_and_a_pad_walks_its_buttons() {
    for theme in ["glass", "paper", "neon"] {
        walk_buttons(theme, kinds::main_menu(), &["play", "settings", "quit"]);
    }
}

#[test]
fn the_pause_screen_opens_in_three_themes_and_a_pad_walks_its_buttons() {
    for theme in ["glass", "paper", "neon"] {
        walk_buttons(theme, kinds::pause(), &["resume", "settings", "quit"]);
    }
}

#[test]
fn the_confirm_dialog_opens_in_three_themes_and_a_pad_walks_its_buttons() {
    for theme in ["glass", "paper", "neon"] {
        walk_buttons(theme, kinds::confirm(), &["cancel", "accept"]);
    }
}

#[test]
fn the_page_opens_in_three_themes_with_focus_on_done() {
    for theme in ["glass", "paper", "neon"] {
        walk_buttons(theme, kinds::page(), &["done"]);
    }
}

#[test]
fn the_settings_frame_opens_in_three_themes_and_a_pad_reaches_reset_and_done() {
    for theme in ["glass", "paper", "neon"] {
        let mut h = harness_in(theme, MenuConfig::default());
        h.open(kinds::settings());
        h.settle();
        // The frame's tabs node is empty, so the first focusable is Reset.
        let reset = find(&h, "reset");
        let done = find(&h, "done");
        assert_eq!(h.focused(), Some(reset), "{theme}: focus lands on Reset");
        h.gamepad(GamepadButton::DPadRight);
        h.settle();
        assert_eq!(h.focused(), Some(done), "{theme}: Right reaches Done");
        h.gamepad(GamepadButton::South);
        h.settle();
        assert_eq!(h.stack(), vec![], "{theme}: Done pops the frame");
    }
}

#[test]
fn every_new_role_resolves_in_three_themes_and_the_table_is_complete() {
    assert_eq!(roles::ALL.len(), 105);
    for theme in ["glass", "paper", "neon"] {
        let h = harness_in(theme, MenuConfig::default());
        let active = h.world().resource::<slotted_theme::ActiveTheme>().0.clone();
        let theme_asset = h
            .world()
            .resource::<Assets<slotted_theme::Theme>>()
            .get(&active)
            .unwrap();
        assert_eq!(theme_asset.missing_roles(), vec![], "{theme}");
        for role in [
            roles::TOAST,
            roles::TOAST_INFO,
            roles::TOAST_SUCCESS,
            roles::TOAST_WARNING,
            roles::TOAST_ERROR,
            roles::TOAST_TEXT,
            roles::HINT_BAR,
            roles::HINT_GLYPH,
            roles::HINT_LABEL,
            roles::MENU_TITLE,
            roles::MENU_VERSION,
        ] {
            assert!(theme_asset.material(&role).is_some(), "{theme}: {role}");
        }
        assert!(
            theme_asset
                .material(&slotted_theme::Role::new(
                    slotted_menu::hint_bar::GLYPH_TEXT_ROLE
                ))
                .is_some(),
            "{theme}: the glyph text falls back to hint.glyph"
        );
    }
}

// ---------------------------------------------------------------------------
// Menu actions, strings, the main menu header
// ---------------------------------------------------------------------------

#[test]
fn main_menu_buttons_produce_menu_choices_with_their_ids() {
    let mut h = harness();
    h.open(kinds::main_menu());
    h.settle();
    let play = find(&h, "play");
    h.activate(play);
    h.step(1);
    let quit = find(&h, "quit");
    h.click(quit);
    h.settle();
    let seen = seen(&mut h);
    let ids: Vec<(&str, &str, Entity)> = seen
        .choices
        .iter()
        .map(|c| (c.id.as_str(), c.screen.0.as_str(), c.entity))
        .collect();
    assert_eq!(
        ids,
        vec![
            ("play", "slotted:main_menu", play),
            ("quit", "slotted:main_menu", quit)
        ]
    );
    assert_eq!(
        h.stack(),
        vec![kinds::main_menu()],
        "play and quit are the game's: nothing popped"
    );
}

#[test]
fn the_english_fallbacks_fill_every_template_string() {
    let mut h = harness_in(
        "glass",
        MenuConfig {
            title: LocKey("game.title".to_owned()),
            version: "1.2.0".to_owned(),
            ..default()
        },
    );
    h.open(kinds::main_menu());
    h.settle();
    assert_eq!(text_of(&h, find(&h, "play")), "Play");
    assert_eq!(text_of(&h, find(&h, "settings")), "Settings");
    assert_eq!(text_of(&h, find(&h, "quit")), "Quit");
    assert_eq!(
        text_of(&h, find(&h, "title")),
        "game.title",
        "the game's title key is drawn as written until its catalogue answers"
    );
    assert_eq!(text_of(&h, find(&h, "version")), "Version 1.2.0");
    let title = h.world().get::<LocText>(find(&h, "title")).unwrap();
    assert_eq!(title.key.0, "game.title");
}

#[test]
fn an_empty_version_removes_the_version_node() {
    let mut h = harness();
    h.open(kinds::main_menu());
    h.settle();
    assert!(h.try_find(&by::test_id("version")).is_none());
    assert_eq!(text_of(&h, find(&h, "title")), "Main menu");
}

#[test]
fn a_game_catalogue_overrides_the_fallback() {
    struct Game;
    impl slotted_ui::Localizer for Game {
        fn resolve(&self, key: &LocKey, _: &slotted_ui::LocArgs) -> Option<String> {
            (key.0 == "slotted.menu.play").then(|| "Start".to_owned())
        }
    }
    let mut h = harness();
    h.world_mut()
        .resource_mut::<slotted_ui::Localization>()
        .set_primary(Game);
    h.open(kinds::main_menu());
    h.settle();
    assert_eq!(text_of(&h, find(&h, "play")), "Start");
    assert_eq!(text_of(&h, find(&h, "quit")), "Quit");
}

// ---------------------------------------------------------------------------
// Pause
// ---------------------------------------------------------------------------

#[test]
fn menu_opens_the_pause_screen_when_nothing_is_open_and_pops_it_again() {
    let mut h = harness();
    assert_eq!(h.stack(), vec![]);
    h.action(UiAction::Menu);
    h.settle();
    assert_eq!(h.stack(), vec![kinds::pause()]);
    assert_eq!(h.focused(), Some(find(&h, "resume")));
    h.action(UiAction::Menu);
    h.settle();
    assert_eq!(h.stack(), vec![], "Menu on top of the pause pops it");
    h.action(UiAction::Menu);
    h.settle();
    assert_eq!(h.stack(), vec![kinds::pause()]);
    h.action(UiAction::Back);
    h.settle();
    assert_eq!(h.stack(), vec![], "Back pops it too");
}

/// `Escape` is both `Back` and `Menu` by default (menus M2 contract 1.1).
/// With the pause over a page, one press pops the pause and only the pause;
/// with nothing open, the same key pauses; Start on a pad (`Menu` alone)
/// still pops the pause.
#[test]
fn escape_on_the_pause_pops_only_the_pause_and_start_pops_it_too() {
    let mut h = harness();
    assert_eq!(
        h.world().resource::<UiBindings>().first_key(UiAction::Menu),
        Some(KeyCode::Escape)
    );
    assert_eq!(
        h.world().resource::<UiBindings>().first_key(UiAction::Back),
        Some(KeyCode::Escape)
    );
    // A page underneath: pause over it through the pad, then Escape.
    h.open(kinds::page());
    h.settle();
    h.gamepad(GamepadButton::Start);
    h.settle();
    assert_eq!(
        h.stack(),
        vec![kinds::page()],
        "Menu over a page does nothing"
    );
    h.action(UiAction::Back);
    h.settle();
    assert_eq!(h.stack(), vec![]);
    h.open(kinds::page());
    h.settle();
    h.world_mut().commands().queue(|world: &mut World| {
        let def = world
            .resource::<slotted_ui::Screens>()
            .get(&kinds::pause())
            .unwrap()
            .clone();
        slotted_ui::push_screen(&mut world.commands(), def, None);
    });
    h.settle();
    assert_eq!(h.stack(), vec![kinds::page(), kinds::pause()]);
    h.key(KeyCode::Escape);
    h.settle();
    assert_eq!(
        h.stack(),
        vec![kinds::page()],
        "one Escape pops the pause and leaves the page"
    );
    h.key(KeyCode::Escape);
    h.settle();
    assert_eq!(h.stack(), vec![], "the page pops on Escape as Back");
    h.key(KeyCode::Escape);
    h.settle();
    assert_eq!(
        h.stack(),
        vec![kinds::pause()],
        "with nothing open Escape pauses"
    );
    h.gamepad(GamepadButton::Start);
    h.settle();
    assert_eq!(h.stack(), vec![], "Start pops the pause");
}

#[test]
fn menu_does_not_pause_over_a_page_and_pause_on_menu_can_be_turned_off() {
    let mut h = harness();
    h.open(kinds::main_menu());
    h.settle();
    h.action(UiAction::Menu);
    h.settle();
    assert_eq!(h.stack(), vec![kinds::main_menu()]);

    let mut h = harness_in(
        "glass",
        MenuConfig {
            pause_on_menu: false,
            ..default()
        },
    );
    h.action(UiAction::Menu);
    h.settle();
    assert_eq!(h.stack(), vec![]);
}

#[test]
fn resume_pops_the_pause_and_settings_pushes_the_configured_kind() {
    let mut h = harness_in(
        "glass",
        MenuConfig {
            settings_kind: kinds::page(),
            ..default()
        },
    );
    h.action(UiAction::Menu);
    h.settle();
    h.activate(find(&h, "resume"));
    h.settle();
    assert_eq!(h.stack(), vec![]);
    assert_eq!(choices(&mut h), vec!["resume"]);

    h.action(UiAction::Menu);
    h.settle();
    h.activate(find(&h, "settings"));
    h.settle();
    assert_eq!(h.stack(), vec![kinds::pause(), kinds::page()]);
    assert_eq!(choices(&mut h), vec!["settings"]);
    h.action(UiAction::Back);
    h.settle();
    assert_eq!(h.stack(), vec![kinds::pause()]);
}

#[test]
fn settings_with_an_unregistered_kind_warns_and_pushes_nothing() {
    let mut h = harness_in(
        "glass",
        MenuConfig {
            settings_kind: slotted_ui::ScreenKind::new("game:nowhere"),
            ..default()
        },
    );
    h.action(UiAction::Menu);
    h.settle();
    h.activate(find(&h, "settings"));
    h.settle();
    assert_eq!(h.stack(), vec![kinds::pause()]);
    assert_eq!(choices(&mut h), vec!["settings"]);
}

// ---------------------------------------------------------------------------
// Confirm
// ---------------------------------------------------------------------------

fn open_confirm(h: &mut UiHarness, spec: ConfirmSpec) {
    let world = h.world_mut();
    let mut commands = world.commands();
    confirm(&mut commands, spec);
    world.flush();
    h.settle();
}

#[test]
fn confirm_rewrites_the_title_and_the_message_with_args_and_focuses_cancel() {
    let mut h = harness();
    open_confirm(
        &mut h,
        ConfirmSpec::new("quit", "slotted.menu.binding_moved")
            .title("slotted.menu.quit")
            .buttons("slotted.menu.ok", "slotted.menu.back")
            .arg("action", "Jump"),
    );
    assert_eq!(h.stack(), vec![kinds::confirm()]);
    assert_eq!(text_of(&h, find(&h, "title")), "Quit");
    assert_eq!(text_of(&h, find(&h, "cancel")), "Back");
    assert_eq!(text_of(&h, find(&h, "accept")), "OK");
    assert_eq!(
        h.world()
            .get::<slotted_ui::ButtonState>(find(&h, "accept"))
            .unwrap()
            .variant,
        slotted_ui::ButtonVariant::Primary
    );
    let message = find(&h, "message");
    let rich = h.world().get::<slotted_ui::RichText>(message).unwrap();
    assert_eq!(rich.key.0, "slotted.menu.binding_moved");
    assert_eq!(rich.args, args(&[("action", Value::from("Jump"))]));
    assert_eq!(text_of(&h, message), "Jump lost its key");
    assert_eq!(h.focused(), Some(find(&h, "cancel")));
    let root = h.find(&by::screen(kinds::confirm()));
    assert_eq!(
        h.world().get::<PendingConfirm>(root),
        Some(&PendingConfirm("quit".to_owned()))
    );
}

#[test]
fn accept_cancel_and_back_each_answer_exactly_once() {
    let mut h = harness();
    open_confirm(&mut h, ConfirmSpec::new("a", "slotted.menu.confirm_title"));
    h.activate(find(&h, "accept"));
    h.settle();
    assert_eq!(h.stack(), vec![]);
    let first = seen(&mut h);
    assert_eq!(
        first.confirms,
        vec![ConfirmResult {
            id: "a".to_owned(),
            accepted: true
        }]
    );
    assert_eq!(
        first
            .choices
            .iter()
            .map(|c| c.id.as_str())
            .collect::<Vec<_>>(),
        vec!["accept"],
        "the choice still reaches the game"
    );

    open_confirm(&mut h, ConfirmSpec::new("b", "slotted.menu.confirm_title"));
    h.activate(find(&h, "cancel"));
    h.settle();
    assert_eq!(h.stack(), vec![]);
    assert_eq!(
        seen(&mut h).confirms,
        vec![ConfirmResult {
            id: "b".to_owned(),
            accepted: false
        }]
    );

    open_confirm(&mut h, ConfirmSpec::new("c", "slotted.menu.confirm_title"));
    h.action(UiAction::Back);
    h.settle();
    assert_eq!(h.stack(), vec![]);
    assert_eq!(
        seen(&mut h).confirms,
        vec![ConfirmResult {
            id: "c".to_owned(),
            accepted: false
        }]
    );
    h.step(2);
    assert_eq!(seen(&mut h).confirms, vec![], "nothing answers twice");
}

#[test]
fn danger_keeps_the_danger_button_and_a_confirm_stacks_over_a_confirm() {
    let mut h = harness();
    open_confirm(
        &mut h,
        ConfirmSpec::new("delete", "slotted.menu.confirm_title").danger(),
    );
    let danger = find(&h, "accept");
    assert_eq!(
        h.world()
            .get::<slotted_ui::ButtonState>(danger)
            .unwrap()
            .variant,
        slotted_ui::ButtonVariant::Danger
    );
    open_confirm(
        &mut h,
        ConfirmSpec::new("really", "slotted.menu.confirm_title"),
    );
    assert_eq!(h.stack(), vec![kinds::confirm(), kinds::confirm()]);
    // The top dialog's accept answers `really`; the lower one is untouched.
    let top = h.find(&by::screen(kinds::confirm()).index(1));
    let accept = h.find(&by::test_id("accept").within(top));
    h.activate(accept);
    h.settle();
    assert_eq!(h.stack(), vec![kinds::confirm()]);
    assert_eq!(
        seen(&mut h).confirms,
        vec![ConfirmResult {
            id: "really".to_owned(),
            accepted: true
        }]
    );
    h.activate(danger);
    h.settle();
    assert_eq!(h.stack(), vec![]);
    assert_eq!(
        seen(&mut h).confirms,
        vec![ConfirmResult {
            id: "delete".to_owned(),
            accepted: true
        }]
    );
}

// ---------------------------------------------------------------------------
// Toast
// ---------------------------------------------------------------------------

fn show(h: &mut UiHarness, spec: ToastSpec) {
    let world = h.world_mut();
    let mut commands = world.commands();
    toast(&mut commands, spec);
    world.flush();
    h.settle();
}

#[test]
fn toasts_stack_upward_in_their_own_band_and_expire_on_virtual_time() {
    let mut h = harness();
    show(
        &mut h,
        ToastSpec::new("slotted.menu.paused").duration(Duration::from_secs(2)),
    );
    show(
        &mut h,
        ToastSpec::new("slotted.menu.quit")
            .level(ToastLevel::Error)
            .duration(Duration::from_secs(4)),
    );
    let up = toasts(&mut h);
    assert_eq!(up.len(), 2);
    let host = h
        .world_mut()
        .query_filtered::<(Entity, &GlobalZIndex), With<ToastHost>>()
        .single(h.world())
        .unwrap();
    assert_eq!(host.1.0, slotted_ui::zbands::TOAST);
    let (first, second) = (up[0], up[1]);
    assert_eq!(
        h.world().get::<Toast>(first).unwrap().level,
        ToastLevel::Info
    );
    assert_eq!(
        h.world().get::<Toast>(second).unwrap().level,
        ToastLevel::Error
    );
    assert_eq!(
        h.world().get::<slotted_theme::Themed>(second).unwrap().0,
        roles::TOAST_ERROR
    );
    let (a, b) = (h.rect_of(first), h.rect_of(second));
    assert!(
        b.max.y <= a.min.y,
        "the second toast ({b:?}) sits above the first ({a:?})"
    );
    let window = 720.0;
    assert!(
        a.max.y < window && a.max.y > window - 80.0,
        "near the bottom edge: {a:?}"
    );
    assert!((a.center().x - 640.0).abs() < 1.0, "centred: {a:?}");
    let text = h.find(&by::test_id("toast.text").within(first));
    assert_eq!(text_of(&h, text), "Paused");
    assert_eq!(h.stack(), vec![], "toasts are not stack entries");

    h.advance(Duration::from_millis(2500));
    h.settle();
    assert_eq!(toasts(&mut h), vec![second], "the first expired and faded");
    h.advance(Duration::from_millis(2000));
    h.settle();
    assert_eq!(toasts(&mut h), vec![]);
}

#[test]
fn toasts_past_max_visible_queue_in_order() {
    let mut h = harness();
    h.world_mut().insert_resource(Toasts { max_visible: 2 });
    for (i, key) in [
        "slotted.menu.play",
        "slotted.menu.settings",
        "slotted.menu.quit",
    ]
    .iter()
    .enumerate()
    {
        show(
            &mut h,
            ToastSpec::new(*key).duration(Duration::from_secs(1 + i as u64)),
        );
    }
    assert_eq!(toasts(&mut h).len(), 2);
    assert_eq!(queued(&h), 1);
    h.advance(Duration::from_millis(1500));
    h.settle();
    let up = toasts(&mut h);
    assert_eq!(up.len(), 2, "the queued toast took the freed slot");
    assert_eq!(queued(&h), 0);
    let texts: Vec<String> = up
        .iter()
        .map(|t| text_of(&h, h.find(&by::test_id("toast.text").within(*t))))
        .collect();
    assert_eq!(texts, vec!["Settings", "Quit"]);
}

#[test]
fn a_toast_shows_over_a_page_and_a_modal_without_taking_focus() {
    let mut h = harness();
    h.open(kinds::main_menu());
    h.settle();
    h.action(UiAction::Menu);
    h.settle();
    let focused = h.focused();
    show(&mut h, ToastSpec::new("slotted.menu.paused"));
    let up = toasts(&mut h);
    assert_eq!(up.len(), 1);
    assert!(h.is_visible(up[0]));
    assert_eq!(h.focused(), focused);
    assert_eq!(h.stack(), vec![kinds::main_menu()]);
    let default_ms = u64::from(
        h.world()
            .resource::<Assets<slotted_theme::Theme>>()
            .get(&h.world().resource::<slotted_theme::ActiveTheme>().0)
            .unwrap()
            .tokens
            .durations
            .slow,
    ) * 10;
    let remaining = h.world().get::<Toast>(up[0]).unwrap().remaining;
    let default = Duration::from_millis(default_ms);
    assert!(
        remaining <= default && remaining + Duration::from_secs(1) > default,
        "the default duration is ten `slow`: {remaining:?}"
    );
}

#[test]
fn under_reduced_motion_a_toast_appears_and_vanishes_without_a_fade() {
    let mut h = harness();
    h.world_mut().insert_resource(Motion::REDUCED);
    show(
        &mut h,
        ToastSpec::new("slotted.menu.paused").duration(Duration::from_millis(100)),
    );
    let up = toasts(&mut h);
    assert_eq!(up.len(), 1);
    assert!(h.world().get::<slotted_theme::Tween>(up[0]).is_none());
    h.advance(Duration::from_millis(150));
    h.step(2);
    assert_eq!(toasts(&mut h), vec![]);
}

// ---------------------------------------------------------------------------
// Page
// ---------------------------------------------------------------------------

#[test]
fn a_page_opens_with_its_title_and_body_and_closes_on_done_or_back() {
    let mut h = harness();
    {
        let world = h.world_mut();
        let mut commands = world.commands();
        open_page(
            &mut commands,
            PageSpec::new("slotted.menu.settings_title", "slotted.menu.version")
                .arg("version", "9"),
        );
        world.flush();
    }
    h.settle();
    assert_eq!(h.stack(), vec![kinds::page()]);
    assert_eq!(text_of(&h, find(&h, "title")), "Settings");
    assert_eq!(text_of(&h, find(&h, "body")), "Version 9");
    assert_eq!(h.focused(), Some(find(&h, "done")));
    h.gamepad(GamepadButton::South);
    h.settle();
    assert_eq!(h.stack(), vec![]);

    {
        let world = h.world_mut();
        let mut commands = world.commands();
        open_page(&mut commands, PageSpec::new("a", "b"));
        world.flush();
    }
    h.settle();
    assert_eq!(h.stack(), vec![kinds::page()]);
    h.action(UiAction::Back);
    h.settle();
    assert_eq!(h.stack(), vec![]);
}

// ---------------------------------------------------------------------------
// Hint bar
// ---------------------------------------------------------------------------

#[test]
fn the_hint_bar_lists_select_and_close_on_a_pause_button() {
    let mut h = harness();
    h.action(UiAction::Menu);
    h.settle();
    let bar = find(&h, "hints");
    assert_eq!(
        entries(&h, bar),
        vec![
            (UiAction::Accept, "slotted.menu.select".to_owned()),
            (UiAction::Back, "slotted.menu.close".to_owned()),
        ]
    );
    assert!(h.is_visible(bar), "keyboard mode shows the bar");
    let bindings = h.world().resource::<UiBindings>().clone();
    let (glyphs, labels) = rendered(&mut h, bar);
    assert_eq!(
        glyphs,
        vec![
            key_glyph_text(
                UiAction::Accept,
                InputMode::Keyboard,
                &bindings,
                GlyphSet::Xbox
            ),
            key_glyph_text(
                UiAction::Back,
                InputMode::Keyboard,
                &bindings,
                GlyphSet::Xbox
            ),
        ]
    );
    assert_eq!(labels, vec!["Select", "Close"]);
    assert_eq!(glyphs[0], "Enter");
}

#[test]
fn a_page_bar_says_back_and_the_main_menu_bar_has_no_back() {
    let mut h = harness();
    h.open(kinds::main_menu());
    h.settle();
    h.set_input_mode(InputMode::Keyboard);
    h.settle();
    let bar = find(&h, "hints");
    assert_eq!(
        entries(&h, bar),
        vec![(UiAction::Accept, "slotted.menu.select".to_owned())],
        "`back: ignore` on the main menu"
    );
    {
        let world = h.world_mut();
        let mut commands = world.commands();
        open_page(&mut commands, PageSpec::new("a", "b"));
        world.flush();
    }
    h.settle();
    let page = h.find(&by::screen(kinds::page()));
    let bar = h.find(&by::test_id("hints").within(page));
    assert_eq!(
        entries(&h, bar),
        vec![
            (UiAction::Accept, "slotted.menu.select".to_owned()),
            (UiAction::Back, "slotted.menu.back".to_owned()),
        ]
    );
}

#[test]
fn the_hint_bar_shows_adjust_with_two_glyphs_on_a_slider_and_honours_the_accept_tag() {
    let mut h = harness();
    h.open(ScreenDef::from_ron(CONTROLS).unwrap());
    h.settle();
    h.set_input_mode(InputMode::Gamepad);
    h.settle();
    let bar = find(&h, "hints");
    assert_eq!(h.focused(), Some(find(&h, "volume")));
    assert_eq!(
        entries(&h, bar),
        vec![
            (UiAction::Left, "slotted.menu.adjust".to_owned()),
            (UiAction::Right, "slotted.menu.adjust".to_owned()),
            (UiAction::Back, "slotted.menu.close".to_owned()),
        ]
    );
    let (glyphs, labels) = rendered(&mut h, bar);
    assert_eq!(glyphs.len(), 3);
    assert_eq!(
        labels,
        vec!["Adjust", "Close"],
        "two glyphs share one label"
    );
    let bindings = h.world().resource::<UiBindings>().clone();
    assert_eq!(
        glyphs[0],
        key_glyph_text(
            UiAction::Left,
            InputMode::Gamepad,
            &bindings,
            GlyphSet::Xbox
        )
    );

    h.gamepad(GamepadButton::DPadDown);
    h.settle();
    assert_eq!(h.focused(), Some(find(&h, "jump")));
    assert_eq!(
        entries(&h, bar)[0],
        (UiAction::Accept, "test.hint.jump".to_owned()),
        "the `hint.accept` tag overrides the verb"
    );
    let (_, labels) = rendered(&mut h, bar);
    assert_eq!(labels[0], "test.hint.jump");
}

#[test]
fn the_hint_bar_hides_in_pointer_mode_unless_always() {
    let mut h = harness();
    h.open(ScreenDef::from_ron(CONTROLS).unwrap());
    h.settle();
    h.set_input_mode(InputMode::Keyboard);
    h.settle();
    let bar = find(&h, "hints");
    let always = find(&h, "always");
    assert!(h.is_visible(bar));
    assert!(h.is_visible(always));
    h.set_input_mode(InputMode::Pointer);
    h.settle();
    assert!(!h.is_visible(bar), "pointer mode hides the plain bar");
    assert!(h.is_visible(always), "`hint.always` keeps this one");
    assert!(
        !entries(&h, always).is_empty(),
        "the entries are still kept up"
    );
    h.set_input_mode(InputMode::Gamepad);
    h.settle();
    assert!(h.is_visible(bar));
}

#[test]
fn the_hint_bar_re_renders_its_glyphs_when_the_glyph_set_or_the_mode_flips() {
    let mut h = harness();
    h.action(UiAction::Menu);
    h.settle();
    let bar = find(&h, "hints");
    let bindings = h.world().resource::<UiBindings>().clone();
    let (glyphs, _) = rendered(&mut h, bar);
    assert_eq!(glyphs[0], "Enter");

    h.gamepad(GamepadButton::DPadDown);
    h.settle();
    assert_eq!(h.input_mode(), InputMode::Gamepad);
    let (glyphs, _) = rendered(&mut h, bar);
    let xbox = key_glyph_text(
        UiAction::Accept,
        InputMode::Gamepad,
        &bindings,
        GlyphSet::Xbox,
    );
    assert_eq!(glyphs[0], xbox);

    for set in [GlyphSet::PlayStation, GlyphSet::Switch, GlyphSet::Generic] {
        h.world_mut().insert_resource(set);
        h.settle();
        let (glyphs, _) = rendered(&mut h, bar);
        let want = key_glyph_text(UiAction::Accept, InputMode::Gamepad, &bindings, set);
        assert_eq!(glyphs[0], want, "{set:?}");
    }
}

#[test]
fn paper_brackets_the_glyph_and_glass_draws_a_pill() {
    let mut h = harness_in("paper", MenuConfig::default());
    h.action(UiAction::Menu);
    h.settle();
    let bar = find(&h, "hints");
    let (glyphs, _) = rendered(&mut h, bar);
    assert_eq!(glyphs[0], "[Enter]");

    let mut h = harness_in("glass", MenuConfig::default());
    h.action(UiAction::Menu);
    h.settle();
    let bar = find(&h, "hints");
    let (glyphs, _) = rendered(&mut h, bar);
    assert_eq!(glyphs[0], "Enter");
    let pill = h
        .world_mut()
        .query_filtered::<(&slotted_theme::Themed, &BackgroundColor), Without<Text>>()
        .iter(h.world())
        .find(|(themed, _)| themed.0 == roles::HINT_GLYPH)
        .map(|(_, bg)| bg.0);
    assert!(
        pill.is_some_and(|c| c.alpha() > 0.0),
        "the pill has a fill: {pill:?}"
    );
}

#[test]
fn the_hint_bar_lists_the_tab_actions_on_a_screen_with_tabs() {
    const TABBED: &str = r#"
#![enable(implicit_some)]
(
    kind: "test:tabbed",
    presentation: (mode: "modal", back: "pop"),
    root: (
        type: "panel", role: "panel",
        layout: (direction: "column", gap: 2.0, padding: 3.0, width: 480),
        children: [
            (type: "tabs", tabs: [(id: "a", label: "a"), (id: "b", label: "b")],
             tags: {"test_id": "tabs"},
             children: [
                (type: "button", label: "x", tags: {"test_id": "x"}),
                (type: "button", label: "y", tags: {"test_id": "y"}),
             ]),
            (type: "custom", kind: "slotted:hint_bar", tags: {"test_id": "hints"}),
        ],
    ),
)
"#;
    let mut h = harness();
    h.open(ScreenDef::from_ron(TABBED).unwrap());
    h.settle();
    h.set_input_mode(InputMode::Keyboard);
    h.settle();
    let bar = find(&h, "hints");
    let got = entries(&h, bar);
    assert_eq!(
        got.iter().map(|(a, _)| *a).collect::<Vec<_>>(),
        vec![
            UiAction::Accept,
            UiAction::Back,
            UiAction::TabPrev,
            UiAction::TabNext
        ]
    );
    assert_eq!(got[2].1, "slotted.menu.prev_tab");
    assert_eq!(got[3].1, "slotted.menu.next_tab");
    let (_, labels) = rendered(&mut h, bar);
    assert_eq!(labels, vec!["Select", "Close", "Previous tab", "Next tab"]);
}

/// `pop_on_back` never pops an overlay, so its bar must not promise a Back
/// whatever the file's `back` policy says (menus M3 closed the M2 follow-up).
#[test]
fn the_hint_bar_on_an_overlay_lists_no_back_even_with_a_pop_policy() {
    const OVERLAY: &str = r#"
#![enable(implicit_some)]
(
    kind: "test:overlay",
    presentation: (mode: "overlay", back: "pop", focus: true),
    root: (
        type: "panel", role: "panel",
        layout: (direction: "column", gap: 2.0, padding: 3.0, width: 480),
        children: [
            (type: "button", label: "x", tags: {"test_id": "x"}),
            (type: "custom", kind: "slotted:hint_bar", tags: {"test_id": "hints"}),
        ],
    ),
)
"#;
    let mut h = harness();
    h.open(ScreenDef::from_ron(OVERLAY).unwrap());
    h.settle();
    h.set_input_mode(InputMode::Keyboard);
    h.settle();
    let bar = find(&h, "hints");
    assert_eq!(
        entries(&h, bar),
        vec![(UiAction::Accept, "slotted.menu.select".to_owned())],
        "Select for the focused button, and no Back"
    );
}

#[test]
fn the_semantic_roles_map_to_their_verbs() {
    use slotted_menu::hint_bar::verb_for;
    let label = |role: SemanticRole| {
        verb_for(&role)
            .into_iter()
            .map(|e| e.label.0)
            .next()
            .unwrap_or_default()
    };
    assert_eq!(label(SemanticRole::Button), "slotted.menu.select");
    assert_eq!(label(SemanticRole::Toggle), "slotted.menu.toggle");
    assert_eq!(label(SemanticRole::Slider), "slotted.menu.adjust");
    assert_eq!(label(SemanticRole::Select), "slotted.menu.change");
    assert_eq!(label(SemanticRole::RadioGroup), "slotted.menu.change");
    assert_eq!(label(SemanticRole::TextField), "slotted.menu.edit");
    assert_eq!(label(SemanticRole::KeyBinding), "slotted.menu.rebind");
    assert_eq!(label(SemanticRole::Slot), "slotted.menu.pick_up");
    assert_eq!(label(SemanticRole::Tab), "slotted.menu.select");
    assert_eq!(label(SemanticRole::Text), "");
}

#[test]
fn a_focused_primary_button_keeps_its_inverse_label_at_the_neighbours_size() {
    for theme in ["glass", "paper", "neon"] {
        let mut h = harness_in(theme, MenuConfig::default());
        h.open(kinds::main_menu());
        h.settle();
        let label_of = |h: &UiHarness, id: &str| {
            let button = find(h, id);
            let label = h
                .world()
                .get::<Children>(button)
                .unwrap()
                .iter()
                .next()
                .unwrap();
            let role = h
                .world()
                .get::<slotted_theme::Themed>(label)
                .unwrap()
                .0
                .clone();
            let size = h.world().get::<TextFont>(label).unwrap().font_size;
            (role.as_str().to_owned(), size)
        };
        // Play is primary and focused on open.
        assert_eq!(
            h.world()
                .get::<slotted_theme::Themed>(find(&h, "play"))
                .unwrap()
                .0,
            slotted_theme::Role::new("button.primary.focus"),
            "{theme}"
        );
        let (play, play_size) = label_of(&h, "play");
        let (settings, settings_size) = label_of(&h, "settings");
        assert_eq!(
            play, "control.label.inverse",
            "{theme}: inverse while focused"
        );
        assert_eq!(settings, "control.label", "{theme}");
        assert_eq!(
            play_size, settings_size,
            "{theme}: the same size as its neighbour"
        );
        h.gamepad(GamepadButton::DPadDown);
        h.settle();
        let (play, play_size) = label_of(&h, "play");
        assert_eq!(play, "control.label.inverse", "{theme}: inverse at rest");
        assert_eq!(play_size, settings_size, "{theme}");
    }
}

#[test]
fn a_toast_arrives_through_the_fade_preset() {
    // Paper: its toasts are `Solid`, so they carry a `BackgroundColor` with
    // or without the `blur` feature (a glass panel under `blur` is a
    // material node with no alpha to tween, the same limit the stack's
    // page fade has).
    let mut h = harness_in("paper", MenuConfig::default());
    {
        let world = h.world_mut();
        let mut commands = world.commands();
        toast(&mut commands, ToastSpec::new("slotted.menu.paused"));
        world.flush();
    }
    h.step(1);
    let up = toasts(&mut h);
    assert_eq!(up.len(), 1);
    let tween = h
        .world()
        .get::<slotted_theme::Tween>(up[0])
        .expect("the toast fades in");
    assert!(matches!(
        tween.target,
        slotted_theme::TweenTarget::Alpha { from: 0.0, .. }
    ));
    assert!(!tween.duration.is_zero());
    let alpha = h.world().get::<BackgroundColor>(up[0]).unwrap().0.alpha();
    assert!(alpha < 0.9, "still fading in: {alpha}");
    h.settle();
    assert!(h.world().get::<slotted_theme::Tween>(up[0]).is_none());
    let alpha = h.world().get::<BackgroundColor>(up[0]).unwrap().0.alpha();
    assert!(alpha > 0.5, "painted: {alpha}");
}

// ---------------------------------------------------------------------------
// Review fixes (M2): Escape ownership, the opt-in, pointer-mode bars, titles
// ---------------------------------------------------------------------------

/// The HUD editor's pattern: a consumer in the `Input` set that claims `Back`
/// on Escape when it has something to cancel.
#[derive(Resource, Default)]
struct Cancelling(bool);

fn claim_back_when_cancelling(
    cancelling: Res<Cancelling>,
    mut events: MessageReader<slotted_ui::UiActionEvent>,
    mut claims: ResMut<slotted_ui::UiActionClaims>,
) {
    let back = events.read().any(|e| e.action == UiAction::Back);
    if back && cancelling.0 {
        claims.claim(UiAction::Back);
    }
}

#[test]
fn an_escape_another_consumer_claimed_does_not_pause_but_start_still_does() {
    let mut h = UiHarness::builder()
        .plugins((
            SlottedPlugins::headless().set(AssetPlugin {
                file_path: assets_dir(),
                ..default()
            }),
            add_menu(MenuConfig::default()),
            |app: &mut App| {
                app.init_resource::<Cancelling>().add_systems(
                    Update,
                    claim_back_when_cancelling
                        .after(slotted_ui::UiActionEmit)
                        .in_set(slotted_ui::SlottedUiSet::Input),
                );
            },
        ))
        .resolution(1280.0, 720.0)
        .theme("glass")
        .build();
    wait_for_theme(&mut h);
    h.world_mut().resource_mut::<Cancelling>().0 = true;
    h.key(KeyCode::Escape);
    h.settle();
    assert_eq!(
        h.stack(),
        vec![],
        "Escape belonged to the drag cancel, not the pause"
    );
    h.gamepad(GamepadButton::Start);
    h.settle();
    assert_eq!(h.stack(), vec![kinds::pause()], "a pure Menu press pauses");
    h.world_mut().resource_mut::<Cancelling>().0 = false;
    h.key(KeyCode::Escape);
    h.settle();
    assert_eq!(h.stack(), vec![], "Escape as Back pops the pause");
    h.key(KeyCode::Escape);
    h.settle();
    assert_eq!(h.stack(), vec![kinds::pause()], "and unowned, it pauses");
}

#[test]
fn without_a_menu_config_nothing_pauses_and_settings_is_the_games_choice() {
    let mut h = UiHarness::builder()
        .plugins((
            SlottedPlugins::headless().set(AssetPlugin {
                file_path: assets_dir(),
                ..default()
            }),
            |app: &mut App| {
                if !app.is_plugin_added::<MenuPlugin>() {
                    app.add_plugins(MenuPlugin);
                }
                app.init_resource::<Seen>().add_systems(Last, record);
            },
        ))
        .resolution(1280.0, 720.0)
        .theme("glass")
        .build();
    wait_for_theme(&mut h);
    assert!(
        h.world().get_resource::<MenuConfig>().is_none(),
        "the plugin inserts no config of its own"
    );
    h.key(KeyCode::Escape);
    h.settle();
    assert_eq!(h.stack(), vec![], "no config, no pause");
    h.gamepad(GamepadButton::Start);
    h.settle();
    assert_eq!(h.stack(), vec![]);
    // The templates still register and a choice is still written; only the
    // built-in settings push is off.
    h.open(kinds::pause());
    h.settle();
    h.activate(find(&h, "settings"));
    h.settle();
    assert_eq!(h.stack(), vec![kinds::pause()], "settings pushed nothing");
    let ids: Vec<String> = h
        .world()
        .resource::<Seen>()
        .choices
        .iter()
        .map(|c| c.id.clone())
        .collect();
    assert_eq!(ids, vec!["settings".to_owned()]);
    h.activate(find(&h, "resume"));
    h.settle();
    assert_eq!(h.stack(), vec![], "resume still pops: it needs no config");
}

#[test]
fn a_hint_bar_spawned_in_pointer_mode_starts_hidden() {
    let mut h = harness();
    h.set_input_mode(InputMode::Pointer);
    h.settle();
    h.open(ScreenDef::from_ron(CONTROLS).unwrap());
    h.settle();
    let bar = find(&h, "hints");
    let always = find(&h, "always");
    assert!(
        !h.is_visible(bar),
        "a plain bar spawned in pointer mode is hidden"
    );
    assert!(h.is_visible(always), "`hint.always` shows regardless");
    h.set_input_mode(InputMode::Keyboard);
    h.settle();
    assert!(h.is_visible(bar));
}

#[test]
fn a_games_own_main_menu_keeps_its_title_and_version() {
    let own = ScreenDef::from_ron(
        r#"#![enable(implicit_some)]
        (
            kind: "slotted:main_menu",
            initial_focus: "play",
            root: (type: "panel", role: "panel", tags: {"test_id": "own_main"}, children: [
                (type: "text", key: "game.title", style: "display", tags: {"test_id": "title"}),
                (type: "text", key: "game.version", style: "caption", tags: {"test_id": "version"}),
                (type: "button", label: "slotted.menu.play", tags: {"test_id": "play", "menu": "play"}),
            ]),
        )"#,
    )
    .unwrap();
    let mut h = UiHarness::builder()
        .plugins((
            SlottedPlugins::headless().set(AssetPlugin {
                file_path: assets_dir(),
                ..default()
            }),
            move |app: &mut App| {
                app.world_mut()
                    .resource_mut::<slotted_ui::Screens>()
                    .register(own.clone());
                if !app.is_plugin_added::<MenuPlugin>() {
                    app.add_plugins(MenuPlugin);
                }
                app.insert_resource(MenuConfig {
                    title: LocKey("slotted.menu.title".to_owned()),
                    version: String::new(),
                    ..default()
                });
            },
        ))
        .resolution(1280.0, 720.0)
        .theme("glass")
        .build();
    wait_for_theme(&mut h);
    h.open(kinds::main_menu());
    h.settle();
    assert_eq!(text_of(&h, find(&h, "title")), "game.title");
    assert_eq!(
        text_of(&h, find(&h, "version")),
        "game.version",
        "an empty config version removes only the crate's node"
    );
}
