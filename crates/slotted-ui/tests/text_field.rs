//! Headless tests for the text field (menus M1 contract 4.1 and 4.6):
//! Accept enters editing and sets `TextEntryFocused`, typing changes the
//! state, Enter commits and writes, `Back` reverts and claims, the numeric
//! filter drops letters, `max_len` caps the text, a gamepad Accept writes
//! `TextEntryRequested`, the placeholder steps aside, and the store seeds
//! the field.
//!
//! Actions are triggered as `FocusedAction`s on the entity, which is what
//! the dispatch does for a real key; the messages the field writes are
//! collected by a test system rather than read back from the store, so the
//! tests do not depend on the store committing.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::float_cmp,
    clippy::similar_names,
    clippy::items_after_statements
)]

use std::sync::Arc;

use bevy::input_focus::InputFocus;
use bevy::prelude::*;
use pretty_assertions::assert_eq;
use slotted_test::prelude::*;
use slotted_theme::{Motion, Themed, roles};
use slotted_ui::def::{BindDef, LocKey, ScreenDef, ScreenKind, Tags, TextFilter, UiNodeDef};
use slotted_ui::{
    FocusedAction, InputDevice, Layout, LocText, Presentation, Screens, SetValue, TextEntryFocused,
    TextEntryRequested, TextFieldParts, TextFieldState, UiAction, UiActionClaims, Value,
    ValueStore, push_screen,
};

/// Every `SetValue` and `TextEntryRequested` written since the app started.
#[derive(Resource, Default)]
struct Written {
    values: Vec<SetValue>,
    requests: Vec<TextEntryRequested>,
}

fn collect(
    mut values: MessageReader<SetValue>,
    mut requests: MessageReader<TextEntryRequested>,
    mut written: ResMut<Written>,
) {
    written.values.extend(values.read().cloned());
    written.requests.extend(requests.read().copied());
}

struct Collector;

impl Plugin for Collector {
    fn build(&self, app: &mut App) {
        app.init_resource::<Written>().add_systems(Last, collect);
    }
}

fn harness() -> UiHarness {
    UiHarness::builder()
        .plugins((SlottedPlugins::headless(), Collector))
        .resolution(1280.0, 720.0)
        .theme("glass")
        .motion(Motion::REDUCED)
        .build()
}

fn field(id: &str, filter: TextFilter, max_len: Option<u16>, bind: Option<&str>) -> UiNodeDef {
    UiNodeDef::TextField {
        label: Some(LocKey("field.label".into())),
        placeholder: Some(LocKey("field.placeholder".into())),
        filter,
        max_len,
        bind: BindDef {
            bind: bind.map(str::to_owned),
            property: None,
            disabled: false,
        },
        tags: Tags::new().with(Tags::TEST_ID, id),
    }
}

fn screen(children: Vec<UiNodeDef>) -> ScreenDef {
    ScreenDef {
        kind: ScreenKind::new("t:text_field"),
        initial_focus: None,
        presentation: Presentation::default(),
        inherits: None,
        remove: vec![],
        listring: vec![],
        root: UiNodeDef::Panel {
            role: roles::PANEL,
            layout: Layout {
                width: Some(slotted_ui::def::Length::Px(400.0)),
                direction: slotted_ui::LayoutDirection::Column,
                ..Layout::default()
            },
            children,
            tags: Tags::new().with(Tags::TEST_ID, "root"),
        },
    }
}

fn open(h: &mut UiHarness, screen: ScreenDef) {
    let def: Arc<ScreenDef> = h.world_mut().resource_mut::<Screens>().register(screen);
    push_screen(&mut h.world_mut().commands(), def, None);
    h.world_mut().flush();
    h.settle();
}

fn state(h: &UiHarness, entity: Entity) -> TextFieldState {
    h.world().get::<TextFieldState>(entity).unwrap().clone()
}

fn editable_of(h: &UiHarness, entity: Entity) -> Entity {
    h.world().get::<TextFieldParts>(entity).unwrap().editable
}

fn act(h: &mut UiHarness, entity: Entity, action: UiAction, device: InputDevice) {
    h.world_mut().trigger(FocusedAction {
        entity,
        action,
        device,
        repeat: false,
    });
    h.step(1);
}

fn focused(h: &UiHarness) -> Option<Entity> {
    h.world().resource::<InputFocus>().get()
}

fn values(h: &UiHarness) -> Vec<SetValue> {
    h.world().resource::<Written>().values.clone()
}

