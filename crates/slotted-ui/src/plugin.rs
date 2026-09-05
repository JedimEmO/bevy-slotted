//! Plugin, config and system sets.

use bevy::prelude::*;
use bevy::ui::UiSystems;
use slotted_ecs::SlottedEcsSet;
use slotted_theme::SlottedThemeSet;

use crate::input::{DragPaint, clear_drag_suppression};
use crate::item::{ItemView, on_slot_changed, render_items, spawn_item_view_children};
use crate::layers::{CarriedItem, CarriedLayer, TooltipLayer, update_carried_layer, zbands};
use crate::motion::{
    GestureTarget, clear_gesture_target, despawn_finished_flights, drop_squash, fly_to_slot,
    record_gesture_target, slot_motion,
};
use crate::nav::{NavKeys, directional_nav_keys};
use crate::screen::{Injections, Screens, WidgetRegistry, emit_screen_layout};
use crate::semantic::{SemanticRole, sync_accessibility};
use crate::tooltip::{
    TooltipParts, despawn_orphan_tooltips, place_tooltips, register_builtin_parts, show_tooltip,
    tooltip_delay,
};
use crate::widgets::{hotbar_swap_keys, register_builtins, slot_state_roles};
use slotted_theme::{Themed, roles};

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
        let mut parts = TooltipParts::default();
        register_builtin_parts(&mut parts);
        app.insert_resource(widgets)
            .insert_resource(parts)
            .init_resource::<Screens>()
            .init_resource::<Injections>()
            .init_resource::<NavKeys>()
            .init_resource::<DragPaint>()
            .init_resource::<GestureTarget>()
            .insert_resource(self.config.clone())
            .add_observer(show_tooltip)
            .add_observer(on_slot_changed)
            .add_observer(record_gesture_target)
            .add_observer(drop_squash)
            .add_observer(fly_to_slot)
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
                    (
                        directional_nav_keys,
                        hotbar_swap_keys,
                        clear_drag_suppression,
                    )
                        .in_set(SlottedUiSet::Input),
                    (
                        slot_state_roles,
                        update_carried_layer,
                        render_items,
                        slot_motion,
                        despawn_finished_flights,
                        tooltip_delay,
                        despawn_orphan_tooltips,
                        clear_gesture_target,
                    )
                        .chain()
                        .in_set(SlottedUiSet::Render),
                    sync_accessibility.in_set(SlottedUiSet::Semantics),
                ),
            )
            .add_systems(
                PostUpdate,
                (emit_screen_layout, place_tooltips).in_set(SlottedUiSet::Layout),
            );
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
    let carried = commands
        .spawn((
            full(),
            GlobalZIndex(zbands::CARRIED),
            Pickable::IGNORE,
            CarriedLayer,
            SemanticRole::Carried,
        ))
        .id();
    // The one child of the carried layer is an item view that follows the
    // pointer; it is not a slot, so it carries no `SemanticRole`.
    commands.queue(move |world: &mut World| {
        let size = crate::widgets::SLOT_SIZE;
        let item = world
            .spawn((
                Node {
                    position_type: PositionType::Absolute,
                    width: px(size),
                    height: px(size),
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::Center,
                    ..default()
                },
                Themed(roles::CARRIED),
                ItemView::default(),
                CarriedItem,
                Pickable::IGNORE,
                Visibility::Hidden,
                ChildOf(carried),
            ))
            .id();
        spawn_item_view_children(world, item);
    });
}
