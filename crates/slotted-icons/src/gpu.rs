//! The GPU bake: every icon rendered once, into a grid that *is* the atlas.
//!
//! There is no readback. One orthographic camera looks down `-Z` at a grid of
//! meshes laid out exactly on the atlas cells, and its render target is the
//! image [`AtlasIcons`](crate::AtlasIcons) hands the renderer. That is what
//! makes the path work on WebGL2, where `Readback` and buffer copies from a
//! texture are not available: nothing ever leaves the GPU.
//!
//! The rig is the fixed three-point one section 2 of
//! `docs/research/research-modern-ui.md` asks for: a key from the upper left,
//! a dim warm fill from the lower right, and a cool rim from behind. Because
//! the camera is orthographic and the lights are directional, every cell is
//! lit identically no matter where in the grid it sits.
//!
//! What a cell draws is decided once, in
//! [`CellDraw`], and read by both bakes. A
//! [`CellDraw::Shape`] becomes a `Mesh` primitive here and a set of flat
//! polygons in [`crate::shape`]; a [`CellDraw::Model`] becomes a glTF scene
//! here and its declared stand-in there. Either way the view angle comes from
//! [`shape::view_rotation`] and the size from [`shape::cell_fill`], so the two
//! pictures sit the same way in the cell.
//!
//! ## Knowing when to stop
//!
//! The rig draws into a target that keeps its contents, so it only has to
//! render once and can then switch its camera off. Deciding when that has
//! happened is the awkward part: a mesh pipeline is specialised and compiled
//! asynchronously, so the first frames after the rig appears draw nothing at
//! all, and a glTF scene is not even in the world until its asset has loaded.
//!
//! [`IconBakeProgress`] answers it with an actual signal rather than a guess.
//! A system in the render world looks at the bake camera's own render phases
//! and reports how many of their batch-set keys have a pipeline in
//! [`CachedPipelineState::Ok`]. The main world switches the camera off once
//! every key is ready and the phase is not empty, and warns and gives up
//! after [`BAKE_TIMEOUT_FRAMES`].

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

use bevy::asset::RenderAssetUsages;
use bevy::camera::primitives::MeshAabb;
use bevy::camera::visibility::RenderLayers;
use bevy::camera::{Camera, ClearColorConfig, Projection, RenderTarget, ScalingMode};
use bevy::core_pipeline::core_3d::{AlphaMask3d, Opaque3d};
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::image::Image;
use bevy::pbr::{MeshMaterial3d, StandardMaterial};
use bevy::prelude::*;
use bevy::render::render_phase::ViewBinnedRenderPhases;
use bevy::render::render_resource::{
    CachedPipelineState, CachedRenderPipelineId, PipelineCache, TextureUsages,
};
use bevy::render::sync_world::MainEntity;
use bevy::render::{Render, RenderApp, RenderSystems};
use slotted_registry::icon::{ShapeIcon, ShapeKind};
use wgpu_types::{Extent3d, TextureDimension, TextureFormat};

use crate::atlas::BakedAtlas;
use crate::shape::{self, CellDraw};

/// The render layer the bake rig lives on. Far above the viewport widget's
/// `VIEWPORT_LAYER_BASE` so the two never share a light.
pub const ICON_BAKE_LAYER: usize = 60;

/// How long the rig waits for its glTF scenes and its pipelines before it
/// gives up, warns and switches off anyway.
///
/// This is a backstop, not the mechanism: [`IconBakeProgress`] is what
/// normally stops the rig, usually within a handful of frames. Ten seconds at
/// 60 Hz is long enough that a cold shader cache on a slow machine finishes
/// first, and short enough that a genuinely stuck bake does not leave a camera
/// running for the life of the process.
pub const BAKE_TIMEOUT_FRAMES: u32 = 600;

/// Edge of one atlas cell in world units. The mesh is scaled to sit inside it
/// with a small margin, so the rim light has somewhere to fall.
const CELL_WORLD: f32 = 1.0;

