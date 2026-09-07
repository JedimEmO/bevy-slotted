//! The value controls (menus M1 contract 3.2 to 3.7) through the headless
//! harness: every control paints from the store on open, writes on Accept,
//! arrows and clicks, snaps back when a guard refuses, does nothing when
//! disabled, claims what it consumed, the select popup opens in pointer mode
//! only and `Back` closes it without popping the screen, a key capture
//! rebinds `UiBindings` and the next `Accept` on a button uses the new key.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::float_cmp)]

use bevy::input::gamepad::GamepadButton;
use bevy::picking::pointer::PointerButton;
use bevy::prelude::*;
use bevy::ui::{Checked, InteractionDisabled, Pressed};
use slotted_test::prelude::*;
use slotted_theme::{Theme, Themed, roles};
use slotted_ui::def::{BindDef, SelectOption};
use slotted_ui::{
    BindingChanged, ButtonOpts, ButtonState, ButtonVariant, InputDevice, InputMode,
    KeyBindingState, Layout, LocKey, RadioState, ScreenDef, SelectPopup, SelectState, SetValue,
    SliderState, Tags, ToggleState, ToggleStyle, UiAction, UiActionEvent, UiBindings, UiNodeDef,
    Value, ValueBinding, ValueGuards, ValueStore, WidgetKind,
};

const SCREEN: &str = "controls:settings";

/// Every `Activate` and every `SetValue` the app saw, in order.
#[derive(Resource, Default)]
struct Seen {
    activated: Vec<Entity>,
    writes: Vec<SetValue>,
    rebound: Vec<BindingChanged>,
}

fn on_activate(activate: On<bevy::ui_widgets::Activate>, mut seen: ResMut<Seen>) {
    seen.activated.push(activate.entity);
}

fn record(
    mut writes: MessageReader<SetValue>,
    mut rebound: MessageReader<BindingChanged>,
    mut seen: ResMut<Seen>,
) {
    seen.writes.extend(writes.read().cloned());
    seen.rebound.extend(rebound.read().copied());
}

struct SeenPlugin;

impl Plugin for SeenPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Seen>()
            .add_observer(on_activate)
            .add_systems(Update, record);
    }
}

fn tags(id: &str) -> Tags {
    Tags::new().with(Tags::TEST_ID, id)
}

fn bind(key: &str) -> BindDef {
    BindDef {
        bind: Some(key.to_owned()),
        property: None,
        disabled: false,
    }
}

fn options(ids: &[&str]) -> Vec<SelectOption> {
    ids.iter()
        .map(|id| SelectOption {
            id: (*id).to_owned(),
            label: LocKey(format!("opt.{id}")),
        })
        .collect()
}

