//! The plugin, its config and the menu-action routing (menus M2 contract
//! 3.2).

use bevy::prelude::*;
use slotted_ui::{
    LocArgs, LocKey, ScreenKind, ScreenRoot, ScreenStack, Screens, Tags, UiAction, UiActionClaims,
    UiActionEvent, Value, pop_screen, push_screen,
};

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
    /// `slotted.menu.version`. Empty removes the version node.
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

/// The tag a template button carries.
pub const MENU_TAG: &str = "menu";

/// The screen root above `entity`, or `entity` itself when it is one.
pub(crate) fn screen_root_of(
    entity: Entity,
    parents: &Query<&ChildOf>,
    roots: &Query<&ScreenRoot>,
) -> Option<Entity> {
    let mut current = entity;
    loop {
        if roots.contains(current) {
            return Some(current);
        }
        current = parents.get(current).ok()?.parent();
    }
}

/// Observer on `Activate`: a node with a `menu` tag under a stack screen
/// writes a [`MenuChoice`], after the built-in handling: `resume`, `close`
/// and `back` pop; `settings` pushes [`MenuConfig::settings_kind`];
/// `accept` and `cancel` on a confirm dialog answer it (contract 3.3).
#[allow(clippy::too_many_arguments)]
pub fn route_menu_actions(
    activate: On<bevy::ui_widgets::Activate>,
    tags: Query<&Tags>,
    parents: Query<&ChildOf>,
    roots: Query<&ScreenRoot>,
    pending: Query<&crate::confirm::PendingConfirm>,
    stack: Res<ScreenStack>,
    screens: Res<Screens>,
    config: Res<MenuConfig>,
    mut actions: MessageWriter<MenuChoice>,
    mut results: MessageWriter<crate::confirm::ConfirmResult>,
    mut commands: Commands,
) {
    let entity = activate.entity;
    let Some(id) = tags
        .get(entity)
        .ok()
        .and_then(|t| t.get(MENU_TAG))
        .map(str::to_owned)
    else {
        return;
    };
    let Some(root) = screen_root_of(entity, &parents, &roots) else {
        return;
    };
    let Some(entry) = stack.entry(root) else {
        return;
    };
    let screen = entry.kind.clone();
    match id.as_str() {
        "resume" | "close" | "back" => pop_screen(&mut commands),
        "settings" => {
            if let Some(def) = screens.get(&config.settings_kind) {
                push_screen(&mut commands, def.clone(), None);
            } else {
                tracing::warn!(
                    kind = %config.settings_kind.0,
                    "the settings menu action names a screen nobody registered"
                );
            }
        }
        "accept" | "cancel" => {
            if let Ok(pending) = pending.get(root) {
                results.write(crate::confirm::ConfirmResult {
                    id: pending.0.clone(),
                    accepted: id == "accept",
                });
                // Answered: the close observer must not answer again.
                commands
                    .entity(root)
                    .remove::<crate::confirm::PendingConfirm>();
                pop_screen(&mut commands);
            }
        }
        _ => {}
    }
    actions.write(MenuChoice { id, screen, entity });
}

/// `SlottedUiSet::Input`, after `UiActionEmit`: a fresh, unclaimed `Menu`
/// pushes the pause screen when no page or modal is open, and pops it when
/// it is on top. Both claim the action.
pub fn pause_on_menu(
    mut events: MessageReader<UiActionEvent>,
    mut claims: ResMut<UiActionClaims>,
    stack: Res<ScreenStack>,
    screens: Res<Screens>,
    config: Res<MenuConfig>,
    mut commands: Commands,
) {
    let fresh = events
        .read()
        .any(|e| e.action == UiAction::Menu && !e.repeat);
    if !fresh || claims.is_claimed(UiAction::Menu) {
        return;
    }
    match stack.top() {
        None if config.pause_on_menu => {
            let Some(def) = screens.get(&config.pause_kind) else {
                tracing::warn!(
                    kind = %config.pause_kind.0,
                    "Menu wants to pause but the pause screen is not registered"
                );
                return;
            };
            push_screen(&mut commands, def.clone(), None);
            claims.claim(UiAction::Menu);
        }
        Some(top) if top.kind == config.pause_kind => {
            pop_screen(&mut commands);
            claims.claim(UiAction::Menu);
        }
        _ => {}
    }
}

/// `PostStartup`, after the templates register: pushes the crate's fallback
/// strings and fills the main menu's `title` and `version` nodes from
/// [`MenuConfig`]. An empty version removes the version node.
pub fn install_strings(
    mut localization: ResMut<slotted_ui::Localization>,
    mut screens: ResMut<Screens>,
    config: Res<MenuConfig>,
) {
    localization.push_fallback(crate::strings::MenuStrings);
    let Some(mut def) = crate::templates::cloned(&screens, &crate::kinds::main_menu()) else {
        return;
    };
    let mut changed = def.set_text("title", config.title.clone(), LocArgs::new());
    if config.version.is_empty() {
        changed |= def.remove_node("version");
    } else {
        let mut args = LocArgs::new();
        args.insert("version".to_owned(), Value::Text(config.version.clone()));
        changed |= def.set_text("version", LocKey("slotted.menu.version".to_owned()), args);
    }
    if changed {
        screens.register(def);
    }
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
