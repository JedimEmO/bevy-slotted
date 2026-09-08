//! Settings (menus M2 contract 4.6), headless: the generated screen inherits
//! the frame and carries the anchors, defaults and rules derive from the
//! rows, a `MemorySettings` round trip saves a drag and reopens with it,
//! stale saved keys are dropped, bindings persist, reset goes through the
//! guards, a rebind conflict moves the key and toasts, and a mod-style
//! injection lands in `settings.<tab>.end`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::float_cmp)]

use std::path::Path;

use bevy::asset::AssetPlugin;
use bevy::prelude::*;
use pretty_assertions::assert_eq;
use slotted_menu::{
    FileSettings, MemorySettings, MenuPlugin, SavedSettings, Settings, SettingsReset, SettingsRow,
    SettingsSpec, SettingsStorage, SettingsStore, Toast, kinds,
};
use slotted_test::prelude::*;
use slotted_ui::{
    AnchorId, Injection, Injections, InputDevice, KeyBindingState, LocKey, Owner, SelectOption,
    SliderState, ToggleStyle, UiAction, UiBindings, UiNodeDef, Value, ValueGuard, ValueGuards,
    ValueRule, ValueStore,
};

const KIND: &str = "test:settings";

fn assets_dir() -> String {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets")
        .canonicalize()
        .expect("the workspace assets directory exists")
        .to_string_lossy()
        .into_owned()
}

fn key(s: &str) -> LocKey {
    LocKey(s.to_owned())
}

/// A spec with one of each value row, two binding rows and two tabs.
fn spec() -> SettingsSpec {
    SettingsSpec::new(ScreenKind::new(KIND))
        .tab("general", "t.tab.general")
        .row(SettingsRow::Heading(key("t.heading")))
        .row(SettingsRow::Slider {
            key: "t.volume".to_owned(),
            label: key("t.volume"),
            min: 0.0,
            max: 100.0,
            step: 5.0,
            default: 50.0,
            format: "{value}%".to_owned(),
        })
        .row(SettingsRow::Toggle {
            key: "t.fullscreen".to_owned(),
            label: key("t.fullscreen"),
            default: false,
            style: ToggleStyle::Switch,
        })
        .row(SettingsRow::Select {
            key: "t.quality".to_owned(),
            label: key("t.quality"),
            options: vec![
                SelectOption {
                    id: "low".to_owned(),
                    label: key("t.low"),
                },
                SelectOption {
                    id: "high".to_owned(),
                    label: key("t.high"),
                },
            ],
            default: "high".to_owned(),
        })
        .row(SettingsRow::Separator)
        .row(SettingsRow::Text {
            key: "t.name".to_owned(),
            label: key("t.name"),
            default: "Ada".to_owned(),
            placeholder: None,
            max_len: Some(8),
        })
        .tab("controls", "t.tab.controls")
        .row(SettingsRow::Binding {
            action: UiAction::Accept,
            device: InputDevice::Keyboard,
            label: None,
        })
        .row(SettingsRow::Binding {
            action: UiAction::TabNext,
            device: InputDevice::Keyboard,
            label: None,
        })
        .row(SettingsRow::Binding {
            action: UiAction::Accept,
            device: InputDevice::Gamepad,
            label: None,
        })
}

/// `MenuPlugin` unless the facade's default features already put it in
/// `SlottedPlugins` (feature unification under a workspace build).
struct MenuIfMissing;

impl Plugin for MenuIfMissing {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<MenuPlugin>() {
            app.add_plugins(MenuPlugin);
        }
    }
}

/// The spec as `Settings`, the store as `SettingsStorage` when given, and
/// `extra` for whatever a test adds before startup.
struct SpecPlugin {
    store: Option<MemorySettings>,
    extra: fn(&mut App),
}

impl Plugin for SpecPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Settings { spec: spec() });
        if let Some(store) = &self.store {
            app.insert_resource(SettingsStorage::new(store.clone()));
        }
        (self.extra)(app);
    }
}

fn harness(store: Option<MemorySettings>, extra: fn(&mut App)) -> UiHarness {
    UiHarness::builder()
        .plugins((
            SlottedPlugins::headless().set(AssetPlugin {
                file_path: assets_dir(),
                ..default()
            }),
            MenuIfMissing,
            SpecPlugin { store, extra },
        ))
        .resolution(1280.0, 720.0)
        .theme("glass")
        .build()
}

fn open(h: &mut UiHarness) -> Entity {
    let root = h.open(ScreenKind::new(KIND));
    h.settle();
    root
}

fn find(h: &UiHarness, id: &str) -> Entity {
    h.find(&by::test_id(id))
}

