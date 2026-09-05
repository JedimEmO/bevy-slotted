//! The harness: app, virtual window, camera, pointer, time.

use std::sync::Arc;
use std::time::Duration;

use bevy::app::Plugins;
use bevy::camera::{Camera, Camera2d, ComputedCameraValues, RenderTargetInfo};
use bevy::picking::pointer::{PointerId, PointerLocation};
use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use bevy::ui::IsDefaultUiCamera;
use bevy::window::{PrimaryWindow, Window, WindowResolution};
use slotted_ecs::{Inventory, MenuIdAllocator, PendingRoundTrips, open_menu};
use slotted_theme::{ActiveMotions, Motion};
use slotted_ui::{ScreenKind, Screens, spawn_screen};

use crate::fixture::{MenuFixture, Opened};

type PluginFn = Box<dyn FnOnce(&mut App) + Send>;

/// Builds a [`UiHarness`].
pub struct UiHarnessBuilder {
    plugins: Vec<PluginFn>,
    width: f32,
    height: f32,
    scale_factor: f32,
    theme: Option<String>,
    frame_delta: Duration,
    max_settle_frames: usize,
    motion: Option<Motion>,
    registries: Option<Arc<slotted_registry::FrozenRegistries>>,
}

impl Default for UiHarnessBuilder {
    fn default() -> Self {
        Self {
            plugins: Vec::new(),
            width: 1280.0,
            height: 720.0,
            scale_factor: 1.0,
            theme: None,
            frame_delta: Duration::from_micros(16_667),
            max_settle_frames: 600,
            motion: None,
            registries: None,
        }
    }
}

impl UiHarnessBuilder {
    /// The game's plugins. Must include `SlottedPlugins::headless()` (or the
    /// equivalent Bevy plugins plus `SlottedPlugins { headless: true }`).
    /// Callable more than once; order is preserved.
    #[must_use]
    pub fn plugins<M>(mut self, plugins: impl Plugins<M> + Send + 'static) -> Self {
        self.plugins.push(Box::new(move |app| {
            app.add_plugins(plugins);
        }));
        self
    }

    /// Logical window size. Default 1280x720.
    #[must_use]
    pub fn resolution(mut self, width: f32, height: f32) -> Self {
        self.width = width;
        self.height = height;
        self
    }

    /// Window scale factor. Default 1.0.
    #[must_use]
    pub fn scale_factor(mut self, scale: f32) -> Self {
        self.scale_factor = scale;
        self
    }

    /// Theme to load from `assets/themes/<name>.theme.ron` and set active.
    #[must_use]
    pub fn theme(mut self, name: &str) -> Self {
        self.theme = Some(name.to_owned());
        self
    }

    /// Virtual time per frame. Default 1/60 s.
    #[must_use]
    pub fn frame_delta(mut self, delta: Duration) -> Self {
        self.frame_delta = delta;
        self
    }

    /// Frames `settle()` runs before failing. Default 600 (10 s virtual).
    #[must_use]
    pub fn max_settle_frames(mut self, frames: usize) -> Self {
        self.max_settle_frames = frames;
        self
    }

    /// Override the `Motion` resource; `Motion::REDUCED` makes every tween instant.
    #[must_use]
    pub fn motion(mut self, motion: Motion) -> Self {
        self.motion = Some(motion);
        self
    }

    /// Frozen registries to insert as `slotted_ecs::Registries`.
    #[must_use]
    pub fn registries(mut self, registries: Arc<slotted_registry::FrozenRegistries>) -> Self {
        self.registries = Some(registries);
        self
    }

    /// Builds the app, applies the headless workarounds, runs one frame.
    pub fn build(self) -> UiHarness {
        let mut app = App::new();
        if let Some(r) = self.registries {
            app.insert_resource(slotted_ecs::Registries(r));
        }
        for p in self.plugins {
            p(&mut app);
        }
        if let Some(m) = self.motion {
            app.insert_resource(m);
        }
        app.insert_resource(TimeUpdateStrategy::ManualDuration(self.frame_delta));

        let physical = UVec2::new(
            physical_px(self.width, self.scale_factor),
            physical_px(self.height, self.scale_factor),
        );

        // The window entity exists once WindowPlugin built. Resize it to the
        // requested resolution.
        let window = app
            .world_mut()
            .query_filtered::<Entity, With<PrimaryWindow>>()
            .single(app.world())
            .expect("the headless plugin group spawns a primary window");
        if let Some(mut w) = app.world_mut().get_mut::<Window>(window) {
            w.resolution = WindowResolution::new(physical.x, physical.y)
                .with_scale_factor_override(self.scale_factor);
        }

        // ADR 0002 workaround 1: no `camera_system`, so fill target_info by hand.
        let camera = app
            .world_mut()
            .spawn((
                Camera2d,
                Camera {
                    computed: ComputedCameraValues {
                        target_info: Some(RenderTargetInfo {
                            physical_size: physical,
                            scale_factor: self.scale_factor,
                        }),
                        ..default()
                    },
                    ..default()
                },
                IsDefaultUiCamera,
            ))
            .id();

        // ADR 0002 workaround 3: the primary pointer must be `Mouse`.
        let pointer = app
            .world_mut()
            .spawn((PointerId::Mouse, PointerLocation::default()))
            .id();

        if let Some(name) = self.theme {
            let handle = app
                .world()
                .resource::<AssetServer>()
                .load(format!("themes/{name}.theme.ron"));
            app.insert_resource(slotted_theme::ActiveTheme(handle));
        }

        let mut h = UiHarness {
            app,
            window,
            camera,
            pointer,
            pointer_pos: Vec2::new(-1.0, -1.0),
            frame_delta: self.frame_delta,
            max_settle_frames: self.max_settle_frames,
            held: Vec::new(),
            last_layout: 0,
        };
        h.step(1);
        h
    }
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn physical_px(logical: f32, scale: f32) -> u32 {
    (logical * scale).round() as u32
}

/// `settle()` gave up.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error(
    "UI did not settle within {frames} frames (pending round trips: {pending}, active motions: {motions}, layout dirty: {layout_dirty})"
)]
pub struct SettleTimeout {
    /// Frames run.
    pub frames: usize,
    /// `PendingRoundTrips` at the end.
    pub pending: u32,
    /// `ActiveMotions` at the end.
    pub motions: u32,
    /// Layout still changing.
    pub layout_dirty: bool,
}

