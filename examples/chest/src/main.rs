//! The moodboard chest, in a window, over a 3D scene.
//!
//! ```text
//! cargo run -p chest                          play with it
//! cargo run -p chest -- --shot shots/chest.png     capture and exit
//! cargo run -p chest -- --hover 0 --shot shots/chest-hover.png
//! cargo run -p chest -- --paint --shot shots/chest-paint.png   a right-drag mid-paint
//! cargo run -p chest -- --cheat                  browser Ctrl+click gives
//! cargo run -p chest -- --recipe minecraft:coal --shot shots/chest-recipe.png
//! cargo run -p chest -- --record session.ron       record input, replay it in a test
//! cargo run -p chest -- --theme paper              the same screen in another theme
//! ```
//!
//! What to try once it is up: left-click a stack to pick it up and right-click
//! to place one; shift-click to send a stack across; hold right and drag over
//! empty slots to paint one item into each; press `1` to `9` over a slot to
//! swap it with that hotbar slot; hover for a tooltip and hold shift to expand
//! it; Tab and the arrow keys move a focus ring; the rail on the right sorts
//! and moves stacks in bulk; `Esc` closes the screen and `E` opens it again.
//! `F7` toggles the HUD position editor: drag a HUD layer to move it, `Esc`
//! puts the one you are dragging back, and where you leave them is saved to
//! `examples/chest/hud_layout.ron` (git-ignored). With `--record <path>` every
//! pointer, key and gamepad input is written there on exit, and
//! `UiHarness::replay` feeds it back frame by frame.
//! The browser panel docks beside the chest: type in its search field, press
//! `R` over a card for its recipes, `U` for its uses, `A` to bookmark it, and
//! `Backspace` to go back. The `+` button stays disabled because a chest has
//! no crafting grid to fill.
//!
//! The scene, the orbit and the screenshot plumbing are lifted from
//! `spikes/glass-ui`. Everything else comes out of `lib.rs`, which the headless
//! tests use unchanged.

use std::path::PathBuf;

use bevy::camera::NormalizedRenderTarget;
use bevy::picking::pointer::{Location, PointerAction, PointerId, PointerInput};
use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, save_to_disk};
use bevy::ui::ui_transform::UiGlobalTransform;
use bevy::window::{PrimaryWindow, WindowResolution};
use chest::{ChestBinding, ChestDemoPlugin};
use slotted::prelude::*;
use slotted::theme::blur::{BackdropPlugin, BackdropSource};
use slotted_model::SlotIx;

const WIDTH: f32 = 1600.0;
const HEIGHT: f32 = 900.0;
/// How long the shot mode lets the scene settle before capturing.
const SHOT_AT: f32 = 2.0;

/// Command line. Absent flags mean "interactive, forever".
#[derive(Resource, Debug, Clone, Default)]
struct Cli {
    /// Capture one frame to this path and exit.
    shot: Option<PathBuf>,
    /// Park a synthetic mouse pointer over this menu slot.
    hover: Option<u16>,
    /// Pick a stack up and hold a right-drag over three empty slots, so a
    /// capture shows the phantom preview mid-paint.
    paint: bool,
    /// Open the chest as a player who may cheat, so the browser's Ctrl+click
    /// give is allowed.
    cheat: bool,
    /// Open the browser's recipe page for this item before capturing.
    recipe: Option<String>,
    /// Record every input to this RON file, written on exit.
    record: Option<PathBuf>,
    /// Theme name: `glass` (default), `paper` or `neon`, loaded from
    /// `assets/themes/<name>.theme.ron`.
    theme: String,
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let value = |flag: &str| args.windows(2).find(|w| w[0] == flag).map(|w| w[1].clone());
    let cli = Cli {
        shot: value("--shot").map(PathBuf::from),
        hover: value("--hover").and_then(|v| v.parse().ok()),
        paint: args.iter().any(|a| a == "--paint"),
        cheat: args.iter().any(|a| a == "--cheat"),
        recipe: value("--recipe"),
        record: value("--record").map(PathBuf::from),
        theme: value("--theme").unwrap_or_else(|| "glass".to_owned()),
    };

