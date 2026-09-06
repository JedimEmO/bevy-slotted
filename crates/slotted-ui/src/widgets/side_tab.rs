//! `side_tab`: a tab in a `tab.rail` panel that grows to show its children.
//! Phase 6 contract section 1.3.

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
    pub open_width: f32,
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
    };
    let tokens = ctx.tokens();
    let root = ctx.spawn_node((
        Node {
            // The tab positions nothing: it is a flex child of a `tab.rail`
            // column that changes width. `Left` reverses the row so the
            // content grows away from the rail on that side.
            flex_direction: match params.side {
                Side::Right => FlexDirection::Row,
                Side::Left => FlexDirection::RowReverse,
            },
            width: px(state.width()),
            // The root's height is pinned rather than left to the content:
            // a closed tab's content still lays out (it is only hidden and
            // clipped), so an auto height would make every closed tab as
            // tall as the panel it hides. `measure_side_tabs` keeps it true.
            height: px(closed_height()),
            align_items: AlignItems::FlexStart,
            overflow: Overflow::clip(),
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

    let content = ctx.world.spawn((
        Node {
            flex_direction: FlexDirection::Column,
            row_gap: px(tokens.spacing.sm),
            padding: UiRect::all(px(tokens.spacing.sm)),
            flex_shrink: 0.0,
            ..default()
        },
        ChildOf(root),
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
/// tween with the `Motion` presets, toggles content visibility and the
/// content's `ExclusionZone`.
pub fn on_side_tab_toggle(
    event: On<SideTabToggle>,
    mut tabs: Query<(&mut SideTabState, &mut Themed, &Node, &ComputedNode)>,
    mut contents: Query<(Entity, &SideTabContent, &mut Visibility, &ComputedNode)>,
    motion: Option<Res<Motion>>,
    tokens: crate::tooltip::ThemeTokens,
    mut commands: Commands,
) {
    let tab = event.entity;
    let Ok((mut state, mut themed, node, computed)) = tabs.get_mut(tab) else {
        tracing::warn!(?tab, "SideTabToggle on something that is not a side tab");
        return;
    };
    let opening = !state.open;
    let mut content_height = 0.0f32;
    for (entity, content, mut visibility, computed) in &mut contents {
        if content.tab != tab {
            continue;
        }
        // The content's laid-out size is only known after the first layout;
        // the spawn-time guess stands until then.
        let scale = computed.inverse_scale_factor();
        let width = computed.size().x * scale;
        if width > 0.0 {
            state.open_width = state.closed_width + width;
        }
        content_height = content_height.max(computed.size().y * scale);
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

    let from = Vec2::new(
        match node.width {
            Val::Px(width) => width,
            _ => state.closed_width,
        },
        match node.height {
            Val::Px(height) => height,
            _ => computed.size().y * computed.inverse_scale_factor(),
        },
    );
    // Opening also grows the root down to fit the content; closing takes it
    // back to the header pill.
    let to_height = if state.open {
        closed_height().max(content_height + 2.0 * BORDER_WIDTH)
    } else {
        closed_height()
    };
    let to = Vec2::new(state.width(), to_height);
    let motion = motion.map_or_else(Motion::default, |m| *m);
    commands.entity(tab).insert(motion.preset_tween(
        MotionPreset::Fade,
        TweenTarget::Size { from, to },
        &tokens.get(),
    ));
}

/// Keeps every side tab's root the size its state asks for, measured from the
/// content rather than guessed.
///
/// Three things need it. A tab authored `open: true` never runs the toggle
/// observer, so nothing would ever replace `ASSUMED_CONTENT_WIDTH`. A closed
/// tab's content still takes part in layout, so the root's height has to be
/// written rather than inherited. And content that changes size after the
/// tab opened (an icon button's label, a grid that grew) would otherwise be
/// clipped forever. Tabs mid-tween are skipped so the tween owns the size.
pub fn measure_side_tabs(
    contents: Query<(&SideTabContent, &ComputedNode)>,
    mut tabs: Query<(&mut SideTabState, &mut Node), Without<slotted_theme::Tween>>,
) {
    for (content, computed) in &contents {
        let Ok((mut state, mut node)) = tabs.get_mut(content.tab) else {
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
        let want_height = if state.open {
            closed_height().max(height + 2.0 * BORDER_WIDTH)
        } else {
            closed_height()
        };
        let want_width = state.width();
        if node.width != px(want_width) {
            node.width = px(want_width);
        }
        if node.height != px(want_height) {
            node.height = px(want_height);
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
        };
        assert!((state.width() - SLOT_SIZE).abs() < f32::EPSILON);
        let open = SideTabState {
            open: true,
            ..state
        };
        assert!((open.width() - (SLOT_SIZE + ASSUMED_CONTENT_WIDTH)).abs() < f32::EPSILON);
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
