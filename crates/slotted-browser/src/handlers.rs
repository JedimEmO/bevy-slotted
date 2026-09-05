//! Screen handlers: how the browser fits around a particular screen kind.
//! Contract section 5. Package A.

use std::collections::BTreeMap;
use std::sync::Arc;

use bevy::prelude::*;
use slotted_ecs::{OpenMenu, SlotRef};
use slotted_model::{Button, ClickAction, GiveTarget};
use slotted_ui::ScreenKind;

use crate::category::CategoryId;
use crate::ingredient::Ingredient;

/// What the browser knows about an open screen when it asks a handler.
#[derive(Debug, Clone, PartialEq)]
pub struct ScreenGeometry {
    /// The screen root entity.
    pub screen: Entity,
    /// Its kind.
    pub kind: ScreenKind,
    /// Its menu, if any.
    pub menu: Option<Entity>,
    /// The def root's rectangle in logical px (from `ScreenLayout`).
    pub root_rect: Rect,
    /// The window in logical px.
    pub window: Rect,
    /// `Exclusions::union(screen)`.
    pub exclusions: Vec<Rect>,
}

/// A rectangle on the screen that opens a category when clicked (the
/// furnace's arrow).
#[derive(Debug, Clone, PartialEq)]
pub struct ClickableArea {
    /// Relative to `root_rect`'s top-left.
    pub rect: Rect,
    /// What to open.
    pub category: CategoryId,
}

/// What a ghost drop should do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GhostDrop {
    /// Trigger these actions on the menu, in order.
    Accept(Vec<ClickAction>),
    /// Nothing happens.
    Refuse,
}

/// How one screen kind integrates with the browser. Every method has a
/// sensible default; [`DefaultScreenHandler`] is exactly those defaults.
pub trait ScreenHandler: Send + Sync {
    /// The rectangle the browser must not cover.
    fn bounds(&self, screen: &ScreenGeometry) -> Rect {
        screen.root_rect
    }

    /// Rectangles to exclude in addition to `ExclusionZone` components.
    fn extra_exclusions(&self, _screen: &ScreenGeometry) -> Vec<Rect> {
        Vec::new()
    }

    /// Rectangles that open a category.
    fn clickable_areas(&self) -> Vec<ClickableArea> {
        Vec::new()
    }

    /// The ingredient under the cursor in a non-slot widget (a tank). Slots
    /// are resolved by the browser through `SlotRef`.
    fn stack_under_cursor(
        &self,
        _screen: &ScreenGeometry,
        _world: &World,
        _cursor: Vec2,
    ) -> Option<Ingredient> {
        None
    }

    /// What dropping `ing` on `target` does. The default accepts a drop on a
    /// ghost or filter slot: with `can_cheat`, a `Give { .., Cursor }` followed
    /// by a left click; otherwise nothing yet (Phase 6 wires `GhostHint`).
    fn ghost_drop(
        &self,
        _screen: &ScreenGeometry,
        world: &World,
        target: Entity,
        ing: &Ingredient,
    ) -> GhostDrop {
        let Some(slot_ref) = world.get::<SlotRef>(target) else {
            return GhostDrop::Refuse;
        };
        let Some(menu) = world.get::<OpenMenu>(slot_ref.menu) else {
            return GhostDrop::Refuse;
        };
        let Some(def) = menu.def.slot(slot_ref.slot) else {
            return GhostDrop::Refuse;
        };
        if !def.behaviour.is_ghost() || !menu.actor.can_cheat {
            return GhostDrop::Refuse;
        }
        let Some(item) = ing.item_id() else {
            return GhostDrop::Refuse;
        };
        GhostDrop::Accept(vec![
            ClickAction::Give {
                item,
                count: 1,
                target: GiveTarget::Cursor,
            },
            ClickAction::Pickup {
                slot: slot_ref.slot,
                button: Button::Left,
            },
        ])
    }
}

/// The trait's defaults, as a value. What `AttachPolicy::AllMenus` uses.
#[derive(Debug, Default, Clone, Copy)]
pub struct DefaultScreenHandler;

impl ScreenHandler for DefaultScreenHandler {}

/// Handlers by screen kind. Filled in
/// [`BrowserPhase::ScreenHandlers`](crate::BrowserPhase::ScreenHandlers).
#[derive(Resource, Default, Clone)]
pub struct ScreenHandlers {
    map: BTreeMap<ScreenKind, Arc<dyn ScreenHandler>>,
}

impl ScreenHandlers {
    /// Registers a handler; replaces an earlier one for the kind.
    pub fn register(&mut self, kind: ScreenKind, handler: Arc<dyn ScreenHandler>) {
        self.map.insert(kind, handler);
    }

    /// The handler for a kind.
    pub fn get(&self, kind: &ScreenKind) -> Option<&Arc<dyn ScreenHandler>> {
        self.map.get(kind)
    }

    /// Whether the kind is registered.
    pub fn contains(&self, kind: &ScreenKind) -> bool {
        self.map.contains_key(kind)
    }

    /// Registered kinds.
    pub fn kinds(&self) -> impl Iterator<Item = &ScreenKind> {
        self.map.keys()
    }
}
