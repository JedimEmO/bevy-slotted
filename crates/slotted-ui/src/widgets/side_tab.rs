//! `side_tab`: a tab in a `tab.rail` panel that opens to show its children.
//! Phase 6 contract section 1.3.
//!
//! # The panel does not move
//!
//! The tab's root is a flow child of the rail and it is always exactly one
//! header wide and one header tall. What opens is a separate node anchored
//! *outside* that root with `PositionType::Absolute`, so it takes no part in
//! the rail's layout and the rail's width never changes.
//!
//! Phase 6 grew the root itself instead, which widened the rail, which
//! widened the row the rail shares with the machine panel, which pushed the
//! panel sideways every time a tab opened. Contract 1.3 deviation 2 called
//! the tabs "flow children, not edge-absolute overlays"; the content is now
//! the overlay and the header stays in flow, which keeps the rail a real
//! column of buttons and leaves the panel where the player left it.

use bevy::input_focus::tab_navigation::TabIndex;
use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use slotted_theme::{Motion, MotionPreset, Themed, TweenTarget, roles};

use crate::def::{IconDef, LocKey, Side, Tags, UiNodeDef};
use crate::screen::SpawnCtx;
use crate::semantic::{SemanticLabel, SemanticRole, WidgetNode};
use crate::widgets::{BORDER_WIDTH, SLOT_SIZE};

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
    ///
    /// The root itself no longer changes width. This stays the *total* width
    /// a tab occupies while open, header plus content, which is what the
    /// harness and the contract talk about.
    pub open_width: f32,
    /// Height of the open box; measured from the content after first layout.
    pub open_height: f32,
}

