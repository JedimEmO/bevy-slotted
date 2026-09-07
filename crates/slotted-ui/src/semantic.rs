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
    /// The item browser panel root (Phase 3, `slotted-browser`).
    Browser,
    /// A text input, such as the browser's search field.
    TextField,
    /// A toggle chip, such as a browser category filter.
    Chip,
    /// A browser card: one ingredient entry in the grid.
    Card,
    /// The browser's recipe view.
    RecipeView,
    /// One ingredient position inside a recipe layout.
    RecipeSlot,
    /// A tab, such as a recipe category tab.
    Tab,
    /// One bookmark in the browser's bookmark strip.
    Bookmark,
    /// A fluid tank (Phase 6).
    Tank,
    /// A bar or progress arrow (Phase 6).
    Bar,
    /// A side tab root (Phase 6); its header is a `Button`.
    SideTab,
    /// A 3D viewport (Phase 6).
    Viewport,
    /// A HUD layer root (Phase 6).
    HudLayer,
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
            Self::Slot | Self::RecipeSlot => Role::Cell,
            Self::Button => Role::Button,
            Self::Text => Role::Label,
            Self::Tooltip => Role::Tooltip,
            Self::Rail => Role::Toolbar,
            Self::Browser | Self::HudLayer => Role::Pane,
            Self::TextField => Role::TextInput,
            Self::Chip => Role::CheckBox,
            Self::Card | Self::Bookmark => Role::ListItem,
            Self::RecipeView => Role::Group,
            Self::Tab | Self::SideTab => Role::Tab,
            Self::Tank | Self::Bar => Role::ProgressIndicator,
            Self::Viewport => Role::Image,
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
    /// How it sits on the stack (menus contract 1.3).
    pub presentation: crate::def::Presentation,
    /// The node that took focus when the screen opened, once the nav system
    /// has chosen it (menus contract 2.4).
    pub initial_focus: Option<Entity>,
}

/// The `initial_focus` id a screen asked for, kept on the root until the nav
/// system resolves it into [`ScreenRoot::initial_focus`].
#[derive(Component, Debug, Clone, Default, PartialEq, Eq)]
pub struct ScreenFocusHint(pub Option<String>);

/// On the root entity of a `Custom` widget or a built-in widget with a kind.
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub struct WidgetNode(pub WidgetKind);

/// The localisation key a `Text` node was spawned from.
///
/// `spawn_text` writes the key verbatim into `Text`; `slotted-packs` resolves
/// it through the layered Fluent bundles and rewrites `Text` when a locale
/// loads or changes (Phase 4 contract section 2.7). `SemanticLabel` keeps the
/// key so locators and snapshots do not depend on a language.
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub struct LocText(pub crate::def::LocKey);

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
