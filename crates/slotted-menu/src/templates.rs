//! The embedded screen templates (menus M2 contract 3.1): parsed once from
//! `screens/*.screen.ron` and registered unless a game registered the kind
//! first.

use std::sync::Arc;

use bevy::prelude::*;
use slotted_ui::{ScreenDef, ScreenKind, Screens};

/// The kinds the crate ships.
pub mod kinds {
    use slotted_ui::ScreenKind;

    /// The main menu: a page.
    pub fn main_menu() -> ScreenKind {
        ScreenKind::new("slotted:main_menu")
    }
    /// The pause screen: a modal.
    pub fn pause() -> ScreenKind {
        ScreenKind::new("slotted:pause")
    }
    /// The settings frame a `SettingsSpec` screen inherits from.
    pub fn settings() -> ScreenKind {
        ScreenKind::new("slotted:settings")
    }
    /// The confirm dialog.
    pub fn confirm() -> ScreenKind {
        ScreenKind::new("slotted:confirm")
    }
    /// A scrolling text page.
    pub fn page() -> ScreenKind {
        ScreenKind::new("slotted:page")
    }
    /// The dialogue overlay (menus M3).
    pub fn dialogue() -> ScreenKind {
        ScreenKind::new("slotted:dialogue")
    }

    /// Every template kind.
    pub fn all() -> [ScreenKind; 6] {
        [
            main_menu(),
            pause(),
            settings(),
            confirm(),
            page(),
            dialogue(),
        ]
    }
}

/// The template sources, compiled in.
pub const MAIN_MENU: &str = include_str!("../screens/main_menu.screen.ron");
/// See [`MAIN_MENU`].
pub const PAUSE: &str = include_str!("../screens/pause.screen.ron");
/// See [`MAIN_MENU`].
pub const SETTINGS: &str = include_str!("../screens/settings.screen.ron");
/// See [`MAIN_MENU`].
pub const CONFIRM: &str = include_str!("../screens/confirm.screen.ron");
/// See [`MAIN_MENU`].
pub const PAGE: &str = include_str!("../screens/page.screen.ron");
/// See [`MAIN_MENU`]. Menus M3.
pub const DIALOGUE: &str = include_str!("../screens/dialogue.screen.ron");
/// The one-toast snippet (contract 3.4): a `UiNodeDef`, not a screen.
pub const TOAST_NODE: &str = include_str!("../screens/toast.node.ron");

/// Every template, parsed. Panics on a malformed embedded file, which the
/// crate's own tests catch before a consumer ever does.
pub fn all() -> Vec<ScreenDef> {
    [MAIN_MENU, PAUSE, SETTINGS, CONFIRM, PAGE, DIALOGUE]
        .into_iter()
        .map(|text| ScreenDef::from_ron(text).expect("an embedded template parses"))
        .collect()
}

/// `Startup`: registers every template whose kind nobody registered first.
pub fn register_templates(mut screens: ResMut<Screens>) {
    for def in all() {
        if screens.get(&def.kind).is_none() {
            screens.register(def);
        }
    }
}

/// A registered screen by kind, cloned for rewriting (confirm, page).
pub fn cloned(screens: &Screens, kind: &ScreenKind) -> Option<ScreenDef> {
    screens
        .get(kind)
        .map(|def: &Arc<ScreenDef>| (**def).clone())
}
