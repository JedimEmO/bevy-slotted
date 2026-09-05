//! Screen trees as data. `docs/PLAN.md` section 4.5.
//!
//! Every type here is `serde`, so a `ScreenDef` round-trips through RON and
//! through the registry's untyped `Value` payload. Handles never appear;
//! images and icons are asset paths or item names resolved at spawn.

use std::collections::BTreeMap;

use bevy::ecs::component::Component;
use serde::{Deserialize, Serialize};
use slotted_model::{InventoryRef, Namespaced, PropertyId, SlotIx};
use slotted_registry::Value;
use slotted_theme::Role;

/// Unit enums are written as lowercase strings (`"column"`, `"right"`), not
/// RON variants, so they survive the registry's untyped `Value` round trip
/// (see `slotted_registry::defs`) and map onto Lua strings unchanged.
macro_rules! string_enum {
    ($(#[$m:meta])* $vis:vis enum $name:ident { $($(#[$vm:meta])* $variant:ident = $text:literal),+ $(,)? }) => {
        $(#[$m])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        $vis enum $name {
            $($(#[$vm])* $variant,)+
        }

        impl $name {
            /// The data-file spelling.
            pub const fn as_str(self) -> &'static str {
                match self { $(Self::$variant => $text,)+ }
            }
        }

        impl core::fmt::Display for $name {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                f.write_str(self.as_str())
            }
        }

        impl core::str::FromStr for $name {
            type Err = String;
            fn from_str(s: &str) -> Result<Self, String> {
                match s {
                    $($text => Ok(Self::$variant),)+
                    other => Err(format!(concat!("unknown ", stringify!($name), " `{}`"), other)),
                }
            }
        }

        impl Serialize for $name {
            fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
                s.serialize_str(self.as_str())
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                let s = String::deserialize(d)?;
                s.parse().map_err(serde::de::Error::custom)
            }
        }
    };
}

/// Serialises an externally tagged enum as the one-key map its data form
/// already uses, because RON's own variant syntax (`image("x")`) has no
/// [`ron::Value`] representation and would not survive the registry's untyped
/// payload. Deserialisation needs no help: serde's external tagging already
/// reads a one-key map.
macro_rules! map_variant_serialize {
    ($(#[$m:meta])* $name:ident { $($variant:ident $(($binding:ident))? = $key:literal),+ $(,)? }) => {
        $(#[$m])*
        impl Serialize for $name {
            fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
                use serde::ser::SerializeMap;
                let mut map = s.serialize_map(Some(1))?;
                match self {
                    $(Self::$variant $(($binding))? => {
                        map_variant_serialize!(@entry map, $key $(, $binding)?);
                    })+
                }
                map.end()
            }
        }
    };
    (@entry $map:ident, $key:literal, $binding:ident) => { $map.serialize_entry($key, $binding)? };
    (@entry $map:ident, $key:literal) => { $map.serialize_entry($key, &())? };
}

/// Which screen. Namespaced like everything else: `copper_chest:chest`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ScreenKind(pub Namespaced);

impl ScreenKind {
    /// Parses `namespace:path`. Panics on a malformed id, like a literal.
    pub fn new(id: &str) -> Self {
        Self(Namespaced::parse(id).unwrap_or_else(|e| panic!("bad ScreenKind {id:?}: {e}")))
    }
}

/// Which widget implementation. Built-ins live under `slotted:`; see
/// [`crate::widgets::kinds`].
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct WidgetKind(pub Namespaced);

impl WidgetKind {
    /// Parses `namespace:path`. Panics on a malformed id, like a literal.
    pub fn new(id: &str) -> Self {
        Self(Namespaced::parse(id).unwrap_or_else(|e| panic!("bad WidgetKind {id:?}: {e}")))
    }
}

/// A named point in a screen where injections attach: `title_end`, `rail`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AnchorId(pub String);

impl AnchorId {
    /// From a name.
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }
}

/// Free-form `key = value` pairs on a node. Locators match on them
/// (`by::tag("region", "chest")`); `test_id` is reserved and becomes a
/// [`TestId`](crate::TestId) component.
#[derive(Component, Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Tags(pub BTreeMap<String, String>);

impl Tags {
    /// No tags.
    pub fn new() -> Self {
        Self::default()
    }

    /// Builder: add one tag.
    #[must_use]
    pub fn with(mut self, key: &str, value: &str) -> Self {
        self.0.insert(key.to_owned(), value.to_owned());
        self
    }

    /// The reserved key that becomes a `TestId`.
    pub const TEST_ID: &'static str = "test_id";