fn screen() -> ScreenDef {
    ScreenDef {
        kind: ScreenKind::new(SCREEN),
        initial_focus: Some("ok".to_owned()),
        presentation: Presentation::default(),
        inherits: None,
        remove: vec![],
        listring: vec![],
        root: UiNodeDef::Panel {
            role: roles::PANEL,
            layout: Layout {
                direction: slotted_ui::LayoutDirection::Column,
                gap: 1.0,
                padding: 2.0.into(),
                ..Layout::default()
            },
            children: vec![
                UiNodeDef::Button {
                    widget: None,
                    opts: ButtonOpts {
                        label: Some(LocKey("btn.ok".to_owned())),
                        variant: ButtonVariant::Primary,
                        ..ButtonOpts::default()
                    },
                    tags: tags("ok"),
                },
                UiNodeDef::Button {
                    widget: None,
                    opts: ButtonOpts {
                        label: Some(LocKey("btn.nope".to_owned())),
                        disabled: true,
                        ..ButtonOpts::default()
                    },
                    tags: tags("nope"),
                },
                UiNodeDef::Button {
                    widget: Some(WidgetKind::new("slotted:close")),
                    opts: ButtonOpts {
                        label: Some(LocKey("btn.close".to_owned())),
                        compact: true,
                        ..ButtonOpts::default()
                    },
                    tags: tags("close"),
                },
                UiNodeDef::Toggle {
                    label: Some(LocKey("video.vsync".to_owned())),
                    style: ToggleStyle::Switch,
                    bind: bind("video.vsync"),
                    tags: tags("vsync"),
                },
                UiNodeDef::Toggle {
                    label: Some(LocKey("video.hud".to_owned())),
                    style: ToggleStyle::Checkbox,
                    bind: bind("video.hud"),
                    tags: tags("hud"),
                },
                UiNodeDef::Slider {
                    label: Some(LocKey("audio.master".to_owned())),
                    min: 0.0,
                    max: 100.0,
                    step: 5.0,
                    format: "{value}%".to_owned(),
                    bind: bind("audio.master"),
                    tags: tags("volume"),
                },
                UiNodeDef::Slider {
                    label: None,
                    min: 0.0,
                    max: 1.0,
                    step: 0.0,
                    format: "{value:.2}".to_owned(),
                    bind: BindDef {
                        disabled: true,
                        ..bind("audio.locked")
                    },
                    tags: tags("locked"),
                },
                UiNodeDef::Select {
                    label: Some(LocKey("video.quality".to_owned())),
                    options: options(&["low", "medium", "high"]),
                    bind: bind("video.quality"),
                    tags: tags("quality"),
                },
                UiNodeDef::RadioGroup {
                    label: Some(LocKey("video.mode".to_owned())),
                    options: options(&["windowed", "borderless", "full"]),
                    bind: bind("video.mode"),
                    tags: tags("mode"),
                },
                UiNodeDef::KeyBinding {
                    label: None,
                    action: UiAction::Accept,
                    device: InputDevice::Keyboard,
                    disabled: false,
                    tags: tags("kb_accept"),
                },
            ],
            tags: tags("settings"),
        },
    }
}

fn harness() -> UiHarness {
    UiHarness::builder()
        .plugins((SlottedPlugins::headless(), SeenPlugin))
        .registries(TestRegistries::basic())
        .theme("glass")
        .build()
}

fn seed(h: &mut UiHarness) {
    let mut store = h.world_mut().resource_mut::<ValueStore>();
    store.insert("video.vsync", true);
    store.insert("video.hud", false);
    store.insert("audio.master", 40.0);
    store.insert("audio.locked", 0.5);
    store.insert("video.quality", "medium");
    store.insert("video.mode", "full");
}

/// Seeds the store, opens the settings screen and settles.
fn open(h: &mut UiHarness) -> Entity {
    seed(h);
    let root = h.open(screen());
    h.settle();
    root
}

fn find(h: &UiHarness, id: &str) -> Entity {
    h.find(&by::test_id(id))
}

fn value(h: &UiHarness, key: &str) -> Option<Value> {
    h.world().resource::<ValueStore>().get(key).cloned()
}

fn toggle(h: &UiHarness, id: &str) -> ToggleState {
    *h.world().get::<ToggleState>(find(h, id)).unwrap()
}

fn slider(h: &UiHarness, id: &str) -> SliderState {
    h.world().get::<SliderState>(find(h, id)).unwrap().clone()
}

fn select(h: &UiHarness, id: &str) -> SelectState {
    h.world().get::<SelectState>(find(h, id)).unwrap().clone()
}

fn radio(h: &UiHarness, id: &str) -> RadioState {
    h.world().get::<RadioState>(find(h, id)).unwrap().clone()
}

fn focus(h: &mut UiHarness, id: &str) -> Entity {
    let entity = find(h, id);
    h.set_focus(Some(entity));
    entity
}

fn activated(h: &mut UiHarness) -> Vec<Entity> {
    std::mem::take(&mut h.world_mut().resource_mut::<Seen>().activated)
}

