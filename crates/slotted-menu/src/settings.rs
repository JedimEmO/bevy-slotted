//! Settings (menus M2 contract 4): a spec the game writes, the screen it
//! generates over the `slotted:settings` frame, and persistence through a
//! port.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use slotted_ui::{
    BindDef, InputDevice, Layout, LayoutDirection, Length, LocArgs, LocKey, Presentation,
    ScreenDef, ScreenKind, SelectOption, SetValue, Tags, TextOpts, TextRole, ToggleStyle, UiAction,
    UiBindings, UiNodeDef, Value, ValueChanged, ValueRule, ValueRules, ValueStore,
};

use crate::plugin::MenuChoice;
use crate::toast::{ToastSpec, toast};

/// One row of a settings tab.
#[derive(Debug, Clone, PartialEq)]
pub enum SettingsRow {
    /// A heading text.
    Heading(LocKey),
    /// A hairline.
    Separator,
    /// A switch or checkbox bound to `key`.
    Toggle {
        /// Store key.
        key: String,
        /// Label.
        label: LocKey,
        /// Default.
        default: bool,
        /// Switch or checkbox.
        style: ToggleStyle,
    },
    /// A slider bound to `key`; `min`, `max` and `step` become its rule.
    Slider {
        /// Store key.
        key: String,
        /// Label.
        label: LocKey,
        /// Range start.
        min: f64,
        /// Range end.
        max: f64,
        /// Step; `0` = one percent.
        step: f64,
        /// Default.
        default: f64,
        /// Readout format.
        format: String,
    },
    /// A select bound to `key`; the options become its rule.
    Select {
        /// Store key.
        key: String,
        /// Label.
        label: LocKey,
        /// Options.
        options: Vec<SelectOption>,
        /// Default option id.
        default: String,
    },
    /// A segmented control bound to `key`.
    Radio {
        /// Store key.
        key: String,
        /// Label.
        label: LocKey,
        /// Options.
        options: Vec<SelectOption>,
        /// Default option id.
        default: String,
    },
    /// A text field bound to `key`.
    Text {
        /// Store key.
        key: String,
        /// Label.
        label: LocKey,
        /// Default.
        default: String,
        /// Placeholder.
        placeholder: Option<LocKey>,
        /// Maximum length.
        max_len: Option<u16>,
    },
    /// A key binding row.
    Binding {
        /// Which action.
        action: UiAction,
        /// Keyboard or gamepad.
        device: InputDevice,
        /// Label; the action's name by default.
        label: Option<LocKey>,
    },
    /// Any node.
    Custom(UiNodeDef),
}

impl SettingsRow {
    /// The store key a value row binds, `None` for a heading, a separator,
    /// a binding row or a custom node.
    pub fn key(&self) -> Option<&str> {
        match self {
            Self::Toggle { key, .. }
            | Self::Slider { key, .. }
            | Self::Select { key, .. }
            | Self::Radio { key, .. }
            | Self::Text { key, .. } => Some(key),
            Self::Heading(_) | Self::Separator | Self::Binding { .. } | Self::Custom(_) => None,
        }
    }

    /// The row's default, for a value row.
    pub fn default_value(&self) -> Option<Value> {
        match self {
            Self::Toggle { default, .. } => Some(Value::Bool(*default)),
            Self::Slider { default, .. } => Some(Value::Float(*default)),
            Self::Select { default, .. } | Self::Radio { default, .. } => {
                Some(Value::Text(default.clone()))
            }
            Self::Text { default, .. } => Some(Value::Text(default.clone())),
            Self::Heading(_) | Self::Separator | Self::Binding { .. } | Self::Custom(_) => None,
        }
    }

    /// The rule the row implies: a slider's range and step, a select's or a
    /// radio group's options. `None` for every other row.
    pub fn rule(&self) -> Option<ValueRule> {
        match self {
            Self::Slider { min, max, step, .. } => Some(ValueRule {
                min: Some(*min),
                max: Some(*max),
                step: (*step > 0.0).then_some(*step),
                options: Vec::new(),
            }),
            Self::Select { options, .. } | Self::Radio { options, .. } => Some(ValueRule {
                options: options.iter().map(|o| o.id.clone()).collect(),
                ..ValueRule::default()
            }),
            _ => None,
        }
    }

