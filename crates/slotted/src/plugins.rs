//! Plugin groups.

use bevy::app::{PluginGroup, PluginGroupBuilder};
use bevy::prelude::*;

/// Everything slotted, wired with the defaults: `LocalAuthority`, the
/// placeholder icon atlas, no theme loaded until the game sets `ActiveTheme`.
///
/// Assumes Bevy's own plugins are already added (`DefaultPlugins`, or
/// [`HeadlessBevyPlugins`]). Ports are resources holding an `Arc<dyn Port>`;
/// insert your own `Authority` or `Icons` before `add_plugins` to replace an
/// adapter.
#[derive(Debug, Clone, Default)]
pub struct SlottedPlugins {
    /// No renderer present: widgets skip anything that needs one.
    pub headless: bool,
}

impl SlottedPlugins {
    /// The headless stack: [`HeadlessBevyPlugins`] plus this group with
    /// `headless: true`. Add nothing else from Bevy.
    pub fn headless() -> HeadlessStack {
        HeadlessStack
    }
}

impl PluginGroup for SlottedPlugins {
    fn build(self) -> PluginGroupBuilder {
        #[allow(unused_mut)]
        let mut group = PluginGroupBuilder::start::<Self>().add(slotted_ecs::SlottedEcsPlugin);
        #[cfg(feature = "ui")]
        {
            group = group
                .add(slotted_theme::SlottedThemePlugin)
                .add(slotted_icons::SlottedIconsPlugin::default())
                .add(slotted_ui::SlottedUiPlugin {
                    config: slotted_ui::SlottedUiConfig {
                        headless: self.headless,
                        ..Default::default()
                    },
                });
        }
        #[cfg(feature = "browser")]
        {
            group = group.add(slotted_browser::SlottedBrowserPlugin::default());
        }
        #[cfg(feature = "script-luaur")]
        {
            group = group.add(LuaurHostPlugin);
        }
        #[cfg(feature = "packs")]
        {
            group = group.add(slotted_packs::SlottedPacksPlugin::default());
        }
        #[cfg(feature = "net")]
        {
            group = group.add(SlottedNetPlugin);
        }
        group
    }
}

/// The client end of a connection to a `slotted_net::MenuServer`, waiting to
/// be turned into an [`Authority`](slotted_ecs::Authority).
///
/// Insert one of these before adding [`SlottedPlugins`] and the group wires a
/// `RemoteAuthority` over it. Insert nothing and the group leaves the default
/// `LocalAuthority` alone, so the same binary plays single-player by simply
/// not connecting.
///
/// The transport is taken out on the first frame, which is why it sits behind
/// an `Option`: a `Transport` is not `Clone` in general, and building the
/// authority needs to own it.
#[cfg(feature = "net")]
#[derive(Resource)]
pub struct ClientTransport(pub std::sync::Mutex<Option<slotted_net::BoxedClientTransport>>);

#[cfg(feature = "net")]
impl ClientTransport {
    /// Wraps a transport ready for [`SlottedNetPlugin`] to take.
    pub fn new(
        transport: impl slotted_net::Transport<
            Out = slotted_net::ClientMessage,
            In = slotted_net::ServerMessage,
        > + 'static,
    ) -> Self {
        Self(std::sync::Mutex::new(Some(Box::new(transport))))
    }
}

/// Swaps `LocalAuthority` for a `slotted_net::RemoteAuthority` when a
/// [`ClientTransport`] resource is present.
///
/// It runs in `PreStartup`, before `slotted-ecs` inserts its default, so the
/// prediction loop never sees a local authority it would have to unlearn.
#[cfg(feature = "net")]
#[derive(Debug, Clone, Copy, Default)]
pub struct SlottedNetPlugin;

#[cfg(feature = "net")]
impl Plugin for SlottedNetPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PreStartup, wire_remote_authority);
    }
}

#[cfg(feature = "net")]
fn wire_remote_authority(
    mut commands: Commands,
    transport: Option<ResMut<ClientTransport>>,
    existing: Option<Res<slotted_ecs::Authority>>,
) {
    if existing.is_some() {
        return;
    }
    let Some(transport) = transport else {
        return;
    };
    let taken = transport
        .0
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .take();
    let Some(taken) = taken else {
        return;
    };
    commands.insert_resource(slotted_ecs::Authority::new(
        slotted_net::RemoteAuthority::new(taken),
    ));
}

/// [`HeadlessBevyPlugins`] followed by `SlottedPlugins { headless: true }`.
#[derive(Debug, Clone, Copy, Default)]
pub struct HeadlessStack;

