//! Widgets and screens for slotted on `bevy_ui`.
//!
//! A screen is data: a [`ScreenDef`] holding a tree of [`UiNodeDef`]s. Rust,
//! RON and (later) Lua produce the same tree, and [`spawn_screen`] is the one
//! place that turns it into entities. Every spawned node carries a
//! [`SemanticRole`], optional [`SemanticLabel`], [`Tags`] and [`TestId`];
//! that semantic layer is what `bevy_a11y` announces and what `slotted-test`
//! locates by. This crate has no knowledge of the test harness: the harness
//! only reads components this crate would maintain anyway.
//!
//! Layers: [`zbands`] fixes the `GlobalZIndex` bands for screens, tooltips
//! and the carried stack.
//!
//! See `docs/design/phase2-contract.md`.

pub mod actions;
pub mod def;
pub mod fluids;
pub mod focus_ring;
pub mod hud;
#[cfg(feature = "dev")]
pub mod hud_editor;
pub mod input;
pub mod invalidate;
pub mod item;
pub mod layers;
pub mod loc;
pub mod motion;
pub mod nav;
pub mod plugin;
pub mod preview;
pub mod recording;
pub mod rich;
pub mod scale;
pub mod screen;
pub mod screen_asset;
pub mod semantic;
pub mod stack;
pub mod tooltip;
pub mod values;
pub mod widgets;

