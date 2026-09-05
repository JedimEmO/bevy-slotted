//! Phase 0 spike: driving a `bevy_ui` screen headless through real layout and real picking.
//!
//! Nothing here is meant to ship. The point is to find the exact plugin list and the exact
//! camera/window setup that makes `ComputedNode`, `UiGlobalTransform` and `bevy_picking`'s UI
//! backend behave the same way they do in a windowed app.

use std::time::Duration;

use bevy::app::{ScheduleRunnerPlugin, TaskPoolPlugin};
use bevy::asset::AssetPlugin;
use bevy::camera::{Camera, Camera2d, CameraPlugin, ComputedCameraValues, RenderTargetInfo};
use bevy::diagnostic::FrameCountPlugin;
use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::input::{ButtonState, InputPlugin};
use bevy::input_focus::directional_navigation::DirectionalNavigationPlugin;
use bevy::input_focus::tab_navigation::{TabGroup, TabIndex, TabNavigationPlugin};
use bevy::input_focus::{InputDispatchPlugin, InputFocus, InputFocusPlugin};
use bevy::picking::pointer::{
    Location, PointerAction, PointerButton, PointerId, PointerInput, PointerLocation,
};
use bevy::picking::{InteractionPlugin, PickingPlugin};
use bevy::prelude::*;
use bevy::time::{TimePlugin, TimeUpdateStrategy};
use bevy::transform::TransformPlugin;
use bevy::ui::UiPlugin;
use bevy::ui::ui_transform::UiGlobalTransform;
use bevy::window::{PrimaryWindow, WindowPlugin, WindowResolution};

/// A name we can locate nodes by. Stands in for the semantic-role locators `slotted-test` will have.
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub struct TestName(pub String);

impl TestName {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }
}

/// Set by the spike's own systems to model "the UI is still animating".
#[derive(Resource, Default, Debug)]
pub struct Busy(pub u32);

/// The minimal headless harness.
pub struct Harness {
    pub app: App,
    pub window: Entity,
    pub camera: Entity,
    pub pointer: Entity,
    pointer_pos: Vec2,
    frame_delta: Duration,
}

/// Physical resolution and scale factor of the virtual window.
#[derive(Clone, Copy, Debug)]
pub struct Screen {
    pub width: f32,
    pub height: f32,
    pub scale_factor: f32,
}

impl Default for Screen {
    fn default() -> Self {
        Self {
            width: 1280.0,
            height: 720.0,
            scale_factor: 1.0,
        }
    }
}

impl Harness {
    pub fn new(screen: Screen) -> Self {
        let mut app = App::new();

        // ---- the plugin list this spike proved sufficient -------------------------------
        // Verified by removing each entry and re-running the suite. Entries marked OPTIONAL
        // are not needed for layout, picking, focus or keyboard, but a real game will have
        // them, so the harness keeps them.
        app.add_plugins((
            // MinimalPlugins, spelled out so the list is explicit.
            TaskPoolPlugin::default(), // OPTIONAL for the UI path; asset loading wants it
            FrameCountPlugin,          // OPTIONAL
            TimePlugin,                // REQUIRED (Time<Virtual>, PickingSystems ordering)
            ScheduleRunnerPlugin::run_once(), // OPTIONAL: we drive app.update() ourselves
            TransformPlugin,          // OPTIONAL for UI (bevy_ui computes UiGlobalTransform itself)
            AssetPlugin::default(),   // REQUIRED: fonts and images are asset handles
            WindowPlugin {
                primary_window: Some(Window {
                    resolution: WindowResolution::new(
                            (screen.width * screen.scale_factor) as u32,
                            (screen.height * screen.scale_factor) as u32,
                        )
                        .with_scale_factor_override(screen.scale_factor),
                    ..default()
                }),
                exit_condition: bevy::window::ExitCondition::DontExit,
                close_when_requested: false,
                ..default()
            },
            InputPlugin,                    // REQUIRED: ButtonInput<KeyCode>, KeyboardInput message
            bevy::a11y::AccessibilityPlugin, // OPTIONAL
            CameraPlugin,                   // REQUIRED: InheritedVisibility, which picking checks
        ));
        app.add_plugins((
            bevy::text::TextPlugin, // REQUIRED: bevy_ui's text measurement systems
            UiPlugin::default(),    // brings UiPickingPlugin in with the `bevy_picking` feature
            PickingPlugin,
            InteractionPlugin,
            InputFocusPlugin,
            InputDispatchPlugin,
            TabNavigationPlugin,
            DirectionalNavigationPlugin,
            bevy::ui_widgets::UiWidgetsPlugins,
        ));

        // No `RenderPlugin`, so nothing registers the image asset that `bevy_ui`'s image sizing
        // system reads. Register it ourselves.
        app.init_asset::<Image>();
        app.init_asset::<bevy::image::TextureAtlasLayout>();
        app.init_asset::<bevy::mesh::Mesh>();
        app.init_asset::<bevy::mesh::skinning::SkinnedMeshInverseBindposes>();

        app.init_resource::<Busy>();

        // Deterministic time: every `app.update()` advances exactly one fixed step.
        let frame_delta = Duration::from_micros(16_667);
        app.insert_resource(TimeUpdateStrategy::ManualDuration(frame_delta));

        // Window entity is spawned by WindowPlugin at build time.
        let window = app
            .world_mut()
            .query_filtered::<Entity, With<PrimaryWindow>>()
            .single(app.world())
            .expect("WindowPlugin spawned a primary window");

        // Without `RenderPlugin` nothing populates `Camera::computed`, so the UI would see a
        // zero-sized render target. Fill it in by hand; this is the one piece of the setup that
        // reaches into a normally-engine-owned field.
        let camera = app
            .world_mut()
            .spawn((
                Camera2d,
                Camera {
                    computed: ComputedCameraValues {
                        target_info: Some(RenderTargetInfo {
                            physical_size: UVec2::new(
                                (screen.width * screen.scale_factor) as u32,
                                (screen.height * screen.scale_factor) as u32,
                            ),
                            scale_factor: screen.scale_factor,
                        }),
                        ..default()
                    },
                    ..default()
                },
                IsDefaultUiCamera,
            ))
            .id();

        // NOTE: `bevy_picking::hover::update_is_hovered` only ever looks at `PointerId::Mouse`,
        // so a `PointerId::Custom` pointer drives clicks and `Pressed` but never updates the
        // `Hovered` / `DirectlyHovered` components. Nothing spawns a mouse pointer for us
        // (that is `PointerInputPlugin`, which needs winit), so the harness spawns one itself.
        let pointer = app
            .world_mut()
            .spawn((PointerId::Mouse, PointerLocation::default()))
            .id();

        let mut h = Self {
            app,
            window,
            camera,
            pointer,
            pointer_pos: Vec2::new(-1.0, -1.0),
            frame_delta,
        };
        h.step(1);
        h
    }

