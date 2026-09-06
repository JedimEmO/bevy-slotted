//! The modded example in a window, over the same 3D scene as `chest`.
//!
//! ```text
//! cargo run -p modded                       play with it; edit mods/*/control.lua live
//! cargo run -p modded -- --no-console --shot shots/modded.png
//! cargo run -p modded -- --shot shots/modded-console.png
//! cargo run -p modded -- --reload copper_chest --shot shots/modded-reload.png
//! cargo run -p modded -- --theme paper          the same screen in another theme
//! ```
//!
//! What to try once it is up: click a stack and watch the console in the
//! bottom-left fill with lines `mods/copper_chest/control.lua` printed;
//! `F1` hides and shows it. Alt+left-click a slot to toggle a favourite, a
//! command the script returns and the host applies. Press the `Sort` button
//! beside the title, which the `sorter` mod injected into a screen it does
//! not own. Hover an apple for the tooltip line `appleskin_like` added. Then
//! edit any `mods/*/control.lua` while the window is open: the file watcher
//! reloads the mod and the chest keeps its contents.

use std::path::PathBuf;

use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, save_to_disk};
use bevy::window::WindowResolution;
use modded::{ModdedDemoPlugin, layout, mods_dir};
use showcase::backdrop::MainCamera;
use slotted::prelude::*;
use slotted::theme::blur::{BackdropPlugin, BackdropSource};
use slotted_packs::{PackSourcePlugin, PacksConfig, ReloadMod, SlottedPacksPlugin};
use slotted_script::ModId;

const WIDTH: f32 = 1600.0;
const HEIGHT: f32 = 900.0;
/// How long the shot mode lets the scene settle before capturing.
const SHOT_AT: f32 = 2.0;
/// When `--reload` fires, early enough that the shot shows the result.
const RELOAD_AT: f32 = 1.0;

/// Command line. Absent flags mean "interactive, forever".
#[derive(Resource, Debug, Clone, Default)]
struct Cli {
    /// Capture one frame to this path and exit.
    shot: Option<PathBuf>,
    /// Reload this mod once, a second in.
    reload: Option<String>,
    /// Start with the dev console hidden.
    no_console: bool,
    /// Theme name: `glass` (default), `paper` or `neon`.
    theme: String,
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let value = |flag: &str| args.windows(2).find(|w| w[0] == flag).map(|w| w[1].clone());
    let cli = Cli {
        shot: value("--shot").map(PathBuf::from),
        reload: value("--reload"),
        no_console: args.iter().any(|a| a == "--no-console"),
        theme: value("--theme").unwrap_or_else(|| "glass".to_owned()),
    };

    let mut app = App::new();
    // The `pack://` source has to exist before the asset server is built, so
    // this plugin goes in ahead of `DefaultPlugins`. It also inserts the
    // `PackLayout` the loader reads.
    app.add_plugins(PackSourcePlugin::new(layout(&mods_dir())))
        .add_plugins(
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: "slotted — modded".into(),
                        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                        resolution: WindowResolution::new(WIDTH as u32, HEIGHT as u32)
                            .with_scale_factor_override(1.0),
                        ..default()
                    }),
                    ..default()
                })
                .set(AssetPlugin {
                    file_path: modded::assets_dir().to_string_lossy().into_owned(),
                    ..default()
                }),
        )
        // `DefaultPlugins` carries the widget plugins but not these two, and
        // `slotted-ui`'s navigation systems need their resources.
        .add_plugins((
            bevy::input_focus::tab_navigation::TabNavigationPlugin,
            bevy::input_focus::directional_navigation::DirectionalNavigationPlugin,
        ))
        .add_plugins(SlottedPlugins::default().set(SlottedPacksPlugin {
            config: PacksConfig {
                hud_tick: None,
                reload_on_change: true,
                // `c` by default: the mods here add to `c:ingots` and
                // `c:foods` on purpose.
                ..PacksConfig::default()
            },
        }))
        .add_plugins(BackdropPlugin {
            divisor: 4,
            clear_color: Color::srgb(0.043, 0.055, 0.078),
        })
        .add_plugins(ModdedDemoPlugin)
        .insert_resource(ClearColor(Color::srgb(0.043, 0.055, 0.078)))
        .insert_resource(modded::ConsoleVisible(!cli.no_console))
        .insert_resource(cli)
        .insert_resource(showcase::backdrop::BackdropConfig {
            shadows: true,
            ..showcase::backdrop::BackdropConfig::web()
        })
        .add_plugins(showcase::backdrop::BackdropPlugin)
        .add_systems(Startup, (setup_theme, mark_backdrop_source))
        .add_systems(Update, (reload_once, shot_and_exit))
        .run();
}

/// `Startup`, after [`showcase::backdrop::setup_backdrop`]: the blur pass
/// copies this camera's transform, so the image behind the panel lines up
/// with the world around it. The marker lives behind the facade's `blur`
/// feature, which the shared backdrop crate does not enable, so the example
/// that wants it says so itself.
fn mark_backdrop_source(mut commands: Commands, cameras: Query<Entity, With<MainCamera>>) {
    for camera in &cameras {
        commands.entity(camera).insert(BackdropSource);
    }
}

/// The theme comes through `pack://`, so a resource pack could replace it.
fn setup_theme(mut commands: Commands, assets: Res<AssetServer>, cli: Res<Cli>) {
    commands.insert_resource(ActiveTheme(
        assets.load(format!("themes/{}.theme.ron", cli.theme)),
    ));
}

/// `--reload <mod>`: ask for one reload a second in, so a screenshot taken at
/// [`SHOT_AT`] shows the reloaded state.
fn reload_once(
    cli: Res<Cli>,
    time: Res<Time>,
    mut out: MessageWriter<ReloadMod>,
    mut done: Local<bool>,
) {
    let Some(name) = cli.reload.clone() else {
        return;
    };
    if *done || time.elapsed_secs() < RELOAD_AT {
        return;
    }
    *done = true;
    match ModId::new(&name) {
        Ok(mod_id) => {
            info!("reloading {mod_id}");
            out.write(ReloadMod { mod_id });
        }
        Err(error) => warn!("--reload {name}: {error}"),
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