/// The entities one bake spawned, so the next bake can take them down.
#[derive(Resource, Debug, Default)]
pub struct IconBakeRig {
    /// Camera, lights and meshes.
    pub entities: Vec<Entity>,
    /// The camera that draws into the atlas. Off until the warm-up pass says
    /// the pipelines are compiled, because rendering clears the target.
    pub camera: Option<Entity>,
    /// The camera that draws the same scene into a scratch image, purely to
    /// make the renderer compile the pipelines before the atlas is cleared.
    pub warmup_camera: Option<Entity>,
    /// Cells holding a glTF scene that has not been measured and fitted yet.
    pub pending_models: Vec<PendingModel>,
    /// Frames since the rig was spawned, against [`BAKE_TIMEOUT_FRAMES`].
    pub frames: u32,
    /// Whether the hand-over has happened: the warm-up camera is off and the
    /// atlas camera is on.
    pub camera_on: bool,
    /// Frames in a row the render world has reported every pipeline ready.
    pub ready_frames: u32,
    /// Set once the camera has been switched off, so the rig stops looking.
    pub finished: bool,
}

/// One glTF cell, from the frame it is spawned until its scene has loaded and
/// been fitted into its cell.
#[derive(Debug, Clone)]
pub struct PendingModel {
    /// The entity holding the cell's position, rotation and fitted scale.
    pub cell: Entity,
    /// Its child, holding the `SceneRoot`.
    pub scene: Entity,
    /// The asset path, for the warning when it never arrives.
    pub path: String,
    /// How much of the cell the fitted model should fill.
    pub fill: f32,
}

/// The signal the render world sends back: how ready the bake view's pipelines
/// are.
///
/// Shared as an `Arc` rather than extracted, because it is two counters and an
/// entity id and both worlds want it every frame. The render world only ever
/// writes; the main world only ever reads.
#[derive(Resource, Clone, Debug, Default)]
pub struct IconBakeProgress(Arc<ProgressCounters>);

#[derive(Debug, Default)]
struct ProgressCounters {
    /// `Entity::to_bits` of the bake camera, or `u64::MAX` for "no rig".
    camera: AtomicU64,
    /// Batch-set keys in the bake view's phases, last frame.
    keys: AtomicU32,
    /// How many of them had a pipeline in `CachedPipelineState::Ok`.
    ready: AtomicU32,
}

const NO_CAMERA: u64 = u64::MAX;

impl IconBakeProgress {
    /// Tells the render world which view to watch. `None` means "no rig".
    pub fn watch(&self, camera: Option<Entity>) {
        self.0
            .camera
            .store(camera.map_or(NO_CAMERA, Entity::to_bits), Ordering::Relaxed);
        self.0.keys.store(0, Ordering::Relaxed);
        self.0.ready.store(0, Ordering::Relaxed);
    }

    /// `(keys, ready)` as of the last frame the render world drew.
    #[must_use]
    pub fn counts(&self) -> (u32, u32) {
        (
            self.0.keys.load(Ordering::Relaxed),
            self.0.ready.load(Ordering::Relaxed),
        )
    }

    /// Whether the watched view drew something and every pipeline behind it
    /// is compiled.
    #[must_use]
    pub fn drew_everything(&self) -> bool {
        let (keys, ready) = self.counts();
        keys > 0 && keys == ready
    }
}

/// Adds the render-world half of [`IconBakeProgress`].
///
/// Called from [`crate::plugin::SlottedIconsPlugin::finish`], because the
/// render sub-app does not exist until `RenderPlugin` has been built.
pub fn install_progress_reporter(app: &mut App) {
    let progress = app
        .world_mut()
        .get_resource_or_insert_with(IconBakeProgress::default)
        .clone();
    let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
        // No renderer in this app; the CPU bake is what the atlas holds and
        // there is no rig to stop.
        return;
    };
    render_app
        .insert_resource(progress)
        .add_systems(Render, report_bake_progress.in_set(RenderSystems::Cleanup));
}