fn writes(h: &mut UiHarness) -> Vec<SetValue> {
    std::mem::take(&mut h.world_mut().resource_mut::<Seen>().writes)
}

fn popup(h: &mut UiHarness) -> Option<Entity> {
    let mut q = h.world_mut().query_filtered::<Entity, With<SelectPopup>>();
    q.iter(h.world()).next()
}

/// Writes one action straight into the frame, bypassing the keyboard, so
/// the input mode stays where the test put it.
fn action_event(h: &mut UiHarness, action: UiAction, device: InputDevice) {
    h.world_mut().write_message(UiActionEvent {
        action,
        device,
        repeat: false,
    });
    h.step(1);
}

fn role_of(h: &UiHarness, entity: Entity) -> String {
    h.world()
        .get::<Themed>(entity)
        .unwrap()
        .0
        .as_str()
        .to_owned()
}

// ---------------------------------------------------------------------------
// Painting from the store
// ---------------------------------------------------------------------------

#[test]
fn every_control_paints_from_the_store_on_open() {
    let mut h = harness();
    open(&mut h);
    assert!(toggle(&h, "vsync").on);
    assert!(!toggle(&h, "hud").on);
    assert_eq!(slider(&h, "volume").value, 40.0);
    assert_eq!(slider(&h, "locked").value, 0.5);
    assert_eq!(select(&h, "quality").index, 1);
    assert_eq!(radio(&h, "mode").index, 2);

    // `Checked` mirrors `on`; the readout carries the format.
    let vsync = find(&h, "vsync");
    assert!(h.world().get::<Checked>(vsync).is_some());
    assert!(h.world().get::<Checked>(find(&h, "hud")).is_none());
    let readout = h
        .world()
        .get::<slotted_ui::widgets::slider::SliderParts>(find(&h, "volume"))
        .unwrap()
        .readout;
    assert_eq!(h.text_of(readout).as_deref(), Some("40%"));

    // A seed after open repaints too.
    h.world_mut()
        .resource_mut::<ValueStore>()
        .insert("audio.master", 70.0);
    h.settle();
    assert_eq!(slider(&h, "volume").value, 70.0);
    assert_eq!(h.text_of(readout).as_deref(), Some("70%"));

    // Every control is a focusable at the theme's control height.
    let height = slotted_ui::active_tokens(h.world()).sizes.control_height;
    for id in [
        "ok",
        "vsync",
        "hud",
        "volume",
        "quality",
        "mode",
        "kb_accept",
    ] {
        let entity = find(&h, id);
        assert!(
            h.world().get::<slotted_ui::Focusable>(entity).is_some(),
            "{id} is focusable"
        );
        assert!(
            (h.rect_of(entity).height() - height).abs() < 0.5,
            "{id} is control_height tall"
        );
    }
    let compact = slotted_ui::active_tokens(h.world())
        .sizes
        .control_height_compact;
    assert!((h.rect_of(find(&h, "close")).height() - compact).abs() < 0.5);
    assert!(
        h.world().get::<ValueBinding>(find(&h, "volume")).is_some(),
        "a bound control carries its binding"
    );
}

// ---------------------------------------------------------------------------
// Button
// ---------------------------------------------------------------------------

#[test]
fn accept_on_a_focused_button_activates_from_any_device_and_a_click_does_too() {
    let mut h = harness();
    open(&mut h);
    let ok = find(&h, "ok");
    assert_eq!(h.focused(), Some(ok), "`initial_focus` names the button");
    assert!(
        h.world().get::<bevy::ui_widgets::Button>(ok).is_none(),
        "our button is not Bevy's, so Enter cannot fire twice"
    );

    h.action(UiAction::Accept);
    assert_eq!(activated(&mut h), vec![ok], "keyboard: exactly once");

    h.gamepad(GamepadButton::South);
    assert_eq!(activated(&mut h), vec![ok], "gamepad: exactly once");

    h.click(ok);
    assert_eq!(activated(&mut h), vec![ok], "pointer: exactly once");
    assert!(
        h.world().get::<Pressed>(ok).is_none(),
        "the press ended with the release"
    );
    assert!(!h.world().get::<ButtonState>(ok).unwrap().pressed);
}

