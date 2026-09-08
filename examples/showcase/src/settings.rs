//! The settings demo (menus M2, package C): every M1 control on the
//! `slotted:settings` frame, generated from a [`SettingsSpec`] rather than
//! read from a screen file.
//!
//! What a game supplies for a settings screen is exactly what is here: the
//! spec ([`spec`]), a [`ValueGuard`] for the rule a range cannot express,
//! and a `Settings` resource. `MenuPlugin` does the rest: it generates the
//! `demo:settings` screen over the frame, seeds the store with the saved
//! values over the defaults, installs the rules the rows imply, saves on
//! every change when a `SettingsStorage` is present, and answers the
//! frame's Reset button. The controls own no value: each paints what the
//! store holds and writes a `SetValue` the store may refuse
//! (docs/guide/values.md).
//!
//! The chest example reaches it through the pause screen's Settings button
//! (`MenuConfig::settings_kind`); a test opens it with `UiHarness::open`.

use bevy::prelude::*;
use slotted::menu::{MenuPlugin, Settings, SettingsRow, SettingsSpec};
use slotted::packs::Locales;
use slotted::packs::locale::LocaleTable;
use slotted::prelude::*;
use slotted::ui::{
    InputDevice, LocKey, Localization, SelectOption, Tags, TextOpts, TextRole, ToggleStyle,
    UiNodeDef, Value, ValueGuard, ValueGuards, ValueStore,
};

/// The screen this module opens.
pub const SETTINGS: &str = "demo:settings";

/// The most a player may scale the UI to. The slider's own `max` is 3.0, so
/// the store has something to refuse: a drag past this snaps back.
pub const MAX_UI_SCALE: f64 = 2.0;

fn key(s: &str) -> LocKey {
    LocKey(s.to_owned())
}

fn option(id: &str, label: &str) -> SelectOption {
    SelectOption {
        id: id.to_owned(),
        label: key(label),
    }
}

/// The demo's settings: three tabs with every M1 control, the same
/// `settings.*` keys the M1 fixture bound, and a fourth tab for the switches
/// the examples' demos read (menus M3: `demo.found_key`).
pub fn spec() -> SettingsSpec {
    demo_tab(controls_tab(audio_tab(display_tab(SettingsSpec::new(
        ScreenKind::new(SETTINGS),
    )))))
}

/// The key the menus example's dialogue gates its `secret` option on: a
/// plain toggle here, a `Condition` in `assets/dialogue/greeting.dialogue.ron`.
pub const FOUND_KEY: &str = "demo.found_key";

/// Demo: what the examples' demos read from the store. A game would keep
/// its own flags out of the settings screen; the example puts one here so
/// the dialogue's gated option can be tried without playing for it.
fn demo_tab(spec: SettingsSpec) -> SettingsSpec {
    spec.tab("demo", "demo.settings.tab.demo")
        .row(SettingsRow::Toggle {
            key: FOUND_KEY.to_owned(),
            label: key("demo.settings.demo.found_key"),
            default: false,
            style: ToggleStyle::Switch,
        })
}

/// Display: one of each value control.
fn display_tab(spec: SettingsSpec) -> SettingsSpec {
    spec.tab("display", "demo.settings.tab.display")
        .row(SettingsRow::Select {
            key: "settings.resolution".to_owned(),
            label: key("demo.settings.display.resolution"),
            options: vec![
                option("1280x720", "demo.settings.resolution.hd"),
                option("1920x1080", "demo.settings.resolution.fhd"),
                option("2560x1440", "demo.settings.resolution.qhd"),
            ],
            default: "1920x1080".to_owned(),
        })
        .row(SettingsRow::Slider {
            key: "settings.ui_scale".to_owned(),
            label: key("demo.settings.display.ui_scale"),
            min: 0.5,
            max: 3.0,
            step: 0.25,
            default: 1.0,
            format: "{value:.2}×".to_owned(),
        })
        .row(SettingsRow::Toggle {
            key: "settings.reduced_motion".to_owned(),
            label: key("demo.settings.display.reduced_motion"),
            default: false,
            style: ToggleStyle::Switch,
        })
        .row(SettingsRow::Radio {
            key: "settings.colour_mode".to_owned(),
            label: key("demo.settings.display.colour_mode"),
            options: vec![
                option("normal", "demo.settings.colour.normal"),
                option("deuteranopia", "demo.settings.colour.deuteranopia"),
                option("high_contrast", "demo.settings.colour.high_contrast"),
            ],
            default: "normal".to_owned(),
        })
}