    /// The node id the row spawns with: a value row's key, a binding row's
    /// `bind.<device>.<action>`, a custom node's own, none otherwise.
    pub fn test_id(&self) -> Option<String> {
        match self {
            Self::Binding { action, device, .. } => {
                Some(format!("bind.{}.{}", device.as_str(), action.as_str()))
            }
            Self::Custom(node) => node.id().map(str::to_owned),
            other => other.key().map(str::to_owned),
        }
    }

    /// The M1 control this row is (menus M2 contract 4.1), bound to its key
    /// and tagged with [`test_id`](Self::test_id).
    pub fn node(&self) -> UiNodeDef {
        let tags = self
            .test_id()
            .map_or_else(Tags::new, |id| Tags::new().with(Tags::TEST_ID, &id));
        let bind = |key: &str| BindDef {
            bind: Some(key.to_owned()),
            ..BindDef::default()
        };
        match self {
            Self::Heading(key) => UiNodeDef::Text {
                key: key.clone(),
                style: TextRole::Heading,
                opts: TextOpts::default(),
                tags,
            },
            Self::Separator => UiNodeDef::Separator {
                direction: LayoutDirection::default(),
                tags,
            },
            Self::Toggle {
                key, label, style, ..
            } => UiNodeDef::Toggle {
                label: Some(label.clone()),
                style: *style,
                bind: bind(key),
                tags,
            },
            Self::Slider {
                key,
                label,
                min,
                max,
                step,
                format,
                ..
            } => UiNodeDef::Slider {
                label: Some(label.clone()),
                min: *min,
                max: *max,
                step: *step,
                format: format.clone(),
                bind: bind(key),
                tags,
            },
            Self::Select {
                key,
                label,
                options,
                ..
            } => UiNodeDef::Select {
                label: Some(label.clone()),
                options: options.clone(),
                bind: bind(key),
                tags,
            },
            Self::Radio {
                key,
                label,
                options,
                ..
            } => UiNodeDef::RadioGroup {
                label: Some(label.clone()),
                options: options.clone(),
                bind: bind(key),
                tags,
            },
            Self::Text {
                key,
                label,
                placeholder,
                max_len,
                ..
            } => UiNodeDef::TextField {
                label: Some(label.clone()),
                placeholder: placeholder.clone(),
                filter: slotted_ui::TextFilter::default(),
                max_len: *max_len,
                bind: bind(key),
                tags,
            },
            Self::Binding {
                action,
                device,
                label,
            } => UiNodeDef::KeyBinding {
                label: label.clone(),
                action: *action,
                device: *device,
                disabled: false,
                tags,
            },
            Self::Custom(node) => node.clone(),
        }
    }
}

/// One tab.
#[derive(Debug, Clone, PartialEq)]
pub struct SettingsTab {
    /// The tab id; also the anchors `settings.<id>.start` and `.end`.
    pub id: String,
    /// The tab's label.
    pub label: LocKey,
    /// The rows.
    pub rows: Vec<SettingsRow>,
}

impl SettingsTab {
    /// The anchor at the top of the tab's page.
    pub fn start_anchor(&self) -> String {
        format!("settings.{}.start", self.id)
    }

    /// The anchor at the end of the tab's page, where a mod adds a row.
    pub fn end_anchor(&self) -> String {
        format!("settings.{}.end", self.id)
    }

    /// The page: a `scroll` of the rows between the two anchors, with id
    /// `settings.<tab>.page`.
    pub fn page(&self) -> UiNodeDef {
        let mut children = Vec::with_capacity(self.rows.len() + 2);
        children.push(UiNodeDef::Anchor {
            id: slotted_ui::AnchorId::new(self.start_anchor()),
        });
        children.extend(self.rows.iter().map(SettingsRow::node));
        children.push(UiNodeDef::Anchor {
            id: slotted_ui::AnchorId::new(self.end_anchor()),
        });
        UiNodeDef::Scroll {
            layout: Layout {
                direction: LayoutDirection::Column,
                gap: 1.0,
                max_height: Some(Length::Percent(70.0)),
                grow: 1.0,
                ..Layout::default()
            },
            scrollbar: true,
            children,
            tags: Tags::new().with(Tags::TEST_ID, &format!("settings.{}.page", self.id)),
        }
    }
}

