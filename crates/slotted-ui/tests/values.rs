//! The value store (menus M1 contract 1.1, 3.1, 3.7): rules clamp and snap,
//! options are checked, guards refuse or rewrite, every outcome is one
//! message, `version` moves only on a commit, and a `property` binding
//! round-trips through `SetProperty` and `PropertyChanged`.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::float_cmp)]

use std::sync::Arc;

use bevy::prelude::*;
use slotted_ecs::{MenuProperty, SetProperty};
use slotted_model::{Actor, Inventory, MenuDef, PropertyDef, PropertyId};
use slotted_test::prelude::*;
use slotted_ui::def::BindDef;
use slotted_ui::{
    Layout, ScreenDef, SetValue, Tags, ToggleState, ToggleStyle, UiAction, UiNodeDef, Value,
    ValueChanged, ValueGuards, ValueRefused, ValueRule, ValueRules, ValueStore,
};

// ---------------------------------------------------------------------------
// A bare app around the store
// ---------------------------------------------------------------------------

#[derive(Resource, Default)]
struct Seen {
    changed: Vec<ValueChanged>,
    refused: Vec<ValueRefused>,
}

fn record(
    mut changed: MessageReader<ValueChanged>,
    mut refused: MessageReader<ValueRefused>,
    mut seen: ResMut<Seen>,
) {
    seen.changed.extend(changed.read().cloned());
    seen.refused.extend(refused.read().cloned());
}

/// The store's systems alone: no UI, no window, one `Update` per `step`.
fn store_app() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    slotted_ui::values::build(&mut app);
    app.init_resource::<Seen>()
        .add_systems(Update, record.after(slotted_ui::values::ValueApply));
    app
}

fn set(app: &mut App, key: &str, value: impl Into<Value>) {
    app.world_mut().write_message(SetValue {
        key: key.to_owned(),
        value: value.into(),
        source: None,
    });
    app.update();
}

fn value(app: &App, key: &str) -> Option<Value> {
    app.world().resource::<ValueStore>().get(key).cloned()
}

fn version(app: &App) -> u64 {
    app.world().resource::<ValueStore>().version()
}

fn drain(app: &mut App) -> (Vec<ValueChanged>, Vec<ValueRefused>) {
    let mut seen = app.world_mut().resource_mut::<Seen>();
    (
        std::mem::take(&mut seen.changed),
        std::mem::take(&mut seen.refused),
    )
}

fn rule(app: &mut App, key: &str, rule: ValueRule) {
    app.world_mut()
        .resource_mut::<ValueRules>()
        .insert(key, rule);
}

#[test]
fn a_write_without_a_rule_commits_as_given_and_bumps_the_version() {
    let mut app = store_app();
    let before = version(&app);
    set(&mut app, "audio.master", 0.75);
    assert_eq!(value(&app, "audio.master"), Some(Value::Float(0.75)));
    assert_eq!(version(&app), before + 1);
    let (changed, refused) = drain(&mut app);
    assert_eq!(
        changed,
        vec![ValueChanged {
            key: "audio.master".to_owned(),
            old: None,
            new: Value::Float(0.75),
            source: None,
        }]
    );
    assert!(refused.is_empty());

    // A second write reports the old value.
    set(&mut app, "audio.master", 0.5);
    let (changed, _) = drain(&mut app);
    assert_eq!(changed.len(), 1);
    assert_eq!(changed[0].old, Some(Value::Float(0.75)));
    assert_eq!(version(&app), before + 2);
}

