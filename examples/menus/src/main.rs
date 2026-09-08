//! The slotted menus, in a window, over the chest example's scene.
//!
//! ```text
//! cargo run -p menus                                play with it
//! cargo run -p menus -- --main --shot shots/menus-main.png
//! cargo run -p menus -- --pause --shot shots/menus-pause.png
//! cargo run -p menus -- --confirm --shot shots/menus-confirm.png
//! cargo run -p menus -- --page --shot shots/menus-page.png
//! cargo run -p menus -- --toast --shot shots/menus-toast.png
//! cargo run -p menus -- --theme paper --main --shot shots/menus-main-paper.png
//! ```
//!
//! What to try once it is up: Play opens the chest; `Esc` closes it and
//! `Esc` again pauses; the pause's Settings opens the settings screen, whose
//! values persist under the system's temporary directory; Quit on the pause
//! asks before going back to the title, Quit on the title asks before
//! leaving; About is a text page a mod-style injection added to the
//! template; the chest's Quick stack shows a toast. Everything works on a
//! pad: Start pauses, the d-pad walks, South selects, East goes back.
//!
//! The scene and the screenshot plumbing are the chest example's. Everything
//! else comes out of `lib.rs`, which the headless tests use unchanged.

use std::path::PathBuf;

use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, save_to_disk};
use bevy::window::WindowResolution;
use menus::MenusDemoPlugin;
use showcase::backdrop::{BackdropPlugin as SceneBackdropPlugin, MainCamera};
use slotted::menu::{MenuConfig, SettingsStorage, ToastLevel, ToastSpec, toast};
use slotted::prelude::*;
use slotted::theme::blur::{BackdropPlugin, BackdropSource};

const WIDTH: f32 = 1600.0;
const HEIGHT: f32 = 900.0;
/// How long the shot mode lets the scene settle before capturing.
const SHOT_AT: f32 = 2.0;

/// What a `--shot` shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Shot {
    /// The main menu, focus on Play.
    #[default]
    Main,
    /// The pause screen over the scene.
    Pause,
    /// The leave confirm over the pause.
    Confirm,
    /// The About page.
    Page,
    /// Two toasts over the main menu.
    Toast,
}

/// Command line. Absent flags mean "interactive, forever".
#[derive(Resource, Debug, Clone, Default)]
struct Cli {
    /// Capture one frame to this path and exit.
    shot: Option<PathBuf>,
    /// What to show in it.
    show: Shot,
    /// Theme name: `glass` (default), `paper` or `neon`, loaded from
    /// `assets/themes/<name>.theme.ron`.
    theme: String,
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let value = |flag: &str| args.windows(2).find(|w| w[0] == flag).map(|w| w[1].clone());
    let has = |flag: &str| args.iter().any(|a| a == flag);
    let show = if has("--confirm") {
        Shot::Confirm
    } else if has("--pause") {
        Shot::Pause
    } else if has("--page") {
        Shot::Page
    } else if has("--toast") {
        Shot::Toast
    } else {
        Shot::Main
    };
    let cli = Cli {
        shot: value("--shot").map(PathBuf::from),
        show,
        theme: value("--theme").unwrap_or_else(|| "glass".to_owned()),
    };

    let registries = menus::registries();

    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .set(WindowPlugin {
                primary_window: Some(Window {
                    title: "slotted — menus".into(),
                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    resolution: WindowResolution::new(WIDTH as u32, HEIGHT as u32)
                        .with_scale_factor_override(1.0),
                    ..default()
                }),
                ..default()
            })
            .set(AssetPlugin {
                file_path: menus::assets_dir().to_string_lossy().into_owned(),
                ..default()
            }),
    )
    // See `examples/chest/src/main.rs`: a windowed game adds the two
    // navigation plugins itself.
    .add_plugins((
        bevy::input_focus::tab_navigation::TabNavigationPlugin,
        bevy::input_focus::directional_navigation::DirectionalNavigationPlugin,
    ))
    .insert_resource(Registries(registries))
    .add_plugins(SlottedPlugins::default())
    .add_plugins(BackdropPlugin {
        divisor: 4,
        clear_color: Color::srgb(0.043, 0.055, 0.078),
    })
    .add_plugins(MenusDemoPlugin)
    // Where the settings go: the system's temporary directory, so the
    // example leaves nothing behind in the checkout.
    .insert_resource(SettingsStorage::file(menus::settings_path()))
    .insert_resource(ClearColor(Color::srgb(0.043, 0.055, 0.078)))
    .add_plugins(SceneBackdropPlugin)
    .add_systems(Startup, (setup_theme, mark_backdrop_source))
    .add_systems(Update, (open_main_once, stage_shot, shot_and_exit));
    app.insert_resource(cli);
    app.run();
}

