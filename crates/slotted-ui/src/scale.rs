//! UI units and `UiScale`.
//!
//! Bevy multiplies every `Node` pixel by the render target's scale factor and
//! by [`UiScale`] on its way to the screen, so the numbers a widget writes
//! are neither physical pixels nor the window's logical pixels: they are UI
//! units, and `UiScale` is how many of a window's logical pixels one of them
//! covers.
//!
//! Anything that stays inside the node tree never notices. What does notice
//! is code that plans a layout from something Bevy measured in another space:
//! a pointer position, a window size, the free strip the item browser docks
//! into. Those are logical window pixels and have to be divided by
//! [`UiScale`] before they can be compared with a token or written back as a
//! `Node` pixel. Multiplying instead is the bug this module exists to stop:
//! it scales the panel twice and docks a panel wider than the strip that made
//! room for it.

use bevy::prelude::*;
use bevy::ui::UiScale;

/// Converts what Bevy measured in logical window pixels into the UI units a
/// `Node` is written in.
#[derive(bevy::ecs::system::SystemParam)]
pub struct UiUnits<'w> {
    scale: Option<Res<'w, UiScale>>,
}

impl UiUnits<'_> {
    /// The factor in force. 1.0 when nothing set one.
    pub fn scale(&self) -> f32 {
        self.scale.as_ref().map_or(1.0, |s| s.0).max(f32::EPSILON)
    }

    /// One length, from logical window pixels to UI units.
    pub fn length(&self, logical: f32) -> f32 {
        logical / self.scale()
    }

    /// One point.
    pub fn point(&self, logical: Vec2) -> Vec2 {
        logical / self.scale()
    }

    /// One rectangle, corner by corner.
    pub fn rect(&self, logical: Rect) -> Rect {
        let s = self.scale();
        Rect {
            min: logical.min / s,
            max: logical.max / s,
        }
    }
}

/// The same conversion for code holding a `&World` rather than a system's
/// parameters.
pub fn ui_scale_of(world: &World) -> f32 {
    world
        .get_resource::<UiScale>()
        .map_or(1.0, |s| s.0)
        .max(f32::EPSILON)
}