    let registries = chest::load_registries();

    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .set(WindowPlugin {
                primary_window: Some(Window {
                    title: "slotted — chest".into(),
                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    resolution: WindowResolution::new(WIDTH as u32, HEIGHT as u32)
                        .with_scale_factor_override(1.0),
                    ..default()
                }),
                ..default()
            })
            // Assets resolve against the workspace `assets/` directory, so the
            // binary runs from anywhere.
            .set(AssetPlugin {
                file_path: chest::assets_dir().to_string_lossy().into_owned(),
                ..default()
            }),
    )
    // `DefaultPlugins` carries `InputFocusPlugin`, `InputDispatchPlugin` and
    // the widget plugins, but not these two, and `slotted-ui`'s navigation
    // systems need their resources. The headless group adds them for you; a
    // windowed game has to say so itself (docs/FOLLOWUPS.md).
    .add_plugins((
        bevy::input_focus::tab_navigation::TabNavigationPlugin,
        bevy::input_focus::directional_navigation::DirectionalNavigationPlugin,
    ))
    // The ports go in before the plugin group, which is the documented way to
    // replace an adapter: here it is the frozen registries the data stage made.
    .insert_resource(Registries(registries))
    .add_plugins(SlottedPlugins::default())
    .add_plugins(BackdropPlugin {
        divisor: 4,
        clear_color: Color::srgb(0.043, 0.055, 0.078),
    })
    .add_plugins(ChestDemoPlugin)
    .insert_resource(ClearColor(Color::srgb(0.043, 0.055, 0.078)))
    .insert_resource(chest::CheatMode(cli.cheat))
    // The HUD position editor: `F7` toggles it, and where the player leaves
    // the layers is written beside the example rather than into `assets/`,
    // because it is this player's layout and not the game's data.
    .insert_resource(slotted::ui::hud_editor::HudEditKey(KeyCode::F7))
    .insert_resource(slotted::ui::hud_editor::HudLayoutStore {
        path: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("hud_layout.ron"),
    })
    .add_systems(Startup, (setup_scene, setup_screen).chain())
    .add_systems(
        Update,
        (
            orbit_camera,
            spin_cubes,
            park_pointer,
            park_paint,
            open_recipe_page,
            shot_and_exit,
        ),
    );

    if let Some(path) = cli.record.clone() {
        info!("recording input to {}", path.display());
        app.insert_resource(slotted::ui::recording::InputRecorder::new(
            path,
            Vec2::new(WIDTH, HEIGHT),
            1.0,
            std::time::Duration::from_secs_f64(1.0 / 60.0),
        ));
    }
    app.insert_resource(cli);

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
        Color::srgb(0.90, 0.32, 0.36),
        Color::srgb(0.32, 0.72, 0.95),
        Color::srgb(0.98, 0.75, 0.28),
        Color::srgb(0.45, 0.88, 0.55),
        Color::srgb(0.78, 0.45, 0.95),
        Color::srgb(0.98, 0.55, 0.25),
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
        // The backdrop camera copies this transform, so the blurred image
        // behind the panel lines up with the world around it.
        BackdropSource,
        IsDefaultUiCamera,
    ));
}