#[test]
fn a_rebound_accept_key_activates_a_button() {
    let mut h = harness();
    open(&mut h);
    let ok = find(&h, "ok");
    h.world_mut()
        .resource_mut::<UiBindings>()
        .keys
        .insert(UiAction::Accept, vec![KeyCode::KeyK]);
    h.key(KeyCode::Enter);
    assert!(activated(&mut h).is_empty(), "Enter means nothing now");
    h.key(KeyCode::KeyK);
    assert_eq!(activated(&mut h), vec![ok]);
}

#[test]
fn a_disabled_button_is_inert_and_drawn_as_such() {
    let mut h = harness();
    open(&mut h);
    let nope = find(&h, "nope");
    assert!(h.world().get::<InteractionDisabled>(nope).is_some());
    assert_eq!(role_of(&h, nope), "button.disabled");
    h.set_focus(Some(nope));
    h.action(UiAction::Accept);
    h.click(nope);
    assert!(activated(&mut h).is_empty());
}

#[test]
fn button_roles_follow_variant_hover_focus_and_press() {
    let mut h = harness();
    open(&mut h);
    let ok = find(&h, "ok");
    assert_eq!(
        role_of(&h, ok),
        "button.primary.focus",
        "focused on open, primary variant"
    );
    h.set_focus(None);
    h.step(1);
    assert_eq!(role_of(&h, ok), "button.primary");
    h.hover(ok);
    h.step(1);
    assert_eq!(role_of(&h, ok), "button.primary.hover");
    h.pointer_press(PointerButton::Primary);
    h.step(1);
    assert_eq!(role_of(&h, ok), "button.primary.pressed");
    assert!(h.world().get::<Pressed>(ok).is_some());
    h.pointer_release(PointerButton::Primary);
    h.step(1);
    assert_ne!(role_of(&h, ok), "button.primary.pressed");
    let close = find(&h, "close");
    assert_eq!(role_of(&h, close), "button", "secondary at rest");
    // Every dotted state role resolves in all three themes.
    let themes = h.world().resource::<Assets<Theme>>();
    for theme in themes.iter().map(|(_, t)| t) {
        for role in [
            "button.primary.pressed",
            "button.danger.hover",
            "toggle.on.focus",
            "checkbox.on.hover",
            "slider.active",
            "select.active",
            "radio.active.hover",
            "key_binding.capturing",
        ] {
            assert!(
                theme.material(&slotted_theme::Role::new(role)).is_some(),
                "{role} resolves"
            );
        }
    }
}

#[test]
fn a_close_button_pops_the_screen() {
    let mut h = harness();
    open(&mut h);
    assert_eq!(h.stack(), vec![ScreenKind::new(SCREEN)]);
    focus(&mut h, "close");
    h.action(UiAction::Accept);
    h.settle();
    assert!(h.stack().is_empty(), "popped");
    assert!(h.try_find(&by::test_id("ok")).is_none());
}

// ---------------------------------------------------------------------------
// Toggle
// ---------------------------------------------------------------------------

