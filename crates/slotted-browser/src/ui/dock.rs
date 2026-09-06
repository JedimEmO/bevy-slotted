//! Docking: where the panel goes and how many cards fit. Package B.

use bevy::prelude::*;
use bevy::ui::ui_transform::UiGlobalTransform;
use slotted_ui::widgets::SLOT_SIZE;
use slotted_ui::{Exclusions, ScreenRoot, Side, Tags};

use super::panel::BrowserPanel;
use crate::handlers::{ScreenGeometry, ScreenHandler, ScreenHandlers};

/// Gap between cards, px.
pub const CARD_GAP: f32 = 8.0;
/// Panel padding, px.
pub const PANEL_PADDING: f32 = 14.0;
/// Gap between the panel's five regions, px.
pub const PANEL_GAP: f32 = 10.0;
/// How far the panel keeps off the window edges, px.
pub const PANEL_MARGIN: f32 = 12.0;
/// The panel's width where the strip allows it, before any theme has loaded.
/// A strip wider than this leaves the extra width to the world rather than
/// stretching the cards. The live number is `Tokens::sizes.panel_width`.
pub const PANEL_WIDTH: f32 = 352.0;
/// One card's width, px, before any theme has loaded. Four of them plus three
/// [`CARD_GAP`]s fill the panel's content box exactly.
pub const CARD_WIDTH: f32 = 75.0;
/// One card's height, px: a 44 px icon box, a two-line name and the mod
/// badge. Before any theme has loaded.
pub const CARD_HEIGHT: f32 = 94.0;
/// Height the panel reserves above and below the card grid for the search
/// field, the chip row, the bookmark strip and the status line. Before any
/// theme has loaded.
pub const CHROME_HEIGHT: f32 = 190.0;

/// The sizes a dock decision is made from, in UI units.
///
/// Every number is a theme token; the constants above are the defaults those
/// tokens carry. [`choose`] takes this rather than reading the constants, so
/// a theme with a bigger slot or a wider panel gets a grid that matches what
/// its cards will actually measure.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DockMetrics {
    /// Edge length of a slot; the panel's floor is two of them wide.
    pub slot_size: f32,
    /// The panel's preferred width.
    pub panel_width: f32,
    /// One card's width.
    pub card_width: f32,
    /// One card's height.
    pub card_height: f32,
    /// What the panel reserves for everything that is not the card grid.
    pub chrome_height: f32,
}

impl Default for DockMetrics {
    fn default() -> Self {
        Self {
            slot_size: SLOT_SIZE,
            panel_width: PANEL_WIDTH,
            card_width: CARD_WIDTH,
            card_height: CARD_HEIGHT,
            chrome_height: CHROME_HEIGHT,
        }
    }
}

impl From<&slotted_theme::Tokens> for DockMetrics {
    fn from(tokens: &slotted_theme::Tokens) -> Self {
        Self {
            slot_size: tokens.sizes.slot_size,
            panel_width: tokens.sizes.panel_width,
            card_width: tokens.sizes.card_width,
            card_height: tokens.sizes.card_height,
            chrome_height: tokens.sizes.chrome_height,
        }
    }
}

/// The panel's computed placement. On the panel root; the harness reads it.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct BrowserLayout {
    /// Which side of the screen; the panel is hidden when neither fits.
    pub side: Side,
    /// The panel rectangle in logical px.
    pub rect: Rect,
    /// Card columns.
    pub cols: u16,
    /// Card rows.
    pub rows: u16,
}

/// The free strips left and right of the occupied area (bounds plus
/// exclusions), as `(left, right)` rectangles spanning the window height.
pub fn free_space(window: Rect, bounds: Rect, exclusions: &[Rect]) -> (Rect, Rect) {
    let mut occupied = bounds;
    for r in exclusions {
        occupied = occupied.union(*r);
    }
    // Clamp before building the strips: `Rect::new` normalises its corners, so
    // an occupied area running past a window edge would otherwise come back as
    // a wide strip outside the window rather than as nothing.
    let left_edge = occupied.min.x.clamp(window.min.x, window.max.x);
    let right_edge = occupied.max.x.clamp(window.min.x, window.max.x);
    let left = Rect::new(window.min.x, window.min.y, left_edge, window.max.y);
    let right = Rect::new(right_edge, window.min.y, window.max.x, window.max.y);
    (left, right)
}

