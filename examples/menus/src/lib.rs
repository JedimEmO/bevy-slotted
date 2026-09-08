//! The menus demo, minus the window.
//!
//! What a game writes on top of `slotted-menu` to get a title screen, a
//! pause screen, settings, a confirm, a toast and a text page, over the
//! chest from `examples/chest`:
//!
//! 1. A `MenuConfig` naming the settings screen and the title
//!    ([`showcase::menus::menu_config`]).
//! 2. A `Settings { spec }` and, on native, a `SettingsStorage`
//!    ([`showcase::settings::SettingsDemoPlugin`] supplies the spec;
//!    `main.rs` the file store).
//! 3. A main menu of its own, `menus:main`, that inherits
//!    `slotted:main_menu` ([`MAIN_SCREEN`]) and an `Injection` that adds an
//!    About button at the template's `buttons_end` anchor
//!    ([`register_main_menu`]).
//! 4. Systems that answer the `MenuChoice`s the crate leaves to the game:
//!    `play`, `quit`, `about`, `talk` ([`route_choices`]), and the confirms
//!    they open ([`leave_or_exit`]).
//! 5. A conversation: `assets/dialogue/greeting.dialogue.ron` loaded through
//!    `DialogueAssets` ([`load_dialogue`]), started by the injected Talk
//!    button, whose `secret` option the settings screen's Demo tab unlocks,
//!    and a system that answers the `yes` choice with a toast
//!    ([`thank_on_yes`]).
//!
//! `src/main.rs` adds the 3D backdrop, a renderer and the screenshot flags;
//! `tests/flow.rs` drives the same plugin through `slotted_test::UiHarness`
//! by gamepad with no window at all.

use std::sync::Arc;

use bevy::prelude::*;
use slotted::ecs::MenuAction;
use slotted::menu::{
    ConfirmResult, ConfirmSpec, DialogueAssets, DialogueChoice, DialogueId, MenuChoice, PageSpec,
    ToastLevel, ToastSpec, confirm, kinds, open_page, start_dialogue, toast,
};
use slotted::prelude::*;
use slotted::ui::{AnchorId, Injection, Injections, LocKey, Owner, ScreenStack, Tags, UiNodeDef};
use slotted_model::{ClickAction, ToolbarAction};

pub use chest::{ChestDemoPlugin, assets_dir, load_registries};
pub use showcase::menus::{QUIT_CONFIRM, confirm_quit, exit_on_quit_confirmed, menu_config};
pub use showcase::settings::{FOUND_KEY, SETTINGS, SettingsDemoPlugin};

/// The example's main menu kind.
pub const MAIN: &str = "menus:main";

/// The main menu screen, compiled in: `slotted:main_menu` plus a footer
/// note. See the file for what inheriting a template means.
pub const MAIN_SCREEN: &str = include_str!("../screens/main.screen.ron");

/// The confirm id of "leave the game and go back to the title".
pub const LEAVE_CONFIRM: &str = "leave";

/// The dialogue the Talk button starts.
pub const GREETING: &str = "demo:greeting";

/// Where it lives, relative to the asset root.
pub const GREETING_PATH: &str = "dialogue/greeting.dialogue.ron";

/// The dialogue the Talk button starts.
pub fn greeting_id() -> DialogueId {
    DialogueId::new(GREETING)
}

/// The example's main menu kind.
pub fn main_kind() -> ScreenKind {
    ScreenKind::new(MAIN)
}

/// The compiled-in `menus:main` screen.
///
/// # Panics
///
/// If the embedded file is malformed, which the tests catch first.
pub fn main_screen() -> ScreenDef {
    ScreenDef::from_ron(MAIN_SCREEN).expect("screens/main.screen.ron parses")
}

/// The About and Talk buttons a mod would add: one `Injection` at the
/// template's `buttons_end` anchor, each button with a `menu` tag like the
/// template's own, so the crate turns its `Activate` into a `MenuChoice`
/// (`about`, `talk`).
///
/// An injected node sizes to its content, and two injections at one anchor
/// sit side by side in the anchor's row, so both buttons go in one
/// full-width column panel that takes the width of the buttons above it.
pub fn about_injection() -> Injection {
    let button = |id: &str, label: &str| UiNodeDef::Button {
        widget: None,
        opts: slotted::ui::ButtonOpts {
            label: Some(LocKey(label.to_owned())),
            ..Default::default()
        },
        tags: Tags::new().with("test_id", id).with("menu", id),
    };
    Injection {
        target: main_kind(),
        anchor: AnchorId::new("buttons_end"),
        node: UiNodeDef::Panel {
            role: slotted::theme::Role::new_static("invisible"),
            layout: slotted::ui::Layout {
                direction: slotted::ui::LayoutDirection::Column,
                gap: 1.0,
                width: Some(slotted::ui::Length::Percent(100.0)),
                align: Some(slotted::ui::def::Align::Stretch),
                ..Default::default()
            },
            tags: Tags::new().with("test_id", "about_row"),
            children: vec![
                button("about", "demo.menus.about"),
                button("talk", "demo.menus.talk"),
            ],
        },
        exclusion: false,
        owner: Owner::Game,
    }
}

/// `Startup`: registers `menus:main` (unless something did) and the About
/// and Talk injection (unless something targets the main menu already).
pub fn register_main_menu(mut screens: ResMut<Screens>, mut injections: ResMut<Injections>) {
    if screens.get(&main_kind()).is_none() {
        screens.register(main_screen());
    }
    if !injections.0.iter().any(|i| i.target == main_kind()) {
        injections.0.push(about_injection());
    }
}