// ---------------------------------------------------------------------------
// The generated screen
// ---------------------------------------------------------------------------

#[test]
fn the_generated_screen_inherits_the_frame_and_carries_the_anchors() {
    let spec = spec();
    let def = spec.screen_def();
    assert_eq!(def.kind, ScreenKind::new(KIND));
    assert_eq!(def.inherits, Some(kinds::settings()));
    assert_eq!(def.initial_focus.as_deref(), Some("settings.tabs"));
    assert_eq!(
        def.root.children().len(),
        1,
        "the root's one child is the tabs node"
    );
    let tabs = &def.root.children()[0];
    assert_eq!(tabs.id(), Some("settings.tabs"));
    let UiNodeDef::Tabs {
        tabs: defs,
        bind,
        children,
        ..
    } = tabs
    else {
        panic!("a tabs node, not {tabs:?}");
    };
    assert_eq!(bind.bind.as_deref(), Some("settings.tab"));
    assert_eq!(defs.len(), 2);
    assert_eq!(children.len(), 2, "one page per tab");
    for (tab, page) in defs.iter().zip(children) {
        let UiNodeDef::Scroll { children: rows, .. } = page else {
            panic!("a page is a scroll, not {page:?}");
        };
        assert_eq!(
            page.id(),
            Some(format!("settings.{}.page", tab.id).as_str())
        );
        assert_eq!(
            rows.first().and_then(UiNodeDef::id),
            Some(format!("settings.{}.start", tab.id).as_str())
        );
        assert_eq!(
            rows.last().and_then(UiNodeDef::id),
            Some(format!("settings.{}.end", tab.id).as_str())
        );
    }

    // Opened: the frame's title, footer and Reset are there around the
    // rows, and the anchors are real nodes.
    let mut h = harness(None, |_| {});
    let root = open(&mut h);
    assert_eq!(h.stack(), vec![ScreenKind::new(KIND)]);
    assert_eq!(h.find(&by::screen(ScreenKind::new(KIND))), root);
    for id in [
        "title",
        "reset",
        "done",
        "settings.tabs",
        "settings.general.page",
    ] {
        assert_eq!(h.find_all(&by::test_id(id)).len(), 1, "{id}");
    }
    for anchor in [
        "title_end",
        "settings.general.start",
        "settings.general.end",
        "settings.controls.start",
        "settings.controls.end",
    ] {
        assert_eq!(h.find_all(&by::anchor(anchor)).len(), 1, "{anchor}");
    }
    assert_eq!(h.find_all(&by::control("slider")).len(), 1);
    assert_eq!(h.find_all(&by::control("key_binding")).len(), 3);
    assert_eq!(
        h.find_all(&by::test_id("bind.keyboard.accept")).len(),
        1,
        "a binding row is `bind.<device>.<action>`"
    );
    let focused = h.focused().expect("initial focus");
    assert_eq!(
        h.world().get::<slotted_ui::SemanticRole>(focused).cloned(),
        Some(SemanticRole::Tab),
        "`initial_focus: settings.tabs` lands on the first tab button"
    );
}

#[test]
fn defaults_and_rules_derive_from_the_rows() {
    let spec = spec();
    let defaults = spec.defaults();
    assert_eq!(
        defaults,
        [
            ("t.fullscreen", Value::Bool(false)),
            ("t.name", Value::Text("Ada".to_owned())),
            ("t.quality", Value::Text("high".to_owned())),
            ("t.volume", Value::Float(50.0)),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_owned(), v))
        .collect()
    );
    assert_eq!(
        spec.keys(),
        vec!["t.fullscreen", "t.name", "t.quality", "t.volume"]
    );
    let rules = spec.rules();
    assert_eq!(
        rules.get("t.volume"),
        Some(&ValueRule {
            min: Some(0.0),
            max: Some(100.0),
            step: Some(5.0),
            options: Vec::new(),
        })
    );
    assert_eq!(
        rules.get("t.quality"),
        Some(&ValueRule {
            options: vec!["low".to_owned(), "high".to_owned()],
            ..ValueRule::default()
        })
    );
    assert_eq!(
        rules.get("settings.tab"),
        Some(&ValueRule {
            options: vec!["general".to_owned(), "controls".to_owned()],
            ..ValueRule::default()
        }),
        "the tab key is ruled to the tab ids"
    );
    assert!(rules.get("t.name").is_none(), "a text row implies no rule");
    assert!(rules.get("t.fullscreen").is_none());

    // And they are in the app after startup, seeded and enforced.
    let mut h = harness(None, |_| {});
    assert_eq!(h.value("t.volume"), Some(Value::Float(50.0)));
    assert_eq!(
        h.value("settings.tab"),
        Some(Value::Text("general".to_owned()))
    );
    h.set_value("t.volume", 1000.0);
    assert_eq!(
        h.value("t.volume"),
        Some(Value::Float(100.0)),
        "the derived rule clamps (FOLLOWUPS M1 item 12)"
    );
    h.set_value("t.volume", 42.0);
    assert_eq!(h.value("t.volume"), Some(Value::Float(40.0)), "and snaps");
    h.set_value("t.quality", "ultra");
    assert_eq!(h.value("t.quality"), Some(Value::Text("high".to_owned())));
}

