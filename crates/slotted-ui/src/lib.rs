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
pub mod item;
pub mod layers;
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
pub use item::{ItemCount, ItemIcon, ItemView, render_items};
pub use layers::{CarriedLayer, ExclusionZone, Exclusions, TooltipLayer, zbands};
pub use nav::{NavKeys, directional_nav_keys};
pub use plugin::{SlottedUiConfig, SlottedUiPlugin, SlottedUiSet};
pub use screen::{
    Injection, Injections, ScreenClosed, ScreenLayout, ScreenSpawned, Screens, SpawnCtx,
    SpawnScreen, Widget, WidgetRegistry, close_screen, spawn_screen,
};
pub use semantic::{
    AnchorNode, ScreenRoot, SemanticLabel, SemanticRole, TestId, WidgetNode, sync_accessibility,
};
pub use tooltip::{
    TooltipContent, TooltipCtx, TooltipHost, TooltipPart, TooltipParts, TooltipRequest, TooltipTier,
};

/// Everything a game needs to define and open screens.
pub mod prelude {
    pub use crate::{
        AnchorId, Injection, Injections, ItemView, Layout, ScreenClosed, ScreenDef, ScreenKind,
        ScreenRoot, ScreenSpawned, Screens, SemanticLabel, SemanticRole, SlottedUiPlugin,
        SlottedUiSet, Tags, TestId, TextRole, UiNodeDef, WidgetKind, close_screen, spawn_screen,
        widgets::kinds, zbands,
    };
}
