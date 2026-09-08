//! Plugin, config and system sets.

use bevy::prelude::*;
use bevy::ui::UiSystems;
use slotted_ecs::SlottedEcsSet;
use slotted_theme::SlottedThemeSet;

use crate::input::{DragPaint, SweepQuickMove, clear_drag_suppression};
use crate::item::{
    ItemView, on_slot_changed, render_items, reresolve_icons_on_source_change,
    spawn_item_view_children,
};
use crate::layers::{CarriedItem, CarriedLayer, TooltipLayer, update_carried_layer, zbands};
use crate::motion::{
    GestureTarget, clear_gesture_target, despawn_finished_flights, drop_squash, fly_to_slot,
    record_gesture_target, slot_motion,
};
use crate::nav::{TextEntryFocused, directional_nav_actions};
use crate::preview::{
    DragGhost, HintGlyphs, load_hint_glyphs, render_overlays, update_carried_validity,
    update_drag_phantoms, update_slot_hints,
};
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
    /// Action emission, keyboard navigation, hover tracking. Pointer
    /// observers have already triggered `SlotClicked`.
    Input,
    /// Stack navigation: `Back` pops, focus is kept inside the top screen.
    /// Runs after `Input` and before the ecs sets (menus contract 0).
    Navigate,
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
    /// HUD layer options (Phase 6).
    pub hud: crate::hud::HudConfig,
}