/// A headless app with a virtual window, UI camera and pointer.
pub struct UiHarness {
    /// The app. Public for anything the harness does not wrap.
    pub app: App,
    pub(crate) window: Entity,
    pub(crate) camera: Entity,
    pub(crate) pointer: Entity,
    pub(crate) pointer_pos: Vec2,
    pub(crate) frame_delta: Duration,
    pub(crate) max_settle_frames: usize,
    pub(crate) held: Vec<KeyCode>,
    pub(crate) last_layout: u64,
}

impl UiHarness {
    /// Start here.
    pub fn builder() -> UiHarnessBuilder {
        UiHarnessBuilder::default()
    }

    /// The virtual window entity.
    pub fn window(&self) -> Entity {
        self.window
    }

    /// The UI camera entity.
    pub fn camera(&self) -> Entity {
        self.camera
    }

    /// The primary pointer entity (`PointerId::Mouse`).
    pub fn pointer(&self) -> Entity {
        self.pointer
    }

    /// Virtual time per frame.
    pub fn frame_delta(&self) -> Duration {
        self.frame_delta
    }

    /// The world.
    pub fn world(&self) -> &World {
        self.app.world()
    }

    /// The world, mutably.
    pub fn world_mut(&mut self) -> &mut World {
        self.app.world_mut()
    }

    /// Runs `frames` frames.
    pub fn step(&mut self, frames: usize) {
        for _ in 0..frames {
            self.app.update();
        }
    }

    /// Runs whole frames until at least `duration` of virtual time passed.
    pub fn advance(&mut self, duration: Duration) {
        let frames = duration
            .as_nanos()
            .div_ceil(self.frame_delta.as_nanos().max(1));
        self.step(usize::try_from(frames).unwrap_or(usize::MAX));
    }

    /// Runs frames until no authority round trip is pending, no tween is
    /// running and layout stopped changing. Panics past the frame cap.
    pub fn settle(&mut self) -> usize {
        match self.try_settle() {
            Ok(n) => n,
            Err(e) => panic!("{e}"),
        }
    }

    /// [`settle`](Self::settle) without the panic.
    pub fn try_settle(&mut self) -> Result<usize, SettleTimeout> {
        let mut quiet = 0;
        for n in 0..self.max_settle_frames {
            self.app.update();
            let (pending, motions) = self.counters();
            let fp = self.layout_fingerprint();
            let layout_dirty = fp != self.last_layout;
            self.last_layout = fp;
            if pending == 0 && motions == 0 && !layout_dirty {
                quiet += 1;
                if quiet >= 1 {
                    return Ok(n + 1);
                }
            } else {
                quiet = 0;
            }
        }
        let (pending, motions) = self.counters();
        Err(SettleTimeout {
            frames: self.max_settle_frames,
            pending,
            motions,
            layout_dirty: true,
        })
    }

    fn counters(&self) -> (u32, u32) {
        let pending = self
            .world()
            .get_resource::<PendingRoundTrips>()
            .map_or(0, |p| p.0);
        let motions = self
            .world()
            .get_resource::<ActiveMotions>()
            .map_or(0, |m| m.0);
        (pending, motions)
    }

    /// Hash of every laid-out node's rect, so `settle` can see layout move.
    fn layout_fingerprint(&mut self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        let mut q = self.app.world_mut().query::<(
            Entity,
            &ComputedNode,
            &bevy::ui::ui_transform::UiGlobalTransform,
        )>();
        for (e, node, tf) in q.iter(self.app.world()) {
            e.hash(&mut hasher);
            node.size().x.to_bits().hash(&mut hasher);
            node.size().y.to_bits().hash(&mut hasher);
            tf.translation.x.to_bits().hash(&mut hasher);
            tf.translation.y.to_bits().hash(&mut hasher);
        }
        hasher.finish()
    }

    /// Spawns inventory entities from `fixture`, opens the menu, looks `kind`
    /// up in [`Screens`] and spawns its screen bound to that menu. Runs one
    /// frame so the tree exists. Panics if `kind` is not registered.
    pub fn open_screen(&mut self, kind: ScreenKind, fixture: impl MenuFixture) -> Opened {
        let def = fixture.def();
        let actor = fixture.actor();
        let inventories: Vec<Entity> = fixture
            .inventories()
            .into_iter()
            .map(|inv| self.app.world_mut().spawn(Inventory(inv)).id())
            .collect();
        let screen_def = self
            .world()
            .resource::<Screens>()
            .get(&kind)
            .unwrap_or_else(|| panic!("screen {kind:?} is not registered in Screens"))
            .clone();

        let world = self.app.world_mut();
        let mut ids = world
            .remove_resource::<MenuIdAllocator>()
            .unwrap_or_default();
        let (menu, screen) = {
            let mut commands = world.commands();
            let menu = open_menu(&mut commands, &mut ids, def, inventories.clone(), actor);
            let screen = spawn_screen(&mut commands, screen_def, Some(menu));
            (menu, screen)
        };
        world.insert_resource(ids);
        world.flush();
        self.step(1);
        Opened {
            menu,
            screen,
            inventories,
        }
    }
}
