//! The settings demo, headless (menus M1 contract section 5 and 6, rebuilt on
//! the M2 `SettingsSpec`): the screen `showcase::settings::spec` generates
//! over the `slotted:settings` frame, with the store seed and the rules the
//! spec derives and the showcase's guard, in all three themes. A gamepad walks every control from
//! `initial_focus`, every control writes and reads through the store, the
//! guard's refusal snaps a slider back, the select popup and the screen each
//! take their own `Back`, the footer's `{key:..}` glyphs follow the input
//! mode, and a rebound `Accept` key activates a button.
//!
//! The spec and the guard come from `showcase::settings`, the module the
//! chest example adds, so what this proves is what `cargo run -p chest` opens
//! on `Menu`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::float_cmp)]

use std::path::Path;

use bevy::asset::AssetPlugin;
use bevy::input::gamepad::GamepadButton;
use bevy::prelude::*;
use bevy::ui_widgets::Activate;
use pretty_assertions::assert_eq;
use showcase::settings::{MAX_UI_SCALE, SETTINGS, SettingsDemoPlugin};
use slotted_test::prelude::*;
use slotted_ui::{
    FocusRing, InputMode, KeyBindingState, RadioState, RichKeySpan, SelectPopup, SelectState,
    SliderState, TabButton, TabsState, TextFieldState, ToggleState, UiAction, UiBindings, Value,
    ValueChanged, ValueRefused, key_glyph_text,
};

/// What the store and the buttons said, for the tests that count.
#[derive(Resource, Default)]
struct Seen {
    activated: Vec<Entity>,
    changed: Vec<ValueChanged>,
    refused: Vec<ValueRefused>,
}

fn on_activate(activate: On<Activate>, mut seen: ResMut<Seen>) {
    seen.activated.push(activate.entity);
}

fn record(
    mut seen: ResMut<Seen>,
    mut changed: MessageReader<ValueChanged>,
    mut refused: MessageReader<ValueRefused>,
) {
    seen.changed.extend(changed.read().cloned());
    seen.refused.extend(refused.read().cloned());
}

struct SeenPlugin;

impl Plugin for SeenPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Seen>()
            .add_observer(on_activate)
            .add_systems(Last, record);
    }
}

/// The workspace `assets/` directory. This crate has none of its own, and
/// Bevy resolves `AssetPlugin::file_path` against the crate root, so the
/// headless group is told where the themes and the screen's image live.
fn assets_dir() -> String {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets")
        .canonicalize()
        .expect("the workspace assets directory exists")
        .to_string_lossy()
        .into_owned()
}

/// The settings demo under `theme`, opened and settled, with the theme
/// asset loaded so the controls are the theme's height.
fn open_settings_in(theme: &str) -> (UiHarness, Entity) {
    let mut h = UiHarness::builder()
        .plugins((
            SlottedPlugins::headless().set(AssetPlugin {
                file_path: assets_dir(),
                ..default()
            }),
            SettingsDemoPlugin,
            SeenPlugin,
        ))
        .resolution(1280.0, 720.0)
        .theme(theme)
        .build();
    let root = h.open(ScreenKind::new(SETTINGS));
    wait_for_theme(&mut h);
    h.settle();
    (h, root)
}

fn open_settings() -> (UiHarness, Entity) {
    open_settings_in("glass")
}

fn wait_for_theme(h: &mut UiHarness) -> slotted::theme::Theme {
    for _ in 0..600 {
        let world = h.world();
        let active = world.resource::<slotted::theme::ActiveTheme>().0.clone();
        if let Some(theme) = world
            .resource::<Assets<slotted::theme::Theme>>()
            .get(&active)
        {
            let theme = theme.clone();
            h.settle();
            return theme;
        }
        h.step(1);
    }
    panic!("the theme never loaded; is the asset root right?");
}

fn find(h: &UiHarness, id: &str) -> Entity {
    h.find(&by::test_id(id))
}

