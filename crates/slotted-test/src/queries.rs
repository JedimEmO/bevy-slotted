//! Read-only questions about the UI.

use std::fmt::Write as _;

use bevy::input_focus::InputFocus;
use bevy::prelude::*;
use bevy::ui::ui_transform::UiGlobalTransform;
use slotted_ecs::{Carried, OpenMenu, SlotRef};
use slotted_model::ItemStack;
use slotted_ui::{ItemView, TooltipContent};

use crate::harness::UiHarness;
use crate::locator::Locator;
use crate::tree::{ScreenTree, build, roots};

/// How many nodes a `find` failure lists before it says "and n more".
const NEAR_MISS_LIMIT: usize = 12;

fn indent(text: &str, pad: &str) -> String {
    text.lines()
        .map(|l| format!("{pad}{l}"))
        .collect::<Vec<_>>()
        .join("\n")
}

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
    /// The one entity matching `locator`. Panics on zero or several, with the
    /// near-misses listed so the message says why.
    pub fn find(&self, locator: &Locator) -> Entity {
        let found = locator.resolve(self.world());
        match found.as_slice() {
            [e] => *e,
            [] => panic!("{}", self.explain_no_match(locator)),
            many => panic!("{}", self.explain_several(locator, many)),
        }
    }

    /// The message [`find`](Self::find) panics with when nothing matched.
    fn explain_no_match(&self, locator: &Locator) -> String {
        let world = self.world();
        let mut msg = format!("no node matches: {locator}");
        let near = locator.near_misses(world, NEAR_MISS_LIMIT);
        if near.is_empty() {
            msg.push_str("\n  nothing in the world comes close.");
        } else {
            msg.push_str("\n  near misses:");
            for (e, failed) in &near {
                let _ = write!(
                    msg,
                    "\n    {}  <- fails {failed}",
                    crate::locator::describe(world, *e)
                );
            }
        }
        msg.push_str("\n  semantic tree:\n");
        msg.push_str(&indent(&self.screen_tree().to_string(), "    "));
        msg
    }

    /// The message [`find`](Self::find) panics with when several matched.
    fn explain_several(&self, locator: &Locator, found: &[Entity]) -> String {
        let world = self.world();
        let mut msg = format!(
            "{} nodes match: {locator}\n  add .index(n), .within(..) or another criterion:",
            found.len()
        );
        for (i, e) in found.iter().take(NEAR_MISS_LIMIT).enumerate() {
            let _ = write!(msg, "\n    [{i}] {}", crate::locator::describe(world, *e));
        }
        if found.len() > NEAR_MISS_LIMIT {
            let _ = write!(msg, "\n    ... and {} more", found.len() - NEAR_MISS_LIMIT);
        }
        msg
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

    /// The HUD layer roots, bottom to top (Phase 6 contract 2.4).
    pub fn hud_layers(&self) -> Vec<Entity> {
        let world = self.world();
        let Some(layers) = world.get_resource::<slotted_ui::HudLayers>() else {
            return Vec::new();
        };
        layers
            .order()
            .iter()
            .filter_map(|id| layers.root(id))
            .collect()
    }

    /// One HUD layer's root, if spawned.
    pub fn hud_layer(&self, id: &str) -> Option<Entity> {
        self.world()
            .get_resource::<slotted_ui::HudLayers>()
            .and_then(|l| l.root(&slotted_ui::HudLayerId::new(id)))
    }

    /// The semantic tree of every HUD layer root, same elision as
    /// [`screen_tree`](Self::screen_tree).
    pub fn hud_tree(&self) -> ScreenTree {
        // PHASE6-IMPL: B. `build` over `hud_layers()`.
        ScreenTree { roots: Vec::new() }
    }

    /// Binds HUD slot widgets to `menu` (`slotted_ui::HudMenu`).
    pub fn set_hud_menu(&mut self, menu: Option<Entity>) {
        self.world_mut().insert_resource(slotted_ui::HudMenu(menu));
        self.step(1);
    }

    /// A tank's or bar's `FillValue` (Phase 6 contract 1.7).
    pub fn fill_of(&self, entity: Entity) -> Option<slotted_ui::FillValue> {
        self.world().get::<slotted_ui::FillValue>(entity).copied()
    }

    /// `value / max` of a tank or bar, `0.0` when unbound.
    pub fn tank_fill(&self, entity: Entity) -> f32 {
        self.fill_of(entity).map_or(0.0, |f| f.fraction())
    }

    /// The bound value property and its current value, from the node's
    /// `PropertyBinding` and the menu's `MenuProperty` child.
    pub fn property_of(&self, entity: Entity) -> Option<(slotted_model::PropertyId, i32)> {
        // PHASE6-IMPL: A.
        let _ = entity;
        None
    }

    /// Whether a side tab is open.
    pub fn side_tab_open(&self, entity: Entity) -> Option<bool> {
        self.world()
            .get::<slotted_ui::SideTabState>(entity)
            .map(|s| s.open)
    }

    /// A viewport's runtime subject.
    pub fn viewport_subject(&self, entity: Entity) -> Option<slotted_ui::ViewportSubject> {
        self.world()
            .get::<slotted_ui::ViewportSubject>(entity)
            .cloned()
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