/// Render world: counts the bake view's batch-set keys and how many of them
/// have a compiled pipeline.
///
/// Runs in `Cleanup`, after the render node has drawn, so a frame reported
/// here is a frame that has actually been submitted.
fn report_bake_progress(
    progress: Res<IconBakeProgress>,
    cache: Res<PipelineCache>,
    opaque: Res<ViewBinnedRenderPhases<Opaque3d>>,
    alpha_mask: Res<ViewBinnedRenderPhases<AlphaMask3d>>,
) {
    let bits = progress.0.camera.load(Ordering::Relaxed);
    if bits == NO_CAMERA {
        return;
    }
    let Some(camera) = Entity::try_from_bits(bits) else {
        return;
    };
    let watched = MainEntity::from(camera);
    let mut keys = 0u32;
    let mut ready = 0u32;
    let mut count = |pipeline: CachedRenderPipelineId| {
        keys += 1;
        if matches!(
            cache.get_render_pipeline_state(pipeline),
            CachedPipelineState::Ok(_)
        ) {
            ready += 1;
        }
    };
    for (view, phase) in opaque.iter() {
        if view.main_entity != watched {
            continue;
        }
        for key in phase.multidrawable_meshes.keys() {
            count(key.pipeline);
        }
        for (key, _) in phase.batchable_meshes.keys() {
            count(key.pipeline);
        }
        for (key, _) in phase.unbatchable_meshes.keys() {
            count(key.pipeline);
        }
    }
    for (view, phase) in alpha_mask.iter() {
        if view.main_entity != watched {
            continue;
        }
        for key in phase.multidrawable_meshes.keys() {
            count(key.pipeline);
        }
        for (key, _) in phase.batchable_meshes.keys() {
            count(key.pipeline);
        }
        for (key, _) in phase.unbatchable_meshes.keys() {
            count(key.pipeline);
        }
    }
    progress.0.keys.store(keys, Ordering::Relaxed);
    progress.0.ready.store(ready, Ordering::Relaxed);
}

/// Replaces `baked`'s CPU image with a render target the rig draws into, and
/// spawns the rig. Returns the target handle, or `None` when the atlas is
/// empty.
///
/// The target starts life holding the CPU bake, so a screen drawn before the
/// rig's pipelines finish compiling shows flat-shaded icons rather than
/// nothing at all.
pub fn spawn_bake_rig(world: &mut World, baked: &BakedAtlas, cell: u32) -> Option<Handle<Image>> {
    if baked.cells.is_empty() {
        return None;
    }
    despawn_bake_rig(world);

    let size = baked.image.size();
    let cols = (size.x / cell).max(1);
    let rows = (size.y / cell).max(1);
    // The target holds the CPU bake, and keeps holding it until the rig can
    // replace it in one frame. That is the whole reason for the warm-up pass
    // below: the first frame a camera renders clears its target, so a camera
    // switched on before its pipelines are compiled turns the atlas into a
    // hole for as long as compilation takes. See `quiet_bake_rig`.
    let mut image = baked.image.clone();
    image.texture_descriptor.usage |=
        bevy::render::render_resource::TextureUsages::RENDER_ATTACHMENT;
    let target = world.resource_mut::<Assets<Image>>().add(image);

    let layers = RenderLayers::layer(ICON_BAKE_LAYER);
    let mut entities = Vec::with_capacity(baked.cells.len() + 4);

    #[allow(clippy::cast_precision_loss)]
    let (grid_w, grid_h) = (cols as f32 * CELL_WORLD, rows as f32 * CELL_WORLD);
    // Off until the warm-up camera reports that every pipeline this scene
    // needs is compiled: this camera clears the atlas when it renders, so it
    // must not render until it can draw the whole picture in the same frame.
    let camera = spawn_rig_camera(
        world,
        RigCamera {
            order: -100,
            active: false,
            target: target.clone(),
            grid: (grid_w, grid_h),
            layers: layers.clone(),
        },
    );
    entities.push(camera);

    // The warm-up camera. Same scene, same materials, same projection, so the
    // render pipelines it makes the renderer specialise are keyed identically
    // to the ones the real camera will want — but it draws into a scratch
    // image nobody ever looks at. By the time it reports everything ready,
    // the real camera can clear the atlas and refill it in the same frame,
    // which is what stops a screenshot taken during the bake from catching an
    // empty grid.
    //
    // A pipeline key does not depend on the target's size, only its format
    // and sample count, and the orthographic projection is fixed, so every
    // mesh is inside this camera's frustum no matter how small the image is.
    // One cell is enough.
    let scratch = scratch_target(world, cell, baked.image.texture_descriptor.format);
    let warmup = spawn_rig_camera(
        world,
        RigCamera {
            order: -101,
            active: true,
            target: scratch,
            grid: (grid_w, grid_h),
            layers: layers.clone(),
        },
    );
    entities.push(warmup);

    for light in rig_lights() {
        entities.push(world.spawn((light.0, light.1, layers.clone())).id());
    }

    let mut pending_models = Vec::new();
    for (cell_index, draw) in baked.cells.iter().enumerate() {
        let cell_index = u32::try_from(cell_index).unwrap_or(u32::MAX);
        #[allow(clippy::cast_precision_loss)]
        let (col, row) = ((cell_index % cols) as f32, (cell_index / cols) as f32);
        // Cell centres, in a grid whose origin is the middle of the atlas and
        // whose rows run downwards, exactly as the layout rects do.
        let x = (col + 0.5).mul_add(CELL_WORLD, -grid_w / 2.0);
        let y = grid_h / 2.0 - (row + 0.5) * CELL_WORLD;
        let kind = draw.shape().shape;
        let placement = Transform::from_xyz(x, y, 0.0)
            .with_rotation(shape::view_rotation(kind))
            .with_scale(Vec3::splat(shape::cell_fill(kind)));

        match draw {
            CellDraw::Shape(icon) => {
                spawn_shape_cell(world, &mut entities, &layers, placement, icon);
            }
            CellDraw::Model { path, stand_in } => {
                spawn_model_cell(
                    world,
                    &mut entities,
                    &mut pending_models,
                    &layers,
                    // A model is normalised to a unit cube by `fit_bake_models`,
                    // so the cell holds the placement without the fill; the
                    // fill is applied once the real extent is known.
                    placement.with_scale(Vec3::ONE),
                    path,
                    stand_in,
                    shape::cell_fill(kind),
                );
            }
        }
    }

    // The warm-up camera is the one being watched to begin with; the real one
    // takes over in `quiet_bake_rig` once the pipelines are hot.
    let progress = world
        .get_resource_or_insert_with(IconBakeProgress::default)
        .clone();
    progress.watch(Some(warmup));

    world.insert_resource(IconBakeRig {
        entities,
        camera: Some(camera),
        warmup_camera: Some(warmup),
        pending_models,
        camera_on: false,
        frames: 0,
        ready_frames: 0,
        finished: false,
    });
    Some(target)
}