/// `Startup`: the theme through the `AssetServer`.
fn setup_theme(mut commands: Commands, assets: Res<AssetServer>, cli: Res<Cli>) {
    commands.insert_resource(ActiveTheme(
        assets.load(format!("themes/{}.theme.ron", cli.theme)),
    ));
}

/// `Startup`, after the backdrop: the blur pass copies this camera.
fn mark_backdrop_source(mut commands: Commands, cameras: Query<Entity, With<MainCamera>>) {
    for camera in &cameras {
        commands.entity(camera).insert(BackdropSource);
    }
}

/// The first frame `menus:main` is registered: open it.
fn open_main_once(mut commands: Commands, screens: Res<Screens>, mut opened: Local<bool>) {
    if *opened || screens.get(&menus::main_kind()).is_none() {
        return;
    }
    *opened = true;
    menus::open_main_menu(&mut commands);
}

/// `--shot`: what the flag asked for, staged a second before the capture,
/// on the keyboard so the focus ring shows.
fn stage_shot(
    cli: Res<Cli>,
    time: Res<Time>,
    stack: Res<ScreenStack>,
    mut mode: ResMut<slotted::ui::InputMode>,
    mut commands: Commands,
    mut stage: Local<u8>,
) {
    if cli.shot.is_none() {
        return;
    }
    let t = time.elapsed_secs();
    // The real mouse may sit over the window and flip the mode back to
    // pointer, which hides the ring and the hint bar; a shot is a keyboard
    // shot.
    if *stage >= 1 && *mode != slotted::ui::InputMode::Keyboard {
        *mode = slotted::ui::InputMode::Keyboard;
    }
    match (*stage, cli.show) {
        (0, _) if t >= SHOT_AT * 0.4 => {
            *stage = 1;
            *mode = slotted::ui::InputMode::Keyboard;
        }
        (1, Shot::Main) => *stage = 9,
        (1, Shot::Pause | Shot::Confirm) => {
            // Leave the title, then pause over nothing, as Play then Esc
            // then Esc would.
            *stage = 2;
            slotted::ui::pop_screen(&mut commands);
        }
        (2, Shot::Pause | Shot::Confirm) if stack.top().is_none() => {
            *stage = 3;
            commands.queue(|world: &mut World| {
                let kind = world.resource::<MenuConfig>().pause_kind.clone();
                let Some(def) = world.resource::<Screens>().get(&kind).cloned() else {
                    warn!("--pause: {} is not registered", kind.0);
                    return;
                };
                push_screen(&mut world.commands(), def, None);
            });
        }
        (3, Shot::Confirm) if t >= SHOT_AT * 0.7 => {
            *stage = 9;
            menus::confirm_leave(&mut commands);
        }
        (1, Shot::Page) => {
            *stage = 9;
            slotted::menu::open_page(
                &mut commands,
                slotted::menu::PageSpec::new("demo.menus.about.title", "demo.menus.about.body"),
            );
        }
        (1, Shot::Toast) => {
            *stage = 9;
            toast(
                &mut commands,
                ToastSpec::new("demo.menus.quick_stack").level(ToastLevel::Success),
            );
            toast(
                &mut commands,
                ToastSpec::new("slotted.menu.binding_moved")
                    .arg("action", "tab_next")
                    .level(ToastLevel::Warning),
            );
        }
        _ => {}
    }
}

/// `--shot`: capture at [`SHOT_AT`] seconds, then leave.
fn shot_and_exit(
    mut commands: Commands,
    time: Res<Time>,
    cli: Res<Cli>,
    mut exit: MessageWriter<AppExit>,
    mut taken: Local<bool>,
) {
    let Some(path) = cli.shot.clone() else { return };
    if time.elapsed_secs() < SHOT_AT {
        return;
    }
    if !*taken {
        *taken = true;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        info!("capturing {}", path.display());
        commands
            .spawn(Screenshot::primary_window())
            .observe(save_to_disk(path));
        return;
    }
    // One extra half-second so the screenshot reaches the disk.
    if time.elapsed_secs() > SHOT_AT + 0.5 {
        exit.write(AppExit::Success);
    }
}