/// Picks a side and a grid for a strip pair; `None` when neither fits two
/// columns.
///
/// The panel is [`PANEL_WIDTH`] wide wherever the strip allows it, hugging the
/// outer window edge, and never taller than the window: the rectangle is the
/// strip inset by [`PANEL_MARGIN`], so the panel cannot run off the top or the
/// bottom however tall the screen beside it is.
pub fn choose(left: Rect, right: Rect, metrics: DockMetrics) -> Option<BrowserLayout> {
    let min_width = 2.0 * (metrics.slot_size + CARD_GAP) + 2.0 * PANEL_PADDING;
    let (side, strip) = if right.width() >= left.width() {
        (Side::Right, right)
    } else {
        (Side::Left, left)
    };
    if strip.width() < min_width || strip.height() < metrics.chrome_height {
        return None;
    }
    let width = metrics
        .panel_width
        .min(strip.width() - 2.0 * PANEL_MARGIN)
        .max(min_width);
    let x = match side {
        Side::Right => (strip.max.x - PANEL_MARGIN - width).max(strip.min.x),
        Side::Left => strip.min.x + PANEL_MARGIN,
    };
    let top = strip.min.y + PANEL_MARGIN;
    let height = (strip.height() - 2.0 * PANEL_MARGIN).max(metrics.chrome_height);
    let rect = Rect::new(x, top, x + width, top + height);
    let content = width - 2.0 * PANEL_PADDING;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let cols = (((content + CARD_GAP) / (metrics.card_width + CARD_GAP)).floor()).max(1.0) as u16;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let rows = (((height - 2.0 * PANEL_PADDING - metrics.chrome_height + CARD_GAP)
        / (metrics.card_height + CARD_GAP))
        .floor())
    .max(1.0) as u16;
    Some(BrowserLayout {
        side,
        rect,
        cols,
        rows,
    })
}

/// The placement for one screen: ask the handler for the area to keep clear,
/// add the exclusion union, then [`choose`] a strip.
pub fn plan(
    handler: Option<&dyn ScreenHandler>,
    geometry: &ScreenGeometry,
    metrics: DockMetrics,
) -> Option<BrowserLayout> {
    let bounds = handler.map_or(geometry.root_rect, |h| h.bounds(geometry));
    let mut exclusions = geometry.exclusions.clone();
    if let Some(handler) = handler {
        exclusions.extend(handler.extra_exclusions(geometry));
    }
    let (left, right) = free_space(geometry.window, bounds, &exclusions);
    choose(left, right, metrics)
}

/// What a handler is asked about: the screen's rectangle, the window and the
/// exclusion union.
///
/// `root_rect` is the def root's rectangle, which is the screen root's first
/// laid-out child, matching what `ScreenLayout` reports.
pub fn geometry_of(
    screen: Entity,
    root: &ScreenRoot,
    window: Rect,
    nodes: &Query<(&ComputedNode, &UiGlobalTransform)>,
    children: &Query<&Children>,
    exclusions: &Exclusions,
) -> ScreenGeometry {
    let root_rect = children
        .get(screen)
        .ok()
        .and_then(|kids| kids.iter().find_map(|c| nodes.get(c).ok()))
        .map(|(node, transform)| {
            let size = node.size() * node.inverse_scale_factor();
            let center = transform.translation * node.inverse_scale_factor();
            Rect::from_center_size(center, size)
        })
        .unwrap_or_default();
    ScreenGeometry {
        screen,
        kind: root.kind.clone(),
        menu: root.menu,
        root_rect,
        window,
        exclusions: exclusions.union(screen),
    }
}

/// The primary window's rectangle in UI units.
///
/// The window measures itself in logical pixels and a `Node` is written in UI
/// units, which `UiScale` divides out of them. Everything else in this module
/// -- the screen's rect, the exclusion zones, the panel's own numbers -- is
/// already in UI units, so the window is converted once, here, and the dock
/// never sees a mixed pair of spaces.
pub fn window_rect(windows: &Query<&Window>, ui_scale: f32) -> Rect {
    let scale = ui_scale.max(f32::EPSILON);
    windows
        .iter()
        .next()
        .map(|w| Rect::new(0.0, 0.0, w.width() / scale, w.height() / scale))
        .unwrap_or_default()
}

