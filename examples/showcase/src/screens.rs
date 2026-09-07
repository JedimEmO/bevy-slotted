//! The screen files the showcase opens, compiled in.
//!
//! A browser tab has no filesystem, and the screens the showcase opens from
//! Rust (`demo:chest`, `machine:furnace` and `demo:settings`) are checked-in
//! RON beside the examples that own them. `include_str!` is the whole mechanism: the file
//! on disk stays the one copy, a native example still loads it through the
//! `AssetServer` so it hot-reloads, and the page gets the same bytes with no
//! fetch to wait for.
//!
//! Parsing is synchronous, which is what makes a scene switch a single frame:
//! [`crate::chest::screen`] hands `spawn_screen` a `ScreenDef` in the same
//! `enter()` that spawns the menu, rather than polling an asset handle.

use slotted::ui::ScreenDef;

/// `assets/screens/demo_chest.screen.ron`, the chest and browser scenes.
pub const CHEST_SCREEN_RON: &str = include_str!("../../../assets/screens/demo_chest.screen.ron");

/// `examples/machine/screens/furnace.screen.ron`, the machine scene.
pub const FURNACE_SCREEN_RON: &str = include_str!("../../machine/screens/furnace.screen.ron");

/// `assets/screens/demo_settings.screen.ron`, the settings demo.
pub const SETTINGS_SCREEN_RON: &str =
    include_str!("../../../assets/screens/demo_settings.screen.ron");

/// Parses one of the constants above.
///
/// # Panics
///
/// When the RON does not parse, which means the checked-in screen file is
/// broken; every caller here passes a `const` from this module.
pub fn parse(what: &str, text: &str) -> ScreenDef {
    ScreenDef::from_ron(text).unwrap_or_else(|e| panic!("parsing the {what} screen: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_bundled_screen_parses_and_names_its_kind() {
        assert_eq!(
            parse("chest", CHEST_SCREEN_RON).kind.0.to_string(),
            "demo:chest"
        );
        assert_eq!(
            parse("furnace", FURNACE_SCREEN_RON).kind.0.to_string(),
            "machine:furnace"
        );
        assert_eq!(
            parse("settings", SETTINGS_SCREEN_RON).kind.0.to_string(),
            "demo:settings"
        );
    }
}