#[test]
fn a_toggle_flips_on_accept_and_on_a_click_through_the_store() {
    let mut h = harness();
    open(&mut h);
    let vsync = focus(&mut h, "vsync");
    h.action(UiAction::Accept);
    h.settle();
    assert_eq!(value(&h, "video.vsync"), Some(Value::Bool(false)));
    assert!(!toggle(&h, "vsync").on);
    assert!(h.world().get::<Checked>(vsync).is_none());
    let w = writes(&mut h);
    assert_eq!(w.len(), 1);
    assert_eq!(w[0].source, Some(vsync));

    h.click(vsync);
    h.settle();
    assert_eq!(value(&h, "video.vsync"), Some(Value::Bool(true)));
    assert!(toggle(&h, "vsync").on);

    // The checkbox is the same widget with its own roles.
    let hud = find(&h, "hud");
    let parts = *h
        .world()
        .get::<slotted_ui::widgets::toggle::ToggleParts>(hud)
        .unwrap();
    assert!(parts.thumb.is_none() && parts.tick.is_some());
    assert!(role_of(&h, parts.track).starts_with("checkbox"));
    h.click(hud);
    h.settle();
    assert!(toggle(&h, "hud").on);
    assert_eq!(role_of(&h, parts.track), "checkbox.on.focus");
    assert_eq!(
        h.world().get::<Visibility>(parts.tick.unwrap()),
        Some(&Visibility::Inherited)
    );
}

#[test]
fn a_switch_thumb_slides_by_spacing_md() {
    let mut h = harness();
    open(&mut h);
    let vsync = find(&h, "vsync");
    let parts = *h
        .world()
        .get::<slotted_ui::widgets::toggle::ToggleParts>(vsync)
        .unwrap();
    let thumb = parts.thumb.unwrap();
    let md = slotted_ui::active_tokens(h.world()).spacing.md;
    let x = |h: &UiHarness| match h.world().get::<UiTransform>(thumb) {
        Some(tf) => match tf.translation.x {
            Val::Px(x) => x,
            _ => 0.0,
        },
        None => 0.0,
    };
    assert!((x(&h) - md).abs() < 0.01, "on: at the far end");
    h.click(vsync);
    assert!(
        h.world().get::<slotted_theme::Tween>(thumb).is_some(),
        "the flip starts a tween"
    );
    h.settle();
    assert!(x(&h).abs() < 0.01, "off: back at the start");
}

#[test]
fn a_disabled_control_takes_no_action() {
    let mut h = harness();
    open(&mut h);
    let locked = focus(&mut h, "locked");
    assert!(h.world().get::<InteractionDisabled>(locked).is_some());
    h.action(UiAction::Right);
    h.action(UiAction::PageNext);
    h.click(locked);
    h.settle();
    assert_eq!(slider(&h, "locked").value, 0.5);
    assert!(writes(&mut h).is_empty());
}

// ---------------------------------------------------------------------------
// Slider
// ---------------------------------------------------------------------------

#[test]
fn a_slider_steps_on_the_arrows_and_pages_by_a_tenth_and_claims_them() {
    let mut h = harness();
    open(&mut h);
    let volume = focus(&mut h, "volume");
    h.action(UiAction::Right);
    h.settle();
    assert_eq!(value(&h, "audio.master"), Some(Value::Float(45.0)));
    assert_eq!(slider(&h, "volume").value, 45.0);
    assert_eq!(h.focused(), Some(volume), "Right was claimed: focus stayed");
    h.action(UiAction::Left);
    h.action(UiAction::Left);
    h.settle();
    assert_eq!(slider(&h, "volume").value, 35.0);
    assert_eq!(h.focused(), Some(volume));
    h.action(UiAction::PageNext);
    h.settle();
    assert_eq!(slider(&h, "volume").value, 45.0, "a tenth of the range");
    h.action(UiAction::PagePrev);
    h.settle();
    assert_eq!(slider(&h, "volume").value, 35.0);
    let w = writes(&mut h);
    assert_eq!(w.len(), 5);
    assert!(w.iter().all(|s| s.source == Some(volume)));

    // A held direction repeats on virtual time.
    h.hold(KeyCode::ArrowRight);
    h.advance(std::time::Duration::from_millis(600));
    h.release(KeyCode::ArrowRight);
    h.settle();
    assert!(
        slider(&h, "volume").value > 45.0,
        "repeats moved it past one step: {}",
        slider(&h, "volume").value
    );
    assert_eq!(h.focused(), Some(volume));
}

