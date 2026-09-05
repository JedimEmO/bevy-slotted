//! Backdrop blur for glass panels (`blur` feature). Lifted from
//! `spikes/glass-ui` and ADR 0003.
//!
//! A second camera renders the scene the primary camera sees into a
//! quarter-resolution image; [`GlassPanelMaterial`] samples it with a 13-tap
//! tent, deriving its screen UV from `@builtin(position)`, so no layout data
//! ever reaches the material.

use bevy::asset::Handle;
use bevy::camera::{ClearColorConfig, RenderTarget};
use bevy::image::{Image, ImageSampler};
use bevy::prelude::*;
use bevy::render::render_resource::AsBindGroup;
use bevy::render::render_resource::TextureFormat;
use bevy::shader::ShaderRef;
use bevy::ui_render::ui_material::UiMaterial;
use bevy::window::PrimaryWindow;

/// The frosted panel material. Only styling lives here.
#[derive(AsBindGroup, Asset, TypePath, Debug, Clone)]
pub struct GlassPanelMaterial {
    /// Tint over the backdrop.
    #[uniform(0)]
    pub tint: LinearRgba,
    /// Border colour.
    #[uniform(0)]
    pub edge_color: LinearRgba,
    /// Blur radius in backdrop texels.
    #[uniform(0)]
    pub blur_radius: f32,
    /// 1.0 = sample the backdrop, 0.0 = flat tint.
    #[uniform(0)]
    pub blur_enabled: f32,
    /// Padding for uniform alignment.
    #[uniform(0)]
    pub pad: Vec2,
    /// The blurred scene.
    #[texture(1)]
    #[sampler(2)]
    pub backdrop: Handle<Image>,
}

impl UiMaterial for GlassPanelMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/glass_panel.wgsl".into()
    }
}

/// Marks the game's primary camera so the backdrop camera can follow it.
#[derive(Component, Debug, Default, Clone, Copy)]
pub struct BackdropSource;

/// The backdrop camera spawned by [`BackdropPlugin`].
#[derive(Component, Debug, Default, Clone, Copy)]
pub struct BackdropCamera;

/// Handle of the backdrop image, for the glass material.
#[derive(Resource, Debug, Clone)]
pub struct BackdropImage(pub Handle<Image>);

/// Spawns the backdrop camera and keeps its transform synced to the
/// [`BackdropSource`] camera every frame.
///
/// The backdrop camera is a `Camera3d` at order `-1`, so it draws the same
/// world one pass before the window camera. A game whose world is 2D should
/// skip this plugin and use a `Solid` panel role instead; see
/// `docs/design/phase2-notes-B.md`.
#[derive(Debug, Clone, Copy)]
pub struct BackdropPlugin {
    /// The backdrop image is the window divided by this on each axis.
    /// Four means one sixteenth of the pixels.
    pub divisor: u32,
    /// Colour the backdrop camera clears to.
    pub clear_color: Color,
}

impl Default for BackdropPlugin {
    fn default() -> Self {
        Self {
            divisor: 4,
            clear_color: Color::BLACK,
        }
    }
}

impl Plugin for BackdropPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(bevy::ui_render::prelude::UiMaterialPlugin::<
            GlassPanelMaterial,
        >::default())
            .insert_resource(BackdropConfig {
                divisor: self.divisor.max(1),
                clear_color: self.clear_color,
            })
            .add_systems(Startup, spawn_backdrop_camera)
            .add_systems(PostUpdate, sync_backdrop_camera);
    }
}

#[derive(Resource, Debug, Clone, Copy)]
struct BackdropConfig {
    divisor: u32,
    clear_color: Color,
}

fn spawn_backdrop_camera(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    config: Res<BackdropConfig>,
    window: Query<&Window, With<PrimaryWindow>>,
) {
    let Ok(window) = window.single() else {
        tracing::warn!("no primary window; backdrop camera not spawned");
        return;
    };
    let size = window.physical_size().max(UVec2::splat(config.divisor)) / config.divisor;
    let mut backdrop =
        Image::new_target_texture(size.x, size.y, TextureFormat::Rgba8UnormSrgb, None);
    // Linear filtering matters: the blur taps fall between texels of a small
    // image.
    backdrop.sampler = ImageSampler::linear();
    let backdrop = images.add(backdrop);
    commands.insert_resource(BackdropImage(backdrop.clone()));
    commands.spawn((
        Camera3d::default(),
        Camera {
            // Negative order so it runs before the window camera.
            order: -1,
            clear_color: ClearColorConfig::Custom(config.clear_color),
            ..default()
        },
        RenderTarget::Image(backdrop.into()),
        BackdropCamera,
    ));
}

/// The backdrop camera must track the source camera exactly, or the blurred
/// image will not line up with the world behind the panel.
fn sync_backdrop_camera(
    source: Query<&GlobalTransform, (With<BackdropSource>, Without<BackdropCamera>)>,
    mut backdrop: Query<&mut Transform, With<BackdropCamera>>,
) {
    let Ok(src) = source.single() else {
        return;
    };
    let target = src.compute_transform();
    for mut dst in &mut backdrop {
        if *dst != target {
            *dst = target;
        }
    }
}
