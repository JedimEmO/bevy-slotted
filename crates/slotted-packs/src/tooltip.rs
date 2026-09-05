//! Script-driven tooltip parts and template widgets. Contract section 2.5.

use bevy::prelude::*;
use slotted_model::Namespaced;
use slotted_script::{
    ModId, ScriptCommand, ScriptEvent, ScriptId, Tier, TierFilter, TooltipFilter,
};
use slotted_ui::def::{AnchorId, UiNodeDef, WidgetKind};
use slotted_ui::screen::{SpawnCtx, Widget};
use slotted_ui::semantic::{SemanticRole, WidgetNode};
use slotted_ui::tooltip::{TooltipCtx, TooltipPart, TooltipTier};

use crate::ScriptHost;
use crate::route::{TOOLTIP_BUILD, stack_info};

/// The anchor a [`TemplateWidget`] splices the caller's children into.
pub const CHILDREN_ANCHOR: &str = "children";

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

impl StaticPart {
    /// Whether this part shows at `tier`.
    fn matches_tier(&self, tier: TooltipTier) -> bool {
        matches!(
            (self.tier, tier),
            (TierFilter::Any, _)
                | (TierFilter::Compact, TooltipTier::Compact)
                | (TierFilter::Expanded, TooltipTier::Expanded)
        )
    }

    /// Whether this part applies to the hovered stack. Both filters empty
    /// means every stack, which is how a "press shift" hint is written.
    fn matches_stack(&self, ctx: &TooltipCtx<'_>) -> bool {
        if self.when.items.is_empty() && self.when.tags.is_empty() {
            return true;
        }
        let Some(stack) = ctx.stack else {
            return false;
        };
        let Some(name) = ctx.registries.items.name_of(stack.id) else {
            return false;
        };
        if self.when.items.iter().any(|item| item == &name.to_string()) {
            return true;
        }
        self.when.tags.iter().any(|tag| {
            Namespaced::parse(tag.trim_start_matches('#'))
                .is_ok_and(|tag| ctx.registries.tag_index.has_tag(stack.id, &tag))
        })
    }
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
        for part in &self.statics {
            if part.matches_tier(ctx.tier) && part.matches_stack(ctx) {
                out.extend(part.nodes.iter().cloned());
            }
        }

        let (Some(host), Some(stack)) = (self.host.as_ref(), ctx.stack) else {
            return;
        };
        if self.dynamic.is_empty() {
            return;
        }
        let event = ScriptEvent::TooltipBuild {
            stack: stack_info(ctx.registries, stack),
            tier: match ctx.tier {
                TooltipTier::Compact => Tier::Compact,
                TooltipTier::Expanded => Tier::Expanded,
            },
        };
        for (mod_id, script) in &self.dynamic {
            let commands = match host.lock().call(*script, &event) {
                Ok(commands) => commands,
                Err(error) => {
                    // A tooltip is composed inside a query-heavy system with no
                    // world access, so this is the one place a script failure
                    // is reported through `tracing` alone.
                    tracing::warn!(mod_id = %mod_id, %error, "tooltip_build handler failed");
                    continue;
                }
            };
            for command in commands {
                let ScriptCommand::AddTooltipPart { nodes, .. } = command else {
                    tracing::warn!(
                        mod_id = %mod_id,
                        command = command.name(),
                        "only add_tooltip_part is read in reply to tooltip_build"
                    );
                    continue;
                };
                for node in nodes {
                    match slotted_model::from_value::<UiNodeDef>(node.0) {
                        Ok(node) => out.push(node),
                        Err(error) => tracing::warn!(
                            mod_id = %mod_id,
                            %error,
                            "tooltip node is not a UiNodeDef"
                        ),
                    }
                }
            }
        }
    }
}

/// A `RegisterWidget` entry: a `UiNodeDef` template whose
/// `Anchor { id: "children" }` receives the caller's children.
#[derive(Debug, Clone, PartialEq)]
pub struct TemplateWidget {
    /// Which kind this template is registered under.
    pub kind: WidgetKind,
    /// The template.
    pub template: UiNodeDef,
}

/// Replaces every `Anchor { id: "children" }` in `node` with `children`.
fn splice(node: &mut UiNodeDef, children: &[UiNodeDef]) {
    let Some(list) = node.children_mut() else {
        return;
    };
    let mut out = Vec::with_capacity(list.len());
    for mut child in list.drain(..) {
        if matches!(&child, UiNodeDef::Anchor { id } if id.0 == CHILDREN_ANCHOR) {
            out.extend(children.iter().cloned());
        } else {
            splice(&mut child, children);
            out.push(child);
        }
    }
    *list = out;
}

impl Widget for TemplateWidget {
    fn spawn(
        &self,
        ctx: &mut SpawnCtx<'_>,
        _params: &slotted_registry::Value,
        children: &[UiNodeDef],
    ) -> Entity {
        // A template whose root *is* the anchor has nowhere to hang the
        // children, so it becomes a bare node holding them.
        if matches!(&self.template, UiNodeDef::Anchor { id } if id.0 == CHILDREN_ANCHOR) {
            let entity = ctx.spawn_node((
                Node::default(),
                SemanticRole::Custom(self.kind.0.to_string()),
                WidgetNode(self.kind.clone()),
            ));
            ctx.spawn_children(entity, children);
            return entity;
        }
        let mut template = self.template.clone();
        splice(&mut template, children);
        let entity = ctx.spawn_child(&template);
        ctx.world
            .entity_mut(entity)
            .insert(WidgetNode(self.kind.clone()));
        if ctx.world.get::<SemanticRole>(entity).is_none() {
            ctx.world
                .entity_mut(entity)
                .insert(SemanticRole::Custom(self.kind.0.to_string()));
        }
        entity
    }
}

/// The anchor id template widgets look for, as an [`AnchorId`].
pub fn children_anchor() -> AnchorId {
    AnchorId::new(CHILDREN_ANCHOR)
}

/// The event name a control script subscribes to for dynamic parts.
pub const DYNAMIC_EVENT: &str = TOOLTIP_BUILD;