#[test]
fn accept_enters_editing_and_sets_text_entry_focused() {
    let mut h = harness();
    open(
        &mut h,
        screen(vec![field(
            "name",
            TextFilter::Any,
            None,
            Some("player.name"),
        )]),
    );
    let row = h.find(&by::test_id("name"));
    let editable = editable_of(&h, row);

    h.set_focus(Some(row));
    let s = state(&h, row);
    assert!(!s.editing, "focus alone does not edit");
    assert!(
        !h.world().resource::<TextEntryFocused>().0,
        "a focused row does not own the keyboard"
    );

    act(&mut h, row, UiAction::Accept, InputDevice::Keyboard);
    assert_eq!(
        focused(&h),
        Some(editable),
        "Accept hands focus to the editable"
    );
    assert!(state(&h, row).editing);
    assert!(
        h.world().resource::<TextEntryFocused>().0,
        "editing owns the keyboard"
    );
    let frame = h.world().get::<TextFieldParts>(row).unwrap().frame;
    assert_eq!(
        h.world().get::<Themed>(frame).unwrap().0,
        roles::TEXT_FIELD_FOCUS
    );
}

#[test]
fn typing_changes_the_state_and_enter_commits_and_writes() {
    let mut h = harness();
    open(
        &mut h,
        screen(vec![field(
            "name",
            TextFilter::Any,
            None,
            Some("player.name"),
        )]),
    );
    let row = h.find(&by::test_id("name"));
    h.set_focus(Some(row));
    act(&mut h, row, UiAction::Accept, InputDevice::Keyboard);

    h.type_text("Ada");
    h.settle();
    assert_eq!(state(&h, row).text, "Ada");
    assert!(
        values(&h).is_empty(),
        "an unfiltered field writes only on commit"
    );

    h.key(KeyCode::Enter);
    h.settle();
    let written = values(&h);
    assert_eq!(
        written,
        vec![SetValue {
            key: "player.name".into(),
            value: Value::Text("Ada".into()),
            source: Some(row),
        }]
    );
    assert!(!state(&h, row).editing, "Enter leaves editing");
    assert_eq!(focused(&h), Some(row), "focus is back on the row");
    assert!(!h.world().resource::<TextEntryFocused>().0);
}

#[test]
fn back_reverts_claims_and_does_not_pop_the_screen() {
    let mut h = harness();
    open(
        &mut h,
        screen(vec![field(
            "name",
            TextFilter::Any,
            None,
            Some("player.name"),
        )]),
    );
    let row = h.find(&by::test_id("name"));
    let editable = editable_of(&h, row);
    h.set_focus(Some(row));
    act(&mut h, row, UiAction::Accept, InputDevice::Keyboard);
    h.type_text("Ada");
    h.key(KeyCode::Enter);
    h.settle();

    act(&mut h, row, UiAction::Accept, InputDevice::Keyboard);
    h.type_text("m");
    h.settle();
    assert_eq!(state(&h, row).text, "Adam");

    h.world_mut().trigger(FocusedAction {
        entity: editable,
        action: UiAction::Back,
        device: InputDevice::Keyboard,
        repeat: false,
    });
    assert!(
        h.world()
            .resource::<UiActionClaims>()
            .is_claimed(UiAction::Back),
        "Back is claimed, so the stack leaves the screen alone"
    );
    h.settle();
    assert_eq!(
        state(&h, row).text,
        "Ada",
        "Back reverts to the committed text"
    );
    assert!(!state(&h, row).editing);
    assert_eq!(focused(&h), Some(row));
    assert_eq!(values(&h).len(), 1, "the revert writes nothing");
    assert_eq!(h.stack().len(), 1, "the screen is still open");

    // The same key, delivered as a real press while editing, reaches the
    // field as `Back` through the emitter, since `Back` is the one keyboard
    // action a text field lets through.
    act(&mut h, row, UiAction::Accept, InputDevice::Keyboard);
    h.type_text("x");
    h.key(KeyCode::Escape);
    h.settle();
    assert_eq!(h.stack().len(), 1, "Escape while editing does not pop");
}

#[test]
fn the_numeric_filter_drops_letters_and_writes_live() {
    let mut h = harness();
    open(
        &mut h,
        screen(vec![field(
            "age",
            TextFilter::Numeric,
            None,
            Some("player.age"),
        )]),
    );
    let row = h.find(&by::test_id("age"));
    h.set_focus(Some(row));
    act(&mut h, row, UiAction::Accept, InputDevice::Keyboard);
    h.type_text("1a2.5");
    h.settle();
    assert_eq!(state(&h, row).text, "12.5");
    let written = values(&h);
    assert!(
        !written.is_empty(),
        "a filtered field writes on every change so a guard can clamp live"
    );
    assert_eq!(
        written.last().unwrap().value,
        Value::Text("12.5".into()),
        "the last live write is the current text"
    );
    assert!(
        written.iter().all(|w| w.key == "player.age"),
        "every write goes to the binding"
    );
}

#[test]
fn max_len_caps_the_text() {
    let mut h = harness();
    open(
        &mut h,
        screen(vec![field("tag", TextFilter::Any, Some(3), None)]),
    );
    let row = h.find(&by::test_id("tag"));
    h.set_focus(Some(row));
    act(&mut h, row, UiAction::Accept, InputDevice::Keyboard);
    h.type_text("abcdef");
    h.settle();
    assert_eq!(state(&h, row).text, "abc");
}