/// The game's settings, as data the crate turns into a screen, defaults and
/// rules.
#[derive(Debug, Clone, PartialEq)]
pub struct SettingsSpec {
    /// The generated screen's kind.
    pub kind: ScreenKind,
    /// The tabs, in order.
    pub tabs: Vec<SettingsTab>,
}

impl SettingsSpec {
    /// A spec for `kind` with no tabs.
    pub fn new(kind: ScreenKind) -> Self {
        Self {
            kind,
            tabs: Vec::new(),
        }
    }

    /// Adds a tab.
    #[must_use]
    pub fn tab(mut self, id: impl Into<String>, label: impl Into<String>) -> Self {
        self.tabs.push(SettingsTab {
            id: id.into(),
            label: LocKey(label.into()),
            rows: Vec::new(),
        });
        self
    }

    /// Adds a row to the last tab. Panics with no tab, like a literal.
    #[must_use]
    pub fn row(mut self, row: SettingsRow) -> Self {
        self.tabs
            .last_mut()
            .expect("a tab before a row")
            .rows
            .push(row);
        self
    }

    /// The store key of the active tab binding.
    pub fn tab_key(&self) -> String {
        format!("{}.tab", self.kind.0.path())
    }

    /// The id of the `tabs` node, the one the frame's empty tabs node is
    /// replaced by.
    pub const TABS_ID: &'static str = "settings.tabs";

    /// The screen: `inherits: slotted:settings`, one `tabs` node with id
    /// `settings.tabs`, one page per tab with the rows and the anchors.
    ///
    /// The root takes the embedded frame's shape (role, layout, tags),
    /// because an inheriting root always wins on shape; a game that replaced
    /// the frame gets its own shape through [`screen_def_over`](Self::screen_def_over),
    /// which is what [`apply_settings`] uses.
    pub fn screen_def(&self) -> ScreenDef {
        let frame = ScreenDef::from_ron(crate::templates::SETTINGS)
            .expect("the embedded settings frame parses");
        self.screen_def_over(&frame)
    }

    /// [`screen_def`](Self::screen_def) over `frame`, the registered
    /// `slotted:settings` (a game's own, or the embedded one).
    pub fn screen_def_over(&self, frame: &ScreenDef) -> ScreenDef {
        let mut root = frame.root.clone();
        let tabs = UiNodeDef::Tabs {
            tabs: self
                .tabs
                .iter()
                .map(|tab| slotted_ui::TabDef {
                    id: tab.id.clone(),
                    label: tab.label.clone(),
                    icon: None,
                })
                .collect(),
            bind: BindDef {
                bind: Some(self.tab_key()),
                ..BindDef::default()
            },
            children: self.tabs.iter().map(SettingsTab::page).collect(),
            tags: Tags::new().with(Tags::TEST_ID, Self::TABS_ID),
        };
        match root.children_mut() {
            Some(children) => *children = vec![tabs],
            None => {
                root = UiNodeDef::Panel {
                    role: slotted_theme::roles::PANEL,
                    layout: Layout::default(),
                    children: vec![tabs],
                    tags: Tags::new(),
                };
            }
        }
        ScreenDef {
            kind: self.kind.clone(),
            inherits: Some(crate::kinds::settings()),
            root,
            listring: Vec::new(),
            remove: Vec::new(),
            initial_focus: Some(Self::TABS_ID.to_owned()),
            // The default keeps the frame's (modal, scrim, fade, pop).
            presentation: Presentation::default(),
        }
    }

    /// Every declared key with its default: the value rows' keys. The tab
    /// key is UI state, not a setting: seeded, ruled, never saved or reset.
    pub fn defaults(&self) -> BTreeMap<String, Value> {
        self.rows()
            .filter_map(|row| Some((row.key()?.to_owned(), row.default_value()?)))
            .collect()
    }

