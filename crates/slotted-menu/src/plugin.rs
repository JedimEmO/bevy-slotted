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
    config: Option<Res<MenuConfig>>,
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
    let Some(root) = slotted_ui::screen_root_where(entity, &parents, |e| roots.contains(e)) else {
        return;
    };
    let Some(entry) = stack.entry(root) else {
        return;
    };
    let screen = entry.kind.clone();
    match id.as_str() {
        "resume" | "close" | "back" => pop_screen(&mut commands),
        // Without a `MenuConfig` the game has not opted into the flow: the
        // choice is still written, and the game decides what settings are.
        "settings" => {
            if let Some(config) = config.as_deref() {
                if let Some(def) = screens.get(&config.settings_kind) {
                    push_screen(&mut commands, def.clone(), None);
                } else {
                    tracing::warn!(
                        kind = %config.settings_kind.0,
                        "the settings menu action names a screen nobody registered"
                    );
                }
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

/// `SlottedUiSet::Navigate`, before `pop_on_back`: a fresh, unclaimed
/// `Menu` pushes the pause screen when no page or modal is open, and pops it
/// when it is on top. Both claim the action. Nothing happens without a
/// [`MenuConfig`]: a game that never inserted one has not opted in.
///
/// Escape is both `Back` and `Menu` by default, so the rule is: when the
/// same frame carries a `Back`, `Menu` yields whenever `Back` already has an
/// owner. That owner is the stack (`pop_on_back`, when a page or modal is on
/// top) or any consumer that claimed `Back` in the `Input` set (the HUD
/// editor cancelling a drag, a select popup, a key capture). Only a pure
/// `Menu` press, Start on a pad, reaches the pause from the pause.
pub fn pause_on_menu(
    mut events: MessageReader<UiActionEvent>,
    mut claims: ResMut<UiActionClaims>,
    stack: Res<ScreenStack>,
    screens: Res<Screens>,
    config: Option<Res<MenuConfig>>,
    mut commands: Commands,
) {
    let mut fresh = false;
    let mut back = false;
    for event in events.read() {
        fresh |= event.action == UiAction::Menu && !event.repeat;
        back |= event.action == UiAction::Back;
    }
    let Some(config) = config else {
        return;
    };
    if !fresh || claims.is_claimed(UiAction::Menu) {
        return;
    }
    let top = stack.top();
    if back && (claims.is_claimed(UiAction::Back) || top.is_some()) {
        return;
    }
    match top {
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
            claims.claim(UiAction::Back);
        }
        Some(top) if top.kind == config.pause_kind => {
            pop_screen(&mut commands);
            claims.claim(UiAction::Menu);
        }
        _ => {}
    }
}

/// `PostStartup`, after the templates register: pushes the crate's fallback
/// strings and, when a [`MenuConfig`] exists, fills the main menu's `title`
/// and `version` nodes from it. Only the crate's own nodes are touched: a
/// game that registered its own `slotted:main_menu` keeps its title (the
/// `title` node's key is no longer `slotted.menu.title`). An empty version
/// removes the crate's version node.
pub fn install_strings(
    mut localization: ResMut<slotted_ui::Localization>,
    mut screens: ResMut<Screens>,
    config: Option<Res<MenuConfig>>,
) {
    localization.push_fallback(crate::strings::MenuStrings);
    let Some(config) = config else {
        return;
    };
    let Some(mut def) = crate::templates::cloned(&screens, &crate::kinds::main_menu()) else {
        return;
    };
    let key_of = |def: &slotted_ui::ScreenDef, id: &str| -> Option<String> {
        match def.root.find(id) {
            Some(slotted_ui::UiNodeDef::Text { key, .. }) => Some(key.0.clone()),
            _ => None,
        }
    };
    let mut changed = false;
    if key_of(&def, "title").as_deref() == Some("slotted.menu.title") {
        changed |= def.set_text("title", config.title.clone(), LocArgs::new());
    }
    if key_of(&def, "version").as_deref() == Some("slotted.menu.version") {
        if config.version.is_empty() {
            changed |= def.remove_node("version");
        } else {
            let mut args = LocArgs::new();
            args.insert("version".to_owned(), Value::Text(config.version.clone()));
            changed |= def.set_text("version", LocKey("slotted.menu.version".to_owned()), args);
        }
    }
    if changed {
        screens.register(def);
    }
}

impl Plugin for MenuPlugin {
    fn build(&self, app: &mut App) {
        // No `MenuConfig` by default: pausing and settings routing are a
        // game's opt-in, so a consumer that only wants the templates or the
        // toasts does not grow a pause screen on Escape.
        app.init_resource::<crate::toast::Toasts>()
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
                    .before(slotted_ui::pop_on_back)
                    .in_set(slotted_ui::SlottedUiSet::Navigate),
            );
        crate::hint_bar::build(app);
        crate::toast::build(app);
        crate::confirm::build(app);
        crate::settings::build(app);
    }
}
