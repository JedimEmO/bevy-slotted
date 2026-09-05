//! Read-only questions about the UI.

use bevy::input_focus::InputFocus;
use bevy::prelude::*;
use bevy::ui::ui_transform::UiGlobalTransform;
use slotted_ecs::{Carried, OpenMenu, SlotRef};
use slotted_model::ItemStack;
use slotted_ui::{ItemView, TooltipContent};

use crate::harness::UiHarness;
use crate::locator::Locator;
use crate::tree::{ScreenTree, build, roots};

/// Laid out and not hidden. A node with a zero-size `ComputedNode` or
/// `Visibility::Hidden` (own or inherited) is not visible.
pub fn is_visible(world: &World, entity: Entity) -> bool {
    let laid_out = world
        .get::<ComputedNode>(entity)
        .is_some_and(|n| n.size().x > 0.0 && n.size().y > 0.0);
    let shown = world
        .get::<InheritedVisibility>(entity)
        .is_none_or(|v| v.get());
    laid_out && shown
}

/// Holds keyboard focus.
pub fn is_focused(world: &World, entity: Entity) -> bool {
    world
        .get_resource::<InputFocus>()
        .is_some_and(|f| f.get() == Some(entity))
}

impl UiHarness {
    /// The one entity matching `locator`. Panics on zero or several.
    pub fn find(&self, locator: &Locator) -> Entity {
        let found = locator.resolve(self.world());
        match found.as_slice() {
            [e] => *e,
            [] => panic!("no node matches {locator:?}"),
            many => panic!("{} nodes match {locator:?}; add .index(n)", many.len()),
        }
    }

    /// The first match, if any.
    pub fn try_find(&self, locator: &Locator) -> Option<Entity> {
        locator.resolve(self.world()).into_iter().next()
    }

    /// Every match in tree order.
    pub fn find_all(&self, locator: &Locator) -> Vec<Entity> {
        locator.resolve(self.world())
    }

    /// The stack a slot entity shows, read from the model through `SlotRef`
    /// (not from the widget), so it is true even before rendering catches up.
    pub fn stack_at(&self, slot: Entity) -> Option<ItemStack> {
        let world = self.world();
        let slot_ref = world.get::<SlotRef>(slot)?;
        let menu = world.get::<OpenMenu>(slot_ref.menu)?;
        let def = menu.def.slot(slot_ref.slot)?;
        let inv_entity = *menu.inventories.get(def.source.index())?;
        let inv = world.get::<slotted_ecs::Inventory>(inv_entity)?;
        inv.get(usize::from(def.index)).cloned()
    }

    /// What the slot widget currently displays (may lag the model by a frame).
    pub fn displayed_stack(&self, entity: Entity) -> Option<ItemStack> {
        self.world()
            .get::<ItemView>(entity)
            .and_then(|v| v.stack.clone())
    }

    /// The carried stack of `menu`.
    pub fn carried(&self, menu: Entity) -> Option<ItemStack> {
        self.world().get::<Carried>(menu).and_then(|c| c.0.clone())
    }

    /// See [`is_visible`].
    pub fn is_visible(&self, entity: Entity) -> bool {
        is_visible(self.world(), entity)
    }

    /// See [`is_focused`].
    pub fn is_focused(&self, entity: Entity) -> bool {
        is_focused(self.world(), entity)
    }

    /// The focused entity.
    pub fn focused(&self) -> Option<Entity> {
        self.world()
            .get_resource::<InputFocus>()
            .and_then(InputFocus::get)
    }

    /// The text a node displays: its `Text`, else its `SemanticLabel`.
    pub fn text_of(&self, entity: Entity) -> Option<String> {
        let w = self.world();
        w.get::<Text>(entity).map(|t| t.0.clone()).or_else(|| {
            w.get::<slotted_ui::SemanticLabel>(entity)
                .map(|l| l.0.clone())
        })
    }

    /// The composed tooltip currently shown, if any.
    pub fn tooltip(&self) -> Option<TooltipContent> {
        self.world()
            .iter_entities()
            .find_map(|e| e.get::<TooltipContent>().cloned())
    }

    /// Logical-pixel rect of a laid-out node in window coordinates.
    pub fn rect_of(&self, entity: Entity) -> Rect {
        let w = self.world();
        let node = w
            .get::<ComputedNode>(entity)
            .unwrap_or_else(|| panic!("{entity} has no ComputedNode"));
        let tf = w
            .get::<UiGlobalTransform>(entity)
            .unwrap_or_else(|| panic!("{entity} has no UiGlobalTransform"));
        let size = node.size() * node.inverse_scale_factor();
        let center = tf.translation * node.inverse_scale_factor();
        Rect::from_center_size(center, size)
    }

    /// Centre of [`rect_of`](Self::rect_of).
    pub fn center_of(&self, entity: Entity) -> Vec2 {
        self.rect_of(entity).center()
    }

    /// Exclusion rects under `screen`.
    pub fn exclusion_zones(&mut self, screen: Entity) -> Vec<Rect> {
        let world = self.world_mut();
        let mut state: bevy::ecs::system::SystemState<slotted_ui::Exclusions> =
            bevy::ecs::system::SystemState::new(world);
        state
            .get(world)
            .expect("Exclusions has no fallible params")
            .union(screen)
    }

    /// The semantic tree of every open screen.
    pub fn screen_tree(&self) -> ScreenTree {
        let world = self.world();
        ScreenTree {
            roots: roots(world)
                .into_iter()
                .filter_map(|r| build(world, r))
                .collect(),
        }
    }
}
