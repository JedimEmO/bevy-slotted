//! The harness: app, virtual window, camera, pointer, time.

use std::collections::{BTreeMap, HashMap};
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
use slotted_ui::spawn_screen;

use crate::fixture::{MenuFixture, Opened, ScreenSource};

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
    double_click_window: Option<Duration>,
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
            double_click_window: None,
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

    /// Override `ClickInterpreter::double_click_window`. Default 250 ms.
    ///
    /// The window is compared against accumulated `Time<Virtual>` and does
    /// not depend on how coarsely the harness steps frames.
    #[must_use]
    pub fn double_click_window(mut self, window: Duration) -> Self {
        self.double_click_window = Some(window);
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
        if let Some(window) = self.double_click_window {
            app.world_mut()
                .get_resource_or_init::<slotted_ecs::ClickInterpreter>()
                .double_click_window = window;
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
            last_rects: HashMap::new(),
            prev_rects: HashMap::new(),
            conserved: None,
        };
        h.step(1);
        h
    }
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn physical_px(logical: f32, scale: f32) -> u32 {
    (logical * scale).round() as u32
}

/// `settle()` gave up: something is still running after the frame cap.
///
/// The `Display` form names what: the pending authority round trips, every
/// tween still advancing with its progress, and every node whose rect moved on
/// the last frame. A `settle` that never terminates is almost always one of
/// those three, and guessing which costs more than printing all of them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettleTimeout {
    /// Frames run.
    pub frames: usize,
    /// `PendingRoundTrips` at the end.
    pub pending: u32,
    /// `ActiveMotions` at the end.
    pub motions: u32,
    /// Tweens still running, described.
    pub tweens: Vec<String>,
    /// Nodes whose rect changed on the last frame, described.
    pub moving: Vec<String>,
}

impl SettleTimeout {
    /// Layout was still changing when the cap was reached.
    pub fn layout_dirty(&self) -> bool {
        !self.moving.is_empty()
    }
}

impl std::fmt::Display for SettleTimeout {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "the UI did not settle within {} frames: {} pending round trip(s), {} active motion(s), {} node(s) still moving",
            self.frames,
            self.pending,
            self.motions,
            self.moving.len()
        )?;
        for tween in &self.tweens {
            write!(f, "\n  tween: {tween}")?;
        }
        for node in &self.moving {
            write!(f, "\n  moving: {node}")?;
        }
        Ok(())
    }
}