    pub fn pointer_id(&self) -> PointerId {
        *self.app.world().get::<PointerId>(self.pointer).unwrap()
    }

    fn location(&self, pos: Vec2) -> Location {
        Location {
            target: bevy::camera::NormalizedRenderTarget::Window(
                bevy::window::WindowRef::Primary
                    .normalize(Some(self.window))
                    .unwrap(),
            ),
            position: pos,
        }
    }

    pub fn step(&mut self, frames: usize) {
        for _ in 0..frames {
            self.app.update();
        }
    }

    /// Run frames until nothing claims to be busy, or `max_frames` elapse.
    /// Returns the number of frames actually run.
    pub fn settle(&mut self, max_frames: usize) -> usize {
        for n in 0..max_frames {
            if self.app.world().resource::<Busy>().0 == 0 && n > 0 {
                return n;
            }
            self.app.update();
        }
        max_frames
    }

    pub fn frame_delta(&self) -> Duration {
        self.frame_delta
    }

    pub fn find_by_name(&mut self, name: &str) -> Option<Entity> {
        let mut q = self.app.world_mut().query::<(Entity, &TestName)>();
        q.iter(self.app.world())
            .find(|(_, n)| n.0 == name)
            .map(|(e, _)| e)
    }

    /// Logical rect of a laid-out node, in window coordinates.
    pub fn rect_of(&self, entity: Entity) -> Rect {
        let node = self
            .app
            .world()
            .get::<ComputedNode>(entity)
            .expect("entity has ComputedNode");
        let tf = self
            .app
            .world()
            .get::<UiGlobalTransform>(entity)
            .expect("entity has UiGlobalTransform");
        let size = node.size() * node.inverse_scale_factor;
        let center = tf.translation * node.inverse_scale_factor;
        Rect::from_center_size(center, size)
    }

    /// Centre of a node in logical window coordinates.
    pub fn center_of(&self, entity: Entity) -> Vec2 {
        let tf = self
            .app
            .world()
            .get::<UiGlobalTransform>(entity)
            .expect("entity has UiGlobalTransform");
        let node = self.app.world().get::<ComputedNode>(entity).unwrap();
        tf.translation * node.inverse_scale_factor
    }

    pub fn pointer_move_to(&mut self, pos: Vec2) {
        let delta = pos - self.pointer_pos;
        self.pointer_pos = pos;
        let loc = self.location(pos);
        let id = self.pointer_id();
        self.app
            .world_mut()
            .write_message(PointerInput::new(id, loc, PointerAction::Move { delta }));
        self.step(1);
    }

    pub fn pointer_press(&mut self) {
        let loc = self.location(self.pointer_pos);
        let id = self.pointer_id();
        self.app.world_mut().write_message(PointerInput::new(
            id,
            loc,
            PointerAction::Press(PointerButton::Primary),
        ));
        self.step(1);
    }