/// The tab button at `index` of the screen's one tabs node.
fn tab_button(h: &mut UiHarness, index: usize) -> Entity {
    let tabs = find(h, "settings.tabs");
    let mut q = h.world_mut().query::<(Entity, &TabButton)>();
    q.iter(h.world())
        .find(|(_, b)| b.tabs == tabs && b.index == index)
        .map(|(e, _)| e)
        .expect("every tab has a button")
}

fn popup(h: &mut UiHarness) -> Option<Entity> {
    let mut q = h.world_mut().query_filtered::<Entity, With<SelectPopup>>();
    q.iter(h.world()).next()
}

fn seen(h: &mut UiHarness) -> Seen {
    std::mem::take(&mut h.world_mut().resource_mut::<Seen>())
}

/// The ring's rect in logical px.
fn ring_rect(h: &mut UiHarness) -> Rect {
    let ring = h
        .world_mut()
        .query_filtered::<Entity, With<FocusRing>>()
        .single(h.world())
        .expect("the harness app spawns one focus ring");
    h.rect_of(ring)
}

/// Focus and the ring both sit on `entity`, and the ring frames it.
#[track_caller]
fn assert_ring_on(h: &mut UiHarness, entity: Entity, what: &str) {
    assert_eq!(h.focused(), Some(entity), "focus after {what}");
    let ring = h.focus_ring();
    assert!(ring.visible, "the ring shows in gamepad mode ({what})");
    assert_eq!(ring.target, Some(entity), "the ring target after {what}");
    let target = h.rect_of(entity);
    let ring = ring_rect(h);
    assert!(
        ring.min.x <= target.min.x
            && ring.min.y <= target.min.y
            && ring.max.x >= target.max.x
            && ring.max.y >= target.max.y,
        "after {what} the ring {ring:?} frames the target {target:?}"
    );
}

/// The `{key:..}` glyphs of the footer, in order.
fn footer_glyphs(h: &mut UiHarness) -> Vec<String> {
    let footer = find(h, "footer_hint");
    let mut q = h
        .world_mut()
        .query::<(Entity, &RichKeySpan, &bevy::text::TextSpan)>();
    let mut spans: Vec<(Entity, String)> = q
        .iter(h.world())
        .map(|(e, _, span)| (e, span.0.clone()))
        .collect();
    // Tree order: the spans are the footer's children, in run order.
    let children: Vec<Entity> = h
        .world()
        .get::<Children>(footer)
        .map(|c| c.iter().collect())
        .unwrap_or_default();
    spans.sort_by_key(|(e, _)| children.iter().position(|c| c == e));
    spans.into_iter().map(|(_, s)| s).collect()
}

// ---------------------------------------------------------------------------
// Opening, in three themes
// ---------------------------------------------------------------------------

#[test]
fn the_settings_screen_opens_in_every_theme_with_focus_on_the_first_tab() {
    for theme in ["glass", "paper", "neon"] {
        let (mut h, root) = open_settings_in(theme);
        assert_eq!(h.stack(), vec![ScreenKind::new(SETTINGS)], "{theme}");
        assert_eq!(h.find(&by::screen(ScreenKind::new(SETTINGS))), root);
        let first_tab = tab_button(&mut h, 0);
        assert_eq!(
            h.focused(),
            Some(first_tab),
            "{theme}: `initial_focus: tabs` lands on the first tab button"
        );

        // Every control the spec names is there, once; one scroll page per
        // tab; the frame's separator, spacer and hint bar around them.
        assert_eq!(h.find_all(&by::control("tabs")).len(), 1, "{theme}");
        assert_eq!(h.find_all(&by::control("select")).len(), 2, "{theme}");
        assert_eq!(h.find_all(&by::control("slider")).len(), 4, "{theme}");
        assert_eq!(h.find_all(&by::control("toggle")).len(), 4, "{theme}");
        assert_eq!(h.find_all(&by::control("radio_group")).len(), 1, "{theme}");
        assert_eq!(h.find_all(&by::control("key_binding")).len(), 4, "{theme}");
        assert_eq!(h.find_all(&by::control("text_field")).len(), 1, "{theme}");
        assert_eq!(h.find_all(&by::control("scroll")).len(), 3, "{theme}");
        assert_eq!(h.find_all(&by::control("separator")).len(), 4, "{theme}");
        assert_eq!(h.find_all(&by::control("spacer")).len(), 1, "{theme}");
        assert_eq!(h.find_all(&by::test_id("footer_hint")).len(), 1, "{theme}");
        for tab in ["display", "audio", "controls"] {
            assert_eq!(
                h.find_all(&by::anchor(&format!("settings.{tab}.end")))
                    .len(),
                1,
                "{theme}: the {tab} page carries its end anchor"
            );
        }

        // The theme, not the file, sets a control's height.
        let theme_asset = wait_for_theme(&mut h);
        let height = theme_asset.tokens.sizes.control_height;
        for id in [
            "settings.resolution",
            "settings.ui_scale",
            "settings.reduced_motion",
            "settings.colour_mode",
        ] {
            let row = find(&h, id);
            let rect = h.rect_of(row);
            assert!(
                (rect.height() - height).abs() < 0.5,
                "{theme}: {id} is {} px tall, the theme says {height}",
                rect.height()
            );
        }

        // The labels resolved through the compiled-in catalogue.
        assert_eq!(
            h.text_of(find(&h, "title")).as_deref(),
            Some("Settings"),
            "{theme}"
        );
    }
}