#[test]
fn a_slider_snaps_back_when_a_guard_refuses() {
    let mut h = harness();
    h.world_mut().resource_mut::<ValueGuards>().push(
        |key: &str, proposed: &Value, _: &ValueStore| {
            if key == "audio.master" && proposed.as_f64().is_some_and(|v| v > 50.0) {
                Err("too loud".to_owned())
            } else {
                Ok(proposed.clone())
            }
        },
    );
    open(&mut h);
    focus(&mut h, "volume");
    h.action(UiAction::Right);
    h.action(UiAction::Right);
    h.settle();
    assert_eq!(slider(&h, "volume").value, 50.0);
    h.action(UiAction::Right);
    h.settle();
    assert_eq!(
        slider(&h, "volume").value,
        50.0,
        "refused: the thumb stayed"
    );
    assert_eq!(value(&h, "audio.master"), Some(Value::Float(50.0)));
}

#[test]
fn a_press_on_the_track_jumps_and_a_drag_scrubs_with_a_write_per_move() {
    let mut h = harness();
    open(&mut h);
    let volume = find(&h, "volume");
    let parts = *h
        .world()
        .get::<slotted_ui::widgets::slider::SliderParts>(volume)
        .unwrap();
    let track = h.rect_of(parts.track);
    let at = |fraction: f32| Vec2::new(track.min.x + track.width() * fraction, track.center().y);

    h.click_at(at(0.8), PointerButton::Primary);
    h.settle();
    assert_eq!(slider(&h, "volume").value, 80.0, "a press jumps");
    assert_eq!(h.focused(), Some(volume), "a press focuses the row");
    assert!(!slider(&h, "volume").dragging);
    writes(&mut h);

    h.pointer_move_to(at(0.8));
    h.pointer_press(PointerButton::Primary);
    assert!(slider(&h, "volume").dragging);
    h.pointer_move_to(at(0.6));
    h.pointer_move_to(at(0.4));
    h.pointer_move_to(at(0.2));
    h.settle();
    assert_eq!(slider(&h, "volume").value, 20.0);
    h.pointer_release(PointerButton::Primary);
    h.settle();
    assert!(!slider(&h, "volume").dragging);
    let w = writes(&mut h);
    let values: Vec<f64> = w.iter().filter_map(|s| s.value.as_f64()).collect();
    assert!(
        values.contains(&60.0) && values.contains(&40.0) && values.contains(&20.0),
        "every move wrote: {values:?}"
    );
    assert!(w.iter().all(|s| s.source == Some(volume)));
    let thumb = h.rect_of(parts.thumb).center().x;
    assert!(
        (thumb - at(0.2).x).abs() < 1.5,
        "the thumb sits at a fifth: {thumb} vs {}",
        at(0.2).x
    );
}

// ---------------------------------------------------------------------------
// Select and radio group
// ---------------------------------------------------------------------------

#[test]
fn a_select_cycles_in_place_on_the_arrows_and_wraps() {
    let mut h = harness();
    open(&mut h);
    let quality = focus(&mut h, "quality");
    h.action(UiAction::Right);
    h.settle();
    assert_eq!(
        value(&h, "video.quality"),
        Some(Value::Text("high".to_owned()))
    );
    assert_eq!(select(&h, "quality").index, 2);
    h.action(UiAction::Right);
    h.settle();
    assert_eq!(select(&h, "quality").index, 0, "wrapped");
    h.action(UiAction::Left);
    h.settle();
    assert_eq!(select(&h, "quality").index, 2);
    assert_eq!(h.focused(), Some(quality), "the arrows were claimed");
    assert!(popup(&mut h).is_none());
    let parts = *h
        .world()
        .get::<slotted_ui::widgets::select::SelectParts>(quality)
        .unwrap();
    assert_eq!(h.text_of(parts.value).as_deref(), Some("opt.high"));
}