impl SideTabState {
    /// The width the root has in its current state.
    pub fn width(&self) -> f32 {
        if self.open {
            self.open_width
        } else {
            self.closed_width
        }
    }
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

/// The absolutely positioned box the content opens inside, anchored to the
/// root's outer edge. This is the node the open/close tween resizes, and it
/// is the reason opening a tab moves nothing else on screen.
#[derive(Component, Debug, Clone, Copy)]
pub struct SideTabPanel {
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

/// Content width assumed until the content has been laid out once. Contract
/// 1.3: `4 * SLOT_SIZE` of root width before the first measurement.
pub const ASSUMED_CONTENT_WIDTH: f32 = 3.0 * SLOT_SIZE;

/// Content height assumed until the content has been laid out once.
pub const ASSUMED_CONTENT_HEIGHT: f32 = 3.0 * SLOT_SIZE;

/// Height of a tab header, in logical px. A header is a pill the width of a
/// slot and shorter than one, so a rail of tabs beside a panel reads as a
/// stack of icon buttons rather than a stack of slots.
pub const HEADER_HEIGHT: f32 = 28.0;

/// Height of the header's icon, in logical px.
const HEADER_ICON: f32 = 18.0;

/// The root's outer height when only the header shows.
fn closed_height() -> f32 {
    HEADER_HEIGHT + 2.0 * BORDER_WIDTH
}

/// Spawns a side tab with its header and content. Contract 1.3.
pub fn spawn_side_tab(
    ctx: &mut SpawnCtx<'_>,
    params: &SideTabParams,
    children: &[UiNodeDef],
    _tags: &Tags,
) -> Entity {
    let state = SideTabState {
        open: params.open,
        side: params.side,
        closed_width: SLOT_SIZE,
        open_width: SLOT_SIZE + ASSUMED_CONTENT_WIDTH,
        open_height: ASSUMED_CONTENT_HEIGHT,
    };
    let tokens = ctx.tokens();
    let root = ctx.spawn_node((
        Node {
            // Fixed, in both axes and in both states. The header is the only
            // flow child; the content hangs off the side absolutely, so
            // nothing the tab does can change the rail's size. `overflow` is
            // visible for exactly that reason.
            width: px(state.closed_width),
            height: px(closed_height()),
            flex_shrink: 0.0,
            align_items: AlignItems::FlexStart,
            overflow: Overflow::visible(),
            border: UiRect::all(px(BORDER_WIDTH)),
            border_radius: BorderRadius::all(px(tokens.radii.md)),
            ..default()
        },
        Themed(role_for(params.open)),
        SemanticRole::SideTab,
        WidgetNode(crate::widgets::kinds::side_tab()),
        state,
    ));

    let label = params
        .label
        .as_ref()
        .map_or_else(|| icon_label(&params.icon), |l| l.0.clone());
    let header = ctx.world.spawn((
        Node {
            width: px(SLOT_SIZE),
            height: px(HEADER_HEIGHT),
            flex_shrink: 0.0,
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            border_radius: BorderRadius::all(px(tokens.radii.sm)),
            ..default()
        },
        ChildOf(root),
        Themed(roles::TAB_SIDE_HEADER),
        SemanticRole::Button,
        SemanticLabel(label),
        SideTabHeader { tab: root },
        bevy::ui_widgets::Button,
        Hovered::default(),
        TabIndex(0),
        crate::focus_ring::Focusable,
        Pickable::default(),
        Tags::new().with("side_tab", "header"),
    ));
    let header = header.id();
    let icon = crate::widgets::icon_image(ctx.world, &params.icon);
    ctx.world.spawn((
        Node {
            width: px(HEADER_ICON),
            height: px(HEADER_ICON),
            ..default()
        },
        icon,
        Pickable::IGNORE,
        ChildOf(header),
    ));
    ctx.world.entity_mut(header).observe(on_header_activate);

    let panel = ctx
        .world
        .spawn((
            panel_node(params.side, panel_size(&state)),
            ChildOf(root),
            SideTabPanel { tab: root },
            Pickable::IGNORE,
        ))
        .id();

    let content = ctx.world.spawn((
        Node {
            flex_direction: FlexDirection::Column,
            row_gap: px(tokens.spacing.sm),
            padding: UiRect::all(px(tokens.spacing.sm)),
            // The content keeps its natural size inside a panel that may be
            // clipped to nothing, which is what lets `measure_side_tabs`
            // read a true width off a closed tab.
            flex_shrink: 0.0,
            ..default()
        },
        ChildOf(panel),
        Themed(roles::TAB_SIDE_CONTENT),
        SemanticRole::Panel,
        SideTabContent { tab: root },
        if params.open {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        },
    ));
    let content = content.id();
    if params.open {
        ctx.world
            .entity_mut(content)
            .insert(crate::layers::ExclusionZone);
    }
    ctx.spawn_children(content, children);
    root
}

/// The open box's node: absolute, anchored just outside the root on the
/// tab's side, top-aligned with the header, and clipped to its own size so a
/// half-open tab shows half its content.
fn panel_node(side: Side, extent: Vec2) -> Node {
    let mut node = Node {
        position_type: PositionType::Absolute,
        top: px(0.0),
        width: px(extent.x),
        height: px(extent.y),
        flex_direction: FlexDirection::Column,
        align_items: AlignItems::FlexStart,
        overflow: Overflow::clip(),
        ..default()
    };
    match side {
        Side::Right => node.left = Val::Percent(100.0),
        Side::Left => node.right = Val::Percent(100.0),
    }
    node
}

/// The open box's size for a state: the content's measured size while open,
/// nothing at all while closed.
fn panel_size(state: &SideTabState) -> Vec2 {
    if state.open {
        Vec2::new(
            (state.open_width - state.closed_width).max(0.0),
            state.open_height,
        )
    } else {
        Vec2::ZERO
    }
}

/// The root's role in a given state.
fn role_for(open: bool) -> slotted_theme::Role {
    if open {
        roles::TAB_SIDE_OPEN
    } else {
        roles::TAB_SIDE
    }
}

/// The header's label when the def named none: the icon's own path, which is
/// at least a stable, findable string.
fn icon_label(icon: &IconDef) -> String {
    match icon {
        IconDef::Item(name) => name.to_string(),
        IconDef::Image(path) => path.clone(),
    }
}

/// Observer on the header: `Activate` (a click, `Enter` or `Space` through
/// `bevy_ui_widgets`) becomes a [`SideTabToggle`] on the tab root.
pub fn on_header_activate(
    activate: On<bevy::ui_widgets::Activate>,
    headers: Query<&SideTabHeader>,
    mut commands: Commands,
) {
    if let Ok(header) = headers.get(activate.entity) {
        commands.trigger(SideTabToggle { entity: header.tab });
    }
}

/// Observer: flips `open`, swaps the root role, starts a `TweenTarget::Size`
/// tween on the tab's absolute panel with the `Motion` presets, toggles
/// content visibility and the content's `ExclusionZone`.
///
/// The tween is on the panel, never on the root: the root's size is what the
/// rail lays out against, and nothing a tab does may change it.
pub fn on_side_tab_toggle(
    event: On<SideTabToggle>,
    mut tabs: Query<(&mut SideTabState, &mut Themed)>,
    mut contents: Query<(Entity, &SideTabContent, &mut Visibility, &ComputedNode)>,
    panels: Query<(Entity, &SideTabPanel, &Node)>,
    motion: Option<Res<Motion>>,
    tokens: crate::tooltip::ThemeTokens,
    mut commands: Commands,
) {
    let tab = event.entity;
    let Ok((mut state, mut themed)) = tabs.get_mut(tab) else {
        tracing::warn!(?tab, "SideTabToggle on something that is not a side tab");
        return;
    };
    let opening = !state.open;
    for (entity, content, mut visibility, computed) in &mut contents {
        if content.tab != tab {
            continue;
        }
        // The content's laid-out size is only known after the first layout;
        // the spawn-time guess stands until then.
        let scale = computed.inverse_scale_factor();
        let (width, height) = (computed.size().x * scale, computed.size().y * scale);
        if width > 0.0 {
            state.open_width = state.closed_width + width;
        }
        if height > 0.0 {
            state.open_height = height;
        }
        *visibility = if opening {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        let mut entity = commands.entity(entity);
        if opening {
            // While open the content is a zone the browser and the HUD keep
            // clear of; contract 1.3 and deviation 2.
            entity.insert(crate::layers::ExclusionZone);
        } else {
            entity.remove::<crate::layers::ExclusionZone>();
        }
    }

    state.open = opening;
    let role = role_for(state.open);
    if themed.0 != role {
        themed.0 = role;
    }

    let to = panel_size(&state);
    let motion = motion.map_or_else(Motion::default, |m| *m);
    for (entity, panel, node) in &panels {
        if panel.tab != tab {
            continue;
        }
        let from = Vec2::new(px_of(node.width), px_of(node.height));
        commands.entity(entity).insert(motion.preset_tween(
            MotionPreset::Fade,
            TweenTarget::Size { from, to },
            &tokens.get(),
        ));
    }
}

/// The pixels a `Val` names, or zero for anything else. A side tab only ever
/// writes `Val::Px` on the nodes it owns.
fn px_of(val: Val) -> f32 {
    match val {
        Val::Px(v) => v,
        _ => 0.0,
    }
}

/// Keeps every side tab's open box the size its state asks for, measured from
/// the content rather than guessed.
///
/// Three things need it. A tab authored `open: true` never runs the toggle
/// observer, so nothing would ever replace [`ASSUMED_CONTENT_WIDTH`]. A
/// closed tab's content still takes part in layout, which is what makes the
/// measurement available before the tab has ever opened. And content that
/// changes size after the tab opened (an icon button's label, a grid that
/// grew) would otherwise be clipped forever. Panels mid-tween are skipped so
/// the tween owns the size.
pub fn measure_side_tabs(
    contents: Query<(&SideTabContent, &ComputedNode)>,
    mut tabs: Query<&mut SideTabState>,
    mut panels: Query<(&SideTabPanel, &mut Node), Without<slotted_theme::Tween>>,
) {
    for (content, computed) in &contents {
        let Ok(mut state) = tabs.get_mut(content.tab) else {
            continue;
        };
        let scale = computed.inverse_scale_factor();
        let (width, height) = (computed.size().x * scale, computed.size().y * scale);
        if width > 0.0 {
            let open_width = state.closed_width + width;
            if (state.open_width - open_width).abs() > 0.5 {
                state.open_width = open_width;
            }
        }
        if height > 0.0 && (state.open_height - height).abs() > 0.5 {
            state.open_height = height;
        }
        let want = panel_size(&state);
        for (panel, mut node) in &mut panels {
            if panel.tab != content.tab {
                continue;
            }
            if node.width != px(want.x) {
                node.width = px(want.x);
            }
            if node.height != px(want.y) {
                node.height = px(want.y);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_closed_tab_is_one_slot_wide_and_an_open_one_adds_its_content() {
        let state = SideTabState {
            open: false,
            side: Side::Right,
            closed_width: SLOT_SIZE,
            open_width: SLOT_SIZE + ASSUMED_CONTENT_WIDTH,
            open_height: ASSUMED_CONTENT_HEIGHT,
        };
        assert!((state.width() - SLOT_SIZE).abs() < f32::EPSILON);
        let open = SideTabState {
            open: true,
            ..state
        };
        assert!((open.width() - (SLOT_SIZE + ASSUMED_CONTENT_WIDTH)).abs() < f32::EPSILON);
    }

    /// The open box carries the content and nothing else: a closed tab's box
    /// is empty, so opening one cannot change the rail's width.
    #[test]
    fn the_open_box_holds_the_content_and_a_closed_one_holds_nothing() {
        let closed = SideTabState {
            open: false,
            side: Side::Right,
            closed_width: SLOT_SIZE,
            open_width: SLOT_SIZE + ASSUMED_CONTENT_WIDTH,
            open_height: 90.0,
        };
        assert_eq!(panel_size(&closed), Vec2::ZERO);
        let open = SideTabState {
            open: true,
            ..closed
        };
        assert_eq!(panel_size(&open), Vec2::new(ASSUMED_CONTENT_WIDTH, 90.0));
    }

    /// The box hangs off the outer edge, which is the whole mechanism.
    #[test]
    fn the_open_box_is_anchored_outside_the_root() {
        let right = panel_node(Side::Right, Vec2::new(10.0, 20.0));
        assert_eq!(right.position_type, PositionType::Absolute);
        assert_eq!(right.left, Val::Percent(100.0));
        assert_eq!(right.right, Val::Auto);
        let left = panel_node(Side::Left, Vec2::new(10.0, 20.0));
        assert_eq!(left.right, Val::Percent(100.0));
        assert_eq!(left.left, Val::Auto);
    }

    #[test]
    fn a_header_without_a_label_falls_back_to_the_icon_path() {
        assert_eq!(
            icon_label(&IconDef::Image("icons/tab.png".to_owned())),
            "icons/tab.png"
        );
    }

    #[test]
    fn the_root_role_says_whether_the_tab_is_open() {
        assert_eq!(role_for(true), roles::TAB_SIDE_OPEN);
        assert_eq!(role_for(false), roles::TAB_SIDE);
    }
}