#[test]
fn the_settings_tree_snapshot_in_glass() {
    let (h, _) = open_settings_in("glass");
    assert_tree_snapshot!("settings_tree_glass", h.screen_tree());
}

#[test]
fn the_settings_tree_snapshot_in_paper() {
    let (h, _) = open_settings_in("paper");
    assert_tree_snapshot!("settings_tree_paper", h.screen_tree());
}

#[test]
fn the_settings_tree_snapshot_in_neon() {
    let (h, _) = open_settings_in("neon");
    assert_tree_snapshot!("settings_tree_neon", h.screen_tree());
}

// ---------------------------------------------------------------------------
// The gamepad walk
// ---------------------------------------------------------------------------

#[test]
fn a_gamepad_reaches_every_control_on_every_tab_from_initial_focus_and_back() {
    let (mut h, _) = open_settings();
    h.set_input_mode(InputMode::Gamepad);
    h.settle();
    let first_tab = tab_button(&mut h, 0);
    assert_ring_on(&mut h, first_tab, "opening");

    let press = |h: &mut UiHarness, button: GamepadButton, id: &str| {
        h.gamepad(button);
        h.settle();
        let target = find(h, id);
        assert_ring_on(h, target, &format!("{button:?} to {id}"));
    };

    // Display: down the page from the tab bar.
    press(&mut h, GamepadButton::DPadDown, "settings.resolution");
    press(&mut h, GamepadButton::DPadDown, "settings.ui_scale");
    press(&mut h, GamepadButton::DPadDown, "settings.reduced_motion");
    press(&mut h, GamepadButton::DPadDown, "settings.colour_mode");

    // Audio: the right trigger switches tabs from inside a page and focus
    // lands on the new page's first control; the scroll panel follows the
    // ring down its twelve rows.
    press(&mut h, GamepadButton::RightTrigger, "settings.audio.master");
    assert_eq!(
        h.value("settings.tab"),
        Some(Value::Text("audio".to_owned())),
        "the tab switch wrote the store"
    );
    press(&mut h, GamepadButton::DPadDown, "settings.audio.music");
    press(&mut h, GamepadButton::DPadDown, "settings.audio.effects");
    press(&mut h, GamepadButton::DPadDown, "settings.audio.device");
    press(
        &mut h,
        GamepadButton::DPadDown,
        "settings.audio.mute_in_background",
    );
    press(&mut h, GamepadButton::DPadDown, "settings.audio.subtitles");
    press(&mut h, GamepadButton::DPadDown, "settings.audio.mono");
    let viewport = h.rect_of(find(&h, "settings.audio.page"));
    let mono = h.rect_of(find(&h, "settings.audio.mono"));
    assert!(
        mono.min.y >= viewport.min.y - 0.5 && mono.max.y <= viewport.max.y + 0.5,
        "the last row scrolled into view: {mono:?} in {viewport:?}"
    );

    // Controls.
    press(&mut h, GamepadButton::RightTrigger, "bind.keyboard.accept");
    press(&mut h, GamepadButton::DPadDown, "bind.keyboard.back");
    press(&mut h, GamepadButton::DPadDown, "bind.keyboard.tab_prev");
    press(&mut h, GamepadButton::DPadDown, "bind.keyboard.tab_next");
    press(&mut h, GamepadButton::DPadDown, "settings.player_name");

    // The footer: Down from the full-width field lands on the nearest
    // button, Right and Left walk the two.
    press(&mut h, GamepadButton::DPadDown, "reset");
    press(&mut h, GamepadButton::DPadRight, "done");
    press(&mut h, GamepadButton::DPadLeft, "reset");

    // And back: the trigger wraps to the display tab (focus outside the
    // pages stays where it is), then Up walks the page to the tab bar.
    press(&mut h, GamepadButton::RightTrigger, "reset");
    assert_eq!(
        h.value("settings.tab"),
        Some(Value::Text("display".to_owned())),
        "the trigger wrapped to the first tab"
    );
    press(&mut h, GamepadButton::DPadUp, "settings.colour_mode");
    press(&mut h, GamepadButton::DPadUp, "settings.reduced_motion");
    press(&mut h, GamepadButton::DPadUp, "settings.ui_scale");
    press(&mut h, GamepadButton::DPadUp, "settings.resolution");
    // Up from a row lands on the nearest tab button, which is the one over
    // the row's middle; Left is the first tab, where the walk began.
    h.gamepad(GamepadButton::DPadUp);
    h.settle();
    let audio_tab = tab_button(&mut h, 1);
    assert_ring_on(&mut h, audio_tab, "DPadUp to the tab bar");
    h.gamepad(GamepadButton::DPadLeft);
    h.settle();
    assert_ring_on(&mut h, first_tab, "DPadLeft back to the first tab");
    assert_eq!(h.input_mode(), InputMode::Gamepad, "the walk kept the mode");
}

