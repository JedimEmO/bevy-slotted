//! Scene 3: the title, the pause, settings that persist, a confirm and a
//! toast, and the game is a chest behind them.
//!
//! `docs/design/showcase-refresh-contract.md` section 4.2. Everything on the
//! canvas is `slotted-menu`'s: the title inherits `slotted:main_menu`
//! (`examples/showcase/screens/main.screen.ron`), the pause is the crate's
//! own, the settings screen is generated from `showcase::settings::spec`,
//! and the confirms, the toast and the About page are the crate's
//! templates. What the scene adds is what a game writes on top
//! ([`MenusDemoPlugin`]): the `MenuConfig`, the answers to the choices the
//! crate leaves to the game (`play`, `about`, `quit`), and the two confirms'
//! outcomes. Nothing here calls `AppExit`: a browser tab has nowhere to go,
//! so an accepted Quit on the title shows the title again with a toast that
//! says so.
//!
//! Persistence is the [`crate::settings_store::PageSettings`] store, put in
//! by `build_app`; this module's [`restore_settings`] is the world side of
//! `restore_settings(ron)`, the values back into the store after the page
//! hands them over.

use bevy::prelude::*;
use slotted::menu::settings::DefaultBindings;
use slotted::menu::toast::ToastQueue;
use slotted::menu::{
    ConfirmResult, MenuChoice, MenuConfig, PageSpec, SavedSettings, Settings, SettingsDirty,
    ToastLevel, ToastSpec, kinds, open_page, same_kind, toast,
};
use slotted::prelude::*;
use slotted::ui::{UiBindings, ValueStore, clear_screens, pop_to};

use crate::bus::Bus;
use crate::scenes;
use crate::showcase::{ActiveScene, MenuScreen, Scene, SceneHandler};

/// Scene 3.
pub struct MenusScene;

impl SceneHandler for MenusScene {
    fn enter(&self, world: &mut World) {
        // `Esc` with nothing open pauses, in this scene and no other: the
        // config is app-wide because `MenuPlugin` fills the title from it at
        // `PostStartup`, but a pause screen appearing over the HUD scene
        // would be a screen that scene never asked for.
        if let Some(mut config) = world.get_resource_mut::<MenuConfig>() {
            config.pause_on_menu = true;
        }
        // `E` reopens the last chest the binding saw, which on a title screen
        // is the previous scene's.
        forget_chest(world);
        scenes::register_screen(world, showcase::menus::main_screen());
        {
            let mut commands = world.commands();
            showcase::menus::open_main_menu(&mut commands);
        }
        world.flush();
    }

    fn leave(&self, world: &mut World) {
        if let Some(mut config) = world.get_resource_mut::<MenuConfig>() {
            config.pause_on_menu = false;
        }
        forget_chest(world);
        // Every menu entry on the stack is a `ScreenRoot`, and the teardown
        // closes them all; the toasts are not, and would otherwise finish
        // fading over the next scene.
        close_toasts(world);
        scenes::teardown(world);
    }
}

/// Everything the playground adds on top of `SlottedPlugins` for the Menus
/// and Dialogue scenes: the settings spec and locale, the `MenuConfig`, the
/// title screen and its About injection, the smith's conversation, and the
/// systems that answer what `slotted-menu` leaves to the game.
///
/// No `SettingsStorage`: `build_app` inserts the page store and a test
/// inserts its own, the same way the HUD layout store is handled.
#[derive(Debug, Clone, Copy, Default)]
pub struct MenusDemoPlugin;

impl Plugin for MenusDemoPlugin {
    fn build(&self, app: &mut App) {
        let mut config =
            showcase::menus::menu_config("demo.showcase.title", env!("CARGO_PKG_VERSION"));
        // Off until the Menus scene is entered; see `MenusScene::enter`.
        config.pause_on_menu = false;
        app.add_plugins(showcase::settings::SettingsDemoPlugin)
            .insert_resource(config)
            .add_systems(
                Startup,
                (
                    showcase::menus::register_main_menu,
                    showcase::dialogue::register_smith,
                ),
            )
            .add_systems(
                Update,
                (
                    route_choices,
                    leave_or_stay,
                    showcase::dialogue::thank_on_yes,
                    scenes::dialogue::log_transcript,
                    // After the emit, which clears the frame's claims first.
                    scenes::dialogue::shield_furnace_from_back
                        .after(slotted::ui::actions::UiActionEmit),
                )
                    .in_set(SlottedUiSet::Input),
            );
    }
}

