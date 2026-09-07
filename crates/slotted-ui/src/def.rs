//! Screen trees as data. `docs/PLAN.md` section 4.5.
//!
//! Every type here is `serde`, so a `ScreenDef` round-trips through RON and
//! through the registry's untyped `Value` payload. Handles never appear;
//! images and icons are asset paths or item names resolved at spawn.

use std::collections::BTreeMap;

use bevy::asset::Asset;
use bevy::ecs::component::Component;
use bevy::reflect::TypePath;
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
    /// Reserved: the node focus moves to on `Up` (menus contract 2.4).
    pub const NAV_UP: &'static str = "nav.up";
    /// Reserved: the node focus moves to on `Down`.
    pub const NAV_DOWN: &'static str = "nav.down";
    /// Reserved: the node focus moves to on `Left`.
    pub const NAV_LEFT: &'static str = "nav.left";
    /// Reserved: the node focus moves to on `Right`.
    pub const NAV_RIGHT: &'static str = "nav.right";

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

string_enum! {
    /// Cross-axis alignment of a panel's children.
    pub enum Align {
        /// Pack at the start of the cross axis.
        Start = "start",
        /// Centre on the cross axis.
        Center = "center",
        /// Pack at the end of the cross axis.
        End = "end",
        /// Stretch to fill the cross axis.
        Stretch = "stretch",
    }
}

string_enum! {
    /// Main-axis distribution of a panel's children.
    pub enum Justify {
        /// Pack at the start of the main axis.
        Start = "start",
        /// Centre on the main axis.
        Center = "center",
        /// Pack at the end of the main axis.
        End = "end",
        /// First at the start, last at the end, the rest spread evenly.
        SpaceBetween = "space_between",
    }
}

string_enum! {
    /// What a panel does with children that do not fit.
    pub enum Overflow {
        /// They spill out and stay visible.
        Visible = "visible",
        /// They clip and the panel scrolls vertically.
        Scroll = "scroll",
    }
}

#[allow(clippy::derivable_impls)]
impl Default for Align {
    fn default() -> Self {
        Self::Start
    }
}

#[allow(clippy::derivable_impls)]
impl Default for Justify {
    fn default() -> Self {
        Self::Start
    }
}

#[allow(clippy::derivable_impls)]
impl Default for Overflow {
    fn default() -> Self {
        Self::Visible
    }
}

/// A length in a screen file. A bare number is pixels; a string is `"50%"`,
/// `"fill"` (100%), `"auto"` or `"3s"` (three spacing steps).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Length {
    /// Logical pixels.
    Px(f32),
    /// Percent of the parent.
    Percent(f32),
    /// Spacing steps (`1.0` = `spacing.sm`).
    Steps(f32),
    /// `Val::Auto`.
    Auto,
}

impl From<f32> for Length {
    fn from(px: f32) -> Self {
        Self::Px(px)
    }
}

impl core::str::FromStr for Length {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, String> {
        let s = s.trim();
        if s == "auto" {
            return Ok(Self::Auto);
        }
        if s == "fill" {
            return Ok(Self::Percent(100.0));
        }
        if let Some(pct) = s.strip_suffix('%') {
            return pct
                .trim()
                .parse()
                .map(Self::Percent)
                .map_err(|e| format!("bad percent length `{s}`: {e}"));
        }
        if let Some(steps) = s.strip_suffix('s') {
            return steps
                .trim()
                .parse()
                .map(Self::Steps)
                .map_err(|e| format!("bad steps length `{s}`: {e}"));
        }
        Err(format!(
            "unknown length `{s}`: write a number for pixels, `50%`, `3s`, `fill` or `auto`"
        ))
    }
}

impl Serialize for Length {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Px(px) => s.serialize_f32(*px),
            Self::Percent(p) => s.serialize_str(&format!("{p}%")),
            Self::Steps(n) => s.serialize_str(&format!("{n}s")),
            Self::Auto => s.serialize_str("auto"),
        }
    }
}

impl<'de> Deserialize<'de> for Length {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Raw {
            Num(f32),
            Int(i64),
            Str(String),
        }
        match Raw::deserialize(d)? {
            Raw::Num(px) => Ok(Self::Px(px)),
            #[allow(clippy::cast_precision_loss)]
            Raw::Int(px) => Ok(Self::Px(px as f32)),
            Raw::Str(s) => s.parse().map_err(serde::de::Error::custom),
        }
    }
}

