//! The scroll panel (menus M1 contract 4.2): a panel that clips, scrolls on
//! the wheel, follows focus and pages on `PagePrev`/`PageNext`.
//!
//! Two nodes. The outer node carries the screen's `layout` (size, `place`,
//! `grow`) and is a flex row of the viewport and, when asked for, the
//! scrollbar track; the viewport is the panel with `overflow: scroll`, Bevy's
//! [`ScrollArea`] wheel observer and the children. The scrollbar has to be a
//! sibling of the viewport rather than a child: a child scrolls away with
//! the content. The returned entity is the outer node, so tags, `test_id` and
//! the `ScrollView` role land on it; [`ScrollPanel::viewport`] names the node
//! whose `ScrollPosition` moves.

use bevy::input_focus::InputFocus;
use bevy::prelude::*;
use bevy::ui_widgets::{ControlOrientation, ScrollArea, ScrollIntoView, Scrollbar, ScrollbarThumb};
use slotted_theme::{Themed, roles};

use crate::actions::{UiAction, UiActionClaims};
use crate::def::{Layout, LayoutDirection, UiNodeDef};
use crate::nav::FocusedAction;
use crate::screen::SpawnCtx;
use crate::semantic::{SemanticRole, WidgetNode};

/// Marks a scroll panel's outer node.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScrollPanel {
    /// The node that carries `ScrollPosition` and `ScrollArea`.
    pub viewport: Entity,
}

/// Marks the viewport of a [`ScrollPanel`].
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScrollViewport {
    /// The outer node.
    pub panel: Entity,
}

/// Width of the scrollbar track, in logical px.
pub const SCROLLBAR_WIDTH: f32 = crate::widgets::virtual_grid::SCROLLBAR_WIDTH;

/// Shortest the thumb gets, in logical px.
pub const MIN_THUMB: f32 = 16.0;

/// Spawns a scroll panel and its children.
pub fn spawn_scroll(
    ctx: &mut SpawnCtx<'_>,
    layout: &Layout,
    scrollbar: bool,
    children: &[UiNodeDef],
) -> Entity {
    let tokens = ctx.tokens();
    let spacing = tokens.spacing.sm;
    let invisible = slotted_theme::Role::new_static("invisible");

    // The outer node: the screen's geometry, no gap, no padding, a row.
    let mut outer = crate::widgets::layout_node(
        &Layout {
            direction: LayoutDirection::Row,
            gap: 0.0,
            padding: crate::def::Padding::default(),
            overflow: crate::def::Overflow::Visible,
            wrap: false,
            ..layout.clone()
        },
        spacing,
    );
    outer.border = UiRect::ZERO;
    outer.align_items = AlignItems::Stretch;
    outer.justify_content = JustifyContent::FlexStart;
    outer.min_height = if layout.min_height.is_some() {
        outer.min_height
    } else {
        Val::Px(0.0)
    };
    let entity = ctx.spawn_node((
        outer,
        Themed(invisible.clone()),
        SemanticRole::ScrollView,
        WidgetNode(crate::widgets::kinds::scroll()),
    ));
    if let Some(place) = layout.place {
        ctx.world
            .entity_mut(entity)
            .insert(crate::def::nine_anchor_transform(place.anchor, 1.0));
    }

    // The viewport: the panel that scrolls, with the children.
    let viewport_layout = Layout {
        overflow: crate::def::Overflow::Scroll,
        width: None,
        height: None,
        min_width: None,
        max_width: None,
        min_height: None,
        max_height: None,
        grow: 1.0,
        place: None,
        ..layout.clone()
    };
    let parent = std::mem::replace(&mut ctx.parent, entity);
    let viewport = crate::widgets::spawn_panel(ctx, &invisible, &viewport_layout, children);
    ctx.parent = parent;
    {
        let mut v = ctx.world.entity_mut(viewport);
        if let Some(mut node) = v.get_mut::<Node>() {
            // No border: the theme paints none on an invisible panel, and
            // Bevy's `ScrollIntoView` measures from the node's edge, so a
            // border would leave the first child one pixel scrolled.
            node.border = UiRect::ZERO;
            node.flex_grow = 1.0;
            node.flex_shrink = 1.0;
            node.flex_basis = Val::Px(0.0);
            node.min_width = Val::Px(0.0);
            node.min_height = Val::Px(0.0);
            node.align_self = AlignSelf::Stretch;
        }
        v.insert((
            ScrollArea,
            ScrollPosition::default(),
            ScrollViewport { panel: entity },
            Pickable::default(),
        ));
    }
    ctx.world
        .entity_mut(entity)
        .insert(ScrollPanel { viewport });
    // Direct children keep their size: Taffy shrinks flex children to fit
    // by default, which is the M0 follow-up this closes.
    let kids: Vec<Entity> = ctx
        .world
        .get::<Children>(viewport)
        .map(|c| c.iter().collect())
        .unwrap_or_default();
    for kid in kids {
        if let Some(mut node) = ctx.world.get_mut::<Node>(kid) {
            node.flex_shrink = 0.0;
        }
    }

    if scrollbar {
        spawn_scrollbar(ctx, entity, viewport, tokens.radii.sm);
    }
    entity
}

