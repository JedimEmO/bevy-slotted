//! The modded example in a window, over the same 3D scene as `chest`.
//!
//! ```text
//! cargo run -p modded                       play with it; edit mods/*/control.lua live
//! cargo run -p modded -- --no-console --shot shots/modded.png
//! cargo run -p modded -- --shot shots/modded-console.png
//! cargo run -p modded -- --reload copper_chest --shot shots/modded-reload.png
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
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let value = |flag: &str| args.windows(2).find(|w| w[0] == flag).map(|w| w[1].clone());
    let cli = Cli {
        shot: value("--shot").map(PathBuf::from),
        reload: value("--reload"),
        no_console: args.iter().any(|a| a == "--no-console"),
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
        .add_systems(Startup, (setup_scene, setup_theme))
        .add_systems(
            Update,
            (orbit_camera, spin_cubes, reload_once, shot_and_exit),
        );

    app.run();
}

/// Marks the camera the UI and the backdrop both follow.
#[derive(Component)]
struct MainCamera;

/// Rotation speed of a demo cube.
#[derive(Component)]
struct Spin(f32);

/// Ground, a ring of coloured cubes, one directional light, one camera.
fn setup_scene(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(60.0, 60.0))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.10, 0.12, 0.16),
            perceptual_roughness: 0.9,
            ..default()
        })),
    ));

    let cube = meshes.add(Cuboid::new(1.6, 1.6, 1.6));
    let palette = [
        Color::srgb(0.90, 0.55, 0.30),
        Color::srgb(0.32, 0.72, 0.95),
        Color::srgb(0.98, 0.75, 0.28),
        Color::srgb(0.45, 0.88, 0.55),
        Color::srgb(0.78, 0.45, 0.95),
        Color::srgb(0.98, 0.42, 0.36),
    ];
    #[allow(clippy::cast_precision_loss)]
    for (i, color) in palette.into_iter().enumerate() {
        let angle = i as f32 / palette.len() as f32 * std::f32::consts::TAU;
        commands.spawn((
            Mesh3d(cube.clone()),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: color,
                perceptual_roughness: 0.35,
                metallic: 0.1,
                ..default()
            })),
            Transform::from_xyz(
                angle.cos() * 5.0,
                0.8 + (i as f32 * 0.35),
                angle.sin() * 5.0,
            )
            .with_rotation(Quat::from_rotation_y(angle)),
            Spin(0.4 + i as f32 * 0.1),
        ));
    }

    commands.spawn((
        DirectionalLight {
            illuminance: 12_000.0,
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_xyz(6.0, 12.0, 5.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(0.0, 6.0, 14.0).looking_at(Vec3::new(0.0, 1.0, 0.0), Vec3::Y),
        MainCamera,
        BackdropSource,
        IsDefaultUiCamera,
    ));
}

/// The theme comes through `pack://`, so a resource pack could replace it.
fn setup_theme(mut commands: Commands, assets: Res<AssetServer>) {
    commands.insert_resource(ActiveTheme(assets.load("themes/glass.theme.ron")));
}

fn orbit_camera(time: Res<Time>, mut cameras: Query<&mut Transform, With<MainCamera>>) {
    let t = time.elapsed_secs() * 0.18;
    for mut transform in &mut cameras {
        *transform = Transform::from_xyz(t.sin() * 14.0, 6.0, t.cos() * 14.0)
            .looking_at(Vec3::new(0.0, 1.0, 0.0), Vec3::Y);
    }
}

fn spin_cubes(time: Res<Time>, mut cubes: Query<(&mut Transform, &Spin)>) {
    for (mut transform, spin) in &mut cubes {
        transform.rotate_y(spin.0 * time.delta_secs());
        transform.rotate_x(spin.0 * 0.4 * time.delta_secs());
    }
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
