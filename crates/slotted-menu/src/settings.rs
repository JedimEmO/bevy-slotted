//! Settings (menus M2 contract 4): a spec the game writes, the screen it
//! generates over the `slotted:settings` frame, and persistence through a
//! port.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use slotted_ui::{
    InputDevice, LocKey, ScreenDef, ScreenKind, SelectOption, ToggleStyle, UiAction, UiBindings,
    UiNodeDef, Value, ValueRules,
};

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

    /// The screen: `inherits: slotted:settings`, one `tabs` node with id
    /// `settings.tabs`, one page per tab with the rows and the anchors.
    pub fn screen_def(&self) -> ScreenDef {
        // M2-IMPL: C
        ScreenDef {
            kind: self.kind.clone(),
            inherits: Some(crate::kinds::settings()),
            root: UiNodeDef::Panel {
                role: slotted_theme::roles::PANEL,
                layout: slotted_ui::Layout::default(),
                children: Vec::new(),
                tags: slotted_ui::Tags::new(),
            },
            listring: Vec::new(),
            remove: Vec::new(),
            initial_focus: Some("settings.tabs".to_owned()),
            presentation: slotted_ui::Presentation::default(),
        }
    }

    /// Every declared key with its default.
    pub fn defaults(&self) -> BTreeMap<String, Value> {
        // M2-IMPL: C
        BTreeMap::new()
    }

    /// The rules the rows imply.
    pub fn rules(&self) -> ValueRules {
        // M2-IMPL: C
        ValueRules::default()
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
        // M2-IMPL: C
        None
    }

    #[cfg(target_arch = "wasm32")]
    fn load(&self) -> Option<SavedSettings> {
        None
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn save(&self, settings: &SavedSettings) {
        // M2-IMPL: C
        let _ = settings;
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

/// `Startup` (and whenever a `Settings` resource appears): registers the
/// generated screen, loads the store, seeds values and rules, restores
/// bindings (contract 4.2).
pub fn apply_settings(world: &mut World) {
    // M2-IMPL: C
    let _ = world;
}

/// `Last`: saves when a value or binding changed this frame, and on exit.
pub fn save_settings(
    _settings: Option<Res<Settings>>,
    _storage: Option<Res<SettingsStorage>>,
    _store: Res<slotted_ui::ValueStore>,
    _bindings: Res<UiBindings>,
    _changed: MessageReader<slotted_ui::ValueChanged>,
    _rebound: MessageReader<slotted_ui::BindingChanged>,
    _exit: MessageReader<AppExit>,
) {
    // M2-IMPL: C
}

/// Applies [`SettingsReset`] (contract 4.3).
pub fn reset_settings(
    _resets: MessageReader<SettingsReset>,
    _settings: Option<Res<Settings>>,
    _writes: MessageWriter<slotted_ui::SetValue>,
    _bindings: ResMut<UiBindings>,
) {
    // M2-IMPL: C
}

/// Moves a key that another action on the same device already had and
/// toasts about it (contract 4.4).
pub fn resolve_binding_conflicts(
    _rebound: MessageReader<slotted_ui::BindingChanged>,
    _bindings: ResMut<UiBindings>,
    _commands: Commands,
) {
    // M2-IMPL: C
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
            reset_settings,
            resolve_binding_conflicts.in_set(slotted_ui::SlottedUiSet::Render),
        ),
    )
    .add_systems(Last, save_settings);
}