/// `Update`: the choices the crate leaves to the game. `play` on the title
/// pops it and opens the chest; `about` opens the About page; `quit` on the
/// pause asks before going back to the title and on the title asks before
/// staying put. Everything else (`resume`, `settings`, `reset`, `accept`,
/// `cancel`) the crate handled before the choice arrived.
pub fn route_choices(mut choices: MessageReader<MenuChoice>, mut commands: Commands) {
    for choice in choices.read() {
        match choice.id.as_str() {
            "play" => {
                pop_screen(&mut commands);
                commands.queue(|world: &mut World| {
                    // The chest Esc closed a moment ago left its inventories
                    // behind; a fresh open spawns its own.
                    scenes::despawn_orphan_inventories(world);
                    scenes::chest::open(world);
                });
            }
            "about" => open_page(
                &mut commands,
                PageSpec::new("demo.showcase.about.title", "demo.showcase.about.body"),
            ),
            "quit" if choice.screen == kinds::pause() => {
                showcase::menus::confirm_leave(&mut commands);
            }
            "quit" => showcase::menus::confirm_quit(&mut commands),
            _ => {}
        }
    }
}

/// `Update`: an accepted leave confirm pops the pause and shows the title
/// again, the chest keeping its contents for the next Play. An accepted
/// quit confirm shows the title with a toast: nothing calls `AppExit`,
/// because a browser tab has nowhere to go.
pub fn leave_or_stay(mut results: MessageReader<ConfirmResult>, mut commands: Commands) {
    for result in results.read() {
        if !result.accepted {
            continue;
        }
        match result.id.as_str() {
            showcase::menus::LEAVE_CONFIRM => {
                // The confirm popped itself. What is left is the pause and,
                // when the page's Pause button made it, the chest under it;
                // leaving means all of it goes, or the next Play would stack
                // a second chest over the first.
                back_to_title(&mut commands);
            }
            showcase::menus::QUIT_CONFIRM => {
                // The confirm popped itself; the title is what is left.
                toast(
                    &mut commands,
                    ToastSpec::new("demo.showcase.no_exit").level(ToastLevel::Info),
                );
            }
            _ => {}
        }
    }
}

/// Opens one of the scene's screens: the page's Title, Pause and Settings
/// buttons.
///
/// Pause and Settings push over whatever is open, unless it is already the
/// top. Title is the bottom of this scene's stack, so it pops back to the
/// title when one is open underneath, and closes the game's chest and
/// pushes a fresh title when none is: a title over a pause over a chest is
/// not a state the keyboard can reach, and the page's button should not
/// reach it either.
///
/// # Errors
///
/// The Menus scene is not the one open. The page keeps the last scene in
/// its URL and its controls follow the rail, so this is a line on the
/// console rather than a screen appearing over another scene.
pub fn open(world: &mut World, which: MenuScreen) -> Result<(), String> {
    if world.resource::<ActiveScene>().0 != Scene::Menus {
        return Err(format!(
            "no menus: the Menus scene is not open, so `{}` went nowhere",
            which.id()
        ));
    }
    {
        let mut commands = world.commands();
        match which {
            MenuScreen::Title => back_to_title(&mut commands),
            MenuScreen::Pause => push_unless_on_top(&mut commands, kinds::pause()),
            MenuScreen::Settings => showcase::settings::open_settings(&mut commands),
        }
    }
    world.flush();
    Ok(())
}

/// Pops everything above the title, or everything and then pushes a title.
///
/// `pop_to` and `clear_screens` rather than a counted run of `pop_screen`:
/// `pop_screen` skips overlays and a count from `kinds()` includes them, so
/// the two disagree the moment a toast-like overlay sits on the stack.
fn back_to_title(commands: &mut Commands) {
    commands.queue(|world: &mut World| {
        let title = showcase::menus::main_kind();
        let has_title = world.resource::<ScreenStack>().kinds().contains(&title);
        let mut commands = world.commands();
        if has_title {
            pop_to(&mut commands, &title);
        } else {
            clear_screens(&mut commands);
        }
        forget_chest_later(&mut commands);
        showcase::menus::open_main_menu(&mut commands);
    });
}

/// [`forget_chest`], queued after the pops in front of it.
fn forget_chest_later(commands: &mut Commands) {
    commands.queue(forget_chest);
}

