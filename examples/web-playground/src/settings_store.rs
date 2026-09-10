//! The playground's [`SettingsStore`] adapter.
//!
//! `docs/design/showcase-refresh-contract.md` section 4.2. The same
//! arrangement as [`crate::hud_store`], for the same reason: a browser tab
//! has no filesystem, `localStorage` is the page's, and the page is the
//! right owner of when to save and what key to save under. So the adapter
//! here is the bus. A save serialises [`SavedSettings`] to RON and publishes
//! it on the [`Bus`], where `settings_ron()` finds it and the page puts it in
//! `localStorage["slotted.settings"]`; a load parses whatever the bus holds,
//! which is what the page handed back through `restore_settings(ron)` before
//! the first frame.
//!
//! One copy, on the bus, rather than a value here mirrored to the bus: the
//! `wasm-bindgen` exports are free functions with no handle on the world,
//! and a store that kept its own value would have to publish it anyway.
//!
//! [`SettingsStore`]: slotted::menu::SettingsStore

use slotted::menu::{SavedSettings, SettingsStorage, SettingsStore};

use crate::bus::Bus;

/// The store: the bus's settings slot, read as RON and written as RON.
#[derive(Debug, Clone)]
pub struct PageSettings {
    bus: Bus,
}

impl PageSettings {
    /// A store over `bus`.
    pub fn new(bus: Bus) -> Self {
        Self { bus }
    }

    /// The store the running app uses, over [`Bus::global`].
    pub fn global() -> Self {
        Self::new(Bus::global())
    }

    /// This store as the resource `MenuPlugin` looks for.
    pub fn storage(self) -> SettingsStorage {
        SettingsStorage::new(self)
    }

    /// Replaces what is saved, or forgets it when `saved` is `None`.
    pub fn set(&self, saved: Option<&SavedSettings>) {
        match saved.map(to_ron) {
            Some(Ok(ron)) => self.bus.set_settings(ron),
            Some(Err(error)) => self.bus.log(
                "warn",
                "showcase",
                format!("the settings did not serialise: {error}"),
            ),
            None => self.bus.set_settings(""),
        }
    }

    /// The saved settings as RON, or an empty string when nothing was saved.
    pub fn ron(&self) -> String {
        self.bus.settings()
    }
}

/// `SavedSettings` as the RON the page keeps.
///
/// # Errors
///
/// A value that will not serialise, which for a map of keys to plain values
/// means something is very wrong.
pub fn to_ron(saved: &SavedSettings) -> Result<String, String> {
    ron::ser::to_string(saved).map_err(|e| e.to_string())
}

/// The RON the page kept, back as `SavedSettings`.
///
/// # Errors
///
/// Text that is not a `SavedSettings`.
pub fn from_ron(text: &str) -> Result<SavedSettings, String> {
    ron::from_str(text).map_err(|e| format!("the stored settings are not readable: {e}"))
}

impl SettingsStore for PageSettings {
    fn load(&self) -> Option<SavedSettings> {
        let text = self.bus.settings();
        if text.trim().is_empty() {
            return None;
        }
        match from_ron(&text) {
            Ok(saved) => Some(saved),
            Err(error) => {
                self.bus.log("warn", "showcase", error);
                None
            }
        }
    }

    fn save(&self, settings: &SavedSettings) {
        self.set(Some(settings));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use slotted::ui::Value;

    #[test]
    fn a_save_lands_on_the_bus_and_loads_back_out_of_it() {
        let bus = Bus::new();
        let store = PageSettings::new(bus.clone());
        assert!(store.load().is_none(), "nothing saved yet");
        assert_eq!(store.ron(), "");

        let mut saved = SavedSettings::default();
        saved
            .values
            .insert("settings.ui_scale".to_owned(), Value::Float(1.25));
        store.save(&saved);
        assert!(
            bus.settings().contains("settings.ui_scale"),
            "the save reached the bus: {}",
            bus.settings()
        );
        assert_eq!(store.load(), Some(saved));

        store.set(None);
        assert_eq!(bus.settings(), "");
        assert!(store.load().is_none());
    }

    #[test]
    fn unreadable_text_on_the_bus_loads_nothing_and_says_so() {
        let bus = Bus::new();
        bus.set_settings("not ron at all");
        let store = PageSettings::new(bus.clone());
        assert!(store.load().is_none());
        assert!(
            bus.drain_console()
                .iter()
                .any(|line| line.level == "warn" && line.text.contains("not readable")),
        );
    }
}