#[test]
fn a_rule_clamps_and_snaps_numbers() {
    let mut app = store_app();
    rule(
        &mut app,
        "video.fov",
        ValueRule {
            min: Some(60.0),
            max: Some(120.0),
            step: Some(5.0),
            options: vec![],
        },
    );
    set(&mut app, "video.fov", 200.0);
    assert_eq!(
        value(&app, "video.fov"),
        Some(Value::Float(120.0)),
        "clamped"
    );
    set(&mut app, "video.fov", 10.0);
    assert_eq!(
        value(&app, "video.fov"),
        Some(Value::Float(60.0)),
        "clamped low"
    );
    set(&mut app, "video.fov", 92.0);
    assert_eq!(
        value(&app, "video.fov"),
        Some(Value::Float(90.0)),
        "snapped to the step grid from min"
    );
    set(&mut app, "video.fov", 93i64);
    assert_eq!(
        value(&app, "video.fov"),
        Some(Value::Int(95)),
        "an Int stays an Int"
    );
    let (changed, refused) = drain(&mut app);
    assert_eq!(changed.len(), 4, "every clamped write commits");
    assert!(refused.is_empty());
}

#[test]
fn a_rule_with_options_refuses_a_text_outside_them() {
    let mut app = store_app();
    rule(
        &mut app,
        "video.quality",
        ValueRule {
            options: vec!["low".to_owned(), "high".to_owned()],
            ..ValueRule::default()
        },
    );
    set(&mut app, "video.quality", "high");
    assert_eq!(
        value(&app, "video.quality"),
        Some(Value::Text("high".to_owned()))
    );
    let before = version(&app);
    set(&mut app, "video.quality", "ultra");
    assert_eq!(
        value(&app, "video.quality"),
        Some(Value::Text("high".to_owned())),
        "the refused write left the value alone"
    );
    assert_eq!(version(&app), before, "a refusal does not move the version");
    let (changed, refused) = drain(&mut app);
    assert_eq!(changed.len(), 1);
    assert_eq!(refused.len(), 1);
    assert_eq!(refused[0].key, "video.quality");
    assert_eq!(refused[0].value, Value::Text("ultra".to_owned()));
    assert!(refused[0].reason.contains("ultra"), "{}", refused[0].reason);
}

#[test]
fn a_guard_refuses_and_a_guard_rewrites_in_order() {
    let mut app = store_app();
    {
        let mut guards = app.world_mut().resource_mut::<ValueGuards>();
        // The first guard vetoes anything under 0.2; the second halves it.
        guards.push(|key: &str, proposed: &Value, _store: &ValueStore| {
            if key == "audio.master" && proposed.as_f64().is_some_and(|v| v < 0.2) {
                Err("too quiet".to_owned())
            } else {
                Ok(proposed.clone())
            }
        });
        guards.push(|_key: &str, proposed: &Value, _store: &ValueStore| {
            Ok(match proposed {
                Value::Float(f) => Value::Float(f / 2.0),
                other => other.clone(),
            })
        });
    }
    set(&mut app, "audio.master", 0.8);
    assert_eq!(
        value(&app, "audio.master"),
        Some(Value::Float(0.4)),
        "the rewrite committed"
    );
    let v = version(&app);
    set(&mut app, "audio.master", 0.1);
    assert_eq!(value(&app, "audio.master"), Some(Value::Float(0.4)));
    assert_eq!(version(&app), v);
    let (changed, refused) = drain(&mut app);
    assert_eq!(changed.len(), 1);
    assert_eq!(changed[0].new, Value::Float(0.4));
    assert_eq!(refused.len(), 1);
    assert_eq!(refused[0].reason, "too quiet");
}

#[test]
fn a_guard_sees_the_store_as_it_is_before_the_commit() {
    let mut app = store_app();
    app.world_mut()
        .resource_mut::<ValueStore>()
        .insert("video.vsync", true);
    app.world_mut().resource_mut::<ValueGuards>().push(
        |key: &str, proposed: &Value, store: &ValueStore| {
            // A frame cap needs vsync off.
            if key == "video.fps_cap" && store.get("video.vsync") == Some(&Value::Bool(true)) {
                Err("vsync is on".to_owned())
            } else {
                Ok(proposed.clone())
            }
        },
    );
    set(&mut app, "video.fps_cap", 60i64);
    assert_eq!(value(&app, "video.fps_cap"), None);
    set(&mut app, "video.vsync", false);
    set(&mut app, "video.fps_cap", 60i64);
    assert_eq!(value(&app, "video.fps_cap"), Some(Value::Int(60)));
}