    /// Lookup.
    pub fn get(&self, key: &str) -> Option<&str> {
        self.0.get(key).map(String::as_str)
    }
}

/// A localisation key (`chest.title`). Phase 2 shows the key itself when no
/// locale is loaded.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct LocKey(pub String);

/// A data source for a virtual grid (browser results, a search index).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DataSourceId(pub Namespaced);

string_enum! {
    /// Which text role a [`UiNodeDef::Text`] uses. Maps onto theme text roles.
    pub enum TextRole {
        /// `panel.title`.
        Title = "title",
        /// `text`.
        Body = "body",
        /// `text.muted`.
        Muted = "muted",
        /// `count`.
        Count = "count",
    }
}

impl TextRole {
    /// The theme role.
    pub const fn role(self) -> Role {
        use slotted_theme::roles;
        match self {
            Self::Title => roles::PANEL_TITLE,
            Self::Body => roles::TEXT,
            Self::Muted => roles::TEXT_MUTED,
            Self::Count => roles::COUNT,
        }
    }
}

string_enum! {
    /// Flow direction of a panel.
    pub enum LayoutDirection {
        /// Children left to right.
        Row = "row",
        /// Children top to bottom.
        Column = "column",
    }
}

// The string_enum macro does not derive Default.
#[allow(clippy::derivable_impls)]
impl Default for LayoutDirection {
    fn default() -> Self {
        Self::Column
    }
}

/// Panel layout, the subset of `Node` a screen author sets. Everything else
/// comes from the theme's spacing tokens.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Layout {
    /// Flow.
    #[serde(default)]
    pub direction: LayoutDirection,
    /// Gap between children, in spacing steps (`1.0` = `spacing.sm`).
    #[serde(default)]
    pub gap: f32,
    /// Padding, in spacing steps.
    #[serde(default)]
    pub padding: f32,
    /// Fixed width in px. `None` = fit content.
    #[serde(default)]
    pub width: Option<f32>,
    /// Fixed height in px. `None` = fit content.
    #[serde(default)]
    pub height: Option<f32>,
    /// Centre children on the cross axis.
    #[serde(default)]
    pub center: bool,
}

string_enum! {
    /// Fill direction of a tank.
    pub enum Orientation {
        /// Fills bottom to top.
        Vertical = "vertical",
        /// Fills left to right.
        Horizontal = "horizontal",
    }
}

string_enum! {
    /// Fill direction of a bar.
    pub enum Direction {
        /// Left to right.
        Right = "right",
        /// Right to left.
        Left = "left",
        /// Bottom to top.
        Up = "up",
        /// Top to bottom.
        Down = "down",
    }
}

string_enum! {
    /// Which edge a side tab hangs off.
    pub enum Side {
        /// Left edge.
        Left = "left",
        /// Right edge.
        Right = "right",
    }
}

/// An icon named in data. Resolved to an `IconRef` at spawn. Written as a
/// one-key map: `(item: "demo:chest")` or `(image: "icons/sort.png")`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IconDef {
    /// An item's icon through the `IconSource`.
    Item(Namespaced),
    /// An image asset path.
    Image(String),
}

map_variant_serialize! {
    /// `Image(p)` writes `(image: p)`, not RON's `image(p)`.
    IconDef {
        Item(v) = "item",
        Image(v) = "image",
    }
}

/// What a viewport shows. Written as `(player: ())` or `(item: "demo:chest")`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ViewSubject {
    /// The player model.
    Player,
    /// An item model.
    Item(Namespaced),
}

map_variant_serialize! {
    /// `Player` writes `(player: ())`.
    ViewSubject {
        Player = "player",
        Item(v) = "item",
    }
}

