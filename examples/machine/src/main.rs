//! The machine example in a window.
//!
//! ```text
//! cargo run -p machine                       play with it; R toggles redstone
//! cargo run -p machine -- --shot shots/machine.png
//! cargo run -p machine -- --redstone         start with the signal on
//! cargo run -p machine -- --tab tab_redstone --shot shots/machine-tab.png
//! cargo run -p machine -- --theme neon         the same screen in another theme
//! ```
//!
//! What to look at once it is up: the tank and the energy bar follow the
//! menu's properties, the two arrows fill as the furnace cooks, and the two
//! tabs on the right open on a click. The `Sort` button beside the title came
//! from `mods/sorter`, which was written for a screen it has never seen.

use std::path::PathBuf;

use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, save_to_disk};
use bevy::window::WindowResolution;
use machine::{MachineDemoPlugin, layout, mods_dir};
use slotted::prelude::*;
use slotted::theme::blur::{BackdropPlugin, BackdropSource};
use slotted_packs::{PackSourcePlugin, PacksConfig, SlottedPacksPlugin};

const WIDTH: f32 = 1600.0;
const HEIGHT: f32 = 900.0;
/// How long the shot mode lets the scene settle before capturing.
const SHOT_AT: f32 = 2.0;
/// When `--tab` opens a side tab, early enough that the shot shows it open.
const TAB_AT: f32 = 1.0;

/// Command line. Absent flags mean "interactive, forever".
#[derive(Resource, Debug, Clone, Default)]
struct Cli {
    /// Capture one frame to this path and exit.
    shot: Option<PathBuf>,
    /// Start with the redstone signal on.
    redstone: bool,
    /// Open this side tab a second in, by `test_id`.
    tab: Option<String>,
    /// Theme name: `glass` (default), `paper` or `neon`.
    theme: String,
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let value = |flag: &str| args.windows(2).find(|w| w[0] == flag).map(|w| w[1].clone());
    let cli = Cli {
        shot: value("--shot").map(PathBuf::from),
        redstone: args.iter().any(|a| a == "--redstone"),
        tab: value("--tab"),
        theme: value("--theme").unwrap_or_else(|| "glass".to_owned()),
    };

    let mut app = App::new();
    // The `pack://` source has to exist before the asset server is built, so
    // this plugin goes in ahead of `DefaultPlugins`.
    app.add_plugins(PackSourcePlugin::new(layout(&mods_dir())))
        .add_plugins(
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: "slotted — machine".into(),
                        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                        resolution: WindowResolution::new(WIDTH as u32, HEIGHT as u32)
                            .with_scale_factor_override(1.0),
                        ..default()
                    }),
                    ..default()
                })
                .set(AssetPlugin {
                    file_path: machine::assets_dir().to_string_lossy().into_owned(),
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
                ..PacksConfig::default()
            },
        }))
        .add_plugins(BackdropPlugin {
            divisor: 4,
            clear_color: Color::srgb(0.043, 0.055, 0.078),
        })
        .add_plugins(MachineDemoPlugin)
        .insert_resource(ClearColor(Color::srgb(0.043, 0.055, 0.078)))
        .insert_resource(machine::Redstone(cli.redstone))
        .insert_resource(cli)
        .add_systems(Startup, (setup_scene, setup_theme, open_furnace))
        .add_systems(Update, (orbit_camera, open_tab_once, shot_and_exit));

    app.run();
}

/// Marks the camera the UI and the backdrop both follow.
#[derive(Component)]
struct MainCamera;

/// `Startup`: the inventories, the menu and the screen the demo opens on.
///
/// The screen file is read from this crate rather than from `assets/`: it is
/// the example's own content, and reading it here is what lets `tests/ui.rs`
/// drive the identical bytes.
fn open_furnace(
    mut commands: Commands,
    mut ids: ResMut<slotted::ecs::MenuIdAllocator>,
    mut screens: ResMut<Screens>,
) {
    let registries = machine::load_registries();
    let def = screens.register(machine::furnace_screen());
    let entities: Vec<Entity> = machine::inventories(&registries)
        .into_iter()
        .map(|inventory| commands.spawn(slotted::ecs::Inventory(inventory)).id())
        .collect();
    commands.insert_resource(Registries(registries));
    let menu = open_menu(
        &mut commands,
        &mut ids,
        machine::menu_def(),
        entities,
        slotted_model::Actor::SURVIVAL,
    );
    spawn_screen(&mut commands, def, Some(menu));
}

/// Ground, one block, one light, one orbiting camera. The machine screen is
/// the subject here, so the scene behind it stays quieter than the chest
/// demo's ring of cubes.
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
    commands.spawn((
        Mesh3d(meshes.add(Cuboid::new(2.4, 2.4, 2.4))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.62, 0.44, 0.28),
            perceptual_roughness: 0.5,
            metallic: 0.2,
            ..default()
        })),
        Transform::from_xyz(0.0, 1.2, 0.0),
    ));
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
fn setup_theme(mut commands: Commands, assets: Res<AssetServer>, cli: Res<Cli>) {
    commands.insert_resource(ActiveTheme(
        assets.load(format!("themes/{}.theme.ron", cli.theme)),
    ));
}

fn orbit_camera(time: Res<Time>, mut cameras: Query<&mut Transform, With<MainCamera>>) {
    let t = time.elapsed_secs() * 0.14;
    for mut transform in &mut cameras {
        *transform = Transform::from_xyz(t.sin() * 13.0, 6.0, t.cos() * 13.0)
            .looking_at(Vec3::new(0.0, 1.0, 0.0), Vec3::Y);
    }
}

/// `--tab <test_id>`: open one side tab a second in, so a screenshot of the
/// tabs shows one of them open.
fn open_tab_once(
    cli: Res<Cli>,
    time: Res<Time>,
    tabs: Query<(Entity, &slotted::ui::TestId), With<slotted::ui::SideTabState>>,
    mut commands: Commands,
    mut done: Local<bool>,
) {
    let Some(wanted) = cli.tab.clone() else {
        return;
    };
    if *done || time.elapsed_secs() < TAB_AT {
        return;
    }
    *done = true;
    if let Some((entity, _)) = tabs.iter().find(|(_, id)| id.0 == wanted) {
        commands.trigger(slotted::ui::SideTabToggle { entity });
    } else {
        warn!("--tab {wanted}: no side tab with that test_id");
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