// ---------------------------------------------------------------------------
// Persistence
// ---------------------------------------------------------------------------

#[test]
fn a_memory_store_round_trip_saves_a_drag_and_a_fresh_app_opens_with_it() {
    let store = MemorySettings::default();
    assert!(store.get().is_none());
    {
        let mut h = harness(Some(store.clone()), |_| {});
        open(&mut h);
        assert!(store.get().is_none(), "opening saves nothing");
        let slider = find(&h, "t.volume");
        h.drag_slider(slider, 0.8);
        h.settle();
        assert_eq!(h.value("t.volume"), Some(Value::Float(80.0)));
        let saved = store.get().expect("the drag saved");
        assert_eq!(saved.values.get("t.volume"), Some(&Value::Float(80.0)));
        assert_eq!(
            saved.values.len(),
            4,
            "every declared key, and nothing else: {:?}",
            saved.values.keys().collect::<Vec<_>>()
        );
        assert!(
            !saved.values.contains_key("settings.tab"),
            "the active tab is UI state"
        );
        assert!(saved.bindings.is_some(), "the bindings ride along");

        // A change to an undeclared key does not save.
        store.set(None);
        h.set_value("settings.tab", "controls");
        h.settle();
        assert!(store.get().is_none(), "a tab switch is not a setting");
        h.set_value("other.thing", 1.0);
        h.settle();
        assert!(store.get().is_none());
    }
    store.set(Some(SavedSettings {
        values: [("t.volume".to_owned(), Value::Float(80.0))]
            .into_iter()
            .collect(),
        bindings: None,
    }));

    let mut h = harness(Some(store.clone()), |_| {});
    assert_eq!(
        h.value("t.volume"),
        Some(Value::Float(80.0)),
        "the saved value seeds the store"
    );
    assert_eq!(
        h.value("t.name"),
        Some(Value::Text("Ada".to_owned())),
        "a key the file lacks takes its default"
    );
    open(&mut h);
    let slider = find(&h, "t.volume");
    assert_eq!(h.world().get::<SliderState>(slider).unwrap().value, 80.0);
}

#[test]
fn stale_saved_keys_are_dropped_and_a_game_seed_sits_under_a_saved_value() {
    let store = MemorySettings::with(SavedSettings {
        values: [
            ("t.volume".to_owned(), Value::Float(20.0)),
            ("t.removed".to_owned(), Value::Bool(true)),
        ]
        .into_iter()
        .collect(),
        bindings: None,
    });
    let mut h = harness(Some(store.clone()), |app| {
        let mut values = ValueStore::default();
        values.insert("t.volume", 5.0);
        values.insert("t.name", "Grace");
        app.insert_resource(values);
    });
    assert_eq!(
        h.value("t.volume"),
        Some(Value::Float(20.0)),
        "saved wins over the game's seed"
    );
    assert_eq!(
        h.value("t.name"),
        Some(Value::Text("Grace".to_owned())),
        "the game's seed wins over the default"
    );
    assert!(
        h.value("t.removed").is_none(),
        "a key the spec no longer declares is not loaded"
    );
    h.set_value("t.fullscreen", true);
    h.settle();
    let saved = store.get().unwrap();
    assert!(
        !saved.values.contains_key("t.removed"),
        "and not written back"
    );
    assert_eq!(saved.values.get("t.volume"), Some(&Value::Float(20.0)));
}

#[test]
fn bindings_persist_across_apps() {
    let store = MemorySettings::default();
    {
        let mut h = harness(Some(store.clone()), |_| {});
        open(&mut h);
        h.switch_tab(find(&h, "settings.tabs"), "controls");
        h.capture_key(find(&h, "bind.keyboard.tab_next"), KeyCode::KeyN);
        h.settle();
        let saved = store.get().expect("the rebind saved");
        assert_eq!(
            saved
                .bindings
                .as_ref()
                .unwrap()
                .first_key(UiAction::TabNext),
            Some(KeyCode::KeyN)
        );
    }
    let h = harness(Some(store), |_| {});
    assert_eq!(
        h.world()
            .resource::<UiBindings>()
            .first_key(UiAction::TabNext),
        Some(KeyCode::KeyN),
        "a fresh app restores the bindings"
    );
}