impl std::error::Error for SettleTimeout {}

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
    pub(crate) last_rects: HashMap<Entity, [u32; 4]>,
    pub(crate) prev_rects: HashMap<Entity, [u32; 4]>,
    pub(crate) conserved: Option<BTreeMap<slotted_model::ItemId, u64>>,
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
        for n in 0..self.max_settle_frames {
            self.app.update();
            let (pending, motions) = self.counters();
            let fp = self.layout_fingerprint();
            let layout_dirty = fp != self.last_layout;
            self.last_layout = fp;
            if pending == 0 && motions == 0 && !layout_dirty {
                return Ok(n + 1);
            }
        }
        let (pending, motions) = self.counters();
        Err(SettleTimeout {
            frames: self.max_settle_frames,
            pending,
            motions,
            tweens: self.running_tweens(),
            moving: self.moving_nodes(),
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

    /// The tweens that are still running, described for a failure message.
    fn running_tweens(&mut self) -> Vec<String> {
        let mut q = self
            .app
            .world_mut()
            .query::<(Entity, &slotted_theme::Tween)>();
        q.iter(self.app.world())
            .map(|(e, t)| {
                format!(
                    "{} {:?} ({:.0} of {:.0} ms elapsed)",
                    crate::locator::describe(self.app.world(), e),
                    t.target,
                    t.elapsed.as_secs_f32() * 1000.0,
                    t.duration.as_secs_f32() * 1000.0,
                )
            })
            .collect()
    }

    /// The nodes whose rect changed on the last frame, described.
    fn moving_nodes(&self) -> Vec<String> {
        self.last_rects
            .iter()
            .filter(|(e, rect)| self.prev_rects.get(e) != Some(rect))
            .map(|(e, _)| crate::locator::describe(self.world(), *e))
            .collect()
    }

    /// Every laid-out node's rect, so `settle` can see layout move and say
    /// which nodes are still moving when it gives up.
    fn rects(&mut self) -> HashMap<Entity, [u32; 4]> {
        let mut q = self.app.world_mut().query::<(
            Entity,
            &ComputedNode,
            &bevy::ui::ui_transform::UiGlobalTransform,
        )>();
        q.iter(self.app.world())
            .map(|(e, node, tf)| {
                (
                    e,
                    [
                        node.size().x.to_bits(),
                        node.size().y.to_bits(),
                        tf.translation.x.to_bits(),
                        tf.translation.y.to_bits(),
                    ],
                )
            })
            .collect()
    }

    /// Hash of every laid-out node's rect, so `settle` can see layout move.
    fn layout_fingerprint(&mut self) -> u64 {
        use std::hash::{Hash, Hasher};
        let rects = self.rects();
        let mut sorted: Vec<(Entity, [u32; 4])> = rects.iter().map(|(e, r)| (*e, *r)).collect();
        sorted.sort_by_key(|(e, _)| e.to_bits());
        self.prev_rects = std::mem::replace(&mut self.last_rects, rects);
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        sorted.hash(&mut hasher);
        hasher.finish()
    }

    /// Spawns inventory entities from `fixture`, opens the menu, resolves
    /// `screen` and spawns it bound to that menu. Runs one frame so the tree
    /// exists.
    ///
    /// `screen` is either a [`slotted_ui::ScreenKind`] already registered in
    /// [`slotted_ui::Screens`] (this panics listing the known kinds if not) or a
    /// [`slotted_ui::ScreenDef`], which is registered on the way through.
    pub fn open_screen(&mut self, screen: impl ScreenSource, fixture: impl MenuFixture) -> Opened {
        let def = fixture.def();
        let actor = fixture.actor();
        let inventories: Vec<Entity> = fixture
            .inventories()
            .into_iter()
            .map(|inv| self.app.world_mut().spawn(Inventory(inv)).id())
            .collect();
        let screen_def = screen.screen_def(self.app.world_mut());

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
        self.conserved = Some(census(self.world()));
        Opened {
            menu,
            screen,
            inventories,
        }
    }
}

/// Every item in the world, counted per kind: all `Inventory` components,
/// every menu's `Carried` stack, and the `Dropped` resource.
///
/// This is the whole of "the items that exist" as far as Phase 2 is
/// concerned. Nothing else in the harness holds a stack.
fn census(world: &World) -> BTreeMap<slotted_model::ItemId, u64> {
    let mut out: BTreeMap<slotted_model::ItemId, u64> = BTreeMap::new();
    let mut add = |stack: &slotted_model::ItemStack| {
        *out.entry(stack.id).or_default() += u64::from(stack.count);
    };
    let mut inventories = world.try_query::<&Inventory>();
    if let Some(query) = inventories.as_mut() {
        for inventory in query.iter(world) {
            for i in 0..inventory.len() {
                if let Some(stack) = inventory.get(i) {
                    add(stack);
                }
            }
        }
    }
    let mut carried = world.try_query::<&slotted_ecs::Carried>();
    if let Some(query) = carried.as_mut() {
        for held in query.iter(world) {
            if let Some(stack) = held.0.as_ref() {
                add(stack);
            }
        }
    }
    if let Some(dropped) = world.get_resource::<slotted_ecs::Dropped>() {
        for entry in &dropped.0 {
            add(&entry.stack);
        }
    }
    out
}

impl UiHarness {
    /// Every item that existed when the screen was opened still exists.
    ///
    /// Sums per-kind counts across every `Inventory` component, every menu's
    /// `Carried` stack and the `Dropped` resource, and compares that to the
    /// baseline `open_screen` captured. Panics naming the kinds that gained or
    /// lost, which is the failure mode a click-handling bug produces.
    ///
    /// Panics if no screen has been opened on this harness.
    #[track_caller]
    pub fn assert_conserved(&self) {
        let baseline = self
            .conserved
            .as_ref()
            .expect("assert_conserved needs a baseline: call open_screen first");
        let now = census(self.world());
        let mut differences = Vec::new();
        let kinds: std::collections::BTreeSet<slotted_model::ItemId> =
            baseline.keys().chain(now.keys()).copied().collect();
        for kind in kinds {
            let before = baseline.get(&kind).copied().unwrap_or(0);
            let after = now.get(&kind).copied().unwrap_or(0);
            if before != after {
                differences.push(format!("  {}: {before} -> {after}", self.item_name(kind)));
            }
        }
        assert!(
            differences.is_empty(),
            "items were created or destroyed since open_screen:\n{}",
            differences.join("\n")
        );
    }

    /// The registered name of an item id, or `#<id>` when no registries are
    /// present.
    fn item_name(&self, id: slotted_model::ItemId) -> String {
        self.world()
            .get_resource::<slotted_ecs::Registries>()
            .and_then(|r| r.items.name_of(id).map(ToString::to_string))
            .unwrap_or_else(|| format!("#{}", id.0))
    }
}