// ---------------------------------------------------------------------------
// Every control through the store
// ---------------------------------------------------------------------------

#[test]
fn a_toggle_writes_on_accept_and_paints_from_a_store_write() {
    let (mut h, _) = open_settings();
    let row = find(&h, "settings.reduced_motion");
    assert_eq!(h.value("settings.reduced_motion"), Some(Value::Bool(false)));
    h.set_focus(Some(row));
    h.action(UiAction::Accept);
    h.settle();
    assert_eq!(h.value("settings.reduced_motion"), Some(Value::Bool(true)));
    assert!(h.world().get::<ToggleState>(row).unwrap().on);

    h.set_value("settings.reduced_motion", false);
    h.settle();
    assert!(!h.world().get::<ToggleState>(row).unwrap().on);
}

#[test]
fn a_slider_writes_on_a_drag_and_paints_from_a_store_write() {
    let (mut h, _) = open_settings();
    let row = find(&h, "settings.ui_scale");
    assert_eq!(h.value("settings.ui_scale"), Some(Value::Float(1.0)));

    // 0.5 + 2.5 * 0.4 = 1.5, on the 0.25 grid.
    h.drag_slider(row, 0.4);
    h.settle();
    assert_eq!(h.value("settings.ui_scale"), Some(Value::Float(1.5)));
    assert_eq!(h.world().get::<SliderState>(row).unwrap().value, 1.5);

    h.set_value("settings.ui_scale", 1.75);
    h.settle();
    assert_eq!(h.world().get::<SliderState>(row).unwrap().value, 1.75);

    // The rule snaps and clamps a write from anywhere.
    h.set_value("settings.ui_scale", 0.1);
    h.settle();
    assert_eq!(h.value("settings.ui_scale"), Some(Value::Float(0.5)));
    assert_eq!(h.world().get::<SliderState>(row).unwrap().value, 0.5);
}

