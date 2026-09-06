//! The 3D scene every example and every showcase scene shares: a ground
//! plane, a ring of spinning cubes, one light and one orbiting camera.
//!
//! Spawned once and never torn down. A showcase scene switch swaps screens and
//! menus; the orbit keeps going behind them, which is the visual cue that the
//! page did not reload (`docs/design/showcase-contract.md` section 1).

use bevy::prelude::*;

/// Marks the camera the UI, and the blur backdrop when there is one, follow.
#[derive(Component, Debug, Clone, Copy)]
pub struct MainCamera;

/// Rotation speed of one demo cube, in radians per second.
#[derive(Component, Debug, Clone, Copy)]
pub struct Spin(pub f32);

/// How the backdrop differs between the window and the page.
#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct BackdropConfig {
    /// Cube colours. The chest example and the playground use different rings
    /// only because they always have; nothing reads the difference.
    pub palette: [Color; 6],
    /// Shadow maps. The first thing to cost real time on WebGL2, and the
    /// scene reads the same without them, so the page says `false`.
    pub shadows: bool,
}

impl Default for BackdropConfig {
    fn default() -> Self {
        Self {
            palette: [
                Color::srgb(0.90, 0.32, 0.36),
                Color::srgb(0.32, 0.72, 0.95),
                Color::srgb(0.98, 0.75, 0.28),
                Color::srgb(0.45, 0.88, 0.55),
                Color::srgb(0.78, 0.45, 0.95),
                Color::srgb(0.98, 0.55, 0.25),
            ],
            shadows: true,
        }
    }
}

impl BackdropConfig {
    /// The page's backdrop: the playground's warmer first cube and no shadow
    /// maps.
    pub fn web() -> Self {
        Self {
            palette: [
                Color::srgb(0.90, 0.55, 0.30),
                Color::srgb(0.32, 0.72, 0.95),
                Color::srgb(0.98, 0.75, 0.28),
                Color::srgb(0.45, 0.88, 0.55),
                Color::srgb(0.78, 0.45, 0.95),
                Color::srgb(0.98, 0.42, 0.36),
            ],
            shadows: false,
        }
    }
}

/// `Startup`: the ground, the ring, the light and the camera.
///
/// The camera is spawned without the blur `BackdropSource` marker, because
/// that component lives behind the facade's `blur` feature and this crate does
/// not enable it. `examples/chest` adds the marker to
/// [`MainCamera`] itself.
pub fn setup_backdrop(
    mut commands: Commands,
    config: Option<Res<BackdropConfig>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let config = config.map_or_else(BackdropConfig::default, |c| *c);

    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(60.0, 60.0))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.10, 0.12, 0.16),
            perceptual_roughness: 0.9,
            ..default()
        })),
    ));

    let cube = meshes.add(Cuboid::new(1.6, 1.6, 1.6));
    #[allow(clippy::cast_precision_loss)]
    for (i, color) in config.palette.into_iter().enumerate() {
        let angle = i as f32 / config.palette.len() as f32 * std::f32::consts::TAU;
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
            shadow_maps_enabled: config.shadows,
            ..default()
        },
        Transform::from_xyz(6.0, 12.0, 5.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(0.0, 6.0, 14.0).looking_at(Vec3::new(0.0, 1.0, 0.0), Vec3::Y),
        MainCamera,
        IsDefaultUiCamera,
    ));
}

/// `Update`: the slow orbit. Wall time, not virtual, so a paused simulation
/// still turns.
pub fn orbit_camera(time: Res<Time>, mut cameras: Query<&mut Transform, With<MainCamera>>) {
    let t = time.elapsed_secs() * 0.18;
    for mut transform in &mut cameras {
        *transform = Transform::from_xyz(t.sin() * 14.0, 6.0, t.cos() * 14.0)
            .looking_at(Vec3::new(0.0, 1.0, 0.0), Vec3::Y);
    }
}

/// `Update`: each cube on its own axis and its own rate.
pub fn spin_cubes(time: Res<Time>, mut cubes: Query<(&mut Transform, &Spin)>) {
    for (mut transform, spin) in &mut cubes {
        transform.rotate_y(spin.0 * time.delta_secs());
        transform.rotate_x(spin.0 * 0.4 * time.delta_secs());
    }
}

/// The backdrop as a plugin: spawn it once, orbit it forever.
#[derive(Debug, Default, Clone, Copy)]
pub struct BackdropPlugin;

impl Plugin for BackdropPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup_backdrop)
            .add_systems(Update, (orbit_camera, spin_cubes));
    }
}