/// One node of a screen tree. `docs/PLAN.md` 4.5, plus a `tags` field on
/// every variant that becomes an entity, so locators and `test_id` work on
/// any node.
///
/// Serialised internally tagged: `(type: "slot_grid", inventory: 0, cols: 9,
/// rows: 3, first: 0)`. A RON variant (`SlotGrid(..)`) would not survive the
/// registry's `Value` round trip, and a Lua table has no variants either;
/// this one shape is what Rust, RON and Lua all produce.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum UiNodeDef {
    /// A themed container.
    Panel {
        /// Theme role (`panel`, `rail`, a custom role).
        role: Role,
        /// Flow, gap, padding, size.
        #[serde(default)]
        layout: Layout,
        /// Children.
        #[serde(default)]
        children: Vec<UiNodeDef>,
        /// Locator tags.
        #[serde(default)]
        tags: Tags,
    },
    /// `cols x rows` slots bound to `first..first + cols*rows` of the menu.
    SlotGrid {
        /// Which inventory the slots draw from; becomes the `region` tag when
        /// no explicit one is set.
        inventory: InventoryRef,
        /// Columns.
        cols: u16,
        /// Rows.
        rows: u16,
        /// First `SlotIx`.
        first: u16,
        /// Locator tags.
        #[serde(default)]
        tags: Tags,
    },
    /// A scrolling grid over a data source (Phase 3).
    VirtualGrid {
        /// Data source.
        source: DataSourceId,
        /// Columns.
        cols: u16,
        /// Locator tags.
        #[serde(default)]
        tags: Tags,
    },
    /// A single slot.
    Slot {
        /// Slot index in the menu.
        slot: SlotIx,
        /// Locator tags.
        #[serde(default)]
        tags: Tags,
    },
    /// A text label.
    Text {
        /// Localisation key.
        key: LocKey,
        /// Style.
        style: TextRole,
        /// Locator tags.
        #[serde(default)]
        tags: Tags,
    },
    /// A button whose behaviour is `widget`.
    Button {
        /// Behaviour kind (`slotted:sort`, `slotted:close`...).
        widget: WidgetKind,
        /// Locator tags.
        #[serde(default)]
        tags: Tags,
    },
    /// A fluid tank bound to two properties (Phase 6).
    Tank {
        /// Amount.
        property: PropertyId,
        /// Capacity.
        capacity: PropertyId,
        /// Fill direction.
        orientation: Orientation,
        /// Locator tags.
        #[serde(default)]
        tags: Tags,
    },
    /// A progress bar bound to two properties (Phase 6).
    Bar {
        /// Value.
        property: PropertyId,
        /// Maximum.
        max: PropertyId,
        /// Fill direction.
        direction: Direction,
        /// Locator tags.
        #[serde(default)]
        tags: Tags,
    },
    /// A tab on the panel's edge (Phase 6).
    SideTab {
        /// Tab icon.
        icon: IconDef,
        /// Which edge.
        side: Side,
        /// Contents.
        #[serde(default)]
        children: Vec<UiNodeDef>,
        /// Locator tags.
        #[serde(default)]
        tags: Tags,
    },
    /// A live 3D view (Phase 6).
    Viewport {
        /// What to show.
        subject: ViewSubject,
        /// Locator tags.
        #[serde(default)]
        tags: Tags,
    },
    /// An injection point. Spawns an empty node carrying `AnchorNode`.
    Anchor {
        /// Anchor name.
        id: AnchorId,
    },
    /// A registered widget kind with free-form parameters.
    Custom {
        /// Which widget.
        kind: WidgetKind,
        /// Parameters, interpreted by the widget. `Value::Unit` when absent.
        #[serde(default = "unit")]
        params: Value,
        /// Children handed to the widget.
        #[serde(default)]
        children: Vec<UiNodeDef>,
        /// Locator tags.
        #[serde(default)]
        tags: Tags,
    },
}

fn unit() -> Value {
    Value::Unit
}

impl UiNodeDef {
    /// The node's tags, empty for `Anchor`.
    pub fn tags(&self) -> Option<&Tags> {
        match self {
            Self::Panel { tags, .. }
            | Self::SlotGrid { tags, .. }
            | Self::VirtualGrid { tags, .. }
            | Self::Slot { tags, .. }
            | Self::Text { tags, .. }
            | Self::Button { tags, .. }
            | Self::Tank { tags, .. }
            | Self::Bar { tags, .. }
            | Self::SideTab { tags, .. }
            | Self::Viewport { tags, .. }
            | Self::Custom { tags, .. } => Some(tags),
            Self::Anchor { .. } => None,
        }
    }

    /// Direct children, empty for leaves.
    pub fn children(&self) -> &[UiNodeDef] {
        match self {
            Self::Panel { children, .. }
            | Self::SideTab { children, .. }
            | Self::Custom { children, .. } => children,
            _ => &[],
        }
    }

    /// Mutable direct children, `None` for leaves.
    pub fn children_mut(&mut self) -> Option<&mut Vec<UiNodeDef>> {
        match self {
            Self::Panel { children, .. }
            | Self::SideTab { children, .. }
            | Self::Custom { children, .. } => Some(children),
            _ => None,
        }
    }

    /// Depth-first walk.
    pub fn walk(&self, visit: &mut impl FnMut(&UiNodeDef)) {
        visit(self);
        for c in self.children() {
            c.walk(visit);
        }
    }

    /// Every anchor id in the tree.
    pub fn anchors(&self) -> Vec<AnchorId> {
        let mut out = Vec::new();
        self.walk(&mut |n| {
            if let Self::Anchor { id } = n {
                out.push(id.clone());
            }
        });
        out
    }
}