#[test]
fn the_audio_sliders_in_the_scroll_panel_write_and_read_too() {
    let (mut h, _) = open_settings();
    let tabs = find(&h, "settings.tabs");
    h.switch_tab(tabs, "audio");
    assert_eq!(
        h.value("settings.tab"),
        Some(Value::Text("audio".to_owned()))
    );
    assert_eq!(h.world().get::<TabsState>(tabs).unwrap().active, 1);

    let master = find(&h, "settings.audio.master");
    h.drag_slider(master, 0.5);
    h.settle();
    assert_eq!(h.value("settings.audio.master"), Some(Value::Float(50.0)));

    let music = find(&h, "settings.audio.music");
    h.set_value("settings.audio.music", 25.0);
    h.settle();
    assert_eq!(h.world().get::<SliderState>(music).unwrap().value, 25.0);

    let effects = find(&h, "settings.audio.effects");
    h.set_focus(Some(effects));
    h.action(UiAction::Left);
    h.settle();
    assert_eq!(h.value("settings.audio.effects"), Some(Value::Float(95.0)));
    assert_eq!(
        h.focused(),
        Some(effects),
        "the slider claimed Left; focus stayed"
    );
}

#[test]
fn a_select_writes_from_its_popup_and_paints_from_a_store_write() {
    let (mut h, _) = open_settings();
    let row = find(&h, "settings.resolution");
    assert_eq!(h.world().get::<SelectState>(row).unwrap().index, 1);

    h.select_option(row, "2560x1440");
    h.settle();
    assert_eq!(
        h.value("settings.resolution"),
        Some(Value::Text("2560x1440".to_owned()))
    );
    assert_eq!(h.world().get::<SelectState>(row).unwrap().index, 2);
    assert!(popup(&mut h).is_none(), "the pick closed the popup");

    h.set_value("settings.resolution", "1280x720");
    h.settle();
    assert_eq!(h.world().get::<SelectState>(row).unwrap().index, 0);

    // A write outside the option list is refused and nothing moves.
    h.set_value("settings.resolution", "640x480");
    h.settle();
    assert_eq!(h.world().get::<SelectState>(row).unwrap().index, 0);
    let seen = seen(&mut h);
    assert_eq!(seen.refused.len(), 1);
    assert!(seen.refused[0].reason.contains("640x480"));
}

#[test]
fn a_radio_group_writes_on_the_arrows_and_paints_from_a_store_write() {
    let (mut h, _) = open_settings();
    let row = find(&h, "settings.colour_mode");
    h.set_focus(Some(row));
    h.action(UiAction::Right);
    h.settle();
    assert_eq!(
        h.value("settings.colour_mode"),
        Some(Value::Text("deuteranopia".to_owned()))
    );
    assert_eq!(h.focused(), Some(row), "the group claimed Right");

    h.set_value("settings.colour_mode", "high_contrast");
    h.settle();
    assert_eq!(h.world().get::<RadioState>(row).unwrap().index, 2);
}

#[test]
fn a_text_field_writes_on_enter_and_paints_from_a_store_write() {
    let (mut h, _) = open_settings();
    let tabs = find(&h, "settings.tabs");
    h.switch_tab(tabs, "controls");
    let field = find(&h, "settings.player_name");
    assert_eq!(
        h.world().get::<TextFieldState>(field).unwrap().text,
        "Steve",
        "seeded from the store"
    );

    h.set_value("settings.player_name", "");
    h.settle();
    h.type_into(field, "Ada");
    h.settle();
    assert_eq!(
        h.value("settings.player_name"),
        Some(Value::Text("Ada".to_owned()))
    );
    let state = h.world().get::<TextFieldState>(field).unwrap();
    assert_eq!(state.text, "Ada");
    assert!(!state.editing, "Enter left editing");
    assert_eq!(h.focused(), Some(field), "focus is back on the row");
    assert_eq!(
        h.stack(),
        vec![ScreenKind::new(SETTINGS)],
        "typing never popped the screen"
    );

    h.set_value("settings.player_name", "Grace");
    h.settle();
    assert_eq!(
        h.world().get::<TextFieldState>(field).unwrap().text,
        "Grace"
    );
}