#[test]
fn a_select_popup_opens_in_pointer_mode_only_and_back_closes_it_without_popping() {
    let mut h = harness();
    open(&mut h);
    let quality = focus(&mut h, "quality");

    // Keyboard Accept: no popup.
    h.action(UiAction::Accept);
    h.settle();
    assert_eq!(h.input_mode(), InputMode::Keyboard);
    assert!(popup(&mut h).is_none());
    assert!(!select(&h, "quality").open);

    // Accept in pointer mode: the popup opens under the pill with focus on
    // the active option.
    h.set_input_mode(InputMode::Pointer);
    action_event(&mut h, UiAction::Accept, InputDevice::Keyboard);
    h.settle();
    let popup_entity = popup(&mut h).expect("a popup");
    assert!(select(&h, "quality").open);
    let options: Vec<Entity> = h.find_all(&by::role(slotted_ui::SemanticRole::ListItem));
    assert_eq!(options.len(), 3);
    assert_eq!(h.focused(), Some(options[1]), "the active option");
    let pill = h.rect_of(
        h.world()
            .get::<slotted_ui::widgets::select::SelectParts>(quality)
            .unwrap()
            .pill,
    );
    let popup_rect = h.rect_of(popup_entity);
    assert!(
        popup_rect.min.y >= pill.max.y - 0.5,
        "under the pill: {popup_rect:?} vs {pill:?}"
    );

    // Down moves, focus stays inside the popup, the stack is untouched.
    h.action(UiAction::Down);
    h.step(1);
    assert_eq!(h.focused(), Some(options[2]));
    h.action(UiAction::Down);
    h.step(1);
    assert_eq!(h.focused(), Some(options[0]), "wraps");
    assert_eq!(h.stack(), vec![ScreenKind::new(SCREEN)]);

    // Back closes the popup, claimed: the screen stays.
    h.action(UiAction::Back);
    h.settle();
    assert!(popup(&mut h).is_none());
    assert!(!select(&h, "quality").open);
    assert_eq!(h.focused(), Some(quality));
    assert_eq!(h.stack(), vec![ScreenKind::new(SCREEN)]);
    assert_eq!(select(&h, "quality").index, 1, "nothing was written");

    // A second Back pops the screen.
    h.action(UiAction::Back);
    h.settle();
    assert!(h.stack().is_empty());
}

#[test]
fn a_click_on_the_select_opens_the_popup_and_accept_picks() {
    let mut h = harness();
    open(&mut h);
    let quality = find(&h, "quality");
    h.click(quality);
    h.settle();
    assert!(popup(&mut h).is_some());
    let options: Vec<Entity> = h.find_all(&by::role(slotted_ui::SemanticRole::ListItem));
    h.gamepad(GamepadButton::DPadDown);
    h.step(1);
    assert_eq!(h.focused(), Some(options[2]));
    h.gamepad(GamepadButton::South);
    h.settle();
    assert!(popup(&mut h).is_none());
    assert_eq!(
        value(&h, "video.quality"),
        Some(Value::Text("high".to_owned()))
    );
    assert_eq!(select(&h, "quality").index, 2);
    assert_eq!(h.focused(), Some(quality));

    // A click on an option picks it too.
    h.click(quality);
    h.settle();
    let options: Vec<Entity> = h.find_all(&by::role(slotted_ui::SemanticRole::ListItem));
    h.click(options[0]);
    h.settle();
    assert!(popup(&mut h).is_none());
    assert_eq!(select(&h, "quality").index, 0);
    assert_eq!(
        value(&h, "video.quality"),
        Some(Value::Text("low".to_owned()))
    );
}

