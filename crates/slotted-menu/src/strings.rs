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

/// `text` with every `{name}` that `args` names replaced by the value's
/// plain text (`true`, `3`, `0.5`, the string). A `{name}` nothing names
/// stays as written, so a `{key:accept}` in a rich string reaches the
/// markup parser untouched.
pub fn substitute(text: &str, args: &LocArgs) -> String {
    let mut out = text.to_owned();
    for (name, value) in args {
        let placeholder = format!("{{{name}}}");
        if out.contains(&placeholder) {
            out = out.replace(&placeholder, &slotted_ui::rich::value_text(value));
        }
    }
    out
}

impl Localizer for MenuStrings {
    fn resolve(&self, key: &LocKey, args: &LocArgs) -> Option<String> {
        english(&key.0).map(|text| substitute(text, args))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use slotted_ui::Value;

    #[test]
    fn arguments_are_substituted_plainly() {
        let mut args = LocArgs::new();
        args.insert("version".to_owned(), Value::Text("1.2.0".to_owned()));
        assert_eq!(
            MenuStrings.resolve(&LocKey("slotted.menu.version".to_owned()), &args),
            Some("Version 1.2.0".to_owned())
        );
        args.insert("action".to_owned(), Value::Int(3));
        assert_eq!(substitute("{action} of {none}", &args), "3 of {none}");
        assert_eq!(
            MenuStrings.resolve(&LocKey("game.title".to_owned()), &args),
            None
        );
    }
}
