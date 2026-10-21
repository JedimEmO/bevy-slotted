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
    /// A switch or checkbox (menus M1).
    Toggle,
    /// A slider (menus M1).
    Slider,
    /// A select's value pill (menus M1); its popup options are `Button`s.
    Select,
    /// A segmented control (menus M1); its segments are `Tab`s.
    RadioGroup,
    /// A key binding row (menus M1).
    KeyBinding,
    /// A list of rows (menus M1); rows are `ListItem`s.
    List,
    /// One list row (menus M1).
    ListItem,
    /// A scrolling panel (menus M1).
    ScrollView,
    /// A tab bar (menus M1); its tabs are `Tab`s.
    Tabs,
    /// A separator, spacer or image (menus M1).
    Decor,
    /// A widget with no better role.
    Custom(String),
}

impl SemanticRole {
    /// The AccessKit role announced for this node.
    pub fn accesskit_role(&self) -> accesskit::Role {
        use accesskit::Role;
        match self {
            Self::Screen => Role::Dialog,
            Self::Panel | Self::Anchor | Self::Carried | Self::Decor | Self::Custom(_) => {
                Role::GenericContainer
            }
            Self::Grid | Self::Hotbar => Role::Grid,
            Self::Slot | Self::RecipeSlot => Role::Cell,
            Self::Button => Role::Button,
            Self::Text => Role::Label,
            Self::Tooltip => Role::Tooltip,
            Self::Rail => Role::Toolbar,
            Self::Browser | Self::HudLayer => Role::Pane,
            Self::TextField | Self::KeyBinding => Role::TextInput,
            Self::Chip => Role::CheckBox,
            Self::Card | Self::Bookmark => Role::ListItem,
            Self::RecipeView => Role::Group,
            Self::Tab | Self::SideTab => Role::Tab,
            Self::Tank | Self::Bar => Role::ProgressIndicator,
            Self::Viewport => Role::Image,
            Self::Toggle => Role::Switch,
            Self::Slider => Role::Slider,
            Self::Select => Role::ComboBox,
            Self::RadioGroup | Self::Tabs => Role::TabList,
            Self::List => Role::ListBox,
            Self::ListItem => Role::ListBoxOption,
            Self::ScrollView => Role::ScrollView,
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
#[derive(Component, Debug, Clone, PartialEq)]
pub struct LocText {
    /// The key.
    pub key: crate::def::LocKey,
    /// Fluent arguments (menus M1 contract 2.3).
    pub args: crate::loc::LocArgs,
}

impl LocText {
    /// A key with no arguments.
    pub fn new(key: crate::def::LocKey) -> Self {
        Self {
            key,
            args: crate::loc::LocArgs::new(),
        }
    }

    /// A key with arguments.
    pub fn with_args(key: crate::def::LocKey, args: crate::loc::LocArgs) -> Self {
        Self { key, args }
    }
}

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

/// Every live [`TestId`] by name, kept exact by observers on the component's
/// insert and discard (a despawn discards too), so a system that drives a
/// handful of known rows does that many lookups instead of scanning and
/// string-matching every tagged node of every screen each frame.
///
/// An id may stand on several nodes (the same row in two layers, a screen's
/// `close` beside another's), so a name maps to every holder, in insertion
/// order. The map is ordered so a family of rows (`vitals.`) is one range.
#[derive(Resource, Debug, Default)]
pub struct TestIdIndex {
    by_name: std::collections::BTreeMap<String, Vec<Entity>>,
}

impl TestIdIndex {
    /// Every node carrying exactly `id`.
    #[must_use]
    pub fn entities(&self, id: &str) -> &[Entity] {
        self.by_name.get(id).map_or(&[], Vec::as_slice)
    }

    /// Every node whose id starts with `prefix`, with its id.
    pub fn with_prefix<'a>(
        &'a self,
        prefix: &'a str,
    ) -> impl Iterator<Item = (&'a str, Entity)> + 'a {
        self.by_name
            .range::<str, _>((
                std::ops::Bound::Included(prefix),
                std::ops::Bound::Unbounded,
            ))
            .take_while(move |(id, _)| id.starts_with(prefix))
            .flat_map(|(id, nodes)| nodes.iter().map(move |node| (id.as_str(), *node)))
    }

    /// Registers the index and the observers that keep it, once. The UI
    /// plugin calls it; a test that runs one HUD system on a bare world
    /// calls it too.
    pub fn install(world: &mut World) {
        if world.contains_resource::<Self>() {
            return;
        }
        world.init_resource::<Self>();
        world.add_observer(index_test_id);
        world.add_observer(unindex_test_id);
    }

    /// How many nodes carry an id at all.
    #[must_use]
    pub fn len(&self) -> usize {
        self.by_name.values().map(Vec::len).sum()
    }

    /// Whether no node carries an id.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.by_name.is_empty()
    }

    fn add(&mut self, id: &str, entity: Entity) {
        let nodes = self.by_name.entry(id.to_owned()).or_default();
        if !nodes.contains(&entity) {
            nodes.push(entity);
        }
    }

    fn remove(&mut self, id: &str, entity: Entity) {
        if let Some(nodes) = self.by_name.get_mut(id) {
            nodes.retain(|node| *node != entity);
            if nodes.is_empty() {
                self.by_name.remove(id);
            }
        }
    }
}

/// Observer on `Insert<TestId>`: files the node under its id.
pub(crate) fn index_test_id(
    trigger: On<Insert, TestId>,
    ids: Query<&TestId>,
    mut index: ResMut<TestIdIndex>,
) {
    if let Ok(id) = ids.get(trigger.entity) {
        index.add(&id.0, trigger.entity);
    }
}

/// Observer on `Discard<TestId>`: the old id is still readable, so a changed
/// id or a despawn takes the node out of its old entry before anything else.
pub(crate) fn unindex_test_id(
    trigger: On<Discard, TestId>,
    ids: Query<&TestId>,
    mut index: ResMut<TestIdIndex>,
) {
    if let Ok(id) = ids.get(trigger.entity) {
        index.remove(&id.0, trigger.entity);
    }
}