#[test]
fn a_key_binding_row_captures_and_the_tabs_bind_the_store() {
    let (mut h, _) = open_settings();
    let tabs = find(&h, "settings.tabs");
    h.set_value("settings.tab", "controls");
    h.settle();
    assert_eq!(
        h.world().get::<TabsState>(tabs).unwrap().active,
        2,
        "a store write switches the tab"
    );
    let row = find(&h, "bind.keyboard.tab_next");
    h.capture_key(row, KeyCode::KeyN);
    assert!(!h.world().get::<KeyBindingState>(row).unwrap().capturing);
    assert_eq!(
        h.world()
            .resource::<UiBindings>()
            .first_key(UiAction::TabNext),
        Some(KeyCode::KeyN)
    );
    // The new key works: N is the next tab now.
    h.key(KeyCode::KeyN);
    h.settle();
    assert_eq!(
        h.value("settings.tab"),
        Some(Value::Text("display".to_owned()))
    );
}

#[test]
fn the_reset_button_writes_every_default_back_through_the_store() {
    let (mut h, _) = open_settings();
    h.set_value("settings.ui_scale", 1.5);
    h.set_value("settings.reduced_motion", true);
    h.set_value("settings.resolution", "1280x720");
    h.settle();
    seen(&mut h);

    let reset = find(&h, "reset");
    h.click(reset);
    h.settle();
    assert_eq!(seen(&mut h).activated, vec![reset]);
    assert_eq!(h.value("settings.ui_scale"), Some(Value::Float(1.0)));
    assert_eq!(h.value("settings.reduced_motion"), Some(Value::Bool(false)));
    assert_eq!(
        h.value("settings.resolution"),
        Some(Value::Text("1920x1080".to_owned()))
    );
    assert_eq!(
        h.world()
            .get::<SliderState>(find(&h, "settings.ui_scale"))
            .unwrap()
            .value,
        1.0
    );
}

// ---------------------------------------------------------------------------
// The guard
// ---------------------------------------------------------------------------

#[test]
fn the_ui_scale_guard_refuses_a_drag_past_two_and_the_slider_snaps_back() {
    let (mut h, _) = open_settings();
    let row = find(&h, "settings.ui_scale");
    h.set_value("settings.ui_scale", 1.5);
    h.settle();
    seen(&mut h);

    // A drag to the end asks for 3.0; the guard says no to everything past
    // 2.0, so the slider is back where the store is.
    h.drag_slider(row, 1.0);
    h.settle();
    let state = h.world().get::<SliderState>(row).unwrap().clone();
    assert!(!state.dragging);
    let kept = h.value("settings.ui_scale").unwrap().as_f64().unwrap();
    assert!(
        kept <= MAX_UI_SCALE,
        "the store never held more than {MAX_UI_SCALE}: {kept}"
    );
    assert_eq!(state.value, kept, "the slider paints what the store kept");
    let seen = seen(&mut h);
    assert!(
        !seen.refused.is_empty(),
        "at least one move past the limit was refused"
    );
    assert!(
        seen.refused.iter().all(|r| r.source == Some(row)),
        "every refusal names the slider"
    );
    assert!(
        seen.refused[0].reason.contains("more than 2"),
        "{}",
        seen.refused[0].reason
    );

    // Arrow steps past the limit are refused one by one too.
    h.set_value("settings.ui_scale", 2.0);
    h.settle();
    seen_clear(&mut h);
    h.set_focus(Some(row));
    h.action(UiAction::Right);
    h.settle();
    assert_eq!(h.value("settings.ui_scale"), Some(Value::Float(2.0)));
    assert_eq!(h.world().get::<SliderState>(row).unwrap().value, 2.0);
    assert_eq!(h.world_mut().resource::<Seen>().refused.len(), 1);
}

fn seen_clear(h: &mut UiHarness) {
    seen(h);
}

// ---------------------------------------------------------------------------
// Back: the popup, then the screen
// ---------------------------------------------------------------------------

