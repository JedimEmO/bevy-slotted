//! Plugin, config and system sets.

use bevy::prelude::*;
use bevy::ui::UiSystems;
use slotted_ecs::SlottedEcsSet;
use slotted_theme::SlottedThemeSet;

use crate::item::render_items;
use crate::layers::{CarriedLayer, TooltipLayer, zbands};
use crate::nav::{NavKeys, directional_nav_keys};
use crate::screen::{Injections, ScreenLayout, Screens, WidgetRegistry};
use crate::semantic::{ScreenRoot, SemanticRole, sync_accessibility};
use crate::tooltip::{TooltipParts, show_tooltip};
use crate::widgets::{register_builtins, slot_state_roles};

/// UI systems. `Input`, `Render`, `Semantics` run in `Update`; `Layout` runs
/// in `PostUpdate` after `UiSystems::Layout`.
///
/// Frame order in `Update`:
/// `SlottedUiSet::Input` -> `SlottedEcsSet::{Input, Predict, Submit, Reconcile}`
/// -> `SlottedUiSet::Render` -> `SlottedThemeSet::{Motion, Apply}` -> `SlottedUiSet::Semantics`.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SlottedUiSet {
    /// Keyboard navigation, hover tracking. Pointer observers have already
    /// triggered `SlotClicked`.
    Input,
    /// Item views, slot state roles, carried layer position.
    Render,
    /// Post-layout: `ScreenLayout` events, tooltip placement.
    Layout,
    /// AccessKit sync.
    Semantics,
}

/// Plugin options. Also inserted as a resource so systems can read it.
#[derive(Resource, Debug, Clone, PartialEq, Eq)]
pub struct SlottedUiConfig {
    /// Skip anything that needs a renderer or a real window: fonts are still
    /// measured, nothing is drawn. Set by `SlottedPlugins::headless()`.
    pub headless: bool,
    /// Spawn the carried and tooltip layer roots at startup.
    pub spawn_layers: bool,
}

impl Default for SlottedUiConfig {
    fn default() -> Self {
        Self {
            headless: false,
            spawn_layers: true,
        }
    }
}

/// Registers screens, widgets, injections, tooltip parts, the semantic
/// sync, the overlay layers and the system sets.
///
/// Requires `SlottedEcsPlugin` and `SlottedThemePlugin` (ordering) and the
/// `bevy_ui`, picking and input-focus plugins the headless group lists.
#[derive(Debug, Clone, Default)]
pub struct SlottedUiPlugin {
    /// Options.
    pub config: SlottedUiConfig,
}

impl Plugin for SlottedUiPlugin {
    fn build(&self, app: &mut App) {
        let mut widgets = WidgetRegistry::default();
        register_builtins(&mut widgets);
        app.insert_resource(widgets)
            .init_resource::<Screens>()
            .init_resource::<Injections>()
            .init_resource::<TooltipParts>()
            .init_resource::<NavKeys>()
            .insert_resource(self.config.clone())
            .add_observer(show_tooltip)
            .configure_sets(
                Update,
                (
                    SlottedUiSet::Input,
                    (
                        SlottedEcsSet::Input,
                        SlottedEcsSet::Predict,
                        SlottedEcsSet::Submit,
                        SlottedEcsSet::Reconcile,
                    ),
                    SlottedUiSet::Render,
                    (SlottedThemeSet::Motion, SlottedThemeSet::Apply),
                    SlottedUiSet::Semantics,
                )
                    .chain(),
            )
            .configure_sets(PostUpdate, SlottedUiSet::Layout.after(UiSystems::Layout))
            .add_systems(Startup, (load_registry_screens, spawn_layers))
            .add_systems(
                Update,
                (
                    directional_nav_keys.in_set(SlottedUiSet::Input),
                    (render_items, slot_state_roles).in_set(SlottedUiSet::Render),
                    sync_accessibility.in_set(SlottedUiSet::Semantics),
                ),
            )
            .add_systems(PostUpdate, emit_screen_layout.in_set(SlottedUiSet::Layout));
    }
}

fn load_registry_screens(
    registries: Option<Res<slotted_ecs::Registries>>,
    mut screens: ResMut<Screens>,
) {
    if let Some(r) = registries {
        screens.load_from_registry(&r);
    }
}

fn spawn_layers(mut commands: Commands, config: Res<SlottedUiConfig>) {
    if !config.spawn_layers {
        return;
    }
    let full = || Node {
        position_type: PositionType::Absolute,
        width: percent(100),
        height: percent(100),
        ..default()
    };
    commands.spawn((
        full(),
        GlobalZIndex(zbands::TOOLTIP),
        Pickable::IGNORE,
        TooltipLayer,
    ));
    commands.spawn((
        full(),
        GlobalZIndex(zbands::CARRIED),
        Pickable::IGNORE,
        CarriedLayer,
        SemanticRole::Carried,
    ));
}

/// Triggers [`ScreenLayout`] once per screen root, the first frame its root
/// has a non-zero `ComputedNode`.
// PHASE2-IMPL: agent B. Track "already reported" with a marker component and
// use the first Panel child's rect, not the full-window root.
fn emit_screen_layout(
    _roots: Query<(Entity, &ComputedNode), With<ScreenRoot>>,
    _commands: Commands,
) {
    let _ = ScreenLayout {
        entity: Entity::PLACEHOLDER,
        rect: Rect::default(),
    };
}