/// A whole screen.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScreenDef {
    /// Its id.
    pub kind: ScreenKind,
    /// A screen whose tree this one starts from; this one's anchors and
    /// injections apply on top. Resolved by [`crate::Screens`].
    #[serde(default)]
    pub inherits: Option<ScreenKind>,
    /// The tree.
    pub root: UiNodeDef,
    /// Inventories in the order a quick-move ring walks them. Mirrors
    /// `MenuDef::listring`.
    #[serde(default)]
    pub listring: Vec<InventoryRef>,
}

impl ScreenDef {
    /// Parses a RON screen. The same text works as a registry `screens/*.ron`
    /// payload.
    ///
    /// The text is parsed to an untyped [`Value`] and typed by
    /// [`from_value`](Self::from_value), so a screen reads identically whether
    /// it arrives as text, as a registry payload or as a table from a mod's
    /// `data.lua`. One consequence: an untyped `params` payload keeps its
    /// numbers as `i64`/`f64` rather than the narrowest RON type that fit.
    pub fn from_ron(text: &str) -> Result<Self, ron::error::SpannedError> {
        let value: Value = ron::from_str(text)?;
        Self::from_value(value).map_err(|e| {
            // The failure is in the typed shape, not the syntax, so there is
            // no position to point at; the message names the node.
            let at = ron::error::Position { line: 1, col: 1 };
            ron::error::SpannedError {
                code: e,
                span: ron::error::Span { start: at, end: at },
            }
        })
    }

    /// Converts a registry payload (the untyped `Value` the data stage kept).
    ///
    /// Not `Value::into_rust`: the payload is converted to a
    /// [`slotted_model::Value`] first, so a tree written in a `.ron` file and a
    /// tree registered by a mod's `data.lua` deserialise under one set of
    /// rules and an optional field accepts both `Some(x)` and a bare `x`. See
    /// [`slotted_registry::ron_value`].
    ///
    /// # Errors
    ///
    /// [`ron::Error::Message`] naming the first node that did not fit.
    pub fn from_value(value: Value) -> Result<Self, ron::Error> {
        let message = |m: String| ron::Error::Message(m);
        let untyped = slotted_registry::to_model(&value).map_err(|e| message(e.to_string()))?;
        slotted_model::from_value(untyped).map_err(|e| message(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    const CHEST: &str = r#"
        #![enable(implicit_some)]
        (
            kind: "demo:chest",
            root: (
                type: "panel",
                role: "panel",
                layout: (direction: "column", gap: 2.0, padding: 2.0),
                children: [
                    (type: "panel", role: "panel", layout: (direction: "row"), children: [
                        (type: "text", key: "demo.chest.title", style: "title"),
                        (type: "anchor", id: "title_end"),
                    ]),
                    (type: "slot_grid", inventory: 0, cols: 9, rows: 3, first: 0, tags: {"region": "chest"}),
                    (type: "slot_grid", inventory: 1, cols: 9, rows: 3, first: 27, tags: {"region": "player"}),
                    (type: "custom", kind: "slotted:hotbar", params: (first: 54), tags: {"region": "hotbar", "test_id": "hotbar"}),
                    (type: "custom", kind: "slotted:action_rail", params: (actions: ["sort", "quick_stack", "deposit_all", "loot_all"])),
                    (type: "side_tab", icon: (image: "icons/tab.png"), side: "right", children: [
                        (type: "bar", property: 0, max: 1, direction: "right"),
                    ]),
                ],
            ),
            listring: [0, 1, 2],
        )
    "#;

    #[test]
    fn chest_screen_parses_and_round_trips() {
        let def = ScreenDef::from_ron(CHEST).expect("parses");
        assert_eq!(def.kind, ScreenKind::new("demo:chest"));
        assert_eq!(def.root.anchors(), vec![AnchorId::new("title_end")]);
        let mut slots = 0;
        def.root.walk(&mut |n| {
            if let UiNodeDef::SlotGrid { cols, rows, .. } = n {
                slots += cols * rows;
            }
        });
        assert_eq!(slots, 54);

        let text = ron::ser::to_string_pretty(&def, ron::ser::PrettyConfig::default())
            .expect("serialises");
        assert_eq!(ScreenDef::from_ron(&text).expect("re-parses"), def);

        // The registry keeps screens as untyped values; the same text must
        // survive that path.
        let value: Value = ron::from_str(CHEST).expect("value parses");
        assert_eq!(ScreenDef::from_value(value).expect("typed"), def);
    }
}
