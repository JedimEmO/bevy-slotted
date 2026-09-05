//! Tooltip request and composition.

use std::sync::Arc;

use bevy::prelude::*;
use slotted_model::ItemStack;
use slotted_registry::FrozenRegistries;

use crate::def::UiNodeDef;

/// Compact (hover) or expanded (shift held / long hover).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum TooltipTier {
    /// Name, count, rarity.
    #[default]
    Compact,
    /// Plus components, tags, mod id.
    Expanded,
}

/// Ask for a tooltip on `entity`. Triggered by the slot widget on hover and
/// by the harness's `hover`. The tooltip system composes parts into
/// [`TooltipContent`] on the entity and spawns the tooltip node under
/// [`crate::TooltipLayer`].
#[derive(EntityEvent, Debug, Clone, Copy, PartialEq, Eq)]
pub struct TooltipRequest {
    /// The hovered entity.
    pub entity: Entity,
    /// Which tier.
    pub tier: TooltipTier,
}

/// The composed tooltip, on the hovered entity while its tooltip is shown.
/// `slotted-test`'s `tooltip()` reads this, not the spawned nodes.
#[derive(Component, Debug, Clone, PartialEq)]
pub struct TooltipContent {
    /// Which tier was composed.
    pub tier: TooltipTier,
    /// The parts, in order.
    pub parts: Vec<UiNodeDef>,
}

/// On the spawned tooltip root, pointing back at the hovered entity.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct TooltipHost(pub Entity);

/// What a part builder sees.
pub struct TooltipCtx<'a> {
    /// The stack under the pointer, if the entity is a slot with content.
    pub stack: Option<&'a ItemStack>,
    /// Registry data.
    pub registries: &'a FrozenRegistries,
    /// Requested tier.
    pub tier: TooltipTier,
}

/// One contributor to a tooltip. Built-ins: item name, count, rarity, and in
/// `dev` builds the item id.
pub trait TooltipPart: Send + Sync {
    /// Append nodes for this part. Append nothing to stay out.
    fn build(&self, ctx: &TooltipCtx<'_>, out: &mut Vec<UiNodeDef>);
}

/// Ordered registry of parts.
#[derive(Resource, Default, Clone)]
pub struct TooltipParts(pub Vec<Arc<dyn TooltipPart>>);

impl TooltipParts {
    /// Append a part.
    pub fn push(&mut self, part: impl TooltipPart + 'static) {
        self.0.push(Arc::new(part));
    }

    /// Run every part.
    pub fn compose(&self, ctx: &TooltipCtx<'_>) -> Vec<UiNodeDef> {
        let mut out = Vec::new();
        for p in &self.0 {
            p.build(ctx, &mut out);
        }
        out
    }
}

/// Observer: compose and show. Removes any previous tooltip for the host.
// PHASE2-IMPL: agent B. Read `ItemView` on the target, compose through
// `TooltipParts`, insert `TooltipContent`, spawn the tooltip widget under
// `TooltipLayer` positioned beside the target rect.
pub fn show_tooltip(request: On<TooltipRequest>, _commands: Commands) {
    tracing::debug!(?request, "show_tooltip is not implemented yet");
}
