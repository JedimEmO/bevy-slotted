//! Phase 0 spike: "Obsidian Glass" on bevy_ui 0.19.
//!
//! Proves out, in one binary:
//!   1. glass panel (BorderRadius + BorderColor + BoxShadow + tinted translucent fill)
//!   2. backdrop blur via a quarter-res second Camera3d into an Image, sampled by a UiMaterial
//!   3. a rarity glow / shimmer UiMaterial for a "legendary" slot
//!   4. slot and slot-grid as `bsn!` scene functions
//!   5. a live 3D ViewportNode inside one slot
//!
//! Flags: --no-blur, --no-viewport, --shot, --bench, --secs <f32>

mod materials;
mod ui;

use bevy::camera::{NormalizedRenderTarget, RenderTarget};
use bevy::picking::pointer::{Location, PointerAction, PointerId, PointerInput};
use bevy::ui::UiGlobalTransform;
use bevy::diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin};
use bevy::image::ImageSampler;
use bevy::prelude::*;
use bevy::camera::visibility::RenderLayers;
use bevy::render::render_resource::TextureFormat;
use bevy::render::view::screenshot::{save_to_disk, Screenshot};
use bevy::window::{PresentMode, WindowResolution};

use materials::{GlassPanelMaterial, SlotGlowMaterial};

const WIDTH: f32 = 1920.0;
const HEIGHT: f32 = 1080.0;
/// Backdrop image is quarter resolution on each axis, i.e. 1/16 the pixels.
const BACKDROP_DIV: u32 = 4;

#[derive(Resource, Clone)]
struct Config {
    blur: bool,
    viewport: bool,
    shot: bool,
    bench: bool,
    shadows: bool,
    backdrop_div: u32,
    /// Slot index to park a synthetic mouse pointer over, for a non-interactive hover shot.
    hover: Option<usize>,
    secs: f32,
}

#[derive(Resource)]
struct GlowHandle(Handle<SlotGlowMaterial>);

#[derive(Resource)]
struct BackdropImage(Handle<Image>);

#[derive(Resource)]
struct ViewportSetup {
    camera: Entity,
}

#[derive(Component)]
struct MainCamera;

#[derive(Component)]
struct BackdropCamera;

#[derive(Component)]
struct Spin(f32);

#[derive(Resource, Default)]
struct Stats {
    samples: Vec<f64>,
    done: bool,
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let has = |f: &str| args.iter().any(|a| a == f);
    let secs = args
        .windows(2)
        .find(|w| w[0] == "--secs")
        .and_then(|w| w[1].parse::<f32>().ok());

    let bench = has("--bench");
    let shot = has("--shot");
    let cfg = Config {
        blur: !has("--no-blur"),
        viewport: !has("--no-viewport"),
        shot,
        bench,
        shadows: !has("--no-shadows"),
        backdrop_div: args
            .windows(2)
            .find(|w| w[0] == "--backdrop-div")
            .and_then(|w| w[1].parse::<u32>().ok())
            .unwrap_or(BACKDROP_DIV),
        hover: args
            .windows(2)
            .find(|w| w[0] == "--hover")
            .and_then(|w| w[1].parse::<usize>().ok()),
        secs: secs.unwrap_or(if bench { 12.0 } else if shot { 4.0 } else { f32::MAX }),
    };

    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .set(WindowPlugin {
                primary_window: Some(Window {
                    title: "slotted glass-ui spike".into(),
                    resolution: WindowResolution::new(WIDTH as u32, HEIGHT as u32)
                        .with_scale_factor_override(1.0),
                    // No vsync, so frame time reflects actual GPU+CPU work.
                    present_mode: PresentMode::AutoNoVsync,
                    ..default()
                }),
                ..default()
            })
            .set(ImagePlugin::default_nearest())
            // Assets and screenshots resolve against the crate root, so the release binary can
            // be launched from anywhere (CI, a benchmark loop) without a cargo wrapper.
            .set(AssetPlugin {
                file_path: format!("{}/assets", env!("CARGO_MANIFEST_DIR")),
                ..default()
            }),
    )
    .add_plugins(FrameTimeDiagnosticsPlugin::default())
    .add_plugins(UiMaterialPlugin::<GlassPanelMaterial>::default())
    .add_plugins(UiMaterialPlugin::<SlotGlowMaterial>::default())
    .insert_resource(cfg)
    .init_resource::<Stats>()
    .insert_resource(ClearColor(ui::BASE))
    .add_systems(Startup, (setup_scene, setup_ui).chain())
    .add_systems(
        Update,
        (
            ui::decorate_slots,
            orbit_camera,
            sync_backdrop_camera.after(orbit_camera),
            spin,
            ui::slot_hover_feedback,
            park_pointer,
            collect_stats,
            shot_and_exit,
        ),
    );

    app.run();
}