    pub fn pointer_release(&mut self) {
        let loc = self.location(self.pointer_pos);
        let id = self.pointer_id();
        self.app.world_mut().write_message(PointerInput::new(
            id,
            loc,
            PointerAction::Release(PointerButton::Primary),
        ));
        self.step(1);
    }

    pub fn click_at(&mut self, pos: Vec2) {
        self.pointer_move_to(pos);
        self.pointer_press();
        self.pointer_release();
    }

    pub fn pointer_click(&mut self, entity: Entity) {
        let pos = self.center_of(entity);
        self.click_at(pos);
    }

    /// Non-mutating lookup, so it can be used inside `h.set_focus(h.find_by_name_ref(..))`.
    pub fn find_by_name_ref(&self, name: &str) -> Option<Entity> {
        self.app
            .world()
            .iter_entities()
            .find(|e| e.get::<TestName>().is_some_and(|n| n.0 == name))
            .map(|e| e.id())
    }

    /// Hold a modifier down across later key presses.
    pub fn hold_key(&mut self, key_code: KeyCode) {
        let window = self.window;
        self.app.world_mut().write_message(KeyboardInput {
            key_code,
            logical_key: Key::Shift,
            state: ButtonState::Pressed,
            text: None,
            repeat: false,
            window,
        });
        self.step(1);
    }

    pub fn release_key(&mut self, key_code: KeyCode) {
        let window = self.window;
        self.app.world_mut().write_message(KeyboardInput {
            key_code,
            logical_key: Key::Shift,
            state: ButtonState::Released,
            text: None,
            repeat: false,
            window,
        });
        self.step(1);
    }

    /// Feed a key press+release the way winit would: a `KeyboardInput` message on the window.
    pub fn key_press(&mut self, key_code: KeyCode, logical: Key) {
        let window = self.window;
        self.app.world_mut().write_message(KeyboardInput {
            key_code,
            logical_key: logical.clone(),
            state: ButtonState::Pressed,
            text: None,
            repeat: false,
            window,
        });
        self.step(1);
        self.app.world_mut().write_message(KeyboardInput {
            key_code,
            logical_key: logical,
            state: ButtonState::Released,
            text: None,
            repeat: false,
            window,
        });
        self.step(1);
    }

    pub fn focus(&self) -> Option<Entity> {
        self.app.world().resource::<InputFocus>().get()
    }

    /// Spawn a second, independent pointer. Useful for multi-pointer tests; note the `Hovered`
    /// caveat on the `pointer` field.
    pub fn spawn_custom_pointer(&mut self) -> Entity {
        self.app
            .world_mut()
            .spawn((
                PointerId::Custom(uuid::Uuid::new_v4()),
                PointerLocation::default(),
            ))
            .id()
    }

    pub fn set_focus(&mut self, entity: Option<Entity>) {
        let mut f = self.app.world_mut().resource_mut::<InputFocus>();
        match entity {
            Some(e) => f.set(e, bevy::input_focus::FocusCause::Navigated),
            None => f.clear(),
        }
    }
}

/// Geometry of the test screen, shared between the harness and the assertions.
pub const GRID_ORIGIN: Vec2 = Vec2::new(100.0, 100.0);
pub const SLOT: f32 = 44.0;
pub const GAP: f32 = 4.0;
pub const COLS: usize = 9;
pub const ROWS: usize = 3;

pub fn expected_slot_rect(col: usize, row: usize) -> Rect {
    let min = GRID_ORIGIN + Vec2::new(col as f32 * (SLOT + GAP), row as f32 * (SLOT + GAP));
    Rect::from_corners(min, min + Vec2::splat(SLOT))
}

/// Spawns a 9x3 grid of button slots at a known position. Returns the grid root.
pub fn spawn_slot_grid(world: &mut World) -> Entity {
    let root = world
        .spawn((
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                ..default()
            },
            TestName::new("root"),
            TabGroup::new(0),
        ))
        .id();

    let grid = world
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(GRID_ORIGIN.x),
                top: Val::Px(GRID_ORIGIN.y),
                display: Display::Grid,
                grid_template_columns: vec![RepeatedGridTrack::px(COLS as u16, SLOT)],
                grid_template_rows: vec![RepeatedGridTrack::px(ROWS as u16, SLOT)],
                row_gap: Val::Px(GAP),
                column_gap: Val::Px(GAP),
                ..default()
            },
            TestName::new("grid"),
            ChildOf(root),
        ))
        .id();

    for row in 0..ROWS {
        for col in 0..COLS {
            world.spawn((
                Node {
                    width: Val::Px(SLOT),
                    height: Val::Px(SLOT),
                    ..default()
                },
                bevy::ui_widgets::Button,
                bevy::picking::hover::Hovered::default(),
                TabIndex((row * COLS + col) as i32),
                bevy::ui::auto_directional_navigation::AutoDirectionalNavigation::default(),
                TestName::new(format!("slot:{col},{row}")),
                ChildOf(grid),
            ));
        }
    }

    root
}
