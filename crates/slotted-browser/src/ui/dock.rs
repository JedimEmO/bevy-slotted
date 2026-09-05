//! Docking: where the panel goes and how many cards fit. Package B.

use bevy::prelude::*;
use slotted_ui::widgets::SLOT_SIZE;
use slotted_ui::{Exclusions, Side};

use super::panel::BrowserPanel;
use crate::handlers::{ScreenGeometry, ScreenHandlers};

/// Gap between cards, px.
pub const CARD_GAP: f32 = 4.0;
/// Panel padding, px.
pub const PANEL_PADDING: f32 = 8.0;

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
    let left = Rect::new(window.min.x, window.min.y, occupied.min.x, window.max.y);
    let right = Rect::new(occupied.max.x, window.min.y, window.max.x, window.max.y);
    (left, right)
}

/// Picks a side and a grid for a strip pair; `None` when neither fits two
/// columns.
pub fn choose(left: Rect, right: Rect) -> Option<BrowserLayout> {
    let cell = SLOT_SIZE + CARD_GAP;
    let min_width = 2.0 * cell + 2.0 * PANEL_PADDING;
    let (side, strip) = if right.width() >= left.width() {
        (Side::Right, right)
    } else {
        (Side::Left, left)
    };
    if strip.width() < min_width {
        return None;
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let cols = ((strip.width() - 2.0 * PANEL_PADDING) / cell).floor() as u16;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let rows = ((strip.height() - 2.0 * PANEL_PADDING - 3.0 * cell) / cell)
        .floor()
        .max(1.0) as u16;
    Some(BrowserLayout {
        side,
        rect: strip,
        cols,
        rows,
    })
}

/// `BrowserSet::Layout`: recompute placement for every panel whose inputs
/// changed (screen layout, window size, `UiScale`, exclusion union).
pub fn dock_panels(
    _panels: Query<(Entity, &BrowserPanel, &mut Node, &mut Visibility)>,
    _handlers: Res<ScreenHandlers>,
    _exclusions: Exclusions,
    _windows: Query<&Window>,
    _commands: Commands,
) {
    // PHASE3-IMPL: B — build `ScreenGeometry`, call the handler's `bounds` and
    // `extra_exclusions`, `free_space`, `choose`, write `Node` and
    // `BrowserLayout`, set `Visibility`, update the `side=` tag.
    let _ = ScreenGeometry {
        screen: Entity::PLACEHOLDER,
        kind: super::panel_kind(),
        menu: None,
        root_rect: Rect::default(),
        window: Rect::default(),
        exclusions: Vec::new(),
    };
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
        let layout = choose(l, r).expect("fits");
        assert_eq!(layout.side, Side::Left);
        assert_eq!(layout.cols, 8);
    }
}