/// Pushes `kind` from the registry, unless it is already the top of the
/// stack.
fn push_unless_on_top(commands: &mut Commands, kind: ScreenKind) {
    commands.queue(move |world: &mut World| {
        if world
            .resource::<ScreenStack>()
            .top()
            .is_some_and(|top| top.kind == kind)
        {
            return;
        }
        let Some(def) = world.resource::<Screens>().get(&kind).cloned() else {
            world.resource::<Bus>().log(
                "error",
                "showcase",
                format!("{} is not registered", kind.0),
            );
            return;
        };
        push_screen(&mut world.commands(), def, None);
    });
}

/// Puts a `settings_ron()` value back into the store and the live values,
/// or resets both when `text` is empty.
///
/// The declared keys only, each made to fit its rule, which is what
/// `MenuPlugin` does with a saved file at startup; a value that will not
/// fit keeps what the store holds. `ValueStore::restore` writes no
/// `ValueChanged`, so the store is not saved straight back, and the version
/// bump repaints every bound control on an open settings screen.
///
/// An empty `text` is the page's Reset button: the saved settings are
/// forgotten, every declared key goes back to its default and the bindings
/// to what the app started with, so the next `settings_ron()` comes back
/// empty and the page can clear its key. The same two meanings
/// `restore_hud_layout` gives the empty string, for the same reason.
///
/// # Errors
///
/// `text` is neither empty nor saved settings.
pub fn restore_settings(world: &mut World, text: &str) -> Result<(), String> {
    let Some(spec) = world.get_resource::<Settings>().map(|s| s.spec.clone()) else {
        return Err("no settings spec: nothing to restore into".to_owned());
    };
    let bus = world.resource::<Bus>().clone();
    if text.trim().is_empty() {
        bus.set_settings("");
        // A save still pending from the frame before would write the
        // defaults straight back over the empty slot.
        if let Some(mut dirty) = world.get_resource_mut::<SettingsDirty>() {
            dirty.0 = false;
        }
        world.resource_mut::<ValueStore>().restore(spec.defaults());
        if let Some(defaults) = world.get_resource::<DefaultBindings>().cloned() {
            *world.resource_mut::<UiBindings>() = defaults.0;
        }
        bus.log(
            "info",
            "showcase",
            "the settings are back to their defaults and nothing is saved",
        );
        return Ok(());
    }
    let saved: SavedSettings = crate::settings_store::from_ron(text)?;
    bus.set_settings(text);
    let rules = spec.rules();
    let mut fitted = std::collections::BTreeMap::new();
    let mut refused = Vec::new();
    for key in spec.keys() {
        let Some(value) = saved.values.get(&key) else {
            continue;
        };
        let conformed = match rules.get(&key) {
            Some(rule) => rule.conform(value),
            None => Ok(value.clone()),
        };
        // The same guard `MenuPlugin`'s own seeding applies: a text rule
        // with no options conforms any text, so a `Text` saved under a
        // slider key has to be refused by kind.
        match (conformed, spec.defaults().get(&key)) {
            (Ok(fit), Some(default)) if same_kind(&fit, default) => {
                fitted.insert(key, fit);
            }
            _ => refused.push(key),
        }
    }
    let restored = fitted.len();
    world.resource_mut::<ValueStore>().restore(fitted);
    if let Some(bindings) = saved.bindings {
        *world.resource_mut::<UiBindings>() = bindings;
    }
    bus.log(
        "info",
        "showcase",
        if refused.is_empty() {
            format!("restored {restored} saved setting(s)")
        } else {
            format!(
                "restored {restored} saved setting(s); {} no longer fit and kept their value",
                refused.join(", ")
            )
        },
    );
    Ok(())
}

/// Forgets the chest the `E` key would reopen. The inventories the binding
/// remembers belong to whichever scene opened them last, and a title screen
/// has no chest behind it until Play.
fn forget_chest(world: &mut World) {
    if let Some(mut binding) = world.get_resource_mut::<showcase::chest::ChestBinding>() {
        binding.def = None;
        binding.open = None;
    }
}

/// Despawns every toast and forgets the ones waiting for room.
pub(crate) fn close_toasts(world: &mut World) {
    let toasts: Vec<Entity> = world
        .query_filtered::<Entity, With<slotted::menu::Toast>>()
        .iter(world)
        .collect();
    for entity in toasts {
        if let Ok(entity) = world.get_entity_mut(entity) {
            entity.despawn();
        }
    }
    if let Some(mut queue) = world.get_resource_mut::<ToastQueue>() {
        queue.0.clear();
    }
}
