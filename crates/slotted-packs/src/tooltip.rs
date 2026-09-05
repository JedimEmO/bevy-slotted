//! Script-driven tooltip parts and template widgets. Contract section 2.5.

use bevy::prelude::*;
use slotted_script::{ModId, ScriptId, TierFilter, TooltipFilter};
use slotted_ui::def::UiNodeDef;
use slotted_ui::screen::{SpawnCtx, Widget};
use slotted_ui::tooltip::{TooltipCtx, TooltipPart};

use crate::ScriptHost;

/// A data-stage `AddTooltipPart`: fixed nodes behind a filter.
#[derive(Debug, Clone, PartialEq)]
pub struct StaticPart {
    /// Owning mod.
    pub mod_id: ModId,
    /// Optional stable id.
    pub id: Option<String>,
    /// Which stacks.
    pub when: TooltipFilter,
    /// Which tier.
    pub tier: TierFilter,
    /// What to append.
    pub nodes: Vec<UiNodeDef>,
}

/// The one `TooltipPart` packs registers: every static part whose filter
/// matches, then every control script subscribed to `tooltip_build`.
pub struct ScriptTooltipPart {
    /// The runtime, for dynamic parts. `None` when there is no host.
    pub host: Option<ScriptHost>,
    /// Data-stage parts.
    pub statics: Vec<StaticPart>,
    /// Control scripts to ask, in load order.
    pub dynamic: Vec<(ModId, ScriptId)>,
}

impl TooltipPart for ScriptTooltipPart {
    fn build(&self, ctx: &TooltipCtx<'_>, out: &mut Vec<UiNodeDef>) {
        // PHASE4-IMPL: B -- filter statics by item id / tag_index / tier; fire
        // TooltipBuild into each dynamic script and append returned nodes.
        let _ = (ctx, &mut *out);
    }
}

/// A `RegisterWidget` entry: a `UiNodeDef` template whose
/// `Anchor { id: "children" }` receives the caller's children.
#[derive(Debug, Clone, PartialEq)]
pub struct TemplateWidget {
    /// The template.
    pub template: UiNodeDef,
}

impl Widget for TemplateWidget {
    fn spawn(
        &self,
        ctx: &mut SpawnCtx<'_>,
        _params: &slotted_registry::Value,
        children: &[UiNodeDef],
    ) -> Entity {
        // PHASE4-IMPL: B -- spawn `template`, then `children` under its
        // `children` anchor; must carry WidgetNode and a SemanticRole.
        let _ = children;
        ctx.spawn_child(&self.template)
    }
}
