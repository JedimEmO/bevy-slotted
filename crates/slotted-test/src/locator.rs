//! Locators over the semantic tree.

use std::fmt;
use std::fmt::Write as _;

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
///
/// ```
/// use slotted_test::prelude::*;
///
/// let loc = by::role(SemanticRole::Slot)
///     .tag("region", "chest")
///     .visible()
///     .index(0);
/// assert_eq!(loc.index, Some(0));
/// assert_eq!(loc.to_string(), r#"role == Slot and tag region="chest" and visible, index 0"#);
/// ```
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
    /// Only nodes that are laid out and shown.
    pub visible: bool,
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

    /// Only nodes that are laid out with a non-zero size and not hidden.
    ///
    /// Unlike [`nth_visible`](Self::nth_visible) this is a filter, not a
    /// selector: it narrows the match set and leaves the count to `find`.
    #[must_use]
    pub fn visible(mut self) -> Self {
        self.visible = true;
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

    /// The criteria this locator applies to a single entity, in the order
    /// they are reported. `index` and `nth_visible` are not criteria; they
    /// select from the matches.
    fn criteria(&self) -> Vec<Criterion<'_>> {
        let mut out = Vec::new();
        if let Some(role) = &self.role {
            out.push(Criterion::Role(role));
        }
        for (k, v) in &self.tags {
            out.push(Criterion::Tag(k, v));
        }
        if let Some(id) = &self.test_id {
            out.push(Criterion::TestId(id));
        }
        if let Some(text) = &self.text {
            out.push(Criterion::Text(text));
        }
        if let Some(anchor) = &self.anchor {
            out.push(Criterion::Anchor(anchor));
        }
        if let Some(kind) = &self.screen {
            out.push(Criterion::Screen(kind));
        }
        if let Some(widget) = &self.widget {
            out.push(Criterion::Widget(widget));
        }
        if let Some(item) = &self.item {
            out.push(Criterion::Item(item));
        }
        if let Some(ancestor) = self.within {
            out.push(Criterion::Within(ancestor));
        }
        if self.visible {
            out.push(Criterion::Visible);
        }
        out
    }

    /// Does `entity` satisfy every criterion except `index`/`nth_visible`?
    pub fn matches(&self, world: &World, entity: Entity) -> bool {
        world.get_entity(entity).is_ok() && self.criteria().iter().all(|c| c.holds(world, entity))
    }

    /// Every matching entity in tree order, `index`/`nth_visible` applied.
    pub fn resolve(&self, world: &World) -> Vec<Entity> {
        let out = self.matching(world, None);
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

    /// Entities matching every criterion except the one at `relax`, in
    /// locator order.
    fn matching(&self, world: &World, relax: Option<usize>) -> Vec<Entity> {
        let criteria = self.criteria();
        candidate_order(world)
            .into_iter()
            .filter(|e| {
                criteria
                    .iter()
                    .enumerate()
                    .all(|(i, c)| Some(i) == relax || c.holds(world, *e))
            })
            .collect()
    }

    /// Nodes that fail exactly one criterion, each with the criterion it
    /// fails. This is what turns "no node matches" into something a test
    /// author can act on. At most `limit` entries, nearest first.
    pub fn near_misses(&self, world: &World, limit: usize) -> Vec<(Entity, String)> {
        let criteria = self.criteria();
        let mut out: Vec<(Entity, String)> = Vec::new();
        if criteria.len() < 2 {
            // Relaxing the only criterion matches every node, which explains
            // nothing. List the nodes that at least carry the data it reads.
            let Some(only) = criteria.first() else {
                return Vec::new();
            };
            for e in candidate_order(world) {
                if only.reads_present(world, e) {
                    out.push((e, only.describe()));
                }
                if out.len() >= limit {
                    break;
                }
            }
            return out;
        }
        for (i, criterion) in criteria.iter().enumerate() {
            for e in self.matching(world, Some(i)) {
                if !out.iter().any(|(seen, _)| *seen == e) {
                    out.push((e, criterion.describe()));
                }
                if out.len() >= limit {
                    return out;
                }
            }
        }
        out
    }
}

/// Every entity a locator may match, in locator order: depth-first from each
/// screen root, then the HUD layers, then the carried layer, then everything
/// else by entity index.
fn candidate_order(world: &World) -> Vec<Entity> {
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for root in crate::tree::candidate_roots(world) {
        crate::tree::walk(world, root, &mut |e| {
            if seen.insert(e) {
                out.push(e);
            }
        });
    }
    let mut loose: Vec<Entity> = world
        .iter_entities()
        .map(|e| e.id())
        .filter(|e| !seen.contains(e))
        .collect();
    loose.sort_by_key(|e| e.index());
    out.extend(loose);
    out
}

