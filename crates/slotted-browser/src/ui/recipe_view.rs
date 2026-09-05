//! `slotted:recipe_view`: category tabs, the laid-out recipe, transfer
//! button, history and "used in". Package B.

use bevy::prelude::*;
use slotted_registry::Value;
use slotted_ui::{SemanticRole, SpawnCtx, UiNodeDef, Widget};

use crate::category::RecipeSlotIx;
use crate::runtime::BrowserRuntime;

/// On the view root.
#[derive(Component, Debug, Default, Clone, Copy)]
pub struct RecipeViewRoot;

/// On a recipe slot node: which layout slot and which alternative shows.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecipeSlotNode {
    /// Index into the current `RecipeLayout`.
    pub ix: RecipeSlotIx,
    /// Which alternative is showing.
    pub alternative: usize,
}

/// On the `+` button.
#[derive(Component, Debug, Default, Clone, Copy)]
pub struct TransferButton;

/// The widget.
#[derive(Debug, Default, Clone, Copy)]
pub struct RecipeViewWidget;

impl Widget for RecipeViewWidget {
    fn spawn(&self, ctx: &mut SpawnCtx<'_>, _params: &Value, _children: &[UiNodeDef]) -> Entity {
        // PHASE3-IMPL: B — tabs, slots, arrow, `+` (TestId browser.transfer),
        // back/forward (browser.back, browser.forward), uses panel (browser.uses).
        ctx.spawn_node((
            Node::default(),
            SemanticRole::RecipeView,
            RecipeViewRoot,
            Visibility::Hidden,
        ))
    }
}

/// `BrowserSet::Render`: rebuild the view when `BrowserRuntime::open` changes;
/// run `dry_run` for the `+` state; paint `TransferFailed.missing`.
pub fn render_recipe_view(
    _runtime: Res<BrowserRuntime>,
    _roots: Query<Entity, With<RecipeViewRoot>>,
) {
    // PHASE3-IMPL: B
}

/// `BrowserSet::Render`: advance `RecipeSlotNode::alternative` every
/// `durations.slow` of `Time<Virtual>`, paused while Shift is held.
pub fn cycle_alternatives(_time: Res<Time<Virtual>>, _slots: Query<&mut RecipeSlotNode>) {
    // PHASE3-IMPL: B
}
