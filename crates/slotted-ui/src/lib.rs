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
pub mod input;
pub mod item;
pub mod layers;
pub mod loc;
pub mod motion;
pub mod nav;
pub mod plugin;
pub mod screen;
pub mod semantic;
pub mod tooltip;
pub mod widgets;

pub use def::{
    AnchorId, DataSourceId, Direction, IconDef, Layout, LayoutDirection, LocKey, Orientation,
    ScreenDef, ScreenKind, Side, Tags, TextRole, UiNodeDef, ViewSubject, WidgetKind,
};
pub use input::{DragPaint, on_slot_press, on_slot_release};
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
    TooltipTier, clear_tooltip, despawn_orphan_tooltips,
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