/// The scrollbar track and thumb, as the outer node's last child.
fn spawn_scrollbar(ctx: &mut SpawnCtx<'_>, outer: Entity, viewport: Entity, radius: f32) {
    let track = ctx
        .world
        .spawn((
            Node {
                width: Val::Px(SCROLLBAR_WIDTH),
                flex_shrink: 0.0,
                align_self: AlignSelf::Stretch,
                border_radius: BorderRadius::all(Val::Px(radius)),
                ..default()
            },
            Themed(roles::SCROLL_BAR),
            SemanticRole::Custom("scrollbar".to_owned()),
            Scrollbar::new(viewport, ControlOrientation::Vertical, MIN_THUMB),
            Pickable::default(),
            ChildOf(outer),
        ))
        .id();
    ctx.world.spawn((
        ScrollbarThumb {
            border_radius: BorderRadius::all(Val::Px(radius)),
            border: UiRect::ZERO,
        },
        Themed(roles::SCROLL_THUMB),
        Pickable::default(),
        ChildOf(track),
    ));
}

/// The nearest `ScrollArea` at or above `entity`.
fn scroll_area_of(
    entity: Entity,
    parents: &Query<&ChildOf>,
    areas: &Query<(), With<ScrollArea>>,
) -> Option<Entity> {
    let mut current = entity;
    loop {
        if areas.contains(current) {
            return Some(current);
        }
        current = parents.get(current).ok()?.parent();
    }
}

/// `SlottedUiSet::Render`: brings the focused node into view when it sits
/// under a scroll area and focus moved this frame.
pub fn scroll_focus_into_view(
    focus: Option<Res<InputFocus>>,
    parents: Query<&ChildOf>,
    areas: Query<(), With<ScrollArea>>,
    mut last: Local<Option<Entity>>,
    mut commands: Commands,
) {
    let focused = focus.and_then(|f| f.get());
    if *last == focused {
        return;
    }
    *last = focused;
    let Some(focused) = focused else {
        return;
    };
    if scroll_area_of(focused, &parents, &areas).is_none() {
        return;
    }
    commands.trigger(ScrollIntoView { entity: focused });
}

/// Observer: `PagePrev`/`PageNext` on a focused descendant of a scroll area
/// scrolls it by the visible height and claims the action.
pub fn on_scroll_page(
    action: On<FocusedAction>,
    parents: Query<&ChildOf>,
    areas: Query<(), With<ScrollArea>>,
    mut viewports: Query<(&Node, &ComputedNode, &mut ScrollPosition), With<ScrollArea>>,
    mut claims: ResMut<UiActionClaims>,
) {
    let direction = match action.action {
        UiAction::PagePrev => -1.0,
        UiAction::PageNext => 1.0,
        _ => return,
    };
    let Some(area) = scroll_area_of(action.entity, &parents, &areas) else {
        return;
    };
    let Ok((node, computed, mut position)) = viewports.get_mut(area) else {
        return;
    };
    let visible = computed.size() * computed.inverse_scale_factor();
    let content = computed.content_size() * computed.inverse_scale_factor();
    let max = (content - visible).max(Vec2::ZERO);
    if node.overflow.y == OverflowAxis::Scroll {
        position.y = (position.y + direction * visible.y).clamp(0.0, max.y);
    } else if node.overflow.x == OverflowAxis::Scroll {
        position.x = (position.x + direction * visible.x).clamp(0.0, max.x);
    }
    claims.claim(action.action);
}

/// Registers the scroll panel's observers.
pub fn build(app: &mut App) {
    app.add_observer(on_scroll_page);
}