/// `Startup`: the conversation, through the asset server. `MenuPlugin`'s
/// `apply_dialogue_assets` registers it in `Dialogues` when it lands, and
/// again on every edit while the game runs.
pub fn load_dialogue(mut dialogues: ResMut<DialogueAssets>, assets: Res<AssetServer>) {
    dialogues.load(&assets, GREETING_PATH);
}

/// Starts the greeting; a warning and nothing else until the asset has
/// loaded.
pub fn talk(commands: &mut Commands) {
    start_dialogue(commands, greeting_id());
}

/// `Update`: the game's answer to a `DialogueChoice`. `yes` earns a toast;
/// everything else the dialogue itself follows through its `next`.
pub fn thank_on_yes(mut choices: MessageReader<DialogueChoice>, mut commands: Commands) {
    for choice in choices.read() {
        if choice.dialogue == greeting_id() && choice.option == "yes" {
            toast(
                &mut commands,
                ToastSpec::new("demo.menus.thanks").level(ToastLevel::Info),
            );
        }
    }
}

/// Pushes the main menu, unless it is already on top.
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

/// Pushes the "leave the game?" confirm with a danger accept button.
pub fn confirm_leave(commands: &mut Commands) {
    confirm(
        commands,
        ConfirmSpec::new(LEAVE_CONFIRM, "demo.menus.leave.message")
            .title("demo.menus.leave.title")
            .buttons("demo.menus.leave.accept", "slotted.menu.cancel")
            .danger(),
    );
}

/// `Update`: the choices the crate leaves to the game. `play` on the main
/// menu pops it and opens the chest; `about` opens the About page; `talk`
/// starts the conversation over the title; `quit` on the main menu asks
/// before exiting and on the pause asks before going back to the title.
/// Everything else (`resume`, `settings`, `reset`, `accept`, `cancel`) the
/// crate handled before the choice arrived.
pub fn route_choices(
    mut choices: MessageReader<MenuChoice>,
    registries: Res<Registries>,
    cheat: Res<chest::CheatMode>,
    mut commands: Commands,
) {
    for choice in choices.read() {
        match choice.id.as_str() {
            "play" => {
                pop_screen(&mut commands);
                chest::open_chest(&mut commands, &registries, cheat.0);
            }
            "about" => open_page(
                &mut commands,
                PageSpec::new("demo.menus.about.title", "demo.menus.about.body"),
            ),
            "talk" => talk(&mut commands),
            "quit" if choice.screen == kinds::pause() => confirm_leave(&mut commands),
            "quit" => confirm_quit(&mut commands),
            _ => {}
        }
    }
}

/// `Update`: an accepted leave confirm pops the pause and shows the title
/// again; the chest keeps its contents for the next Play.
pub fn leave_or_exit(
    mut results: MessageReader<ConfirmResult>,
    mut binding: ResMut<chest::ChestBinding>,
    mut commands: Commands,
) {
    for result in results.read() {
        if result.id == LEAVE_CONFIRM && result.accepted {
            // The confirm popped itself; the pause is what is left.
            pop_screen(&mut commands);
            // `E` reopens the chest from this; on the title it should not.
            binding.def = None;
            open_main_menu(&mut commands);
        }
    }
}

/// Observer on the chest's rail: a quick stack shows a toast, which is the
/// smallest thing a game says without a screen.
pub fn toast_on_quick_stack(action: On<MenuAction>, mut commands: Commands) {
    if matches!(
        action.action,
        ClickAction::Toolbar(ToolbarAction::QuickStack { .. })
    ) {
        toast(
            &mut commands,
            ToastSpec::new("demo.menus.quick_stack").level(ToastLevel::Success),
        );
    }
}

/// Everything the example adds on top of `SlottedPlugins`, the chest demo
/// and the settings demo. No `SettingsStorage`: `main.rs` inserts the file
/// store, a test inserts a memory one.
#[derive(Debug, Clone, Copy, Default)]
pub struct MenusDemoPlugin;

impl Plugin for MenusDemoPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((ChestDemoPlugin, SettingsDemoPlugin))
            .insert_resource(menu_config("demo.menus.title", env!("CARGO_PKG_VERSION")))
            .init_resource::<Injections>()
            .add_systems(Startup, (register_main_menu, load_dialogue))
            .add_systems(
                Update,
                (
                    route_choices,
                    leave_or_exit,
                    exit_on_quit_confirmed,
                    thank_on_yes,
                )
                    .in_set(SlottedUiSet::Input),
            )
            .add_observer(toast_on_quick_stack);
    }
}

/// The settings file the windowed example persists to: under the system's
/// temporary directory, so running the example leaves nothing in the
/// checkout.
pub fn settings_path() -> std::path::PathBuf {
    std::env::temp_dir()
        .join("slotted-menus")
        .join("settings.ron")
}

/// The registries, shared with the chest.
pub fn registries() -> Arc<slotted_registry::FrozenRegistries> {
    load_registries()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_main_screen_parses_and_inherits_the_template() {
        let def = main_screen();
        assert_eq!(def.kind, main_kind());
        assert_eq!(def.inherits, Some(kinds::main_menu()));
        assert_eq!(def.initial_focus.as_deref(), Some("play"));
    }
}
