//! Locators over the semantic tree.

use bevy::prelude::*;
use slotted_model::Namespaced;
use slotted_ui::{
    AnchorId, AnchorNode, ItemView, ScreenKind, ScreenRoot, SemanticLabel, SemanticRole, Tags,
    TestId, WidgetKind, WidgetNode,
};

/// A description of the node(s) a test wants. Build one with [`by`], refine
/// with the methods, resolve with `UiHarness::find`.
///
/// All criteria must match. Results are in tree order (depth-first from each
/// screen root, then any matching entity outside a screen).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Locator {
    /// Semantic role.
    pub role: Option<SemanticRole>,
    /// Every tag pair must be present.
    pub tags: Vec<(String, String)>,
    /// `TestId`.
    pub test_id: Option<String>,
    /// `SemanticLabel` equals, or the node's `Text` equals.
    pub text: Option<String>,
    /// `AnchorNode`.
    pub anchor: Option<AnchorId>,
    /// `ScreenRoot::kind`.
    pub screen: Option<ScreenKind>,
    /// `WidgetNode`.
    pub widget: Option<WidgetKind>,
    /// `ItemView` holds this item.
    pub item: Option<Namespaced>,
    /// Must be a descendant of this entity.
    pub within: Option<Entity>,
    /// Pick the n-th match.
    pub index: Option<usize>,
    /// Pick the n-th match that is visible.
    pub nth_visible: Option<usize>,
}

impl Locator {
    /// Require a tag.
    #[must_use]
    pub fn tag(mut self, key: &str, value: &str) -> Self {
        self.tags.push((key.to_owned(), value.to_owned()));
        self
    }

    /// The n-th match.
    #[must_use]
    pub fn index(mut self, n: usize) -> Self {
        self.index = Some(n);
        self
    }

    /// The n-th visible match.
    #[must_use]
    pub fn nth_visible(mut self, n: usize) -> Self {
        self.nth_visible = Some(n);
        self
    }

    /// Only descendants of `ancestor`.
    #[must_use]
    pub fn within(mut self, ancestor: Entity) -> Self {
        self.within = Some(ancestor);
        self
    }

    /// Only slots whose `ItemView` holds `item` (`"minecraft:cobblestone"`).
    #[must_use]
    pub fn with_item(mut self, item: &str) -> Self {
        self.item =
            Some(Namespaced::parse(item).unwrap_or_else(|e| panic!("bad item id {item:?}: {e}")));
        self
    }

    /// Does `entity` satisfy every criterion except `index`/`nth_visible`?
    // PHASE2-IMPL: agent C. `text` should also match `Text` content; `item`
    // needs the `Registries` resource to map `ItemId` back to a name.
    pub fn matches(&self, world: &World, entity: Entity) -> bool {
        let e = world.entity(entity);
        if let Some(role) = &self.role
            && e.get::<SemanticRole>() != Some(role)
        {
            return false;
        }
        if !self.tags.is_empty() {
            let Some(tags) = e.get::<Tags>() else {
                return false;
            };
            if !self
                .tags
                .iter()
                .all(|(k, v)| tags.get(k) == Some(v.as_str()))
            {
                return false;
            }
        }
        if let Some(id) = &self.test_id
            && e.get::<TestId>().map(|t| t.0.as_str()) != Some(id.as_str())
        {
            return false;
        }
        if let Some(text) = &self.text
            && e.get::<SemanticLabel>().map(|l| l.0.as_str()) != Some(text.as_str())
        {
            return false;
        }
        if let Some(anchor) = &self.anchor
            && e.get::<AnchorNode>().map(|a| &a.0) != Some(anchor)
        {
            return false;
        }
        if let Some(kind) = &self.screen
            && e.get::<ScreenRoot>().map(|s| &s.kind) != Some(kind)
        {
            return false;
        }
        if let Some(widget) = &self.widget
            && e.get::<WidgetNode>().map(|w| &w.0) != Some(widget)
        {
            return false;
        }
        if let Some(item) = &self.item {
            let Some(view) = e.get::<ItemView>() else {
                return false;
            };
            let Some(stack) = &view.stack else {
                return false;
            };
            let Some(registries) = world.get_resource::<slotted_ecs::Registries>() else {
                return false;
            };
            if registries.items.name_of(stack.id) != Some(item) {
                return false;
            }
        }
        if let Some(ancestor) = self.within
            && !is_descendant(world, entity, ancestor)
        {
            return false;
        }
        true
    }

    /// Every matching entity in tree order, `index`/`nth_visible` applied.
    pub fn resolve(&self, world: &World) -> Vec<Entity> {
        let mut out = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for root in crate::tree::roots(world) {
            crate::tree::walk(world, root, &mut |e| {
                seen.insert(e);
                if self.matches(world, e) {
                    out.push(e);
                }
            });
        }
        // Nodes outside any screen (a game's own UI, or a bare test scene),
        // in entity order.
        let mut loose: Vec<Entity> = world
            .iter_entities()
            .map(|e| e.id())
            .filter(|e| !seen.contains(e) && self.matches(world, *e))
            .collect();
        loose.sort_by_key(|e| e.index());
        out.extend(loose);
        if let Some(n) = self.nth_visible {
            return out
                .into_iter()
                .filter(|e| crate::queries::is_visible(world, *e))
                .nth(n)
                .into_iter()
                .collect();
        }
        if let Some(n) = self.index {
            return out.into_iter().nth(n).into_iter().collect();
        }
        out
    }
}

fn is_descendant(world: &World, mut e: Entity, ancestor: Entity) -> bool {
    loop {
        if e == ancestor {
            return true;
        }
        match world.get::<ChildOf>(e) {
            Some(p) => e = p.parent(),
            None => return false,
        }
    }
}

/// Locator constructors.
pub mod by {
    use super::{AnchorId, Locator, ScreenKind, SemanticRole, WidgetKind};

    /// By semantic role.
    pub fn role(role: SemanticRole) -> Locator {
        Locator {
            role: Some(role),
            ..Default::default()
        }
    }

    /// By tag.
    pub fn tag(key: &str, value: &str) -> Locator {
        Locator::default().tag(key, value)
    }

    /// By `TestId`.
    pub fn test_id(id: &str) -> Locator {
        Locator {
            test_id: Some(id.to_owned()),
            ..Default::default()
        }
    }

    /// By label text.
    pub fn text(text: &str) -> Locator {
        Locator {
            text: Some(text.to_owned()),
            ..Default::default()
        }
    }

    /// By anchor id.
    pub fn anchor(id: &str) -> Locator {
        Locator {
            anchor: Some(AnchorId::new(id)),
            ..Default::default()
        }
    }

    /// The root of a screen kind.
    pub fn screen(kind: ScreenKind) -> Locator {
        Locator {
            screen: Some(kind),
            ..Default::default()
        }
    }

    /// By widget kind.
    pub fn widget_kind(kind: WidgetKind) -> Locator {
        Locator {
            widget: Some(kind),
            ..Default::default()
        }
    }
}