impl Default for SlottedUiConfig {
    fn default() -> Self {
        Self {
            headless: false,
            spawn_layers: true,
            hud: crate::hud::HudConfig::default(),
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
    #[allow(clippy::too_many_lines)]
    fn build(&self, app: &mut App) {
        let mut widgets = WidgetRegistry::default();
        register_builtins(&mut widgets);
        let mut parts = TooltipParts::default();
        register_builtin_parts(&mut parts);
        let hotbar = crate::hud::HudHotbar::default();
        let mut hud = crate::hud::HudLayers::default();
        if self.config.hud.builtins {
            crate::hud::register_builtin_layers(&mut hud, &hotbar);
        }
        app.insert_resource(widgets)
            .insert_resource(parts)
            .insert_resource(hud)
            .insert_resource(hotbar)
            .init_resource::<crate::hud::HudMenu>()
            .init_resource::<crate::hud::HudLayout>()
            .init_resource::<crate::fluids::Fluids>()
            .init_resource::<crate::widgets::virtual_grid::VirtualGridSources>()
            .add_message::<crate::hud::HudUpdate>()
            .add_message::<crate::invalidate::ScreenDropped>()
            .add_observer(crate::widgets::tank::on_property_changed)
            .add_observer(crate::widgets::side_tab::on_side_tab_toggle)
            .add_observer(crate::widgets::icon_button::on_icon_button_cycle)
            .add_observer(crate::widgets::icon_button::on_icon_button_property)
            .init_resource::<Screens>()
            .init_resource::<crate::loc::Localization>()
            .init_resource::<crate::loc::MissingLocKeys>()
            .init_resource::<Injections>()
            .init_resource::<TextEntryFocused>()
            .init_resource::<DragPaint>()
            .init_resource::<SweepQuickMove>()
            .init_resource::<DragGhost>()
            .init_resource::<HintGlyphs>()
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
                    SlottedUiSet::Navigate,
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
            .add_systems(
                Startup,
                (load_registry_screens, spawn_layers, load_hint_glyphs),
            );
        // The `*.screen.ron` asset and its loader, when this app has assets.
        crate::screen_asset::register(app);
        // Menus M0: actions, the focus ring, the stack.
        crate::actions::build(app);
        crate::focus_ring::build(app);
        crate::stack::build(app);
        // Menus M1: values, rich text, the controls' systems.
        crate::values::build(app);
        crate::widgets::controls::build(app);
        crate::rich::build(app);
        app.add_message::<crate::widgets::key_binding::BindingChanged>()
            .add_message::<crate::widgets::text_field::TextEntryRequested>()
            .add_systems(
                Update,
                (
                    crate::nav::dispatch_focused_actions
                        .after(crate::actions::UiActionEmit)
                        .after(crate::nav::track_text_entry_focus)
                        .before(directional_nav_actions)
                        .in_set(SlottedUiSet::Input),
                    // A capture swallows the press before the dispatch sees
                    // it, so the key that ends a capture never reaches the
                    // row as an `Accept` that would start the next one.
                    crate::widgets::key_binding::capture_key_bindings
                        .after(crate::actions::UiActionEmit)
                        .before(crate::nav::dispatch_focused_actions)
                        .in_set(SlottedUiSet::Input),
                    (
                        crate::widgets::text::enforce_max_lines,
                        crate::widgets::scroll::scroll_focus_into_view,
                    )
                        .in_set(SlottedUiSet::Render),
                ),
            );
        app.add_observer(crate::nav::focus_on_spawn)
            .add_observer(crate::nav::on_slot_accept);
        // Menus M1 package C: the text field, scroll, list and tabs.
        crate::widgets::text_field::build(app);
        crate::widgets::scroll::build(app);
        crate::widgets::list::build(app);
        crate::widgets::tabs::build(app);
        #[cfg(feature = "viewport")]
        app.add_systems(
            Update,
            (
                crate::widgets::viewport::spawn_viewport_cameras,
                crate::widgets::viewport::orbit_viewport_cameras,
                crate::widgets::viewport::despawn_viewport_cameras,
            )
                .in_set(SlottedUiSet::Render),
        )
        .init_resource::<crate::widgets::viewport::ViewportLayers>();
        #[cfg(feature = "dev")]
        app.init_resource::<crate::hud_editor::HudEditMode>()
            .init_resource::<crate::hud_editor::HudEditKey>()
            .add_systems(Startup, crate::hud_editor::load_hud_layout)
            .add_systems(First, crate::recording::record_inputs)
            .add_systems(
                Update,
                (
                    (
                        crate::hud_editor::toggle_hud_edit,
                        crate::hud_editor::cancel_hud_drag,
                    )
                        .chain()
                        .after(crate::actions::UiActionEmit)
                        .in_set(SlottedUiSet::Input),
                    crate::hud_editor::apply_hud_edit_mode.in_set(SlottedUiSet::Render),
                ),
            )
            .add_systems(
                Last,
                (
                    crate::hud_editor::save_hud_layout,
                    crate::recording::flush_recording,
                ),
            );
        app.add_systems(
            Update,
            (
                (
                    crate::nav::track_text_entry_focus,
                    (directional_nav_actions, hotbar_swap_keys)
                        .after(crate::nav::track_text_entry_focus)
                        .after(crate::actions::UiActionEmit),
                    clear_drag_suppression,
                )
                    .in_set(SlottedUiSet::Input),
                (
                    slot_state_roles,
                    crate::widgets::icon_button::icon_button_roles,
                    update_drag_phantoms,
                    update_slot_hints,
                    update_carried_layer,
                    update_carried_validity,
                    render_overlays,
                    reresolve_icons_on_source_change,
                    render_items,
                    crate::loc::resolve_loc_text,
                    slot_motion,
                    despawn_finished_flights,
                    tooltip_delay,
                    despawn_orphan_tooltips,
                    clear_gesture_target,
                )
                    .chain()
                    .in_set(SlottedUiSet::Render),
                // Phase 6: property-driven fills, virtual grids, HUD.
                (
                    crate::widgets::tank::bind_properties,
                    crate::widgets::tank::render_fills,
                    crate::widgets::side_tab::measure_side_tabs,
                    crate::widgets::virtual_grid::refresh_virtual_grids,
                    crate::hud::sync_hud_layers,
                    crate::hud::reanchor_hud_layers,
                    crate::hud::hud_screen_visibility,
                    crate::hud::apply_hud_updates,
                )
                    .chain()
                    .in_set(SlottedUiSet::Render)
                    .after(clear_gesture_target),
                sync_accessibility.in_set(SlottedUiSet::Semantics),
            ),
        )
        .add_systems(PostUpdate, emit_screen_layout.in_set(SlottedUiSet::Layout))
        // Placement writes `Node`, so it has to run *before* the layout pass
        // that turns `Node` into a `UiGlobalTransform`. Running it after,
        // with the rest of `SlottedUiSet::Layout`, left every move one frame
        // late: that is the tooltip that appeared at the corner and snapped.
        .add_systems(PostUpdate, place_tooltips.before(UiSystems::Layout));
    }
}

fn load_registry_screens(
    registries: Option<Res<slotted_ecs::Registries>>,
    mut screens: ResMut<Screens>,
    mut fluids: ResMut<crate::fluids::Fluids>,
    mut hud: ResMut<crate::hud::HudLayers>,
) {
    if let Some(r) = registries {
        screens.load_from_registry(&r);
        fluids.load_from_registry(&r);
        hud.load_from_registry(&r);
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
                    // A border the validity ring can colour. `carried` itself
                    // paints none, so it costs nothing until a slot says the
                    // stack will or will not be taken.
                    border: UiRect::all(px(crate::widgets::BORDER_WIDTH)),
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