    /// The rules the rows imply (a slider's range and step, a select's
    /// options), plus the tab key's option list.
    pub fn rules(&self) -> ValueRules {
        let mut rules = ValueRules::default();
        for row in self.rows() {
            if let (Some(key), Some(rule)) = (row.key(), row.rule()) {
                rules.insert(key, rule);
            }
        }
        if !self.tabs.is_empty() {
            rules.insert(
                self.tab_key(),
                ValueRule {
                    options: self.tabs.iter().map(|t| t.id.clone()).collect(),
                    ..ValueRule::default()
                },
            );
        }
        rules
    }

    /// Every row of every tab, in order.
    pub fn rows(&self) -> impl Iterator<Item = &SettingsRow> {
        self.tabs.iter().flat_map(|tab| tab.rows.iter())
    }

    /// The tab that carries `id`.
    pub fn tab_by_id(&self, id: &str) -> Option<&SettingsTab> {
        self.tabs.iter().find(|tab| tab.id == id)
    }

    /// Every declared store key, sorted.
    pub fn keys(&self) -> Vec<String> {
        self.defaults().into_keys().collect()
    }
}

/// The game's settings. Insert it and [`crate::MenuPlugin`] does the rest.
#[derive(Resource, Debug, Clone, PartialEq)]
pub struct Settings {
    /// The spec.
    pub spec: SettingsSpec,
}

/// What persists.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SavedSettings {
    /// Declared keys and their values.
    #[serde(default)]
    pub values: BTreeMap<String, Value>,
    /// The bindings, when the player changed any.
    #[serde(default)]
    pub bindings: Option<UiBindings>,
}

/// Where settings persist. Disk on native, memory the browser page reads
/// out, nothing in a test.
pub trait SettingsStore: Send + Sync + 'static {
    /// The saved settings, if any.
    fn load(&self) -> Option<SavedSettings>;
    /// Persists `settings`.
    fn save(&self, settings: &SavedSettings);
}

/// The active store. Absent = not persisted.
#[derive(Resource, Clone)]
pub struct SettingsStorage(pub Arc<dyn SettingsStore>);

impl SettingsStorage {
    /// Wraps a store.
    pub fn new(store: impl SettingsStore) -> Self {
        Self(Arc::new(store))
    }

    /// A RON file at `path`.
    pub fn file(path: impl Into<PathBuf>) -> Self {
        Self::new(FileSettings { path: path.into() })
    }
}

impl std::fmt::Debug for SettingsStorage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SettingsStorage(..)")
    }
}

/// A RON file. On wasm it loads nothing and saves nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileSettings {
    /// The file.
    pub path: PathBuf,
}

impl SettingsStore for FileSettings {
    #[cfg(not(target_arch = "wasm32"))]
    fn load(&self) -> Option<SavedSettings> {
        let text = match std::fs::read_to_string(&self.path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return None,
            Err(e) => {
                tracing::warn!(path = %self.path.display(), %e, "reading the settings file");
                return None;
            }
        };
        match ron::from_str::<SavedSettings>(&text) {
            Ok(loaded) => Some(loaded),
            Err(e) => {
                tracing::warn!(path = %self.path.display(), %e, "the settings file is not RON");
                None
            }
        }
    }

    #[cfg(target_arch = "wasm32")]
    fn load(&self) -> Option<SavedSettings> {
        None
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn save(&self, settings: &SavedSettings) {
        let text = match ron::ser::to_string_pretty(settings, ron::ser::PrettyConfig::default()) {
            Ok(text) => text,
            Err(e) => {
                tracing::warn!(%e, "serialising the settings");
                return;
            }
        };
        if let Some(dir) = self.path.parent()
            && !dir.as_os_str().is_empty()
            && let Err(e) = std::fs::create_dir_all(dir)
        {
            tracing::warn!(path = %dir.display(), %e, "creating the settings directory");
            return;
        }
        if let Err(e) = std::fs::write(&self.path, text) {
            tracing::warn!(path = %self.path.display(), %e, "writing the settings file");
        }
    }

    #[cfg(target_arch = "wasm32")]
    fn save(&self, _settings: &SavedSettings) {}
}

/// In memory: what a test asserts on and what a web page reads out as RON.
#[derive(Debug, Default, Clone)]
pub struct MemorySettings(pub Arc<Mutex<Option<SavedSettings>>>);

impl MemorySettings {
    /// Starts from `saved`.
    pub fn with(saved: SavedSettings) -> Self {
        Self(Arc::new(Mutex::new(Some(saved))))
    }