#[test]
fn back_closes_the_select_popup_and_a_second_back_pops_the_screen() {
    let (mut h, _) = open_settings();
    let row = find(&h, "settings.resolution");
    h.click(row);
    h.settle();
    assert!(popup(&mut h).is_some(), "a click opens the popup");
    assert_ne!(h.focused(), Some(row), "focus moved into the popup");

    h.action(UiAction::Back);
    h.settle();
    assert!(popup(&mut h).is_none(), "Back closed the popup");
    assert_eq!(h.stack(), vec![ScreenKind::new(SETTINGS)], "and only that");
    assert_eq!(h.focused(), Some(row));

    h.action(UiAction::Back);
    h.settle();
    assert!(h.stack().is_empty(), "the second Back popped the screen");
    assert!(h.try_find(&by::screen(ScreenKind::new(SETTINGS))).is_none());
}

// ---------------------------------------------------------------------------
// The footer's glyphs
// ---------------------------------------------------------------------------

#[test]
fn the_footer_reads_the_live_bindings_for_the_input_mode() {
    let (mut h, _) = open_settings();
    h.set_input_mode(InputMode::Keyboard);
    h.settle();
    assert_eq!(footer_glyphs(&mut h), vec!["Esc", "E"]);

    h.set_input_mode(InputMode::Gamepad);
    h.settle();
    let bindings = h.world().resource::<UiBindings>().clone();
    let expected = vec![
        key_glyph_text(
            UiAction::Back,
            InputMode::Gamepad,
            &bindings,
            slotted_ui::GlyphSet::Auto,
        ),
        key_glyph_text(
            UiAction::TabNext,
            InputMode::Gamepad,
            &bindings,
            slotted_ui::GlyphSet::Auto,
        ),
    ];
    assert_eq!(footer_glyphs(&mut h), expected);
    assert_eq!(footer_glyphs(&mut h)[0], "B", "East is the pad's B");

    // A real button press flips the mode the same way.
    h.set_input_mode(InputMode::Keyboard);
    h.gamepad(GamepadButton::DPadDown);
    h.settle();
    assert_eq!(h.input_mode(), InputMode::Gamepad);
    assert_eq!(footer_glyphs(&mut h)[0], "B");

    // And a rebind re-renders without a mode change.
    h.set_input_mode(InputMode::Keyboard);
    h.settle();
    let tabs = find(&h, "settings.tabs");
    h.switch_tab(tabs, "controls");
    h.capture_key(find(&h, "bind.keyboard.back"), KeyCode::KeyX);
    h.settle();
    assert_eq!(footer_glyphs(&mut h)[0], "X");
}

// ---------------------------------------------------------------------------
// A rebound Accept
// ---------------------------------------------------------------------------

#[test]
fn a_rebound_accept_key_activates_a_button() {
    let (mut h, _) = open_settings();
    let tabs = find(&h, "settings.tabs");
    h.switch_tab(tabs, "controls");
    h.capture_key(find(&h, "bind.keyboard.accept"), KeyCode::KeyK);
    assert_eq!(
        h.world()
            .resource::<UiBindings>()
            .first_key(UiAction::Accept),
        Some(KeyCode::KeyK)
    );
    seen(&mut h);

    let reset = find(&h, "reset");
    h.set_focus(Some(reset));
    h.key(KeyCode::KeyK);
    h.settle();
    assert_eq!(
        seen(&mut h).activated,
        vec![reset],
        "K activates the button"
    );
    assert_eq!(
        h.stack(),
        vec![ScreenKind::new(SETTINGS)],
        "a plain button does not pop"
    );

    // Reset is the frame's Reset: it put the bindings back, so K is Accept
    // no longer. Bind it again for the second half.
    assert_eq!(
        h.world()
            .resource::<UiBindings>()
            .first_key(UiAction::Accept),
        Some(KeyCode::Enter),
        "Reset restored the default bindings"
    );
    h.capture_key(find(&h, "bind.keyboard.accept"), KeyCode::KeyK);
    seen(&mut h);

    // The screen's `Done` button is `slotted:close`: K pops it.
    let done = find(&h, "done");
    h.set_focus(Some(done));
    h.key(KeyCode::KeyK);
    h.settle();
    assert_eq!(seen(&mut h).activated, vec![done]);
    assert!(h.stack().is_empty(), "Done popped the screen");
}
