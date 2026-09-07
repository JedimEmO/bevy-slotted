//! The settings demo (menus M1, package D): every M1 control on one modal
//! screen, bound to a `ValueStore` this module seeds, rules and guards.
//!
//! What a game supplies for a settings screen is exactly what is here: the
//! screen file (`assets/screens/demo_settings.screen.ron`, compiled in
//! through [`crate::screens`]), the initial values, the ranges and option
//! lists the store enforces, a [`ValueGuard`] for the rule a range cannot
//! express, and an observer on the one plain button. The controls own no
//! value: each paints what the store holds and writes a `SetValue` the store
//! may refuse (docs/guide/values.md).
//!
//! The chest example opens it over the chest with `Menu` (Tab, or Start on
//! a pad); a test opens it with `UiHarness::open`. It is a fixture for the
//! harness and the showcase, not yet the M2 settings template.

use bevy::prelude::*;
use bevy::ui_widgets::Activate;
use slotted::packs::Locales;
use slotted::packs::locale::LocaleTable;
use slotted::prelude::*;
use slotted::ui::{
    LocKey, Localization, SetValue, TestId, UiActionEvent, Value, ValueGuard, ValueGuards,
    ValueRule, ValueRules, ValueStore,
};

/// The screen this module opens.
pub const SETTINGS: &str = "demo:settings";

/// The screen file, relative to the asset root.
pub const SCREEN_PATH: &str = "screens/demo_settings.screen.ron";

/// The most a player may scale the UI to. A slider's own `max` is 3.0, so
/// the store has something to refuse: a drag past this snaps back.
pub const MAX_UI_SCALE: f64 = 2.0;

/// The compiled-in `demo:settings` screen.
pub fn screen() -> ScreenDef {
    crate::screens::parse("settings", crate::screens::SETTINGS_SCREEN_RON)
}

/// Every `settings.*` key with its default. The seed, and what `Reset`
/// writes back.
pub fn defaults() -> Vec<(&'static str, Value)> {
    vec![
        ("settings.tab", Value::Text("display".to_owned())),
        ("settings.resolution", Value::Text("1920x1080".to_owned())),
        ("settings.ui_scale", Value::Float(1.0)),
        ("settings.reduced_motion", Value::Bool(false)),
        ("settings.colour_mode", Value::Text("normal".to_owned())),
        ("settings.audio.master", Value::Float(80.0)),
        ("settings.audio.music", Value::Float(60.0)),
        ("settings.audio.effects", Value::Float(100.0)),
        ("settings.audio.device", Value::Text("auto".to_owned())),
        ("settings.audio.mute_in_background", Value::Bool(true)),
        ("settings.audio.subtitles", Value::Bool(false)),
        ("settings.audio.mono", Value::Bool(false)),
        ("settings.player_name", Value::Text("Steve".to_owned())),
    ]
}

/// The ranges and option lists the store enforces, whatever writes: a Lua
/// `set_value`, a harness, or a control.
pub fn rules() -> Vec<(&'static str, ValueRule)> {
    let options = |ids: &[&str]| ValueRule {
        options: ids.iter().map(|id| (*id).to_owned()).collect(),
        ..ValueRule::default()
    };
    let range = |min: f64, max: f64, step: f64| ValueRule {
        min: Some(min),
        max: Some(max),
        step: Some(step),
        options: Vec::new(),
    };
    vec![
        ("settings.tab", options(&["display", "audio", "controls"])),
        (
            "settings.resolution",
            options(&["1280x720", "1920x1080", "2560x1440"]),
        ),
        ("settings.ui_scale", range(0.5, 3.0, 0.25)),
        (
            "settings.colour_mode",
            options(&["normal", "deuteranopia", "high_contrast"]),
        ),
        ("settings.audio.master", range(0.0, 100.0, 5.0)),
        ("settings.audio.music", range(0.0, 100.0, 5.0)),
        ("settings.audio.effects", range(0.0, 100.0, 5.0)),
        (
            "settings.audio.device",
            options(&["auto", "speakers", "headphones"]),
        ),
    ]
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

/// Registers the screen, seeds the store, installs the rules and the guard,
/// and gives the `Reset` button something to do.
///
/// The base locale is loaded too, from the compiled-in
/// `assets/locale/en-US.ftl`, unless a `Localization` is already present:
/// the settings labels and the footer's `{key:..}` placeholders are Fluent
/// strings, and a game with no mods installed has nothing else that reads
/// the file. A pack install layers the mods' catalogues over the same base.
#[derive(Debug, Clone, Copy, Default)]
pub struct SettingsDemoPlugin;

impl Plugin for SettingsDemoPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, (register_screen, seed_store, install_locale))
            .add_observer(on_reset);
    }
}

/// `Startup`: the compiled-in definition. The windowed example's
/// `AssetServer` load replaces it a frame later with the bytes on disk, so
/// the file stays hot-reloadable.
pub fn register_screen(mut screens: ResMut<Screens>) {
    if screens.get(&ScreenKind::new(SETTINGS)).is_none() {
        screens.register(screen());
    }
}

/// `Startup`: defaults, rules and the guard. A key the game already seeded
/// is left alone, so a saved settings file wins over the defaults.
pub fn seed_store(
    mut store: ResMut<ValueStore>,
    mut rule_table: ResMut<ValueRules>,
    mut guards: ResMut<ValueGuards>,
) {
    for (key, value) in defaults() {
        if store.get(key).is_none() {
            store.insert(key, value);
        }
    }
    for (key, rule) in rules() {
        rule_table.insert(key, rule);
    }
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

/// The `Reset` button: every default written back through the store, so the
/// rules, the guard and every bound control see it as an ordinary change.
fn on_reset(activate: On<Activate>, ids: Query<&TestId>, mut writes: MessageWriter<SetValue>) {
    if ids.get(activate.entity).map(|id| id.0.as_str()) != Ok("reset") {
        return;
    }
    for (key, value) in defaults() {
        if key == "settings.tab" {
            continue;
        }
        writes.write(SetValue {
            key: key.to_owned(),
            value,
            source: Some(activate.entity),
        });
    }
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

/// `SlottedUiSet::Input`: an unclaimed `Menu` action (Tab, or Start on a
/// pad) opens the settings screen and claims the action. `Back` pops it
/// through the stack like any other entry.
pub fn open_settings_on_menu(
    mut actions: MessageReader<UiActionEvent>,
    mut claims: ResMut<slotted::ui::UiActionClaims>,
    mut commands: Commands,
) {
    let wanted = actions
        .read()
        .any(|event| event.action == UiAction::Menu && !event.repeat);
    if !wanted || claims.is_claimed(UiAction::Menu) {
        return;
    }
    claims.claim(UiAction::Menu);
    open_settings(&mut commands);
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
    fn every_default_has_a_rule_or_is_a_free_value_and_the_guard_refuses_past_the_limit() {
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
        let rules = rules();
        for (key, value) in defaults() {
            if let Some((_, rule)) = rules.iter().find(|(k, _)| *k == key)
                && !rule.options.is_empty()
            {
                let id = value.as_str().expect("an option default is text");
                assert!(rule.options.iter().any(|o| o == id), "{key}: {id}");
            }
        }
    }
}
