//! The plugin, its config and the menu-action routing (menus M2 contract
//! 3.2).

use bevy::prelude::*;
use slotted_ui::{LocKey, ScreenKind};

/// Registers the templates, the English fallbacks, the hint bar kind and
/// every system of the crate.
#[derive(Debug, Default, Clone, Copy)]
pub struct MenuPlugin;

/// What the crate needs to know about the game's menus.
#[derive(Resource, Debug, Clone, PartialEq, Eq)]
pub struct MenuConfig {
    /// The screen `Menu` pushes when nothing is open.
    pub pause_kind: ScreenKind,
    /// The screen the `settings` menu action pushes.
    pub settings_kind: ScreenKind,
    /// Whether `Menu` with no page or modal open pushes `pause_kind`.
    pub pause_on_menu: bool,
    /// The main menu's title.
    pub title: LocKey,
    /// The main menu's version line, as the `{version}` argument of
    /// `slotted.menu.version`.
    pub version: String,
}

impl Default for MenuConfig {
    fn default() -> Self {
        Self {
            pause_kind: crate::kinds::pause(),
            settings_kind: crate::kinds::settings(),
            pause_on_menu: true,
            title: LocKey("slotted.menu.title".to_owned()),
            version: String::new(),
        }
    }
}

/// A template button was activated: its `menu` tag, the screen it sits on
/// and the button itself. The crate handles `resume`, `close`, `back`,
/// `settings`, `accept`, `cancel` and `reset`; everything else is the game's.
#[derive(Message, Debug, Clone, PartialEq, Eq)]
pub struct MenuChoice {
    /// The `menu` tag.
    pub id: String,
    /// The screen kind the button sits on.
    pub screen: ScreenKind,
    /// The button.
    pub entity: Entity,
}

/// Observer on `Activate`: a node with a `menu` tag under a stack screen
/// writes a [`MenuChoice`], after the built-in handling.
pub fn route_menu_actions(
    _activate: On<bevy::ui_widgets::Activate>,
    _tags: Query<&slotted_ui::Tags>,
    _parents: Query<&ChildOf>,
    _roots: Query<&slotted_ui::ScreenRoot>,
    _config: Res<MenuConfig>,
    _actions: MessageWriter<MenuChoice>,
    _commands: Commands,
) {
    // M2-IMPL: B
}

/// `SlottedUiSet::Input`, after `UiActionEmit`: `Menu` pushes the pause
/// screen when nothing is open, and pops it when it is on top.
pub fn pause_on_menu(
    _events: MessageReader<slotted_ui::UiActionEvent>,
    _claims: ResMut<slotted_ui::UiActionClaims>,
    _stack: Res<slotted_ui::ScreenStack>,
    _screens: Res<slotted_ui::Screens>,
    _config: Res<MenuConfig>,
    _commands: Commands,
) {
    // M2-IMPL: B
}

/// `Startup`, after the templates register: pushes the crate's fallback
/// strings and fills the main menu's title and version.
pub fn install_strings(
    _localization: ResMut<slotted_ui::Localization>,
    _screens: ResMut<slotted_ui::Screens>,
    _config: Res<MenuConfig>,
) {
    // M2-IMPL: B
}

impl Plugin for MenuPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MenuConfig>()
            .init_resource::<crate::toast::Toasts>()
            .add_message::<MenuChoice>()
            .add_message::<crate::confirm::ConfirmResult>()
            .add_message::<crate::settings::SettingsReset>()
            .add_observer(route_menu_actions)
            // `PostStartup`, not `Startup`: the browser validates its screen
            // handlers in `Startup` and skips the check while `Screens` is
            // empty, which is how a mod's handler survives until the mod's
            // own screens install. Templates arriving in `Startup` would end
            // that grace period a schedule too early.
            .add_systems(
                PostStartup,
                (
                    crate::templates::register_templates,
                    install_strings,
                    crate::settings::apply_settings,
                )
                    .chain(),
            )
            .add_systems(
                Update,
                pause_on_menu
                    .after(slotted_ui::UiActionEmit)
                    .in_set(slotted_ui::SlottedUiSet::Input),
            );
        crate::hint_bar::build(app);
        crate::toast::build(app);
        crate::confirm::build(app);
        crate::settings::build(app);
    }
}