    /// The last save.
    pub fn get(&self) -> Option<SavedSettings> {
        self.0.lock().ok().and_then(|s| s.clone())
    }

    /// Replaces the saved value.
    pub fn set(&self, saved: Option<SavedSettings>) {
        if let Ok(mut slot) = self.0.lock() {
            *slot = saved;
        }
    }

    /// The last save as RON, for a host page to persist.
    pub fn to_ron(&self) -> Option<String> {
        self.get()
            .and_then(|s| ron::ser::to_string_pretty(&s, ron::ser::PrettyConfig::default()).ok())
    }

    /// From RON a host page kept.
    pub fn from_ron(text: &str) -> Result<Self, ron::error::SpannedError> {
        ron::from_str::<SavedSettings>(text).map(Self::with)
    }
}

impl SettingsStore for MemorySettings {
    fn load(&self) -> Option<SavedSettings> {
        self.get()
    }

    fn save(&self, settings: &SavedSettings) {
        self.set(Some(settings.clone()));
    }
}

/// The Reset button was pressed: every default goes back through the store
/// and the bindings reset.
#[derive(Message, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SettingsReset;

/// Marks a `Settings` resource the crate already applied.
#[derive(Resource, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct SettingsApplied;

/// `PostStartup` (and whenever a `Settings` resource appears): registers the
/// generated screen unless the kind is registered, loads the store, seeds
/// values (saved over a game's own seed over the defaults, declared keys
/// only), inserts the rules, restores the bindings (contract 4.2). Runs
/// once: [`SettingsApplied`] marks it done.
pub fn apply_settings(world: &mut World) {
    if world.get_resource::<SettingsApplied>().is_some() {
        return;
    }
    let Some(spec) = world.get_resource::<Settings>().map(|s| s.spec.clone()) else {
        return;
    };

    {
        let mut screens = world.resource_mut::<slotted_ui::Screens>();
        if screens.get(&spec.kind).is_none() {
            let def = match screens.get(&crate::kinds::settings()) {
                Some(frame) => spec.screen_def_over(frame),
                None => spec.screen_def(),
            };
            screens.register(def);
        }
    }

    let saved = world
        .get_resource::<SettingsStorage>()
        .and_then(|storage| storage.0.load());

    let mut store = world.resource_mut::<ValueStore>();
    let mut seed = spec.defaults();
    for (key, value) in &mut seed {
        if let Some(existing) = store.get(key) {
            *value = existing.clone();
        }
        if let Some(saved) = saved.as_ref().and_then(|s| s.values.get(key)) {
            *value = saved.clone();
        }
    }
    if let Some(first) = spec.tabs.first()
        && store.get(&spec.tab_key()).is_none()
    {
        seed.insert(spec.tab_key(), Value::Text(first.id.clone()));
    }
    store.restore(seed);

    let mut rules = world.resource_mut::<ValueRules>();
    for (key, rule) in spec.rules().0 {
        rules.insert(key, rule);
    }

    if let Some(bindings) = saved.and_then(|s| s.bindings) {
        *world.resource_mut::<UiBindings>() = bindings;
    }

    world.insert_resource(SettingsApplied);
}

/// What [`save_settings`] writes: the declared keys the store holds and the
/// bindings as they are.
pub fn saved_settings(
    spec: &SettingsSpec,
    store: &ValueStore,
    bindings: &UiBindings,
) -> SavedSettings {
    SavedSettings {
        values: spec
            .keys()
            .into_iter()
            .filter_map(|key| store.get(&key).cloned().map(|value| (key, value)))
            .collect(),
        bindings: Some(bindings.clone()),
    }
}

/// `Last`: saves when a declared value or a binding changed this frame, and
/// on exit (contract 4.3). Nothing before [`apply_settings`] ran, so an empty
/// store never overwrites a file.
#[allow(clippy::too_many_arguments)]
pub fn save_settings(
    settings: Option<Res<Settings>>,
    storage: Option<Res<SettingsStorage>>,
    applied: Option<Res<SettingsApplied>>,
    store: Res<ValueStore>,
    bindings: Res<UiBindings>,
    mut changed: MessageReader<ValueChanged>,
    mut rebound: MessageReader<slotted_ui::BindingChanged>,
    mut exit: MessageReader<AppExit>,
) {
    let (Some(settings), Some(storage), Some(_)) = (settings, storage, applied) else {
        changed.clear();
        rebound.clear();
        exit.clear();
        return;
    };
    let spec = &settings.spec;
    let value_changed = changed
        .read()
        .any(|c| spec.rows().any(|row| row.key() == Some(c.key.as_str())));
    let rebound = rebound.read().next().is_some();
    let exiting = exit.read().next().is_some();
    if !(value_changed || rebound || exiting) {
        return;
    }
    storage.0.save(&saved_settings(spec, &store, &bindings));
}

/// `Update`: the template's Reset button (`MenuChoice` id `reset`) sends
/// [`SettingsReset`].
pub fn reset_on_menu_choice(
    mut choices: MessageReader<MenuChoice>,
    mut resets: MessageWriter<SettingsReset>,
) {
    for choice in choices.read() {
        if choice.id == "reset" {
            resets.write(SettingsReset);
        }
    }
}

/// Applies [`SettingsReset`] (contract 4.3): every default back through
/// `SetValue`, so rules and guards still apply, and the bindings back to
/// [`UiBindings::default`].
pub fn reset_settings(
    mut resets: MessageReader<SettingsReset>,
    settings: Option<Res<Settings>>,
    mut writes: MessageWriter<SetValue>,
    mut bindings: ResMut<UiBindings>,
) {
    if resets.read().next().is_none() {
        return;
    }
    let Some(settings) = settings else {
        return;
    };
    for (key, value) in settings.spec.defaults() {
        writes.write(SetValue {
            key,
            value,
            source: None,
        });
    }
    *bindings = UiBindings::default();
}

/// `SlottedUiSet::Render`, the frame a capture lands: the action that was
/// just bound to a key (or a button) takes it away from every other action
/// on the same device, and a toast (`slotted.menu.binding_moved`, argument
/// `action`) says which one lost it (contract 4.4).
pub fn resolve_binding_conflicts(
    mut rebound: MessageReader<slotted_ui::BindingChanged>,
    mut bindings: ResMut<UiBindings>,
    mut commands: Commands,
) {
    for change in rebound.read() {
        let mut moved: Vec<UiAction> = Vec::new();
        match change.device {
            InputDevice::Gamepad => {
                let Some(button) = bindings.first_button(change.action) else {
                    continue;
                };
                for (action, buttons) in &mut bindings.buttons {
                    if *action != change.action && buttons.contains(&button) {
                        buttons.retain(|b| *b != button);
                        moved.push(*action);
                    }
                }
            }
            InputDevice::Keyboard | InputDevice::Pointer => {
                let Some(key) = bindings.first_key(change.action) else {
                    continue;
                };
                for (action, keys) in &mut bindings.keys {
                    if *action != change.action && keys.contains(&key) {
                        keys.retain(|k| *k != key);
                        moved.push(*action);
                    }
                }
            }
        }
        for action in moved {
            tracing::info!(
                %action,
                now = %change.action,
                device = %change.device,
                "a binding moved to another action"
            );
            let mut args = LocArgs::new();
            args.insert("action".to_owned(), Value::Text(action.as_str().to_owned()));
            toast(
                &mut commands,
                ToastSpec {
                    args,
                    ..ToastSpec::new("slotted.menu.binding_moved")
                },
            );
        }
    }
}

/// Registers the settings systems.
pub fn build(app: &mut App) {
    app.add_systems(
        Update,
        (
            apply_settings.run_if(
                |s: Option<Res<Settings>>, done: Option<Res<SettingsApplied>>| {
                    s.is_some() && done.is_none()
                },
            ),
            (reset_on_menu_choice, reset_settings).chain(),
            resolve_binding_conflicts.in_set(slotted_ui::SlottedUiSet::Render),
        ),
    )
    .add_systems(Last, save_settings);
}