/// Audio: twelve rows, three of them sliders.
fn audio_tab(spec: SettingsSpec) -> SettingsSpec {
    let volume = |k: &str, label: &str, default: f64| SettingsRow::Slider {
        key: k.to_owned(),
        label: key(label),
        min: 0.0,
        max: 100.0,
        step: 5.0,
        default,
        format: "{value}%".to_owned(),
    };
    let toggle = |k: &str, label: &str, default: bool, style: ToggleStyle| SettingsRow::Toggle {
        key: k.to_owned(),
        label: key(label),
        default,
        style,
    };
    spec.tab("audio", "demo.settings.tab.audio")
        .row(SettingsRow::Heading(key("demo.settings.audio.volume")))
        .row(volume(
            "settings.audio.master",
            "demo.settings.audio.master",
            80.0,
        ))
        .row(volume(
            "settings.audio.music",
            "demo.settings.audio.music",
            60.0,
        ))
        .row(volume(
            "settings.audio.effects",
            "demo.settings.audio.effects",
            100.0,
        ))
        .row(SettingsRow::Separator)
        .row(SettingsRow::Heading(key("demo.settings.audio.output")))
        .row(SettingsRow::Select {
            key: "settings.audio.device".to_owned(),
            label: key("demo.settings.audio.device"),
            options: vec![
                option("auto", "demo.settings.device.auto"),
                option("speakers", "demo.settings.device.speakers"),
                option("headphones", "demo.settings.device.headphones"),
            ],
            default: "auto".to_owned(),
        })
        .row(toggle(
            "settings.audio.mute_in_background",
            "demo.settings.audio.mute_in_background",
            true,
            ToggleStyle::Switch,
        ))
        .row(toggle(
            "settings.audio.subtitles",
            "demo.settings.audio.subtitles",
            false,
            ToggleStyle::Checkbox,
        ))
        .row(SettingsRow::Separator)
        .row(SettingsRow::Heading(key(
            "demo.settings.audio.accessibility",
        )))
        .row(toggle(
            "settings.audio.mono",
            "demo.settings.audio.mono",
            false,
            ToggleStyle::Checkbox,
        ))
}

/// Controls: four keyboard bindings, a name, and a custom row: a rich-text
/// hint whose `{key:..}` glyphs follow the live bindings.
fn controls_tab(spec: SettingsSpec) -> SettingsSpec {
    let binding = |action: UiAction, label: &str| SettingsRow::Binding {
        action,
        device: InputDevice::Keyboard,
        label: Some(key(label)),
    };
    spec.tab("controls", "demo.settings.tab.controls")
        .row(binding(UiAction::Accept, "demo.settings.controls.accept"))
        .row(binding(UiAction::Back, "demo.settings.controls.back"))
        .row(binding(
            UiAction::TabPrev,
            "demo.settings.controls.tab_prev",
        ))
        .row(binding(
            UiAction::TabNext,
            "demo.settings.controls.tab_next",
        ))
        .row(SettingsRow::Separator)
        .row(SettingsRow::Text {
            key: "settings.player_name".to_owned(),
            label: key("demo.settings.controls.player_name"),
            default: "Steve".to_owned(),
            placeholder: Some(key("demo.settings.controls.player_name_hint")),
            max_len: Some(16),
        })
        .row(SettingsRow::Custom(UiNodeDef::RichText {
            key: key("demo.settings.footer"),
            style: TextRole::Muted,
            opts: TextOpts {
                wrap: false,
                ..TextOpts::default()
            },
            tags: Tags::new().with(Tags::TEST_ID, "footer_hint"),
        }))
}

/// Refuses a UI scale above [`MAX_UI_SCALE`]. The slider's range goes to
/// 3.0 on purpose: the refusal is what the demo shows, and the control snaps
/// back with no code of its own.
#[derive(Debug, Default, Clone, Copy)]
pub struct UiScaleGuard;

impl ValueGuard for UiScaleGuard {
    fn check(&self, key: &str, proposed: &Value, _store: &ValueStore) -> Result<Value, String> {
        if key == "settings.ui_scale"
            && let Some(scale) = proposed.as_f64()
            && scale > MAX_UI_SCALE
        {
            return Err(format!("a UI scale of {scale} is more than {MAX_UI_SCALE}"));
        }
        Ok(proposed.clone())
    }
}