#[test]
fn seeding_bumps_the_version_without_a_message() {
    let mut app = store_app();
    let before = version(&app);
    app.world_mut()
        .resource_mut::<ValueStore>()
        .insert("a", 1i64);
    app.update();
    assert_eq!(version(&app), before + 1);
    let (changed, refused) = drain(&mut app);
    assert!(changed.is_empty() && refused.is_empty());
    let keys: Vec<&str> = app.world().resource::<ValueStore>().keys().collect();
    assert_eq!(keys, vec!["a"]);
}

// ---------------------------------------------------------------------------
// A property binding through a real menu
// ---------------------------------------------------------------------------

const SCREEN: &str = "values:machine";
const MODE: PropertyId = PropertyId(0);

struct OneProperty;

impl MenuFixture for OneProperty {
    fn def(&self) -> Arc<MenuDef> {
        let mut def = MenuDef::generic(1);
        def.properties = vec![PropertyDef {
            id: MODE,
            initial: 1,
        }];
        Arc::new(def)
    }

    fn inventories(&self) -> Vec<Inventory> {
        self.def()
            .inventory_sizes()
            .into_iter()
            .map(Inventory::new)
            .collect()
    }

    fn actor(&self) -> Actor {
        Actor::SURVIVAL
    }
}

fn screen() -> ScreenDef {
    ScreenDef {
        kind: ScreenKind::new(SCREEN),
        initial_focus: Some("mode".to_owned()),
        presentation: Presentation::default(),
        inherits: None,
        remove: vec![],
        listring: vec![],
        root: UiNodeDef::Panel {
            role: slotted_theme::roles::PANEL,
            layout: Layout::default(),
            children: vec![UiNodeDef::Toggle {
                label: None,
                style: ToggleStyle::Switch,
                bind: BindDef {
                    bind: None,
                    property: Some(MODE),
                    disabled: false,
                },
                tags: Tags::new().with(Tags::TEST_ID, "mode"),
            }],
            tags: Tags::new(),
        },
    }
}

fn property(h: &mut UiHarness, menu: Entity, id: PropertyId) -> i32 {
    let mut q = h.world_mut().query::<(&MenuProperty, &ChildOf)>();
    q.iter(h.world())
        .find(|(p, child_of)| child_of.parent() == menu && p.id == id)
        .map(|(p, _)| p.value)
        .expect("the menu has the property")
}

#[test]
fn a_property_binding_seeds_from_the_menu_and_writes_back_through_set_property() {
    let mut h = UiHarness::builder()
        .plugins(SlottedPlugins::headless())
        .registries(TestRegistries::basic())
        .theme("glass")
        .build();
    let opened = h.open_screen(screen(), OneProperty);
    h.settle();
    let toggle = h.find(&by::test_id("mode"));
    assert!(
        h.world().get::<ToggleState>(toggle).unwrap().on,
        "seeded from the property's initial value of 1"
    );

    // The host writes the property: the toggle follows.
    h.world_mut().trigger(SetProperty {
        entity: opened.menu,
        id: MODE,
        value: 0,
    });
    h.settle();
    assert!(!h.world().get::<ToggleState>(toggle).unwrap().on);

    // Accept on the toggle writes the property, not the store.
    assert_eq!(h.focused(), Some(toggle));
    h.action(UiAction::Accept);
    h.settle();
    assert_eq!(property(&mut h, opened.menu, MODE), 1);
    assert!(h.world().get::<ToggleState>(toggle).unwrap().on);
    assert_eq!(
        h.world().resource::<ValueStore>().keys().count(),
        0,
        "nothing reached the store"
    );
}
