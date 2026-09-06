//! Z-order bands, overlay roots and exclusion zones.

use bevy::ecs::system::SystemParam;
use bevy::picking::pointer::PointerLocation;
use bevy::prelude::*;
use bevy::ui::ui_transform::UiGlobalTransform;

/// `GlobalZIndex` bands. Every root this crate spawns sits in one of these;
/// a game's own overlays should pick a band, not a number.
pub mod zbands {
    /// HUD layers (hotbar, health, crosshair).
    pub const HUD: i32 = 0;
    /// Container screens.
    pub const SCREEN: i32 = 100;
    /// The item browser beside a screen (Phase 3).
    pub const BROWSER: i32 = 200;
    /// Tooltips.
    pub const TOOLTIP: i32 = 1000;
    /// The carried stack following the pointer.
    pub const CARRIED: i32 = 2000;
    /// Dev overlays (exclusion-zone highlighter).
    pub const DEV: i32 = 9000;
}

/// Root of the carried-stack layer: full-window, `Pickable::IGNORE`,
/// `GlobalZIndex(zbands::CARRIED)`. One per app, spawned by the plugin. Its
/// single child is an [`crate::ItemView`] that mirrors the active menu's
/// `Carried` and follows the pointer.
#[derive(Component, Debug, Default, Clone, Copy)]
pub struct CarriedLayer;

/// Root of the tooltip layer: full-window, `Pickable::IGNORE`,
/// `GlobalZIndex(zbands::TOOLTIP)`.
#[derive(Component, Debug, Default, Clone, Copy)]
pub struct TooltipLayer;

/// Marks a node whose rect other overlays (browser, HUD) must stay out of.
/// Injected nodes get it when `Injection::exclusion` is set.
#[derive(Component, Debug, Default, Clone, Copy)]
pub struct ExclusionZone;

/// Query helper: the exclusion rects of a screen.
#[derive(SystemParam)]
pub struct Exclusions<'w, 's> {
    zones: Query<
        'w,
        's,
        (Entity, &'static ComputedNode, &'static UiGlobalTransform),
        With<ExclusionZone>,
    >,
    parents: Query<'w, 's, &'static ChildOf>,
}

impl Exclusions<'_, '_> {
    /// Logical-pixel rects of every exclusion zone under `screen`, plus the
    /// screen's own panel rect. Overlapping rects are not merged; callers
    /// test containment against each.
    pub fn union(&self, screen: Entity) -> Vec<Rect> {
        self.zones
            .iter()
            .filter(|(e, _, _)| self.is_descendant(*e, screen))
            .map(|(_, node, tf)| {
                let size = node.size() * node.inverse_scale_factor();
                let center = tf.translation * node.inverse_scale_factor();
                Rect::from_center_size(center, size)
            })
            .collect()
    }

    fn is_descendant(&self, mut e: Entity, ancestor: Entity) -> bool {
        loop {
            if e == ancestor {
                return true;
            }
            match self.parents.get(e) {
                Ok(p) => e = p.parent(),
                Err(_) => return false,
            }
        }
    }
}

/// The single child of [`CarriedLayer`]: an [`ItemView`](crate::ItemView)
/// mirroring the active menu's carried stack.
#[derive(Component, Debug, Default, Clone, Copy)]
pub struct CarriedItem;

/// `SlottedUiSet::Render`: mirror the active menu's `Carried` onto the carried
/// node and park it under the primary pointer.
///
/// The pointer is `PointerId::Mouse`, the same one the harness drives, and the
/// position is read from `PointerLocation` rather than a window event, so this
/// works headless.
pub fn update_carried_layer(
    pointers: Query<(&bevy::picking::pointer::PointerId, &PointerLocation)>,
    screens: Query<&crate::semantic::ScreenRoot>,
    menus: Query<&slotted_ecs::Carried>,
    ghost: Res<crate::preview::DragGhost>,
    mut carried: Query<(&mut crate::item::ItemView, &mut Node, &mut Visibility), With<CarriedItem>>,
) {
    let stack = screens
        .iter()
        .filter_map(|s| s.menu)
        .find_map(|menu| menus.get(menu).ok())
        .and_then(|c| c.0.clone());
    // A paint in progress has already claimed some of the stack: the ghost
    // counts down as the pointer paints, so what it shows is what would be
    // left if the player let go here. The model is untouched until they do.
    let stack = match (stack, ghost.remaining) {
        (Some(stack), Some(remaining)) => (remaining > 0).then(|| stack.with_count(remaining)),
        (stack, _) => stack,
    };
    let position = pointers
        .iter()
        .find(|(id, _)| matches!(id, bevy::picking::pointer::PointerId::Mouse))
        .and_then(|(_, location)| location.location().map(|l| l.position));
    for (mut view, mut node, mut visibility) in &mut carried {
        if view.stack != stack {
            view.stack.clone_from(&stack);
        }
        let wanted = if stack.is_some() {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if *visibility != wanted {
            *visibility = wanted;
        }
        if let Some(p) = position {
            let half = crate::widgets::SLOT_SIZE * 0.5;
            let (left, top) = (Val::Px(p.x - half), Val::Px(p.y - half));
            if node.left != left {
                node.left = left;
            }
            if node.top != top {
                node.top = top;
            }
        }
    }
}