/// What one of the rig's two cameras needs to be built.
struct RigCamera {
    /// Render order. Both are far behind any game camera.
    order: isize,
    /// Whether it renders from the first frame.
    active: bool,
    /// The image it draws into.
    target: Handle<Image>,
    /// Width and height of the cell grid in world units.
    grid: (f32, f32),
    /// The bake layer.
    layers: RenderLayers,
}

/// Spawns one of the rig's two cameras. They differ only in order, target and
/// whether they start active; everything else has to match, because a render
/// pipeline is keyed on the view's configuration and the whole point of the
/// warm-up pass is that it compiles the pipelines the atlas pass will want.
fn spawn_rig_camera(world: &mut World, spec: RigCamera) -> Entity {
    let (grid_w, grid_h) = spec.grid;
    world
        .spawn((
            Camera3d::default(),
            Camera {
                // Behind every game camera, and it must not clear theirs.
                order: spec.order,
                clear_color: ClearColorConfig::Custom(Color::NONE),
                is_active: spec.active,
                ..default()
            },
            // The icons are authored as sRGB colours; tone mapping would pull
            // them away from the palette the theme was chosen with.
            Tonemapping::None,
            Projection::Orthographic(OrthographicProjection {
                scaling_mode: ScalingMode::Fixed {
                    width: grid_w,
                    height: grid_h,
                },
                ..OrthographicProjection::default_3d()
            }),
            RenderTarget::Image(spec.target.into()),
            Transform::from_xyz(0.0, 0.0, 8.0).looking_at(Vec3::ZERO, Vec3::Y),
            spec.layers,
        ))
        .id()
}