#[test]
fn a_radio_group_moves_on_the_arrows_without_wrapping_and_on_a_segment_click() {
    let mut h = harness();
    open(&mut h);
    let mode = focus(&mut h, "mode");
    h.action(UiAction::Right);
    h.settle();
    assert_eq!(radio(&h, "mode").index, 2, "the end is the end");
    assert_eq!(h.focused(), Some(mode), "still claimed");
    assert!(writes(&mut h).is_empty(), "nothing to write at the end");
    h.action(UiAction::Left);
    h.settle();
    assert_eq!(radio(&h, "mode").index, 1);
    assert_eq!(
        value(&h, "video.mode"),
        Some(Value::Text("borderless".to_owned()))
    );

    let mut segments = h
        .world_mut()
        .query::<(Entity, &slotted_ui::widgets::radio_group::RadioSegment)>();
    let first = segments
        .iter(h.world())
        .find(|(_, s)| s.index == 0)
        .map(|(e, _)| e)
        .unwrap();
    h.click(first);
    h.settle();
    assert_eq!(radio(&h, "mode").index, 0);
    assert_eq!(
        value(&h, "video.mode"),
        Some(Value::Text("windowed".to_owned()))
    );
    assert_eq!(
        role_of(&h, first),
        "radio.active.focus",
        "the click focused the row; focus beats hover"
    );
    h.set_focus(None);
    h.step(1);
    assert_eq!(role_of(&h, first), "radio.active.hover");
}

// ---------------------------------------------------------------------------
// Key binding
// ---------------------------------------------------------------------------

#[test]
fn a_key_capture_rebinds_accept_and_the_button_takes_the_new_key() {
    let mut h = harness();
    open(&mut h);
    let row = focus(&mut h, "kb_accept");
    let parts = *h
        .world()
        .get::<slotted_ui::widgets::key_binding::KeyBindingParts>(row)
        .unwrap();
    let shown = h.text_of(parts.text).unwrap();
    assert!(!shown.is_empty(), "shows the current binding");

    h.action(UiAction::Accept);
    h.step(1);
    let state = *h.world().get::<KeyBindingState>(row).unwrap();
    assert!(state.capturing, "Accept enters capture");
    assert_eq!(h.text_of(parts.text).as_deref(), Some("…"));
    assert_eq!(role_of(&h, parts.cell), "key_binding.capturing");

    // Escape cancels and does not pop the screen.
    h.key(KeyCode::Escape);
    h.step(1);
    assert!(!h.world().get::<KeyBindingState>(row).unwrap().capturing);
    assert_eq!(h.stack(), vec![ScreenKind::new(SCREEN)]);

    // Capture again; K becomes Accept's first key.
    h.action(UiAction::Accept);
    h.step(1);
    h.key(KeyCode::KeyK);
    h.step(1);
    assert!(!h.world().get::<KeyBindingState>(row).unwrap().capturing);
    assert_eq!(
        h.world()
            .resource::<UiBindings>()
            .first_key(UiAction::Accept),
        Some(KeyCode::KeyK)
    );
    let rebound = std::mem::take(&mut h.world_mut().resource_mut::<Seen>().rebound);
    assert_eq!(
        rebound,
        vec![BindingChanged {
            action: UiAction::Accept,
            device: InputDevice::Keyboard,
        }]
    );
    assert_ne!(h.text_of(parts.text).as_deref(), Some("…"));
    assert_ne!(h.text_of(parts.text).as_deref(), Some(shown.as_str()));

    // The button answers to K now, and did not answer to the capture press.
    assert!(activated(&mut h).is_empty());
    let ok = focus(&mut h, "ok");
    h.key(KeyCode::KeyK);
    assert_eq!(activated(&mut h), vec![ok]);
}

#[test]
fn a_captured_arrow_is_claimed_so_focus_does_not_move() {
    let mut h = harness();
    open(&mut h);
    let row = focus(&mut h, "kb_accept");
    h.action(UiAction::Accept);
    h.step(1);
    h.key(KeyCode::ArrowUp);
    h.step(1);
    assert_eq!(h.focused(), Some(row), "the press was swallowed");
    assert_eq!(
        h.world()
            .resource::<UiBindings>()
            .first_key(UiAction::Accept),
        Some(KeyCode::ArrowUp)
    );
}
