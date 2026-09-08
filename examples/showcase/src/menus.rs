//! The menu plumbing the native examples share (menus M2, package D): the
//! `MenuConfig` that points the pause screen's Settings at the demo's
//! settings, and the quit flow: a `quit` menu choice opens a danger confirm,
//! and an accepted confirm exits the app.
//!
//! What a game writes on top of `slotted-menu` is exactly this much. The
//! crate turns the template buttons into [`MenuChoice`]s and handles
//! `resume`, `settings` and `back` itself; `quit` is the game's, because
//! only the game knows whether it means "leave" or "back to the title".

use bevy::prelude::*;
use slotted::menu::{ConfirmResult, ConfirmSpec, MenuChoice, MenuConfig, confirm};
use slotted::prelude::*;
use slotted::ui::LocKey;

/// The confirm id the quit flow answers to.
pub const QUIT_CONFIRM: &str = "quit";

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