/// Inserts the [`Settings`] resource, the guard and the base locale, and
/// makes sure `MenuPlugin` is there to turn the spec into a screen.
///
/// The base locale is loaded from the compiled-in `assets/locale/en-US.ftl`
/// unless a `Localization` is already present: the settings labels and the
/// footer's `{key:..}` placeholders are Fluent strings, and a game with no
/// mods installed has nothing else that reads the file. A pack install
/// layers the mods' catalogues over the same base.
#[derive(Debug, Clone, Copy, Default)]
pub struct SettingsDemoPlugin;

impl Plugin for SettingsDemoPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<MenuPlugin>() {
            app.add_plugins(MenuPlugin);
        }
        app.insert_resource(Settings { spec: spec() })
            .add_systems(Startup, (install_guard, install_locale));
    }
}

/// `Startup`: the guard. The defaults and the rules come from the spec
/// through `MenuPlugin`.
pub fn install_guard(mut guards: ResMut<ValueGuards>) {
    guards.push(UiScaleGuard);
}

/// The base catalogue, compiled in.
const EN_US: &str = include_str!("../../../assets/locale/en-US.ftl");

/// `Startup`: the base `en-US` catalogue as the `Localization` port, unless
/// whatever is installed already resolves the settings strings (a pack
/// install layered the same file under the mods' own).
pub fn install_locale(mut commands: Commands, existing: Option<Res<Localization>>) {
    if existing.is_some_and(|loc| {
        loc.resolve(&LocKey("demo.settings.title".to_owned()))
            .is_some()
    }) {
        return;
    }
    let mut table = LocaleTable::default();
    if let Err(error) = table.push_layer(None, EN_US.to_owned()) {
        warn!(%error, "the base locale file is not valid Fluent");
    }
    let locales = Locales::new(table);
    commands.insert_resource(locales.port());
    commands.insert_resource(locales);
}

/// Pushes the settings screen as a modal over whatever is open. Nothing
/// happens when it is already the top of the stack.
pub fn open_settings(commands: &mut Commands) {
    commands.queue(|world: &mut World| {
        let kind = ScreenKind::new(SETTINGS);
        if world
            .resource::<ScreenStack>()
            .top()
            .is_some_and(|top| top.kind == kind)
        {
            return;
        }
        let Some(def) = world.resource::<Screens>().get(&kind).cloned() else {
            error!("{SETTINGS} is not registered");
            return;
        };
        let mut commands = world.commands();
        push_screen(&mut commands, def, None);
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use slotted::ui::{LocKey, Localizer, no_args};

    #[test]
    fn the_base_catalogue_parses_and_resolves_the_settings_strings() {
        let mut table = LocaleTable::default();
        table
            .push_layer(None, EN_US.to_owned())
            .expect("assets/locale/en-US.ftl is valid Fluent");
        let text = |key: &str| Localizer::resolve(&table, &LocKey(key.to_owned()), no_args());
        assert_eq!(text("demo.settings.title").as_deref(), Some("Settings"));
        assert_eq!(
            text("demo.settings.footer").as_deref(),
            Some("Press {key:back} to close, {key:tab_next} for the next tab"),
            "Fluent's string literals hand the rich placeholders through"
        );
    }

    #[test]
    fn the_spec_declares_the_m1_keys_and_the_guard_refuses_past_the_limit() {
        let store = ValueStore::default();
        assert!(
            UiScaleGuard
                .check("settings.ui_scale", &Value::Float(2.0), &store)
                .is_ok()
        );
        assert!(
            UiScaleGuard
                .check("settings.ui_scale", &Value::Float(2.25), &store)
                .is_err()
        );
        assert!(
            UiScaleGuard
                .check("settings.audio.master", &Value::Float(100.0), &store)
                .is_ok()
        );
        let spec = spec();
        assert_eq!(
            spec.keys(),
            vec![
                "demo.found_key",
                "settings.audio.device",
                "settings.audio.effects",
                "settings.audio.master",
                "settings.audio.mono",
                "settings.audio.music",
                "settings.audio.mute_in_background",
                "settings.audio.subtitles",
                "settings.colour_mode",
                "settings.player_name",
                "settings.reduced_motion",
                "settings.resolution",
                "settings.ui_scale",
            ]
        );
        assert_eq!(spec.tab_key(), "settings.tab");
        let rules = spec.rules();
        for (key, value) in spec.defaults() {
            if let Some(rule) = rules.get(&key)
                && !rule.options.is_empty()
            {
                let id = value.as_str().expect("an option default is text");
                assert!(rule.options.iter().any(|o| o == id), "{key}: {id}");
            }
        }
    }
}