impl PluginGroup for HeadlessStack {
    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::start::<Self>()
            .add_group(HeadlessBevyPlugins::default())
            .add_group(SlottedPlugins { headless: true })
    }
}

/// The Bevy plugins ADR 0002 proved sufficient for `bevy_ui` layout, picking,
/// focus and keyboard with no window and no GPU. `RenderPlugin`,
/// `WinitPlugin`, `bevy_ui_render` and `PointerInputPlugin` are absent.
///
/// The window is `WindowPlugin` with `ExitCondition::DontExit`; resize it
/// after build if you need another resolution.
#[derive(Debug, Clone)]
pub struct HeadlessBevyPlugins {
    /// Logical width.
    pub width: f32,
    /// Logical height.
    pub height: f32,
    /// Scale factor.
    pub scale_factor: f32,
}

impl Default for HeadlessBevyPlugins {
    fn default() -> Self {
        Self {
            width: 1280.0,
            height: 720.0,
            scale_factor: 1.0,
        }
    }
}

impl PluginGroup for HeadlessBevyPlugins {
    fn build(self) -> PluginGroupBuilder {
        use bevy::window::{ExitCondition, Window, WindowPlugin, WindowResolution};
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let physical = |v: f32| (v * self.scale_factor) as u32;
        PluginGroupBuilder::start::<Self>()
            .add(bevy::app::TaskPoolPlugin::default())
            .add(bevy::diagnostic::FrameCountPlugin)
            .add(bevy::time::TimePlugin)
            // `slotted-packs` records its lifecycle in a `States` enum.
            .add(bevy::state::app::StatesPlugin)
            .add(bevy::app::ScheduleRunnerPlugin::run_once())
            .add(bevy::transform::TransformPlugin)
            .add(bevy::asset::AssetPlugin::default())
            .add(WindowPlugin {
                primary_window: Some(Window {
                    resolution: WindowResolution::new(physical(self.width), physical(self.height))
                        .with_scale_factor_override(self.scale_factor),
                    ..Default::default()
                }),
                exit_condition: ExitCondition::DontExit,
                close_when_requested: false,
                ..Default::default()
            })
            .add(bevy::input::InputPlugin)
            .add(bevy::a11y::AccessibilityPlugin)
            .add(bevy::camera::CameraPlugin)
            .add(bevy::text::TextPlugin)
            .add(bevy::scene::ScenePlugin)
            .add(bevy::ui::UiPlugin)
            .add(bevy::picking::PickingPlugin)
            .add(bevy::picking::InteractionPlugin)
            .add(bevy::input_focus::InputFocusPlugin)
            .add(bevy::input_focus::InputDispatchPlugin)
            .add(bevy::input_focus::tab_navigation::TabNavigationPlugin)
            .add(bevy::input_focus::directional_navigation::DirectionalNavigationPlugin)
            .add_group(bevy::ui_widgets::UiWidgetsPlugins)
            .add(HeadlessRenderAssets)
    }
}

/// Registers the four asset types `bevy_render` would register, so
/// `bevy_ui`'s image sizing and `bevy_camera`'s bounds systems pass parameter
/// validation without a renderer (ADR 0002, workaround 2).
#[derive(Debug, Clone, Copy, Default)]
pub struct HeadlessRenderAssets;

impl Plugin for HeadlessRenderAssets {
    fn build(&self, app: &mut App) {
        use bevy::asset::AssetApp;
        app.init_asset::<bevy::image::Image>();
        app.init_asset::<bevy::image::TextureAtlasLayout>();
        app.init_asset::<bevy::mesh::Mesh>();
        app.init_asset::<bevy::mesh::skinning::SkinnedMeshInverseBindposes>();
    }
}

/// Inserts `slotted_packs::ScriptHost` holding a `LuaurRuntime` unless the app
/// already provided a host. Added before `SlottedPacksPlugin` so the
/// `PreStartup` load finds it.
///
/// One runtime on every target (ADR 0004), so there is nothing to choose
/// between and no per-target feature to set. A game that wants its own
/// `ScriptRuntime` inserts a `ScriptHost` before `add_plugins` and this plugin
/// leaves it alone.
#[cfg(feature = "script-luaur")]
#[derive(Debug, Clone, Copy, Default)]
pub struct LuaurHostPlugin;

#[cfg(feature = "script-luaur")]
impl Plugin for LuaurHostPlugin {
    fn build(&self, app: &mut App) {
        if !app.world().contains_resource::<slotted_packs::ScriptHost>() {
            app.insert_resource(slotted_packs::ScriptHost::new(
                slotted_script_luaur::LuaurRuntime::default(),
            ));
        }
    }
}