/// One thing a locator asks of a node.
#[derive(Debug)]
enum Criterion<'a> {
    Role(&'a SemanticRole),
    Tag(&'a str, &'a str),
    TestId(&'a str),
    Text(&'a str),
    Anchor(&'a AnchorId),
    Screen(&'a ScreenKind),
    Widget(&'a WidgetKind),
    Item(&'a Namespaced),
    Within(Entity),
    Visible,
}

impl Criterion<'_> {
    fn holds(&self, world: &World, entity: Entity) -> bool {
        let Ok(e) = world.get_entity(entity) else {
            return false;
        };
        match self {
            Self::Role(role) => e.get::<SemanticRole>() == Some(*role),
            Self::Tag(k, v) => e.get::<Tags>().is_some_and(|t| t.get(k) == Some(*v)),
            Self::TestId(id) => e.get::<TestId>().map(|t| t.0.as_str()) == Some(*id),
            Self::Text(text) => {
                e.get::<SemanticLabel>().map(|l| l.0.as_str()) == Some(*text)
                    || e.get::<Text>().map(|t| t.0.as_str()) == Some(*text)
            }
            Self::Anchor(anchor) => e.get::<AnchorNode>().map(|a| &a.0) == Some(*anchor),
            Self::Screen(kind) => e.get::<ScreenRoot>().map(|s| &s.kind) == Some(*kind),
            Self::Widget(widget) => e.get::<WidgetNode>().map(|w| &w.0) == Some(*widget),
            Self::Item(item) => {
                let Some(stack) = e.get::<ItemView>().and_then(|v| v.stack.clone()) else {
                    return false;
                };
                world
                    .get_resource::<slotted_ecs::Registries>()
                    .and_then(|r| r.items.name_of(stack.id))
                    == Some(*item)
            }
            Self::Within(ancestor) => is_descendant(world, entity, *ancestor),
            Self::Visible => crate::queries::is_visible(world, entity),
        }
    }

    /// Does the node carry the data this criterion reads at all? Used to pick
    /// the nodes worth listing when a single-criterion locator finds nothing.
    fn reads_present(&self, world: &World, entity: Entity) -> bool {
        let Ok(e) = world.get_entity(entity) else {
            return false;
        };
        match self {
            Self::Tag(..) => e.contains::<Tags>(),
            Self::TestId(_) => e.contains::<TestId>(),
            Self::Text(_) => e.contains::<SemanticLabel>() || e.contains::<Text>(),
            Self::Anchor(_) => e.contains::<AnchorNode>(),
            Self::Screen(_) => e.contains::<ScreenRoot>(),
            Self::Widget(_) => e.contains::<WidgetNode>(),
            Self::Item(_) => e.contains::<ItemView>(),
            Self::Role(_) | Self::Within(_) | Self::Visible => e.contains::<SemanticRole>(),
        }
    }

    fn describe(&self) -> String {
        match self {
            Self::Role(role) => format!("role == {role:?}"),
            Self::Tag(k, v) => format!("tag {k}={v:?}"),
            Self::TestId(id) => format!("test_id == {id:?}"),
            Self::Text(t) => format!("text == {t:?}"),
            Self::Anchor(a) => format!("anchor == {:?}", a.0),
            Self::Screen(k) => format!("screen == {}", k.0),
            Self::Widget(w) => format!("widget_kind == {}", w.0),
            Self::Item(i) => format!("holds item {i}"),
            Self::Within(e) => format!("within {e}"),
            Self::Visible => "visible".to_owned(),
        }
    }
}

impl fmt::Display for Locator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let parts: Vec<String> = self.criteria().iter().map(Criterion::describe).collect();
        if parts.is_empty() {
            f.write_str("any node")?;
        } else {
            f.write_str(&parts.join(" and "))?;
        }
        if let Some(n) = self.index {
            write!(f, ", index {n}")?;
        }
        if let Some(n) = self.nth_visible {
            write!(f, ", visible #{n}")?;
        }
        Ok(())
    }
}

/// A one-line description of what a node is, for failure messages.
pub fn describe(world: &World, entity: Entity) -> String {
    let Ok(e) = world.get_entity(entity) else {
        return format!("{entity} (despawned)");
    };
    let mut s = match e.get::<SemanticRole>() {
        Some(role) => format!("{entity} {role:?}"),
        None => format!("{entity} (no SemanticRole)"),
    };
    if let Some(id) = e.get::<TestId>() {
        let _ = write!(s, " test_id={:?}", id.0);
    }
    if let Some(label) = e.get::<SemanticLabel>() {
        let _ = write!(s, " label={:?}", label.0);
    }
    if let Some(root) = e.get::<ScreenRoot>() {
        let _ = write!(s, " screen={}", root.kind.0);
    }
    if let Some(widget) = e.get::<WidgetNode>() {
        let _ = write!(s, " widget={}", widget.0.0);
    }
    if let Some(tags) = e.get::<Tags>()
        && !tags.0.is_empty()
    {
        let pairs: Vec<String> = tags.0.iter().map(|(k, v)| format!("{k}={v}")).collect();
        let _ = write!(s, " tags[{}]", pairs.join(", "));
    }
    if !crate::queries::is_visible(world, entity) {
        s.push_str(" (not visible)");
    }
    s
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

    /// By label text, or by the node's own `Text` content.
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

    /// A HUD layer root by id (Phase 6).
    ///
    /// A layer root's `SemanticLabel` is its id, so this is the same match a
    /// reader of the tree would make by eye.
    pub fn hud_layer(id: &str) -> Locator {
        Locator {
            text: Some(id.to_owned()),
            ..role(SemanticRole::HudLayer)
        }
    }

    /// A node by its widget kind.
    pub fn widget_kind(kind: WidgetKind) -> Locator {
        Locator {
            widget: Some(kind),
            ..Default::default()
        }
    }
}