pub use actions::{
    ActionRepeat, InputDevice, InputMode, InputModeChanged, UiAction, UiActionClaims, UiActionEmit,
    UiActionEvent, UiBindings, emit_ui_actions, track_input_mode,
};
pub use def::{
    AnchorId, BindDef, ButtonOpts, ButtonVariant, DataSourceId, Direction,
    IconButtonState as IconButtonStateDef, IconDef, Layout, LayoutDirection, Length, LocKey,
    NavLinks, NineAnchor, Orientation, Overflow, Padding, Place, Presentation, PresentationMode,
    ScreenDef, ScreenKind, SelectOption, Side, TabDef, Tags, TextAlign, TextFilter, TextOpts,
    TextRole, ToggleStyle, Transition, UiNodeDef, ViewSubject, WidgetKind,
};
pub use fluids::{FluidDef, FluidId, Fluids};
pub use focus_ring::{FocusRing, FocusRingFrame, FocusRingState, Focusable, update_focus_ring};
pub use hud::{
    HudAnchor, HudAnchored, HudConfig, HudError, HudHotbar, HudLayerDef, HudLayerId,
    HudLayerPayload, HudLayerRoot, HudLayers, HudLayout, HudMenu, HudPlacement, HudUpdate,
    HudValue,
};
pub use input::{DragPaint, PendingDrag, SweepQuickMove, on_slot_press, on_slot_release};
pub use invalidate::{
    ChangeSet, Owner, Reconciled, ScreenDependencies, ScreenDropped, invalidate_and_respawn,
    invalidate_screens, reconcile_mod_injections, respawn_screens,
};
pub use item::{
    DurabilityBar, DurabilityFill, ItemCount, ItemIcon, ItemView, RarityRing, render_items,
};
pub use layers::{
    CarriedItem, CarriedLayer, Decorative, ExclusionZone, Exclusions, TooltipLayer,
    update_carried_layer, zbands,
};
pub use loc::{LocArgs, Localization, Localizer, NoLocalization, no_args, resolve_loc_text};
pub use motion::{
    FlyingItem, GestureTarget, HOVER_SCALE, MotionTarget, PRESS_SCALE, REST_SCALE, SQUASH_SCALE,
    SlotPressed, despawn_finished_flights, drop_squash, fly_to_slot, slot_motion,
};
pub use nav::{FocusMask, FocusedAction, dispatch_focused_actions};
pub use nav::{
    TextEntryFocused, accept_focused, directional_nav_actions, focus_on_spawn,
    track_text_entry_focus,
};
pub use plugin::{SlottedUiConfig, SlottedUiPlugin, SlottedUiSet};
pub use preview::{
    DragGhost, HintGlyphs, SlotHint, SlotPhantom, Validity, render_overlays,
    update_carried_validity, update_drag_phantoms, update_slot_hints,
};
pub use recording::{RECORDING_VERSION, RecordedButton, RecordedFrame, RecordedInput, Recording};
pub use rich::{RichError, RichRun, RichRuns, RunKind, RunStyle, key_glyph_text};
pub use scale::{UiUnits, ui_scale_of};
pub use screen::{
    Injection, Injections, MAX_INHERIT_DEPTH, ScreenClosed, ScreenLaidOut, ScreenLayout,
    ScreenSpawned, Screens, SpawnCtx, SpawnScreen, UnmatchedInjections, Widget, WidgetRegistry,
    active_tokens, close_screen, emit_screen_layout, spawn_screen,
};
pub use screen_asset::{
    FailedScreenAssets, ScreenAssetError, ScreenAssets, ScreenLoader, apply_screen_assets,
    report_failed_screen_assets,
};
pub use semantic::{
    AnchorNode, LocText, ScreenFocusHint, ScreenRoot, SemanticLabel, SemanticRole, TestId,
    WidgetNode, sync_accessibility,
};
pub use stack::{
    ClearScreens, PopScreen, PopTo, PushScreen, ScreenStack, Scrim, StackChanged, StackEntry,
    clear_screens, pop_screen, pop_to, push_screen, push_screen_at, replace_screen,
};
pub use tooltip::{
    HoverStart, TooltipContent, TooltipCtx, TooltipHost, TooltipPart, TooltipParts, TooltipRequest,
    TooltipTier, TooltipUnplaced, clear_tooltip, despawn_orphan_tooltips,
};
pub use values::{
    BindingTarget, SetValue, Value, ValueBinding, ValueChanged, ValueGuard, ValueGuards,
    ValueRefused, ValueRule, ValueRules, ValueStore,
};
pub use widgets::bar::{BarState, BarStyle, BarText};
pub use widgets::icon_button::{IconButtonCycle, IconButtonState};
pub use widgets::key_binding::{BindingChanged, KeyBindingState};
pub use widgets::list::ListState;
pub use widgets::radio_group::RadioState;
pub use widgets::scroll::ScrollPanel;
pub use widgets::select::{SelectPopup, SelectState};
pub use widgets::side_tab::{
    SideTabContent, SideTabHeader, SideTabPanel, SideTabState, SideTabToggle,
};
pub use widgets::slider::{SliderDef, SliderState};
pub use widgets::tabs::TabsState;
pub use widgets::tank::{
    FillNode, FillValue, PropertyBinding, TankFluid, TankFluidSource, TooltipSource,
    UnknownFluidWarned,
};
pub use widgets::text::{MaxLines, spawn_rich_text, spawn_text};
pub use widgets::text_field::{TextEntryRequested, TextFieldState};
pub use widgets::toggle::ToggleState;
pub use widgets::viewport::{VIEWPORT_LAYER_BASE, ViewportSubject};
pub use widgets::virtual_grid::{
    PooledCell, VirtualCell, VirtualGridSource, VirtualGridSources, VirtualGridState,
    refresh_virtual_grids,
};
pub use widgets::{RailAction, SLOT_SIZE, slot_state_roles};

/// Everything a game needs to define and open screens.
pub mod prelude {
    pub use crate::{
        AnchorId, Injection, Injections, ItemView, Layout, Presentation, PresentationMode,
        ScreenAssets, ScreenClosed, ScreenDef, ScreenKind, ScreenRoot, ScreenSpawned, ScreenStack,
        Screens, SemanticLabel, SemanticRole, SlottedUiPlugin, SlottedUiSet, Tags, TestId,
        TextRole, UiAction, UiActionEvent, UiBindings, UiNodeDef, WidgetKind, close_screen,
        pop_screen, push_screen, spawn_screen, widgets::kinds, zbands,
    };
}