#[test]
fn a_gamepad_accept_requests_text_entry_and_nothing_else() {
    let mut h = harness();
    open(
        &mut h,
        screen(vec![field(
            "name",
            TextFilter::Any,
            None,
            Some("player.name"),
        )]),
    );
    let row = h.find(&by::test_id("name"));
    h.set_focus(Some(row));
    act(&mut h, row, UiAction::Accept, InputDevice::Gamepad);
    assert_eq!(
        h.world().resource::<Written>().requests,
        vec![TextEntryRequested { entity: row }]
    );
    assert!(!state(&h, row).editing, "nothing else happens on a pad");
    assert_eq!(focused(&h), Some(row));
    assert!(!h.world().resource::<TextEntryFocused>().0);
}

#[test]
fn the_placeholder_steps_aside_and_the_label_is_a_loc_text() {
    let mut h = harness();
    open(
        &mut h,
        screen(vec![field("name", TextFilter::Any, None, None)]),
    );
    let row = h.find(&by::test_id("name"));
    let frame = h.world().get::<TextFieldParts>(row).unwrap().frame;
    let placeholder = h
        .world()
        .get::<Children>(frame)
        .unwrap()
        .iter()
        .find(|c| h.world().get::<slotted_ui::TextPlaceholder>(*c).is_some())
        .expect("a placeholder child");
    assert!(h.is_visible(placeholder), "shown while empty");
    assert_eq!(
        h.world().get::<Themed>(placeholder).unwrap().0,
        roles::TEXT_FIELD_PLACEHOLDER
    );

    let label = h
        .world()
        .get::<Children>(row)
        .unwrap()
        .iter()
        .find(|c| h.world().get::<LocText>(*c).is_some())
        .expect("a label child");
    assert_eq!(
        h.world().get::<Themed>(label).unwrap().0,
        roles::CONTROL_LABEL
    );
    assert_eq!(
        h.world().get::<LocText>(label).unwrap().key,
        LocKey("field.label".into())
    );

    h.set_focus(Some(row));
    act(&mut h, row, UiAction::Accept, InputDevice::Keyboard);
    h.type_text("x");
    h.settle();
    assert!(!h.is_visible(placeholder), "hidden once there is text");

    // The row is control height, from the theme.
    let height = h.rect_of(row).height();
    let want = slotted_ui::screen::active_tokens(h.world())
        .sizes
        .control_height;
    assert_eq!(height, want);
}

#[test]
fn a_click_enters_editing_and_a_click_elsewhere_commits() {
    let mut h = harness();
    open(
        &mut h,
        screen(vec![
            field("name", TextFilter::Any, None, Some("player.name")),
            UiNodeDef::Button {
                widget: None,
                opts: slotted_ui::def::ButtonOpts {
                    label: Some(LocKey("ok".into())),
                    ..Default::default()
                },
                tags: Tags::new().with(Tags::TEST_ID, "ok"),
            },
        ]),
    );
    let row = h.find(&by::test_id("name"));
    let editable = editable_of(&h, row);
    h.click(row);
    h.settle();
    assert_eq!(focused(&h), Some(editable), "a click enters editing");
    assert!(state(&h, row).editing);
    h.type_text("Ada");
    let button = h.find(&by::test_id("ok"));
    h.click(button);
    h.settle();
    assert!(!state(&h, row).editing, "focus left the editable");
    assert_eq!(
        values(&h).last().map(|w| w.value.clone()),
        Some(Value::Text("Ada".into())),
        "leaving without Enter commits what was typed"
    );
}

#[test]
fn the_store_seeds_the_field_and_a_disabled_field_is_inert() {
    let mut h = harness();
    h.world_mut()
        .resource_mut::<ValueStore>()
        .insert("player.name", "Grace");
    open(
        &mut h,
        screen(vec![
            field("name", TextFilter::Any, None, Some("player.name")),
            UiNodeDef::TextField {
                label: None,
                placeholder: None,
                filter: TextFilter::Any,
                max_len: None,
                bind: BindDef {
                    bind: None,
                    property: None,
                    disabled: true,
                },
                tags: Tags::new().with(Tags::TEST_ID, "off"),
            },
        ]),
    );
    let row = h.find(&by::test_id("name"));
    assert_eq!(
        state(&h, row).text,
        "Grace",
        "painted from the store on open"
    );
    h.world_mut()
        .resource_mut::<ValueStore>()
        .insert("player.name", "Hopper");
    h.settle();
    assert_eq!(state(&h, row).text, "Hopper", "and repainted on a change");

    let off = h.find(&by::test_id("off"));
    assert!(state(&h, off).disabled);
    assert!(
        h.world()
            .get::<bevy::ui::InteractionDisabled>(off)
            .is_some()
    );
    h.set_focus(Some(off));
    act(&mut h, off, UiAction::Accept, InputDevice::Keyboard);
    assert!(!state(&h, off).editing, "a disabled field never edits");
    let frame = h.world().get::<TextFieldParts>(off).unwrap().frame;
    assert_eq!(
        h.world().get::<Themed>(frame).unwrap().0,
        roles::TEXT_FIELD_DISABLED
    );
}
