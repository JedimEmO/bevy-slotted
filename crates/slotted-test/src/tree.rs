//! The `screen_tree()` snapshot type and tree walking helpers.

use std::collections::BTreeMap;
use std::fmt;
use std::fmt::Write as _;

use bevy::prelude::*;
use serde::Serialize;
use slotted_ui::{
    AnchorNode, CarriedLayer, ItemView, ScreenRoot, SemanticLabel, SemanticRole, Tags, TestId,
    WidgetNode,
};

/// A stack, as a snapshot sees it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ItemSummary {
    /// Item name, or `#<id>` when the registries are absent.
    pub id: String,
    /// Count.
    pub count: u32,
}

/// One semantic node.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TreeNode {
    /// Role.
    pub role: SemanticRole,
    /// Label.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// `TestId`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub test_id: Option<String>,
    /// Tags.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub tags: BTreeMap<String, String>,
    /// Widget kind.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub widget: Option<String>,
    /// Anchor id.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub anchor: Option<String>,
    /// Screen kind, on roots.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub screen: Option<String>,
    /// Visible after layout.
    pub visible: bool,
    /// Has keyboard focus.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub focused: bool,
    /// Displayed stack.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub item: Option<ItemSummary>,
    /// Children with a semantic role. Purely visual nodes are elided; their
    /// semantic descendants are lifted to this level.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<TreeNode>,
}

/// Every open screen (and the carried layer), theme-independent. Meant for
/// `insta` snapshots through [`crate::assert_tree_snapshot!`].
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ScreenTree {
    /// One entry per `ScreenRoot`, plus the carried layer last.
    pub roots: Vec<TreeNode>,
}

impl fmt::Display for ScreenTree {
    /// An indented text tree, one node per line, in layout order. Stable
    /// across themes and layout tweaks, so it snapshots well with
    /// `insta::assert_snapshot!`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.roots.is_empty() {
            return f.write_str("<no screens>");
        }
        for (i, root) in self.roots.iter().enumerate() {
            if i > 0 {
                writeln!(f)?;
            }
            root.fmt_indented(f, 0)?;
        }
        Ok(())
    }
}

impl fmt::Display for TreeNode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.fmt_indented(f, 0)
    }
}

impl TreeNode {
    /// This node and its descendants, two spaces deeper per level. The last
    /// line carries no newline, so callers control the trailing break.
    fn fmt_indented(&self, f: &mut fmt::Formatter<'_>, depth: usize) -> fmt::Result {
        write!(f, "{:indent$}{}", "", self.summary(), indent = depth * 2)?;
        for child in &self.children {
            writeln!(f)?;
            child.fmt_indented(f, depth + 1)?;
        }
        Ok(())
    }

    /// The one-line form: role first, then only what is set.
    fn summary(&self) -> String {
        let mut s = match &self.role {
            SemanticRole::Custom(name) => format!("Custom({name})"),
            role => format!("{role:?}"),
        };
        if let Some(kind) = &self.screen {
            let _ = write!(s, " screen={kind}");
        }
        if let Some(id) = &self.test_id {
            let _ = write!(s, " #{id}");
        }
        if let Some(label) = &self.label {
            let _ = write!(s, " {label:?}");
        }
        if let Some(item) = &self.item {
            let _ = write!(s, " [{} x{}]", item.id, item.count);
        }
        if !self.tags.is_empty() {
            let pairs: Vec<String> = self.tags.iter().map(|(k, v)| format!("{k}={v}")).collect();
            let _ = write!(s, " tags[{}]", pairs.join(" "));
        }
        if let Some(widget) = &self.widget {
            let _ = write!(s, " widget={widget}");
        }
        if let Some(anchor) = &self.anchor {
            let _ = write!(s, " anchor={anchor}");
        }
        if !self.visible {
            s.push_str(" (hidden)");
        }
        if self.focused {
            s.push_str(" (focused)");
        }
        s
    }
}

/// Screen roots in entity-index order, then the carried layer.
///
/// Index order rather than `Entity` order: `Entity`'s `Ord` compares
/// generation first, so a root that landed on a recycled index would sort
/// after a later one. Index order matches spawn order for as long as nothing
/// is despawned, and is stable regardless.
pub fn roots(world: &World) -> Vec<Entity> {
    let mut screens: Vec<Entity> = world
        .iter_entities()
        .filter(EntityRef::contains::<ScreenRoot>)
        .map(|e| e.id())
        .collect();
    screens.sort_by_key(|e| e.index());
    screens.extend(
        world
            .iter_entities()
            .filter(EntityRef::contains::<CarriedLayer>)
            .map(|e| e.id()),
    );
    screens
}

/// Depth-first walk over `Children`, including `root`.
pub fn walk(world: &World, root: Entity, visit: &mut impl FnMut(Entity)) {
    visit(root);
    if let Some(children) = world.get::<Children>(root) {
        for child in children.iter() {
            walk(world, child, visit);
        }
    }
}

/// Builds the tree for `root`. `None` if the root has no role.
pub fn build(world: &World, root: Entity) -> Option<TreeNode> {
    let mut nodes = collect_semantic_children(world, root);
    if world.get::<SemanticRole>(root).is_some() {
        Some(node_of(world, root, std::mem::take(&mut nodes)))
    } else {
        nodes.pop()
    }
}

fn collect_semantic_children(world: &World, parent: Entity) -> Vec<TreeNode> {
    let mut out = Vec::new();
    if let Some(children) = world.get::<Children>(parent) {
        for child in children.iter() {
            if world.get::<SemanticRole>(child).is_some() {
                let grandchildren = collect_semantic_children(world, child);
                out.push(node_of(world, child, grandchildren));
            } else {
                out.extend(collect_semantic_children(world, child));
            }
        }
    }
    out
}

fn node_of(world: &World, e: Entity, children: Vec<TreeNode>) -> TreeNode {
    let role = world
        .get::<SemanticRole>(e)
        .cloned()
        .unwrap_or(SemanticRole::Custom("unknown".into()));
    let item = world
        .get::<ItemView>(e)
        .and_then(|v| v.stack.as_ref())
        .map(|s| ItemSummary {
            id: world
                .get_resource::<slotted_ecs::Registries>()
                .and_then(|r| r.items.name_of(s.id).map(ToString::to_string))
                .unwrap_or_else(|| format!("#{}", s.id.0)),
            count: s.count,
        });
    TreeNode {
        role,
        label: world.get::<SemanticLabel>(e).map(|l| l.0.clone()),
        test_id: world.get::<TestId>(e).map(|t| t.0.clone()),
        tags: world
            .get::<Tags>(e)
            .map(|t| t.0.clone())
            .unwrap_or_default(),
        widget: world.get::<WidgetNode>(e).map(|w| w.0.0.to_string()),
        anchor: world.get::<AnchorNode>(e).map(|a| a.0.0.clone()),
        screen: world.get::<ScreenRoot>(e).map(|s| s.kind.0.to_string()),
        visible: crate::queries::is_visible(world, e),
        focused: crate::queries::is_focused(world, e),
        item,
        children,
    }
}
