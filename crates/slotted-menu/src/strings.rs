//! English fallbacks for every `slotted.menu.*` key the templates use
//! (menus M2 contract 0). A game or a mod overrides a key by shipping it in
//! its own catalogue; the fallback only answers when nothing else does.

use slotted_ui::{LocArgs, LocKey, Localizer};

/// The fallback [`Localizer`]: `slotted.menu.*` keys in English.
#[derive(Debug, Default, Clone, Copy)]
pub struct MenuStrings;

/// The English for `key`, if it is one of ours.
pub fn english(key: &str) -> Option<&'static str> {
    Some(match key {
        "slotted.menu.play" => "Play",
        "slotted.menu.settings" | "slotted.menu.settings_title" => "Settings",
        "slotted.menu.quit" => "Quit",
        "slotted.menu.resume" => "Resume",
        "slotted.menu.reset" => "Reset",
        "slotted.menu.done" => "Done",
        "slotted.menu.cancel" => "Cancel",
        "slotted.menu.ok" => "OK",
        "slotted.menu.close" => "Close",
        "slotted.menu.back" => "Back",
        "slotted.menu.select" => "Select",
        "slotted.menu.toggle" => "Toggle",
        "slotted.menu.edit" => "Edit",
        "slotted.menu.adjust" => "Adjust",
        "slotted.menu.change" => "Change",
        "slotted.menu.rebind" => "Rebind",
        "slotted.menu.pick_up" => "Pick up",
        "slotted.menu.next_tab" => "Next tab",
        "slotted.menu.prev_tab" => "Previous tab",
        "slotted.menu.paused" => "Paused",
        "slotted.menu.confirm_title" => "Are you sure?",
        "slotted.menu.version" => "Version {version}",
        "slotted.menu.binding_moved" => "{action} lost its key",
        _ => return None,
    })
}

impl Localizer for MenuStrings {
    fn resolve(&self, key: &LocKey, args: &LocArgs) -> Option<String> {
        // M2-IMPL: B — substitute `{name}` arguments in the English.
        let _ = args;
        english(&key.0).map(str::to_owned)
    }
}