/// Padding in spacing steps: one number for all sides or `(top, right,
/// bottom, left)`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Padding {
    /// Top, in steps.
    pub top: f32,
    /// Right, in steps.
    pub right: f32,
    /// Bottom, in steps.
    pub bottom: f32,
    /// Left, in steps.
    pub left: f32,
}

impl Padding {
    /// The same on every side.
    pub const fn all(steps: f32) -> Self {
        Self {
            top: steps,
            right: steps,
            bottom: steps,
            left: steps,
        }
    }

    /// True when every side is the same.
    pub fn is_uniform(&self) -> bool {
        let same = |a: f32, b: f32| a.to_bits() == b.to_bits();
        same(self.top, self.right) && same(self.top, self.bottom) && same(self.top, self.left)
    }
}

impl Default for Padding {
    fn default() -> Self {
        Self::all(0.0)
    }
}

impl From<f32> for Padding {
    fn from(steps: f32) -> Self {
        Self::all(steps)
    }
}

impl Serialize for Padding {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        if self.is_uniform() {
            s.serialize_f32(self.top)
        } else {
            (self.top, self.right, self.bottom, self.left).serialize(s)
        }
    }
}

impl<'de> Deserialize<'de> for Padding {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Raw {
            Num(f32),
            Int(i64),
            Sides((f32, f32, f32, f32)),
        }
        Ok(match Raw::deserialize(d)? {
            Raw::Num(n) => Self::all(n),
            #[allow(clippy::cast_precision_loss)]
            Raw::Int(n) => Self::all(n as f32),
            Raw::Sides((top, right, bottom, left)) => Self {
                top,
                right,
                bottom,
                left,
            },
        })
    }
}

/// Where on its parent an absolutely placed node hangs. Lowercase strings in
/// data. Shared by HUD layers and by [`Layout::place`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NineAnchor {
    /// Top left corner.
    TopLeft,
    /// Top centre.
    Top,
    /// Top right corner.
    TopRight,
    /// Left centre.
    Left,
    /// Centre.
    #[default]
    Center,
    /// Right centre.
    Right,
    /// Bottom left corner.
    BottomLeft,
    /// Bottom centre.
    Bottom,
    /// Bottom right corner.
    BottomRight,
}

/// Absolute placement of a node inside its parent (menus contract 1.4).
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Place {
    /// Which of the nine points.
    #[serde(default)]
    pub anchor: NineAnchor,
    /// Logical-pixel offset from that point.
    #[serde(default)]
    pub offset: bevy::math::Vec2,
}

/// Panel layout, the subset of `Node` a screen author sets. Everything else
/// comes from the theme's spacing tokens. Menus contract section 1.4.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Layout {
    /// Flow.
    #[serde(default)]
    pub direction: LayoutDirection,
    /// Gap between children, in spacing steps (`1.0` = `spacing.sm`).
    #[serde(default)]
    pub gap: f32,
    /// Padding, in spacing steps; one number or four sides.
    #[serde(default)]
    pub padding: Padding,
    /// Width. `None` = fit content.
    #[serde(default)]
    pub width: Option<Length>,
    /// Height. `None` = fit content.
    #[serde(default)]
    pub height: Option<Length>,
    /// Minimum width.
    #[serde(default)]
    pub min_width: Option<Length>,
    /// Maximum width.
    #[serde(default)]
    pub max_width: Option<Length>,
    /// Minimum height.
    #[serde(default)]
    pub min_height: Option<Length>,
    /// Maximum height.
    #[serde(default)]
    pub max_height: Option<Length>,
    /// Cross-axis alignment of children.
    #[serde(default)]
    pub align: Option<Align>,
    /// Main-axis distribution of children.
    #[serde(default)]
    pub justify: Justify,
    /// Flex grow.
    #[serde(default)]
    pub grow: f32,
    /// Wrap onto the next line when the main axis is full.
    #[serde(default)]
    pub wrap: bool,
    /// Clip and scroll, or spill.
    #[serde(default)]
    pub overflow: Overflow,
    /// Absolute placement inside the parent.
    #[serde(default)]
    pub place: Option<Place>,
    /// Deprecated alias for `align: center`; honoured only when `align` is
    /// unset.
    #[serde(default)]
    pub center: bool,
}