/// `BrowserSet::Layout`: recompute every panel's placement and write it only
/// when it changed, so a settled frame stays settled.
#[allow(clippy::too_many_arguments)]
pub fn dock_panels(
    panels: Query<(Entity, &BrowserPanel)>,
    screens: Query<&ScreenRoot>,
    nodes: Query<(&ComputedNode, &UiGlobalTransform)>,
    children: Query<&Children>,
    handlers: Res<ScreenHandlers>,
    exclusions: Exclusions,
    windows: Query<&Window>,
    units: slotted_ui::UiUnits,
    tokens: slotted_ui::tooltip::ThemeTokens,
    mut placement: Query<(
        &mut Node,
        &mut Visibility,
        &mut Tags,
        Option<&BrowserLayout>,
    )>,
    mut commands: Commands,
) {
    let window = window_rect(&windows, units.scale());
    let metrics = DockMetrics::from(&tokens.get());
    for (entity, panel) in &panels {
        let Ok(root) = screens.get(panel.screen) else {
            continue;
        };
        let geometry = geometry_of(panel.screen, root, window, &nodes, &children, &exclusions);
        let handler = handlers.get(&root.kind).map(std::convert::AsRef::as_ref);
        let planned = plan(handler, &geometry, metrics);
        let Ok((mut node, mut visibility, mut tags, current)) = placement.get_mut(entity) else {
            continue;
        };
        if let Some(layout) = planned {
            {
                if current != Some(&layout) {
                    commands.entity(entity).insert(layout);
                }
                // The panel wraps its content and is capped by the strip, so
                // a short result set does not leave a tall empty sheet and a
                // long one still cannot run off the bottom of the window.
                let want = (
                    px(layout.rect.min.x),
                    px(layout.rect.min.y),
                    px(layout.rect.width()),
                    px(layout.rect.height()),
                );
                if (node.left, node.top, node.width, node.max_height) != want {
                    (node.left, node.top, node.width, node.max_height) = want;
                }
                if *visibility != Visibility::Inherited {
                    *visibility = Visibility::Inherited;
                }
                set_tag(&mut tags, layout.side.as_str());
            }
        } else {
            {
                if current.is_some() {
                    commands.entity(entity).remove::<BrowserLayout>();
                }
                if *visibility != Visibility::Hidden {
                    *visibility = Visibility::Hidden;
                }
                set_tag(&mut tags, "none");
            }
        }
    }
}

fn set_tag(tags: &mut Mut<'_, Tags>, side: &str) {
    if tags.get("side") != Some(side) {
        tags.0.insert("side".to_owned(), side.to_owned());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn docks_to_the_wider_side() {
        let window = Rect::new(0.0, 0.0, 1280.0, 720.0);
        let bounds = Rect::new(400.0, 100.0, 800.0, 600.0);
        let (l, r) = free_space(window, bounds, &[Rect::new(800.0, 100.0, 900.0, 200.0)]);
        assert!((l.width() - 400.0).abs() < f32::EPSILON);
        assert!((r.width() - 380.0).abs() < f32::EPSILON);
        let layout = choose(l, r, DockMetrics::default()).expect("fits");
        assert_eq!(layout.side, Side::Left);
        assert_eq!(layout.cols, 4);
    }

    #[test]
    fn a_screen_left_of_centre_docks_the_panel_right() {
        let window = Rect::new(0.0, 0.0, 1280.0, 720.0);
        let bounds = Rect::new(60.0, 100.0, 560.0, 600.0);
        let (l, r) = free_space(window, bounds, &[]);
        assert_eq!(
            choose(l, r, DockMetrics::default()).expect("fits").side,
            Side::Right
        );
    }

    #[test]
    fn the_panel_never_runs_past_the_window() {
        let window = Rect::new(0.0, 0.0, 1600.0, 900.0);
        let bounds = Rect::new(400.0, 200.0, 1100.0, 700.0);
        let (l, r) = free_space(window, bounds, &[]);
        let layout = choose(l, r, DockMetrics::default()).expect("fits");
        assert!(layout.rect.min.x >= window.min.x && layout.rect.max.x <= window.max.x);
        assert!(layout.rect.min.y >= window.min.y && layout.rect.max.y <= window.max.y);
        assert!((layout.rect.width() - PANEL_WIDTH).abs() < 0.5);
    }

    #[test]
    fn a_screen_filling_the_window_hides_the_panel() {
        let window = Rect::new(0.0, 0.0, 400.0, 300.0);
        let bounds = Rect::new(-20.0, 0.0, 420.0, 300.0);
        let (l, r) = free_space(window, bounds, &[]);
        assert!(l.width() < 1.0 && r.width() < 1.0);
        assert!(choose(l, r, DockMetrics::default()).is_none());
    }
}
