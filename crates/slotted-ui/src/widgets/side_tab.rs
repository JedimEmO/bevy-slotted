//! `side_tab`: a tab in a `tab.rail` panel that grows to show its children.
//! Phase 6 contract section 1.3.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::def::{IconDef, LocKey, Side, Tags, UiNodeDef};
use crate::screen::SpawnCtx;
use crate::semantic::{SemanticRole, WidgetNode};

/// Side tab state on the root.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct SideTabState {
    /// Content visible and the root at `open_width`.
    pub open: bool,
    /// Which way the content opens.
    pub side: Side,
    /// Root width while closed (the header alone).
    pub closed_width: f32,
    /// Root width while open; measured from the content after first layout.
    pub open_width: f32,
}

/// The header button of a side tab. Carries `SemanticRole::Button`.
#[derive(Component, Debug, Clone, Copy)]
pub struct SideTabHeader {
    /// The tab root.
    pub tab: Entity,
}

/// The content panel of a side tab. `Visibility::Hidden` while closed;
/// carries `ExclusionZone` while open.
#[derive(Component, Debug, Clone, Copy)]
pub struct SideTabContent {
    /// The tab root.
    pub tab: Entity,
}

/// Toggle a side tab. Targets the tab root. The header's `Activate` observer
/// triggers it; the harness may trigger it directly.
#[derive(EntityEvent, Debug, Clone, Copy, PartialEq, Eq)]
pub struct SideTabToggle {
    /// The tab root.
    pub entity: Entity,
}

/// Parameters of `slotted:side_tab`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SideTabParams {
    /// Header icon.
    pub icon: IconDef,
    /// Which way the content opens.
    #[serde(default = "right_side")]
    pub side: Side,
    /// Header label.
    #[serde(default)]
    pub label: Option<LocKey>,
    /// Start open.
    #[serde(default)]
    pub open: bool,
}

fn right_side() -> Side {
    Side::Right
}

impl Default for SideTabParams {
    fn default() -> Self {
        Self {
            icon: IconDef::Image("icons/tab.png".to_owned()),
            side: Side::Right,
            label: None,
            open: false,
        }
    }
}

/// Spawns a side tab with its header and content. Contract 1.3.
pub fn spawn_side_tab(
    ctx: &mut SpawnCtx<'_>,
    params: &SideTabParams,
    children: &[UiNodeDef],
    _tags: &Tags,
) -> Entity {
    // PHASE6-IMPL: A. Root: Node row/row-reverse, overflow clip,
    // Themed(TAB_SIDE | TAB_SIDE_OPEN), SemanticRole::SideTab, SideTabState.
    // Header child: SemanticRole::Button, Themed(TAB_SIDE_HEADER),
    // bevy_ui_widgets::Button, TabIndex(0), Hovered, tag side_tab=header,
    // SemanticLabel, icon ImageNode child, Activate observer -> SideTabToggle.
    // Content child: SemanticRole::Panel, Themed(TAB_SIDE_CONTENT),
    // Visibility per `open`, the def's children inside.
    let root = ctx.spawn_node((
        Node::default(),
        SemanticRole::SideTab,
        WidgetNode(crate::widgets::kinds::side_tab()),
        SideTabState {
            open: params.open,
            side: params.side,
            closed_width: crate::widgets::SLOT_SIZE,
            open_width: 4.0 * crate::widgets::SLOT_SIZE,
        },
    ));
    let content = ctx.spawn_node((
        Node::default(),
        SemanticRole::Panel,
        SideTabContent { tab: root },
        if params.open {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        },
    ));
    ctx.world.entity_mut(content).insert(ChildOf(root));
    ctx.spawn_children(content, children);
    root
}

/// Observer: flips `open`, swaps the root role, starts a `TweenTarget::Size`
/// tween with the `Motion` presets, toggles content visibility and the
/// content's `ExclusionZone`.
pub fn on_side_tab_toggle(
    event: On<SideTabToggle>,
    mut tabs: Query<(&mut SideTabState, &mut slotted_theme::Themed)>,
    mut contents: Query<(Entity, &SideTabContent, &mut Visibility)>,
    mut commands: Commands,
) {
    // PHASE6-IMPL: A.
    let _ = (&event, &mut tabs, &mut contents, &mut commands);
}
