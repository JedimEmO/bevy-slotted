//! The menu plumbing the native examples and the web playground share
//! (menus M2, package D; the showcase refresh, section 4.2): the
//! `MenuConfig` that points the pause screen's Settings at the demo's
//! settings, the quit flow, and the showcase's own title screen with its
//! injected About button.
//!
//! What a game writes on top of `slotted-menu` is exactly this much. The
//! crate turns the template buttons into [`MenuChoice`]s and handles
//! `resume`, `settings` and `back` itself; `quit` is the game's, because
//! only the game knows whether it means "leave" or "back to the title".
//! The two confirms here are the two answers: [`confirm_quit`] on a title
//! and [`confirm_leave`] on a pause. What an accepted confirm does is the
//! example's: `examples/menus` and `examples/chest` exit the app
//! ([`exit_on_quit_confirmed`]), the web playground shows the title again,
//! because a browser tab has nowhere to go.

use bevy::prelude::*;
use slotted::menu::{ConfirmResult, ConfirmSpec, MenuChoice, MenuConfig, confirm};
use slotted::prelude::*;
use slotted::ui::{
    AnchorId, ButtonOpts, Injection, Injections, Layout, LayoutDirection, Length, LocKey, Owner,
    Tags, UiNodeDef, def::Align,
};

/// The confirm id the quit flow answers to.
pub const QUIT_CONFIRM: &str = "quit";

/// The confirm id of "leave the game and go back to the title".
pub const LEAVE_CONFIRM: &str = "leave";

/// The showcase's main menu kind.
pub const MAIN: &str = "showcase:main";

/// The `MenuConfig` every example uses: the crate's pause screen, the demo's
/// settings screen (`demo:settings`), and the example's own title and
/// version on the main menu.
pub fn menu_config(title: &str, version: &str) -> MenuConfig {
    MenuConfig {
        settings_kind: ScreenKind::new(crate::settings::SETTINGS),
        title: LocKey(title.to_owned()),
        version: version.to_owned(),
        ..default()
    }
}

/// Pushes the "Quit the game?" confirm with a danger accept button. The
/// answer comes back as a [`ConfirmResult`] with id [`QUIT_CONFIRM`].
pub fn confirm_quit(commands: &mut Commands) {
    confirm(
        commands,
        ConfirmSpec::new(QUIT_CONFIRM, "demo.quit.message")
            .title("demo.quit.title")
            .buttons("demo.quit.accept", "slotted.menu.cancel")
            .danger(),
    );
}

/// Pushes the "leave the game?" confirm with a danger accept button. The
/// answer comes back as a [`ConfirmResult`] with id [`LEAVE_CONFIRM`].
pub fn confirm_leave(commands: &mut Commands) {
    confirm(
        commands,
        ConfirmSpec::new(LEAVE_CONFIRM, "demo.showcase.leave.message")
            .title("demo.showcase.leave.title")
            .buttons("demo.showcase.leave.accept", "slotted.menu.cancel")
            .danger(),
    );
}

/// `Update`: a `quit` choice on any template opens the quit confirm.
pub fn quit_on_menu_choice(mut choices: MessageReader<MenuChoice>, mut commands: Commands) {
    if choices.read().any(|choice| choice.id == "quit") {
        confirm_quit(&mut commands);
    }
}

/// `Update`: an accepted quit confirm exits the app.
pub fn exit_on_quit_confirmed(
    mut results: MessageReader<ConfirmResult>,
    mut exit: MessageWriter<AppExit>,
) {
    if results
        .read()
        .any(|result| result.id == QUIT_CONFIRM && result.accepted)
    {
        exit.write(AppExit::Success);
    }
}

/// The quit flow: `quit` confirms, an accepted confirm exits.
#[derive(Debug, Clone, Copy, Default)]
pub struct QuitPlugin;

impl Plugin for QuitPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (quit_on_menu_choice, exit_on_quit_confirmed).in_set(SlottedUiSet::Input),
        );
    }
}

// ---------------------------------------------------------------------------
// The showcase's title screen
// ---------------------------------------------------------------------------

/// The showcase's main menu kind.
pub fn main_kind() -> ScreenKind {
    ScreenKind::new(MAIN)
}

/// The compiled-in `showcase:main` screen: `slotted:main_menu` plus a
/// footer note. See `screens/main.screen.ron` for what inheriting a template
/// means.
pub fn main_screen() -> ScreenDef {
    crate::screens::parse("main", crate::screens::MAIN_SCREEN_RON)
}

/// The About button a mod would add: one `Injection` at the template's
/// `buttons_end` anchor, with a `menu` tag like the template's own, so the
/// crate turns its `Activate` into a `MenuChoice` (`about`).
///
/// An injected node sizes to its content, so the button goes in a
/// full-width column panel that takes the width of the buttons above it,
/// the same shape `examples/menus` uses for its two.
pub fn about_injection() -> Injection {
    Injection {
        target: main_kind(),
        anchor: AnchorId::new("buttons_end"),
        node: UiNodeDef::Panel {
            role: slotted::theme::Role::new_static("invisible"),
            layout: Layout {
                direction: LayoutDirection::Column,
                gap: 1.0,
                width: Some(Length::Percent(100.0)),
                align: Some(Align::Stretch),
                ..Default::default()
            },
            tags: Tags::new().with("test_id", "about_row"),
            children: vec![UiNodeDef::Button {
                widget: None,
                opts: ButtonOpts {
                    label: Some(LocKey("demo.showcase.about".to_owned())),
                    ..Default::default()
                },
                tags: Tags::new().with("test_id", "about").with("menu", "about"),
            }],
        },
        exclusion: false,
        owner: Owner::Game,
    }
}

/// `Startup`: registers `showcase:main` (unless something did) and the About
/// injection (unless something targets the main menu already).
pub fn register_main_menu(world: &mut World) {
    {
        let mut screens = world.resource_mut::<Screens>();
        if screens.get(&main_kind()).is_none() {
            screens.register(main_screen());
        }
    }
    let mut injections = world.get_resource_or_init::<Injections>();
    if !injections.0.iter().any(|i| i.target == main_kind()) {
        injections.0.push(about_injection());
    }
}

/// Pushes the showcase's main menu, unless it is already on top.
pub fn open_main_menu(commands: &mut Commands) {
    commands.queue(|world: &mut World| {
        let kind = main_kind();
        if world
            .resource::<ScreenStack>()
            .top()
            .is_some_and(|top| top.kind == kind)
        {
            return;
        }
        let Some(def) = world.resource::<Screens>().get(&kind).cloned() else {
            error!("{MAIN} is not registered");
            return;
        };
        push_screen(&mut world.commands(), def, None);
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use slotted::menu::kinds;

    #[test]
    fn the_main_screen_parses_and_inherits_the_template() {
        let def = main_screen();
        assert_eq!(def.kind, main_kind());
        assert_eq!(def.inherits, Some(kinds::main_menu()));
        assert_eq!(def.initial_focus.as_deref(), Some("play"));
    }

    #[test]
    fn the_about_injection_targets_the_buttons_anchor_with_a_menu_tag() {
        let injection = about_injection();
        assert_eq!(injection.target, main_kind());
        assert_eq!(injection.anchor, AnchorId::new("buttons_end"));
        let UiNodeDef::Panel { children, .. } = &injection.node else {
            panic!("the injection is a column panel");
        };
        let UiNodeDef::Button { tags, .. } = &children[0] else {
            panic!("the panel holds the button");
        };
        assert_eq!(tags.get("menu"), Some("about"));
    }
}