/// A one-cell image with the atlas's format, for the warm-up camera to draw
/// into. Its contents are never read; only its format matters, because that
/// is what a render pipeline is keyed on.
fn scratch_target(world: &mut World, cell: u32, format: TextureFormat) -> Handle<Image> {
    let mut image = Image::new_fill(
        Extent3d {
            width: cell.max(1),
            height: cell.max(1),
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        &[0, 0, 0, 0],
        format,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.texture_descriptor.usage |= TextureUsages::RENDER_ATTACHMENT;
    world.resource_mut::<Assets<Image>>().add(image)
}

fn spawn_shape_cell(
    world: &mut World,
    entities: &mut Vec<Entity>,
    layers: &RenderLayers,
    placement: Transform,
    icon: &ShapeIcon,
) {
    let mesh = world
        .resource_mut::<Assets<Mesh>>()
        .add(mesh_of(icon.shape));
    let material = world
        .resource_mut::<Assets<StandardMaterial>>()
        .add(material_of(icon, false));
    entities.push(
        world
            .spawn((
                Mesh3d(mesh),
                MeshMaterial3d(material),
                placement,
                layers.clone(),
            ))
            .id(),
    );
    if let Some(accent) = accent_mesh(icon) {
        let mesh = world.resource_mut::<Assets<Mesh>>().add(accent.0);
        let material = world
            .resource_mut::<Assets<StandardMaterial>>()
            .add(material_of(icon, true));
        entities.push(
            world
                .spawn((
                    Mesh3d(mesh),
                    MeshMaterial3d(material),
                    placement * accent.1,
                    layers.clone(),
                ))
                .id(),
        );
    }
}

/// Spawns a glTF cell, or falls back to the stand-in shape when this build
/// has no glTF loader.
#[allow(clippy::too_many_arguments)]
fn spawn_model_cell(
    world: &mut World,
    entities: &mut Vec<Entity>,
    pending: &mut Vec<PendingModel>,
    layers: &RenderLayers,
    placement: Transform,
    path: &str,
    stand_in: &ShapeIcon,
    fill: f32,
) {
    #[cfg(feature = "gltf")]
    {
        let Some(server) = world.get_resource::<AssetServer>().cloned() else {
            tracing::warn!(
                model = path,
                "no AssetServer, so this item's model cannot be loaded; the atlas keeps its stand-in shape"
            );
            spawn_shape_cell(
                world,
                entities,
                layers,
                placement.with_scale(Vec3::splat(fill)),
                stand_in,
            );
            return;
        };
        // `WorldAsset` is what Bevy 0.19 calls a loaded glTF scene, and
        // `WorldAssetRoot` is the component that spawns one; both were named
        // `Scene` and `SceneRoot` before the term was given to BSN scenes.
        let scene: Handle<bevy::world_serialization::WorldAsset> =
            server.load(bevy::gltf::GltfAssetLabel::Scene(0).from_asset(path.to_owned()));
        // Two entities, not one. The outer one owns the cell's position, the
        // view rotation and the fitted scale; the inner one owns the offset
        // that centres the model on its own bounding box. Splitting them is
        // what lets the fit be a translation and a scale rather than a matrix
        // that has to undo the rotation first.
        let scene_entity = world
            .spawn((
                bevy::world_serialization::WorldAssetRoot(scene),
                Transform::IDENTITY,
                layers.clone(),
            ))
            .id();
        let cell_entity = world
            .spawn((placement, Visibility::Visible, layers.clone()))
            .add_child(scene_entity)
            .id();
        entities.push(cell_entity);
        entities.push(scene_entity);
        pending.push(PendingModel {
            cell: cell_entity,
            scene: scene_entity,
            path: path.to_owned(),
            fill,
        });
    }
    #[cfg(not(feature = "gltf"))]
    {
        let _ = pending;
        tracing::warn!(
            model = path,
            "this build has no glTF loader (enable slotted-icons' `gltf` feature); the item draws its stand-in shape"
        );
        spawn_shape_cell(
            world,
            entities,
            layers,
            placement.with_scale(Vec3::splat(fill)),
            stand_in,
        );
    }
}

/// Despawns whatever the last bake left in the world.
pub fn despawn_bake_rig(world: &mut World) {
    if let Some(progress) = world.get_resource::<IconBakeProgress>() {
        progress.watch(None);
    }
    let Some(rig) = world.remove_resource::<IconBakeRig>() else {
        return;
    };
    for entity in rig.entities {
        if let Ok(entity) = world.get_entity_mut(entity) {
            entity.despawn();
        }
    }
}

/// `Update`: fits any loaded glTF scene into its cell, then switches the
/// camera off once the render world says the view has drawn everything.
///
/// One exclusive system rather than three, because both halves want the rig
/// resource mutably and the fit needs to walk an arbitrary hierarchy.
pub fn quiet_bake_rig(world: &mut World) {
    if !world.contains_resource::<IconBakeRig>() {
        return;
    }
    fit_bake_models(world);

    let progress = world
        .get_resource::<IconBakeProgress>()
        .cloned()
        .unwrap_or_default();
    let mut rig = world.resource_mut::<IconBakeRig>();
    if rig.finished {
        return;
    }
    rig.frames = rig.frames.saturating_add(1);
    let timed_out = rig.frames >= BAKE_TIMEOUT_FRAMES;

    // Stage one: wait for every glTF to load and be fitted. Until then the
    // scene is incomplete, so the pipelines it needs are not all known yet
    // and the warm-up pass cannot say anything useful. The warm-up camera is
    // already running and compiling what it can.
    if !rig.pending_models.is_empty() {
        if !timed_out {
            return;
        }
        // Out of time with a model still outstanding. Neither camera has
        // touched the atlas, so it is still the CPU bake; leave it that way
        // rather than clearing it to draw a partial picture.
        warn_gave_up(&rig, &progress);
        rig.finished = true;
        progress.watch(None);
        return;
    }

    // Stage two: the warm-up camera is drawing the finished scene into a
    // scratch image. Wait until every pipeline behind it is compiled.
    if !rig.camera_on {
        if progress.drew_everything() {
            rig.ready_frames = rig.ready_frames.saturating_add(1);
        } else {
            rig.ready_frames = 0;
        }
        if rig.ready_frames < 2 && !timed_out {
            return;
        }
        if timed_out && rig.ready_frames < 2 {
            // The pipelines never came. Clearing the atlas now would only
            // replace a flat picture with an empty one.
            warn_gave_up(&rig, &progress);
            rig.finished = true;
            let warmup = rig.warmup_camera;
            progress.watch(None);
            set_camera_active(world, warmup, false);
            return;
        }
        // Hand over. The pipelines are hot, so the atlas camera clears the
        // target and refills it in the same frame.
        rig.camera_on = true;
        rig.ready_frames = 0;
        let (warmup, camera) = (rig.warmup_camera, rig.camera);
        progress.watch(camera);
        set_camera_active(world, warmup, false);
        set_camera_active(world, camera, true);
        return;
    }

    // Stage three: the atlas camera is drawing. One good frame is the picture
    // the atlas keeps; two in a row is cheap insurance on a backend that
    // binds a newly compiled pipeline a frame late.
    if progress.drew_everything() {
        rig.ready_frames = rig.ready_frames.saturating_add(1);
    } else {
        rig.ready_frames = 0;
    }
    if rig.ready_frames < 2 && !timed_out {
        return;
    }
    if timed_out && rig.ready_frames < 2 {
        warn_gave_up(&rig, &progress);
    }
    rig.finished = true;
    let camera = rig.camera;
    progress.watch(None);
    set_camera_active(world, camera, false);
}

/// Switches a rig camera on or off, if it is still there.
fn set_camera_active(world: &mut World, camera: Option<Entity>, active: bool) {
    if let Some(camera) = camera
        && let Some(mut camera) = world.get_mut::<Camera>(camera)
    {
        camera.is_active = active;
    }
}

/// One warning, naming what the rig was still waiting for when it ran out of
/// time. Whatever the atlas holds is what it keeps.
fn warn_gave_up(rig: &IconBakeRig, progress: &IconBakeProgress) {
    let (keys, ready) = progress.counts();
    let stuck: Vec<&str> = rig
        .pending_models
        .iter()
        .map(|model| model.path.as_str())
        .collect();
    tracing::warn!(
        frames = rig.frames,
        pipelines_ready = ready,
        pipelines_total = keys,
        atlas_camera_ever_active = rig.camera_on,
        models_still_loading = ?stuck,
        "the icon bake rig gave up waiting; the atlas keeps whatever it drew, \
         which is at worst the CPU bake it started from"
    );
}

/// Measures each loaded glTF scene and scales it to fill its cell.
///
/// Also puts the bake's [`RenderLayers`] on every entity the scene spawner
/// created. Bevy does not propagate render layers to children, so without
/// this a model's meshes would land on layer 0 and draw into the game's own
/// camera.
fn fit_bake_models(world: &mut World) {
    let pending = world.resource::<IconBakeRig>().pending_models.clone();
    if pending.is_empty() {
        return;
    }
    let mut still_pending = Vec::new();
    for model in pending {
        if model_load_failed(world, model.scene) {
            // A path nobody shipped. Say so now and stop waiting: the cell
            // already holds the stand-in the CPU bake drew, so the atlas is
            // right, and holding the rig open for the full timeout would only
            // delay every other icon.
            tracing::warn!(
                model = %model.path,
                "icon model failed to load; this item keeps its stand-in shape"
            );
            continue;
        }
        match measure_model(world, model.scene) {
            Some((min, max)) => {
                let extent = (max - min).max_element().max(f32::EPSILON);
                let centre = (min + max) / 2.0;
                if let Some(mut transform) = world.get_mut::<Transform>(model.scene) {
                    transform.translation = -centre;
                }
                if let Some(mut transform) = world.get_mut::<Transform>(model.cell) {
                    transform.scale = Vec3::splat(model.fill / extent);
                }
                paint_render_layers(world, model.scene);
            }
            None => still_pending.push(model),
        }
    }
    world.resource_mut::<IconBakeRig>().pending_models = still_pending;
}

/// Whether the glTF `scene` was asked for has come back as an error.
///
/// Without the `gltf` feature there is no scene entity to ask about, and the
/// answer is always "no": a model cell falls back to its stand-in at spawn
/// time and never becomes pending.
#[cfg(feature = "gltf")]
fn model_load_failed(world: &World, scene: Entity) -> bool {
    let Some(server) = world.get_resource::<AssetServer>() else {
        return false;
    };
    let Ok(entity) = world.get_entity(scene) else {
        return true;
    };
    entity
        .get::<bevy::world_serialization::WorldAssetRoot>()
        .is_some_and(|root| {
            matches!(
                server.load_state(root.0.id()),
                bevy::asset::LoadState::Failed(_)
            )
        })
}

#[cfg(not(feature = "gltf"))]
fn model_load_failed(_world: &World, _scene: Entity) -> bool {
    false
}

/// The axis-aligned bounds of everything under `scene`, in `scene`'s own
/// space, or `None` while the scene has not spawned any meshes yet.
///
/// Read from the mesh assets and the local transform chain rather than from
/// `GlobalTransform`, so the answer does not depend on whether transform
/// propagation has run since the scene spawner did its work.
fn measure_model(world: &World, scene: Entity) -> Option<(Vec3, Vec3)> {
    let meshes = world.get_resource::<Assets<Mesh>>()?;
    let mut min = Vec3::splat(f32::INFINITY);
    let mut max = Vec3::splat(f32::NEG_INFINITY);
    let mut found = false;
    let mut stack = vec![(scene, Transform::IDENTITY)];
    while let Some((entity, parent)) = stack.pop() {
        let Ok(entity_ref) = world.get_entity(entity) else {
            continue;
        };
        // The scene root's own transform is the offset this function is being
        // asked to compute, so it must not be folded in; every entity below
        // it contributes its local transform.
        let local = if entity == scene {
            parent
        } else {
            parent * entity_ref.get::<Transform>().copied().unwrap_or_default()
        };
        if let Some(mesh) = entity_ref.get::<Mesh3d>()
            && let Some(mesh) = meshes.get(&mesh.0)
            && let Some(aabb) = mesh.compute_aabb()
        {
            found = true;
            let half = Vec3::from(aabb.half_extents);
            let centre = Vec3::from(aabb.center);
            // Eight corners through the local transform: a rotated child's
            // box is not axis aligned any more.
            for corner in 0..8u8 {
                let sign = Vec3::new(
                    if corner & 1 == 0 { -1.0 } else { 1.0 },
                    if corner & 2 == 0 { -1.0 } else { 1.0 },
                    if corner & 4 == 0 { -1.0 } else { 1.0 },
                );
                let point = local.transform_point(centre + sign * half);
                min = min.min(point);
                max = max.max(point);
            }
        }
        if let Some(children) = entity_ref.get::<Children>() {
            for child in children.iter() {
                stack.push((child, local));
            }
        }
    }
    found.then_some((min, max))
}

/// Puts the bake layer on `root` and everything under it.
fn paint_render_layers(world: &mut World, root: Entity) {
    let layers = RenderLayers::layer(ICON_BAKE_LAYER);
    let mut stack = vec![root];
    while let Some(entity) = stack.pop() {
        let Ok(mut entity_ref) = world.get_entity_mut(entity) else {
            continue;
        };
        if let Some(children) = entity_ref.get::<Children>() {
            stack.extend(children.iter());
        }
        entity_ref.insert(layers.clone());
    }
}

/// The fixed three-point rig: key, fill, and a cool rim from behind.
fn rig_lights() -> [(DirectionalLight, Transform); 3] {
    [
        (
            DirectionalLight {
                illuminance: 9_000.0,
                shadow_maps_enabled: false,
                ..default()
            },
            Transform::from_xyz(-4.0, 5.5, 6.0).looking_at(Vec3::ZERO, Vec3::Y),
        ),
        (
            DirectionalLight {
                color: Color::srgb(1.0, 0.94, 0.86),
                illuminance: 2_600.0,
                shadow_maps_enabled: false,
                ..default()
            },
            Transform::from_xyz(5.0, -3.0, 4.0).looking_at(Vec3::ZERO, Vec3::Y),
        ),
        (
            DirectionalLight {
                color: Color::srgb(0.55, 0.74, 1.0),
                illuminance: 11_000.0,
                shadow_maps_enabled: false,
                ..default()
            },
            Transform::from_xyz(3.0, 2.0, -6.0).looking_at(Vec3::ZERO, Vec3::Y),
        ),
    ]
}

/// The primitive a shape renders as.
pub fn mesh_of(kind: ShapeKind) -> Mesh {
    match kind {
        ShapeKind::Cube => Cuboid::from_length(1.0).into(),
        ShapeKind::Slab => Cuboid::new(1.0, 0.5, 1.0).into(),
        ShapeKind::Ingot => Cuboid::new(1.0, 0.3, 0.55).into(),
        // Two cones base to base is an octahedron with four facets a side,
        // which is close enough to a cut gem at 64 px.
        ShapeKind::Gem => Cone {
            radius: 0.5,
            height: 0.7,
        }
        .into(),
        ShapeKind::Rod => Cylinder {
            radius: 0.06,
            half_height: 0.5,
        }
        .into(),
        ShapeKind::Sphere => Sphere::new(0.5).into(),
    }
}

/// The second mesh an `accent` adds, with the transform it takes relative to
/// the body. `None` when the shape has no accent geometry.
fn accent_mesh(shape: &ShapeIcon) -> Option<(Mesh, Transform)> {
    shape.accent?;
    match shape.shape {
        // The head of a tool, sat on the end of the shaft.
        ShapeKind::Rod => Some((
            Cuboid::new(0.42, 0.16, 0.22).into(),
            Transform::from_xyz(0.0, 0.44, 0.0),
        )),
        // An inlay band around the waist of everything else.
        ShapeKind::Cube | ShapeKind::Slab | ShapeKind::Ingot => Some((
            Cuboid::new(1.04, 0.12, 1.04).into(),
            Transform::from_xyz(0.0, -0.16, 0.0),
        )),
        // A girdle: the thin bright band around the waist of a cut stone.
        ShapeKind::Gem | ShapeKind::Sphere => Some((
            Cylinder {
                radius: 0.53,
                half_height: 0.02,
            }
            .into(),
            Transform::from_xyz(0.0, 0.0, 0.0),
        )),
    }
}

fn material_of(shape: &ShapeIcon, accent: bool) -> StandardMaterial {
    let color = if accent {
        shape.accent.unwrap_or(shape.color)
    } else {
        shape.color
    };
    let [r, g, b, a] = color.0;
    StandardMaterial {
        base_color: Color::srgba(r, g, b, a),
        metallic: shape.metallic.clamp(0.0, 1.0),
        perceptual_roughness: shape.roughness.clamp(0.05, 1.0),
        ..default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use slotted_registry::icon::IconColor;

    #[test]
    fn every_shape_has_a_mesh_and_fits_its_cell() {
        for kind in ShapeKind::ALL {
            let mesh = mesh_of(kind);
            assert!(mesh.count_vertices() > 0, "{kind} has no geometry");
            assert!(shape::cell_fill(kind) > 0.0 && shape::cell_fill(kind) < 1.0);
        }
    }

    #[test]
    fn an_accent_only_adds_geometry_when_declared() {
        let plain = ShapeIcon::new(ShapeKind::Cube, IconColor::rgb(1, 2, 3));
        assert!(accent_mesh(&plain).is_none());
        let mut fancy = plain.clone();
        fancy.accent = Some(IconColor::rgb(4, 5, 6));
        assert!(accent_mesh(&fancy).is_some());
    }

    #[test]
    fn progress_is_only_complete_when_something_was_drawn() {
        let progress = IconBakeProgress::default();
        // No rig: nothing drawn, and an empty phase is not "everything".
        assert!(!progress.drew_everything());
        progress.watch(Some(Entity::from_raw_u32(7).expect("valid")));
        assert_eq!(progress.counts(), (0, 0));
        assert!(!progress.drew_everything());
        progress.0.keys.store(3, Ordering::Relaxed);
        progress.0.ready.store(2, Ordering::Relaxed);
        assert!(
            !progress.drew_everything(),
            "one pipeline is still compiling"
        );
        progress.0.ready.store(3, Ordering::Relaxed);
        assert!(progress.drew_everything());
        // Taking the rig down stops the report and clears the counts, so a
        // stale "ready" cannot end the next bake early.
        progress.watch(None);
        assert!(!progress.drew_everything());
    }
}