impl Layout {
    /// The effective cross-axis alignment: `align`, else `center`.
    pub fn effective_align(&self) -> Align {
        self.align.unwrap_or(if self.center {
            Align::Center
        } else {
            Align::Start
        })
    }
}

/// Explicit focus neighbours of a node (menus contract 2.4). Read from the
/// reserved tags `nav.up`, `nav.down`, `nav.left`, `nav.right`, whose values
/// are node ids in the sense of [`UiNodeDef::id`].
#[derive(Component, Debug, Clone, Default, PartialEq, Eq)]
pub struct NavLinks {
    /// Id of the node focus moves to on `Up`.
    pub up: Option<String>,
    /// Id of the node focus moves to on `Down`.
    pub down: Option<String>,
    /// Id of the node focus moves to on `Left`.
    pub left: Option<String>,
    /// Id of the node focus moves to on `Right`.
    pub right: Option<String>,
}

impl NavLinks {
    /// Reads the four reserved tags. `None` when none is set.
    pub fn from_tags(tags: &Tags) -> Option<Self> {
        let links = Self {
            up: tags.get(Tags::NAV_UP).map(str::to_owned),
            down: tags.get(Tags::NAV_DOWN).map(str::to_owned),
            left: tags.get(Tags::NAV_LEFT).map(str::to_owned),
            right: tags.get(Tags::NAV_RIGHT).map(str::to_owned),
        };
        (links != Self::default()).then_some(links)
    }
}

string_enum! {
    /// How a screen sits on the stack (menus contract 1.3).
    pub enum PresentationMode {
        /// Hides every entry below it and takes focus.
        Page = "page",
        /// Keeps the entries below visible, scrims them, traps focus.
        Modal = "modal",
        /// Takes no focus, blocks nothing, is not counted by `Back`.
        Overlay = "overlay",
    }
}

string_enum! {
    /// How a screen arrives when pushed.
    pub enum Transition {
        /// Alpha 0 to 1.
        Fade = "fade",
        /// Fade plus a slide from below.
        SlideUp = "slide_up",
        /// Fade plus a slide from the right.
        SlideLeft = "slide_left",
        /// Appears at once.
        None = "none",
    }
}

string_enum! {
    /// What an unclaimed `Back` does to the screen.
    pub enum BackPolicy {
        /// Pops it.
        Pop = "pop",
        /// Leaves it; the screen handles `Back` itself.
        Ignore = "ignore",
    }
}

#[allow(clippy::derivable_impls)]
impl Default for PresentationMode {
    fn default() -> Self {
        Self::Page
    }
}

#[allow(clippy::derivable_impls)]
impl Default for Transition {
    fn default() -> Self {
        Self::Fade
    }
}

#[allow(clippy::derivable_impls)]
impl Default for BackPolicy {
    fn default() -> Self {
        Self::Pop
    }
}

/// How a screen presents on the [`ScreenStack`](crate::ScreenStack).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Presentation {
    /// Page, modal or overlay.
    #[serde(default)]
    pub mode: PresentationMode,
    /// Draw a scrim under it. Defaults to `mode == modal`.
    #[serde(default)]
    pub scrim: Option<bool>,
    /// Arrival motion.
    #[serde(default)]
    pub transition: Transition,
    /// What `Back` does.
    #[serde(default)]
    pub back: BackPolicy,
}

impl Presentation {
    /// Whether a scrim is drawn: `scrim`, else `mode == modal`.
    pub fn scrim(&self) -> bool {
        self.scrim.unwrap_or(self.mode == PresentationMode::Modal)
    }
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
    /// A block model (Phase 6). Placeholder geometry until models exist.
    Block(Namespaced),
}

map_variant_serialize! {
    /// `Player` writes `(player: ())`.
    ViewSubject {
        Player = "player",
        Item(v) = "item",
        Block(v) = "block",
    }
}

/// One state of an [`UiNodeDef::IconButton`] (Phase 6).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct IconButtonState {
    /// Stable id, published as the `state` tag and in `WidgetActivate.tags`.
    pub id: String,
    /// Icon shown while this state is current.
    pub icon: IconDef,
    /// Label (tooltip, semantic label).
    pub label: LocKey,
}

fn default_unit() -> String {
    "mB".to_owned()
}

fn three() -> u16 {
    3
}

