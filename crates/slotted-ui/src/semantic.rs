//! The semantic layer: what a node *is*, independent of how it looks.
//!
//! `bevy_a11y` reads it to build the AccessKit tree; `slotted-test` reads it
//! to locate nodes. Neither needs anything else from this crate.

use bevy::a11y::AccessibilityNode;
use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::def::{AnchorId, ScreenKind, WidgetKind};

/// What kind of thing a node is.
#[derive(Component, Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SemanticRole {
    /// A screen root. Carries [`ScreenRoot`].
    Screen,
    /// A container panel.
    Panel,
    /// A grid of slots.
    Grid,
    /// One slot. Carries `slotted_ecs::SlotRef`.
    Slot,
    /// A button.
    Button,
    /// Static text.
    Text,
    /// A tooltip.
    Tooltip,
    /// The action rail.
    Rail,
    /// The hotbar row.
    Hotbar,
    /// An injection anchor. Carries [`AnchorNode`].
    Anchor,
    /// The carried-stack layer.
    Carried,
    /// A widget with no better role.
    Custom(String),
}

impl SemanticRole {
    /// The AccessKit role announced for this node.
    pub fn accesskit_role(&self) -> accesskit::Role {
        use accesskit::Role;
        match self {
            Self::Screen => Role::Dialog,
            Self::Panel | Self::Anchor | Self::Carried | Self::Custom(_) => Role::GenericContainer,
            Self::Grid | Self::Hotbar => Role::Grid,
            Self::Slot => Role::Cell,
            Self::Button => Role::Button,
            Self::Text => Role::Label,
            Self::Tooltip => Role::Tooltip,
            Self::Rail => Role::Toolbar,
        }
    }
}

/// Human-readable label: the item name and count for a slot, the button's
/// text, the screen title. Kept current by the widgets.
#[derive(Component, Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct SemanticLabel(pub String);

/// A consumer-chosen id for tests. From the `test_id` tag in data, or attached
/// directly in Rust.
#[derive(Component, Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TestId(pub String);

impl TestId {
    /// From a name.
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }
}

/// On the root entity of a spawned screen.
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub struct ScreenRoot {
    /// Which screen.
    pub kind: ScreenKind,
    /// The `OpenMenu` entity this screen drives, if any.
    pub menu: Option<Entity>,
}

/// On the root entity of a `Custom` widget or a built-in widget with a kind.
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub struct WidgetNode(pub WidgetKind);

/// On an anchor node.
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub struct AnchorNode(pub AnchorId);

/// Writes `AccessibilityNode` from [`SemanticRole`] and [`SemanticLabel`]
/// whenever either changes. `SlottedUiSet::Semantics`.
pub fn sync_accessibility(
    mut commands: Commands,
    changed: Query<
        (Entity, &SemanticRole, Option<&SemanticLabel>),
        Or<(Changed<SemanticRole>, Changed<SemanticLabel>)>,
    >,
) {
    for (entity, role, label) in &changed {
        let mut node = accesskit::Node::new(role.accesskit_role());
        if let Some(label) = label {
            node.set_label(label.0.as_str());
        }
        commands.entity(entity).insert(AccessibilityNode(node));
    }
}