fn setup_scene(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut standard: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    cfg: Res<Config>,
) {
    // --- world ---------------------------------------------------------------------------
    let plane = meshes.add(Plane3d::default().mesh().size(60.0, 60.0));
    let cube = meshes.add(Cuboid::new(1.6, 1.6, 1.6));

    commands.spawn((
        Mesh3d(plane),
        MeshMaterial3d(standard.add(StandardMaterial {
            base_color: Color::srgb(0.10, 0.12, 0.16),
            perceptual_roughness: 0.9,
            ..default()
        })),
    ));

    let palette = [
        Color::srgb(0.90, 0.32, 0.36),
        Color::srgb(0.32, 0.72, 0.95),
        Color::srgb(0.98, 0.75, 0.28),
        Color::srgb(0.45, 0.88, 0.55),
        Color::srgb(0.78, 0.45, 0.95),
        Color::srgb(0.98, 0.55, 0.25),
    ];
    for (i, color) in palette.into_iter().enumerate() {
        let a = i as f32 / palette.len() as f32 * std::f32::consts::TAU;
        commands.spawn((
            Mesh3d(cube.clone()),
            MeshMaterial3d(standard.add(StandardMaterial {
                base_color: color,
                perceptual_roughness: 0.35,
                metallic: 0.1,
                ..default()
            })),
            Transform::from_xyz(a.cos() * 5.0, 0.8 + (i as f32 * 0.35), a.sin() * 5.0)
                .with_rotation(Quat::from_rotation_y(a)),
            Spin(0.4 + i as f32 * 0.1),
        ));
    }

    commands.spawn((
        DirectionalLight {
            illuminance: 12_000.0,
            shadow_maps_enabled: cfg.shadows,
            ..default()
        },
        Transform::from_xyz(6.0, 12.0, 5.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    // --- main camera ---------------------------------------------------------------------
    commands.spawn((
        Camera3d::default(),
        Camera {
            order: 0,
            ..default()
        },
        Transform::from_xyz(0.0, 6.0, 14.0).looking_at(Vec3::new(0.0, 1.0, 0.0), Vec3::Y),
        MainCamera,
        IsDefaultUiCamera,
    ));

    // --- backdrop camera: same scene, quarter resolution, into an Image -------------------
    let mut backdrop = Image::new_target_texture(
        (WIDTH as u32) / cfg.backdrop_div,
        (HEIGHT as u32) / cfg.backdrop_div,
        TextureFormat::Rgba8UnormSrgb,
        None,
    );
    // Linear filtering matters: the blur taps are between texels of a small image.
    backdrop.sampler = ImageSampler::linear();
    let backdrop = images.add(backdrop);
    commands.insert_resource(BackdropImage(backdrop.clone()));

    if cfg.blur {
        commands.spawn((
            Camera3d::default(),
            Camera {
                // Negative order so it runs before the window camera in the same frame.
                order: -1,
                clear_color: ClearColorConfig::Custom(ui::BASE),
                ..default()
            },
            // In 0.19 the render target is its own component, not a `Camera` field.
            RenderTarget::Image(backdrop.into()),
            Transform::from_xyz(0.0, 6.0, 14.0).looking_at(Vec3::new(0.0, 1.0, 0.0), Vec3::Y),
            BackdropCamera,
        ));
    }

    // --- viewport camera: its own tiny scene rendered into a slot -------------------------
    if cfg.viewport {
        let mut vp = Image::new_target_texture(128, 128, TextureFormat::Rgba8UnormSrgb, None);
        vp.sampler = ImageSampler::linear();
        let vp = images.add(vp);
        let layer = RenderLayers::layer(1);
        commands.spawn((
            Mesh3d(cube.clone()),
            MeshMaterial3d(standard.add(StandardMaterial {
                base_color: ui::WARM,
                perceptual_roughness: 0.3,
                ..default()
            })),
            Transform::from_xyz(0.0, 0.0, 0.0),
            Spin(1.6),
            layer.clone(),
        ));
        commands.spawn((
            DirectionalLight {
                illuminance: 8_000.0,
                ..default()
            },
            Transform::from_xyz(2.0, 3.0, 4.0).looking_at(Vec3::ZERO, Vec3::Y),
            layer.clone(),
        ));
        let camera = commands
            .spawn((
                Camera3d::default(),
                Camera {
                    order: -2,
                    clear_color: ClearColorConfig::Custom(Color::NONE),
                    ..default()
                },
                RenderTarget::Image(vp.into()),
                Transform::from_xyz(0.0, 1.4, 3.2).looking_at(Vec3::ZERO, Vec3::Y),
                layer,
            ))
            .id();
        commands.insert_resource(ViewportSetup { camera });
    }
}

fn setup_ui(
    mut commands: Commands,
    mut glass_materials: ResMut<Assets<GlassPanelMaterial>>,
    mut glow_materials: ResMut<Assets<SlotGlowMaterial>>,
    backdrop: Res<BackdropImage>,
    cfg: Res<Config>,
) {
    let glass = glass_materials.add(GlassPanelMaterial {
        tint: ui::PANEL.with_alpha(if cfg.blur { 0.48 } else { 0.86 }).into(),
        edge_color: LinearRgba::new(1.0, 1.0, 1.0, 0.35),
        blur_radius: 4.0,
        blur_enabled: if cfg.blur { 1.0 } else { 0.0 },
        _pad: Vec2::ZERO,
        backdrop: backdrop.0.clone(),
    });

    let glow = glow_materials.add(SlotGlowMaterial {
        rarity_color: ui::WARM.into(),
        params: Vec4::new(10.0, 8.0, 2.0, 0.22),
    });
    commands.insert_resource(GlowHandle(glow));

    commands.queue_spawn_scene(ui::glass_screen(glass));
}

fn orbit_camera(time: Res<Time>, mut q: Query<&mut Transform, With<MainCamera>>) {
    let t = time.elapsed_secs() * 0.18;
    for mut tf in &mut q {
        *tf = Transform::from_xyz(t.sin() * 14.0, 6.0, t.cos() * 14.0)
            .looking_at(Vec3::new(0.0, 1.0, 0.0), Vec3::Y);
    }
}

/// The backdrop camera must track the main camera exactly, or the blurred image will not line up
/// with the world behind the panel.
fn sync_backdrop_camera(
    main: Query<&Transform, (With<MainCamera>, Without<BackdropCamera>)>,
    mut backdrop: Query<&mut Transform, With<BackdropCamera>>,
) {
    let Ok(src) = main.single() else { return };
    for mut dst in &mut backdrop {
        *dst = *src;
    }
}

fn spin(time: Res<Time>, mut q: Query<(&mut Transform, &Spin)>) {
    for (mut tf, s) in &mut q {
        tf.rotate_y(s.0 * time.delta_secs());
        tf.rotate_x(s.0 * 0.4 * time.delta_secs());
    }
}

/// Drives the *real* picking backend by writing a synthetic `PointerInput` at a slot's centre,
/// so the hover path can be exercised without a human moving the mouse.
fn park_pointer(
    cfg: Res<Config>,
    windows: Query<Entity, With<bevy::window::PrimaryWindow>>,
    slots: Query<(&ui::Slot, &ComputedNode, &UiGlobalTransform)>,
    mut out: MessageWriter<PointerInput>,
) {
    let Some(target_index) = cfg.hover else { return };
    let Ok(window) = windows.single() else { return };
    for (slot, node, tf) in &slots {
        if slot.index != target_index {
            continue;
        }
        // `UiGlobalTransform` is in physical px, `Location::position` in logical px. The window
        // has `scale_factor_override(1.0)`, so they coincide here.
        let _ = node;
        let pos = tf.translation;
        out.write(PointerInput::new(
            PointerId::Mouse,
            Location {
                target: NormalizedRenderTarget::Window(
                    bevy::window::WindowRef::Primary.normalize(Some(window)).unwrap(),
                ),
                position: pos,
            },
            PointerAction::Move { delta: Vec2::ZERO },
        ));
    }
}

fn collect_stats(
    time: Res<Time>,
    diagnostics: Res<DiagnosticsStore>,
    mut stats: ResMut<Stats>,
    cfg: Res<Config>,
) {
    // Skip the first 2 seconds: pipeline compilation and asset loading dominate.
    if time.elapsed_secs() < 2.0 || !cfg.bench {
        return;
    }
    if let Some(ft) = diagnostics.get(&FrameTimeDiagnosticsPlugin::FRAME_TIME)
        && let Some(v) = ft.value()
    {
        stats.samples.push(v);
    }
}

fn shot_and_exit(
    mut commands: Commands,
    time: Res<Time>,
    cfg: Res<Config>,
    mut stats: ResMut<Stats>,
    mut exit: MessageWriter<AppExit>,
    mut shot_taken: Local<bool>,
) {
    if cfg.shot && !*shot_taken && time.elapsed_secs() > 2.0 {
        *shot_taken = true;
        let name = format!(
            "{}/shots/{}",
            env!("CARGO_MANIFEST_DIR"),
            match (cfg.blur, cfg.viewport) {
                _ if cfg.hover.is_some() => "glass-hover.png",
                (true, true) => "glass.png",
                (true, false) => "glass-no-viewport.png",
                (false, _) => "glass-no-blur.png",
            }
        );
        std::fs::create_dir_all(format!("{}/shots", env!("CARGO_MANIFEST_DIR"))).ok();
        info!("capturing {name}");
        commands
            .spawn(Screenshot::primary_window())
            .observe(save_to_disk(name));
    }

    if time.elapsed_secs() > cfg.secs && !stats.done {
        stats.done = true;
        if cfg.bench && !stats.samples.is_empty() {
            let mut s = stats.samples.clone();
            s.sort_by(|a, b| a.partial_cmp(b).unwrap());
            let mean = s.iter().sum::<f64>() / s.len() as f64;
            let p50 = s[s.len() / 2];
            let p99 = s[(s.len() as f64 * 0.99) as usize % s.len()];
            info!(
                "RESULT blur={} viewport={} shadows={} div={} samples={} mean_ms={:.3} p50_ms={:.3} p99_ms={:.3} fps={:.0}",
                cfg.blur,
                cfg.viewport,
                cfg.shadows,
                cfg.backdrop_div,
                s.len(),
                mean,
                p50,
                p99,
                1000.0 / mean
            );
        }
        exit.write(AppExit::Success);
    }
}