fn viewport_size() -> f32 {
    96.0
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
    /// A scrolling grid over a [`VirtualGridSource`](crate::VirtualGridSource)
    /// (Phase 6). Only `rows` rows of cells exist at a time.
    VirtualGrid {
        /// Data source.
        source: DataSourceId,
        /// Columns.
        cols: u16,
        /// Visible rows.
        #[serde(default = "three")]
        rows: u16,
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
    /// A fluid tank bound to two properties (Phase 6, contract 1.2).
    Tank {
        /// Amount.
        property: PropertyId,
        /// Capacity.
        capacity: PropertyId,
        /// Fill direction.
        orientation: Orientation,
        /// Static fluid, by registry name.
        #[serde(default)]
        fluid: Option<Namespaced>,
        /// A property whose value is the frozen fluid id; wins over `fluid`.
        #[serde(default)]
        fluid_property: Option<PropertyId>,
        /// Unit shown in the label and tooltip.
        #[serde(default = "default_unit")]
        unit: String,
        /// Locator tags.
        #[serde(default)]
        tags: Tags,
    },
    /// A bar bound to two properties (Phase 6, contract 1.2).
    Bar {
        /// Value.
        property: PropertyId,
        /// Maximum.
        max: PropertyId,
        /// Fill direction.
        direction: Direction,
        /// Show `value / max` as text on the bar.
        #[serde(default)]
        text: bool,
        /// Locator tags.
        #[serde(default)]
        tags: Tags,
    },
    /// A progress arrow: a [`Bar`](Self::Bar) with the `progress` roles.
    Progress {
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
    /// A tab in a `tab.rail` panel that grows to show its children (Phase 6,
    /// contract 1.3).
    SideTab {
        /// Tab icon.
        icon: IconDef,
        /// Which way the content opens.
        side: Side,
        /// Header label; the icon path when absent.
        #[serde(default)]
        label: Option<LocKey>,
        /// Start open.
        #[serde(default)]
        open: bool,
        /// Contents.
        #[serde(default)]
        children: Vec<UiNodeDef>,
        /// Locator tags.
        #[serde(default)]
        tags: Tags,
    },
    /// A button cycling through states on click (Phase 6, contract 1.4).
    IconButton {
        /// At least one state.
        states: Vec<IconButtonState>,
        /// Property that mirrors the state index, when the screen has a menu.
        #[serde(default)]
        property: Option<PropertyId>,
        /// Locator tags.
        #[serde(default)]
        tags: Tags,
    },
    /// A live 3D view (Phase 6, contract 1.6).
    Viewport {
        /// What to show.
        subject: ViewSubject,
        /// Edge length in px.
        #[serde(default = "viewport_size")]
        size: f32,
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
            | Self::Progress { tags, .. }
            | Self::SideTab { tags, .. }
            | Self::IconButton { tags, .. }
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

    /// The node's merge identity: its `test_id` tag, or an
    /// [`Anchor`](Self::Anchor)'s id. `None` for a node that carries neither,
    /// which is a node screen inheritance cannot address.
    ///
    /// One namespace on purpose: an ancestor's `(type: "anchor", id: "rail")`
    /// and a child's node tagged `test_id: "rail"` are the same point in the
    /// tree, so a child fills an ancestor's anchor by naming it.
    pub fn id(&self) -> Option<&str> {
        match self {
            Self::Anchor { id } => Some(id.0.as_str()),
            other => other.tags().and_then(|t| t.get(Tags::TEST_ID)),
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
///
/// Also a Bevy [`Asset`]: `*.screen.ron` files load through
/// [`ScreenLoader`](crate::screen_asset::ScreenLoader), and
/// [`ScreenAssets`](crate::screen_asset::ScreenAssets) feeds what they hold
/// into [`Screens`](crate::Screens).
#[derive(Asset, TypePath, Debug, Clone, PartialEq, Serialize, Deserialize)]
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
    /// Node ids ([`UiNodeDef::id`]) to delete from the flattened tree after
    /// this screen's overrides are merged onto the one it `inherits`.
    ///
    /// Only meaningful on a screen that inherits; a screen with no ancestor
    /// simply does not write the node it does not want.
    #[serde(default)]
    pub remove: Vec<String>,
    /// Node id ([`UiNodeDef::id`]) that takes focus when the screen opens.
    /// `None` = the first focusable node in tree order (menus contract 2.4).
    #[serde(default)]
    pub initial_focus: Option<String>,
    /// How the screen sits on the stack (menus contract 1.3).
    #[serde(default)]
    pub presentation: Presentation,
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