#[test]
fn the_file_store_round_trips_through_ron() {
    let dir = std::env::temp_dir().join(format!(
        "slotted-menu-settings-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let path = dir.join("nested").join("settings.ron");
    let file = FileSettings { path: path.clone() };
    assert!(file.load().is_none(), "no file, no settings");
    let saved = SavedSettings {
        values: [
            ("t.volume".to_owned(), Value::Float(35.0)),
            ("t.name".to_owned(), Value::Text("Ada".to_owned())),
        ]
        .into_iter()
        .collect(),
        bindings: Some(UiBindings::default()),
    };
    file.save(&saved);
    assert!(path.is_file(), "the directory was created");
    assert_eq!(file.load(), Some(saved.clone()));
    let text = std::fs::read_to_string(&path).unwrap();
    assert_eq!(
        MemorySettings::from_ron(&text).unwrap().get(),
        Some(saved),
        "the same RON a web page keeps"
    );
    std::fs::write(&path, "not ron").unwrap();
    assert!(
        file.load().is_none(),
        "a broken file is a warning, not a panic"
    );
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_settings_resource_inserted_after_startup_is_applied_the_frame_it_appears() {
    let mut h = UiHarness::builder()
        .plugins((
            SlottedPlugins::headless().set(AssetPlugin {
                file_path: assets_dir(),
                ..default()
            }),
            MenuIfMissing,
        ))
        .build();
    assert!(h.value("t.volume").is_none());
    assert!(
        h.world()
            .resource::<slotted_ui::Screens>()
            .get(&ScreenKind::new(KIND))
            .is_none()
    );
    h.world_mut().insert_resource(Settings { spec: spec() });
    h.step(1);
    assert_eq!(h.value("t.volume"), Some(Value::Float(50.0)));
    assert!(
        h.world()
            .resource::<slotted_ui::Screens>()
            .get(&ScreenKind::new(KIND))
            .is_some()
    );
}

// ---------------------------------------------------------------------------
// Reset
// ---------------------------------------------------------------------------

/// Refuses to change `t.name`: what a game's guard might lock.
struct NameLock;

impl ValueGuard for NameLock {
    fn check(&self, key: &str, proposed: &Value, store: &ValueStore) -> Result<Value, String> {
        if key == "t.name" && store.get(key).is_some_and(|v| v != proposed) {
            return Err("the name is locked".to_owned());
        }
        Ok(proposed.clone())
    }
}

#[test]
fn reset_restores_every_default_through_the_guards_and_the_bindings() {
    let store = MemorySettings::default();
    let mut h = harness(Some(store.clone()), |app| {
        app.world_mut().resource_mut::<ValueGuards>().push(NameLock);
    });
    open(&mut h);
    // Seed past the guard, the way a loaded file would.
    h.world_mut()
        .resource_mut::<ValueStore>()
        .insert("t.name", "Grace");
    h.set_value("t.volume", 10.0);
    h.set_value("t.fullscreen", true);
    h.set_value("t.quality", "low");
    h.world_mut()
        .resource_mut::<UiBindings>()
        .keys
        .insert(UiAction::Accept, vec![KeyCode::KeyK]);
    h.settle();
    store.set(None);

    h.click(find(&h, "reset"));
    h.settle();
    assert_eq!(h.value("t.volume"), Some(Value::Float(50.0)));
    assert_eq!(h.value("t.fullscreen"), Some(Value::Bool(false)));
    assert_eq!(h.value("t.quality"), Some(Value::Text("high".to_owned())));
    assert_eq!(
        h.value("t.name"),
        Some(Value::Text("Grace".to_owned())),
        "the guard refused the reset of the locked key"
    );
    assert_eq!(
        h.world()
            .get::<SliderState>(find(&h, "t.volume"))
            .unwrap()
            .value,
        50.0,
        "the slider painted the reset"
    );
    assert_eq!(
        h.world()
            .resource::<UiBindings>()
            .first_key(UiAction::Accept),
        Some(KeyCode::Enter),
        "the bindings are the defaults again"
    );
    let saved = store.get().expect("the reset saved");
    assert_eq!(saved.values.get("t.volume"), Some(&Value::Float(50.0)));

    // The message alone does the same.
    h.set_value("t.volume", 15.0);
    h.world_mut().write_message(SettingsReset);
    h.step(2);
    assert_eq!(h.value("t.volume"), Some(Value::Float(50.0)));
}

// ---------------------------------------------------------------------------
// Key conflicts
// ---------------------------------------------------------------------------

fn toasts(h: &mut UiHarness) -> usize {
    h.world_mut()
        .query_filtered::<Entity, With<Toast>>()
        .iter(h.world())
        .count()
}

#[test]
fn a_rebind_to_another_actions_key_moves_the_key_and_toasts() {
    let mut h = harness(None, |_| {});
    open(&mut h);
    h.switch_tab(find(&h, "settings.tabs"), "controls");
    let before = h.world().resource::<UiBindings>().clone();
    assert_eq!(before.first_key(UiAction::TabNext), Some(KeyCode::KeyE));

    // E was TabNext's; Accept takes it.
    let row = find(&h, "bind.keyboard.accept");
    h.capture_key(row, KeyCode::KeyE);
    h.settle();
    assert!(!h.world().get::<KeyBindingState>(row).unwrap().capturing);
    let bindings = h.world().resource::<UiBindings>().clone();
    assert_eq!(bindings.first_key(UiAction::Accept), Some(KeyCode::KeyE));
    assert!(
        !bindings.keys[&UiAction::TabNext].contains(&KeyCode::KeyE),
        "TabNext lost E: {:?}",
        bindings.keys[&UiAction::TabNext]
    );
    assert_eq!(
        bindings.buttons, before.buttons,
        "the pad's bindings are untouched"
    );
    assert_eq!(
        toasts(&mut h),
        1,
        "one toast for the one action that lost a key"
    );

    // E is Accept now, not TabNext: the tab stays.
    h.key(KeyCode::KeyE);
    h.settle();
    assert_eq!(
        h.value("settings.tab"),
        Some(Value::Text("controls".to_owned()))
    );

    // A key nothing else had moves nothing and toasts nothing.
    h.capture_key(find(&h, "bind.keyboard.tab_next"), KeyCode::KeyN);
    h.settle();
    assert_eq!(toasts(&mut h), 1);

    // The same on a pad: South was Accept's; binding it to the pad row of
    // Accept itself conflicts with nothing, binding East to it takes East
    // from Back.
    let pad = find(&h, "bind.gamepad.accept");
    h.set_focus(Some(pad));
    h.action(UiAction::Accept);
    h.step(1);
    h.gamepad(bevy::input::gamepad::GamepadButton::North);
    h.settle();
    let bindings = h.world().resource::<UiBindings>().clone();
    assert_eq!(
        bindings.first_button(UiAction::Accept),
        Some(bevy::input::gamepad::GamepadButton::North)
    );
    assert_eq!(toasts(&mut h), 1, "North was nobody's");
}

// ---------------------------------------------------------------------------
// Injection
// ---------------------------------------------------------------------------

#[test]
fn a_mod_style_injection_lands_at_the_end_of_a_tab() {
    let mut h = harness(None, |app| {
        app.world_mut()
            .get_resource_or_init::<Injections>()
            .0
            .push(Injection {
                target: ScreenKind::new(KIND),
                anchor: AnchorId::new("settings.general.end"),
                node: UiNodeDef::Toggle {
                    label: Some(key("mod.setting")),
                    style: ToggleStyle::Checkbox,
                    bind: slotted_ui::BindDef {
                        bind: Some("mod.setting".to_owned()),
                        ..Default::default()
                    },
                    tags: slotted_ui::Tags::new().with("test_id", "mod_row"),
                },
                exclusion: false,
                owner: Owner::Mod("mod".to_owned()),
            });
        app.world_mut()
            .get_resource_or_init::<ValueStore>()
            .insert("mod.setting", true);
    });
    open(&mut h);
    let row = find(&h, "mod_row");
    let page = find(&h, "settings.general.page");
    let mut ancestor = h.world().get::<ChildOf>(row).map(ChildOf::parent);
    let mut under_page = false;
    while let Some(e) = ancestor {
        if e == page {
            under_page = true;
            break;
        }
        ancestor = h.world().get::<ChildOf>(e).map(ChildOf::parent);
    }
    assert!(under_page, "the injected row is inside the general page");
    let name = h.rect_of(find(&h, "t.name"));
    let injected = h.rect_of(row);
    assert!(
        injected.min.y >= name.max.y - 0.5,
        "and after the last spec row: {injected:?} under {name:?}"
    );
    assert!(
        h.world().get::<slotted_ui::ToggleState>(row).unwrap().on,
        "and bound to the store like a spec row"
    );
}
