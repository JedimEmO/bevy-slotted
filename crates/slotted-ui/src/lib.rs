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

pub mod def;
pub mod fluids;
pub mod hud;
#[cfg(feature = "dev")]
pub mod hud_editor;
pub mod input;
pub mod item;
pub mod layers;
pub mod loc;
pub mod motion;
pub mod nav;
pub mod plugin;
pub mod preview;
pub mod recording;
pub mod screen;
pub mod semantic;
pub mod tooltip;
pub mod widgets;

pub use def::{
    AnchorId, DataSourceId, Direction, IconButtonState as IconButtonStateDef, IconDef, Layout,
    LayoutDirection, LocKey, Orientation, ScreenDef, ScreenKind, Side, Tags, TextRole, UiNodeDef,
    ViewSubject, WidgetKind,
};
pub use fluids::{FluidDef, FluidId, Fluids};
pub use hud::{
    HudAnchor, HudAnchored, HudConfig, HudError, HudHotbar, HudLayerDef, HudLayerId,
    HudLayerPayload, HudLayerRoot, HudLayers, HudLayout, HudMenu, HudPlacement, HudUpdate,
    HudValue, NineAnchor,
};
pub use input::{DragPaint, PendingDrag, SweepQuickMove, on_slot_press, on_slot_release};
pub use item::{
    DurabilityBar, DurabilityFill, ItemCount, ItemIcon, ItemView, RarityRing, render_items,
};
pub use layers::{
    CarriedItem, CarriedLayer, ExclusionZone, Exclusions, TooltipLayer, update_carried_layer,
    zbands,
};
pub use loc::{Localization, Localizer, NoLocalization};
pub use motion::{
    FlyingItem, GestureTarget, HOVER_SCALE, MotionTarget, PRESS_SCALE, REST_SCALE, SQUASH_SCALE,
    SlotPressed, despawn_finished_flights, drop_squash, fly_to_slot, slot_motion,
};
pub use nav::{NavKeys, directional_nav_keys};
pub use plugin::{SlottedUiConfig, SlottedUiPlugin, SlottedUiSet};
pub use preview::{
    DragGhost, HintGlyphs, SlotHint, SlotPhantom, Validity, render_overlays,
    update_carried_validity, update_drag_phantoms, update_slot_hints,
};
pub use recording::{RECORDING_VERSION, RecordedButton, RecordedFrame, RecordedInput, Recording};
pub use screen::{
    Injection, Injections, ScreenClosed, ScreenLaidOut, ScreenLayout, ScreenSpawned, Screens,
    SpawnCtx, SpawnScreen, UnmatchedInjections, Widget, WidgetRegistry, close_screen,
    emit_screen_layout, spawn_screen,
};
pub use semantic::{
    AnchorNode, LocText, ScreenRoot, SemanticLabel, SemanticRole, TestId, WidgetNode,
    sync_accessibility,
};
pub use tooltip::{
    HoverStart, TooltipContent, TooltipCtx, TooltipHost, TooltipPart, TooltipParts, TooltipRequest,
    TooltipTier, TooltipUnplaced, clear_tooltip, despawn_orphan_tooltips,
};
pub use widgets::bar::{BarState, BarStyle, BarText};
pub use widgets::icon_button::{IconButtonCycle, IconButtonState};
pub use widgets::side_tab::{
    SideTabContent, SideTabHeader, SideTabPanel, SideTabState, SideTabToggle,
};
pub use widgets::tank::{
    FillNode, FillValue, PropertyBinding, TankFluid, TankFluidSource, TooltipSource,
    UnknownFluidWarned,
};
pub use widgets::viewport::{VIEWPORT_LAYER_BASE, ViewportSubject};
pub use widgets::virtual_grid::{
    VirtualCell, VirtualGridSource, VirtualGridSources, VirtualGridState,
};
pub use widgets::{RailAction, SLOT_SIZE, slot_state_roles};

/// Everything a game needs to define and open screens.
pub mod prelude {
    pub use crate::{
        AnchorId, Injection, Injections, ItemView, Layout, ScreenClosed, ScreenDef, ScreenKind,
        ScreenRoot, ScreenSpawned, Screens, SemanticLabel, SemanticRole, SlottedUiPlugin,
        SlottedUiSet, Tags, TestId, TextRole, UiNodeDef, WidgetKind, close_screen, spawn_screen,
        widgets::kinds, zbands,
    };
}