/// Loads the theme, registers the screen from RON and opens the chest.
fn setup_screen(
    mut commands: Commands,
    assets: Res<AssetServer>,
    cli: Res<Cli>,
    mut screens: ResMut<Screens>,
    registries: Res<Registries>,
    cheat: Res<chest::CheatMode>,
) {
    commands.insert_resource(ActiveTheme(
        assets.load(format!("themes/{}.theme.ron", cli.theme)),
    ));
    screens.register(chest::demo_screen());
    chest::open_chest(&mut commands, &registries, cheat.0);
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

/// Drives the real picking backend from a synthetic pointer so `--hover` can
/// take a tooltip screenshot without a human at the mouse.
fn park_pointer(
    cli: Res<Cli>,
    windows: Query<Entity, With<PrimaryWindow>>,
    slots: Query<(&SlotRef, &UiGlobalTransform)>,
    mut out: MessageWriter<PointerInput>,
) {
    let Some(target) = cli.hover else { return };
    let Ok(window) = windows.single() else { return };
    let Some(normalized) = bevy::window::WindowRef::Primary.normalize(Some(window)) else {
        return;
    };
    for (slot, transform) in &slots {
        if slot.slot != SlotIx(target) {
            continue;
        }
        out.write(PointerInput::new(
            PointerId::Mouse,
            Location {
                target: NormalizedRenderTarget::Window(normalized),
                position: transform.translation,
            },
            PointerAction::Move { delta: Vec2::ZERO },
        ));
    }
}

/// `--paint`: pick the fullest chest stack up and hold a right-drag across
/// three empty slots, one synthetic pointer event per frame, leaving the
/// button down. What a capture then shows is a paint in progress: three
/// phantoms with `+1` and a cursor that has already counted itself down.
#[allow(clippy::too_many_arguments, clippy::similar_names)]
fn park_paint(
    cli: Res<Cli>,
    time: Res<Time>,
    windows: Query<Entity, With<PrimaryWindow>>,
    slots: Query<(&SlotRef, &UiGlobalTransform, &slotted::ui::ItemView)>,
    mut out: MessageWriter<PointerInput>,
    mut plan: Local<Vec<PointerAction>>,
    mut spots: Local<Vec<Vec2>>,
    mut step: Local<usize>,
) {
    if !cli.paint || time.elapsed_secs() < SHOT_AT * 0.4 {
        return;
    }
    let Ok(window) = windows.single() else { return };
    let Some(normalized) = bevy::window::WindowRef::Primary.normalize(Some(window)) else {
        return;
    };
    if plan.is_empty() {
        // The fullest container slot is what we pick up; the first three
        // empty ones are what we paint into.
        let mut filled: Vec<(u32, Vec2)> = Vec::new();
        let mut empty: Vec<(u16, Vec2)> = Vec::new();
        for (slot, transform, view) in &slots {
            if slot.slot.0 >= 27 {
                continue;
            }
            match view.stack.as_ref() {
                Some(stack) => filled.push((stack.count, transform.translation)),
                None => empty.push((slot.slot.0, transform.translation)),
            }
        }
        filled.sort_by_key(|(count, _)| std::cmp::Reverse(*count));
        empty.sort_by_key(|(ix, _)| *ix);
        let (Some((_, source)), true) = (filled.first(), empty.len() >= 3) else {
            warn!("--paint needs a full slot and three empty ones");
            return;
        };
        *spots = vec![*source, empty[0].1, empty[1].1, empty[2].1];
        // The deltas are real: `bevy_picking` reads a zero-delta move as the
        // pointer standing still and never calls it a drag.
        let step = |from: usize, to: usize| PointerAction::Move {
            delta: spots[to] - spots[from],
        };
        *plan = vec![
            PointerAction::Move { delta: Vec2::ZERO },
            PointerAction::Press(bevy::picking::pointer::PointerButton::Primary),
            PointerAction::Release(bevy::picking::pointer::PointerButton::Primary),
            step(0, 1),
            PointerAction::Press(bevy::picking::pointer::PointerButton::Secondary),
            step(1, 2),
            step(2, 3),
        ];
    }
    let Some(action) = plan.get(*step).copied() else {
        return;
    };
    // Which spot the pointer is at for each step of the plan above.
    let position = spots[match *step {
        0..=2 => 0,
        3 | 4 => 1,
        5 => 2,
        _ => 3,
    }];
    out.write(PointerInput::new(
        PointerId::Mouse,
        Location {
            target: NormalizedRenderTarget::Window(normalized),
            position,
        },
        action,
    ));
    *step += 1;
}

/// `--recipe <ns:item>`: open the browser's recipe page once, before the shot.
fn open_recipe_page(
    cli: Res<Cli>,
    time: Res<Time>,
    registries: Res<Registries>,
    mut out: MessageWriter<slotted::browser::OpenRecipes>,
    mut done: Local<bool>,
) {
    let Some(name) = cli.recipe.clone() else {
        return;
    };
    if *done || time.elapsed_secs() < SHOT_AT * 0.5 {
        return;
    }
    *done = true;
    let Ok(id) = slotted_model::Namespaced::parse(&name) else {
        warn!("--recipe {name} is not a namespaced id");
        return;
    };
    let Some(item) = registries.item_id(&id) else {
        warn!("--recipe {name} is not a registered item");
        return;
    };
    out.write(slotted::browser::OpenRecipes(
        slotted::browser::Ingredient::item(item),
    ));
}

/// `--shot`: capture at [`SHOT_AT`] seconds, then leave.
fn shot_and_exit(
    mut commands: Commands,
    time: Res<Time>,
    cli: Res<Cli>,
    binding: Res<ChestBinding>,
    mut exit: MessageWriter<AppExit>,
    mut taken: Local<bool>,
) {
    let Some(path) = cli.shot.clone() else { return };
    if time.elapsed_secs() < SHOT_AT {
        return;
    }
    if !*taken {
        *taken = true;
        if binding.open.is_none() {
            warn!("capturing with no chest screen open");
        }
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
