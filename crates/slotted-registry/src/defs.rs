//! The definition types the standard registries hold.
//!
//! Every type here is what a mod actually writes in a `.ron` file, so the
//! shapes are chosen for authoring first and for the runtime second. The
//! runtime lookups happen against the indices built at freeze time
//! ([`TagIndex`](crate::index::TagIndex),
//! [`RecipeIndex`](crate::index::RecipeIndex)), not against these structs.
//!
//! # The `Value`-safe rule
//!
//! Data-stage files are parsed to a generic [`Value`] first so that patch lists
//! can rewrite them before they become typed defs (see [`crate::patch`]). RON's
//! [`Value`] keeps maps, sequences, options, numbers and strings but *forgets
//! struct and enum variant names*, so a def type whose serde representation
//! relies on a variant name cannot survive the trip.
//!
//! Every def in this module therefore encodes its choices as data:
//! [`Rarity`] is a lowercase string, and [`Ingredient`] and [`TagEntry`] use
//! Minecraft's `#` prefix to mark a tag. A new def type must follow the same
//! rule; `defs::tests::every_def_survives_a_value_round_trip` is the check.

use core::fmt;
use core::str::FromStr;
use std::collections::BTreeMap;

use serde::de::{SeqAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use slotted_model::Namespaced;

use crate::Value;

/// Minecraft's default stack size, and ours.
pub const DEFAULT_MAX_STACK_SIZE: u32 = 64;

const fn default_max_stack_size() -> u32 {
    DEFAULT_MAX_STACK_SIZE
}

fn unit_value() -> Value {
    Value::Unit
}

fn is_unit_value(value: &Value) -> bool {
    matches!(value, Value::Unit)
}

const fn default_count() -> u32 {
    1
}

const fn default_grid_size() -> (u16, u16) {
    (1, 1)
}

/// How loudly a stack's name is drawn, and which colour token picks it up.
///
/// Serialised as a lowercase string (`"rare"`) rather than a RON enum variant
/// so that it survives the [`Value`] round trip the patch stage needs.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(into = "&'static str")]
pub enum Rarity {
    /// The default. No tint.
    #[default]
    Common,
    /// Slightly above common.
    Uncommon,
    /// Notable.
    Rare,
    /// Very notable.
    Epic,
    /// One of a kind.
    Legendary,
}

impl Rarity {
    /// Every rarity, from lowest to highest.
    pub const ALL: [Self; 5] = [
        Self::Common,
        Self::Uncommon,
        Self::Rare,
        Self::Epic,
        Self::Legendary,
    ];

    /// The lowercase name used in data files.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Common => "common",
            Self::Uncommon => "uncommon",
            Self::Rare => "rare",
            Self::Epic => "epic",
            Self::Legendary => "legendary",
        }
    }
}

/// The string in a data file was not one of the five rarities.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("unknown rarity `{0}`, expected one of common, uncommon, rare, epic, legendary")]
pub struct UnknownRarity(pub String);

impl FromStr for Rarity {
    type Err = UnknownRarity;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|rarity| rarity.as_str() == s)
            .ok_or_else(|| UnknownRarity(s.to_owned()))
    }
}

impl fmt::Display for Rarity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl From<Rarity> for &'static str {
    fn from(rarity: Rarity) -> Self {
        rarity.as_str()
    }
}

impl<'de> Deserialize<'de> for Rarity {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        raw.parse().map_err(serde::de::Error::custom)
    }
}

/// Splits the `#` a tag reference carries off the front of an id.
fn parse_prefixed(raw: &str) -> Result<(bool, Namespaced), String> {
    let (is_tag, body) = raw
        .strip_prefix('#')
        .map_or((false, raw), |rest| (true, rest));
    let id = Namespaced::parse(body).map_err(|err| err.to_string())?;
    Ok((is_tag, id))
}

/// One entry of a [`TagDef`]: either a member, or another tag to inherit from.
///
/// Written as `"slotted:chest"` for a member and `"#slotted:chests"` for an
/// inherited tag, matching Minecraft's datapack syntax.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TagEntry {
    /// A member of the tag, by id.
    Item(Namespaced),
    /// Another tag whose members this tag also contains.
    Tag(Namespaced),
}

impl TagEntry {
    /// The id, whichever kind this is.
    pub fn id(&self) -> &Namespaced {
        match self {
            Self::Item(id) | Self::Tag(id) => id,
        }
    }

    /// Whether this entry inherits another tag.
    pub const fn is_tag(&self) -> bool {
        matches!(self, Self::Tag(_))
    }
}

impl fmt::Display for TagEntry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Item(id) => write!(f, "{id}"),
            Self::Tag(id) => write!(f, "#{id}"),
        }
    }
}

impl Serialize for TagEntry {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Item(id) => serializer.serialize_str(id.as_str()),
            Self::Tag(id) => serializer.collect_str(&format_args!("#{id}")),
        }
    }
}

impl<'de> Deserialize<'de> for TagEntry {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        let (is_tag, id) = parse_prefixed(&raw).map_err(serde::de::Error::custom)?;
        Ok(if is_tag {
            Self::Tag(id)
        } else {
            Self::Item(id)
        })
    }
}

/// What a recipe accepts in one input position.
///
/// Written as `"slotted:copper"` for an exact item, `"#slotted:copper"` for
/// anything in a tag, and `["slotted:copper", "#slotted:iron"]` for a choice
/// between several of those.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Ingredient {
    /// Exactly this item.
    Item(Namespaced),
    /// Anything in this tag.
    Tag(Namespaced),
    /// Any of these, tried in order.
    AnyOf(Vec<Ingredient>),
}

impl Ingredient {
    /// Walks this ingredient and everything nested inside it.
    pub fn walk(&self, visit: &mut impl FnMut(&Self)) {
        visit(self);
        if let Self::AnyOf(options) = self {
            for option in options {
                option.walk(visit);
            }
        }
    }
}

impl fmt::Display for Ingredient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Item(id) => write!(f, "{id}"),
            Self::Tag(id) => write!(f, "#{id}"),
            Self::AnyOf(options) => {
                f.write_str("[")?;
                for (index, option) in options.iter().enumerate() {
                    if index > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{option}")?;
                }
                f.write_str("]")
            }
        }
    }
}

impl Serialize for Ingredient {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Item(id) => serializer.serialize_str(id.as_str()),
            Self::Tag(id) => serializer.collect_str(&format_args!("#{id}")),
            Self::AnyOf(options) => serializer.collect_seq(options),
        }
    }
}

impl<'de> Deserialize<'de> for Ingredient {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct IngredientVisitor;

        impl<'de> Visitor<'de> for IngredientVisitor {
            type Value = Ingredient;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("an item id, a `#`-prefixed tag id, or a list of either")
            }

            fn visit_str<E: serde::de::Error>(self, raw: &str) -> Result<Self::Value, E> {
                let (is_tag, id) = parse_prefixed(raw).map_err(E::custom)?;
                Ok(if is_tag {
                    Ingredient::Tag(id)
                } else {
                    Ingredient::Item(id)
                })
            }

            fn visit_string<E: serde::de::Error>(self, raw: String) -> Result<Self::Value, E> {
                self.visit_str(&raw)
            }

            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
                let mut options = Vec::with_capacity(seq.size_hint().unwrap_or(0));
                while let Some(option) = seq.next_element()? {
                    options.push(option);
                }
                Ok(Ingredient::AnyOf(options))
            }
        }

        deserializer.deserialize_any(IngredientVisitor)
    }
}

/// An item definition, the entry of the `items` registry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ItemDef {
    /// The id this item is registered under.
    pub name: Namespaced,
    /// A localisation key or literal display name. `None` means the ui falls
    /// back to `item.<namespace>.<path>`.
    #[serde(default)]
    pub display_name: Option<String>,
    /// How many of this item fit in one slot.
    #[serde(default = "default_max_stack_size")]
    pub max_stack_size: u32,
    /// Tags this item declares membership of, in addition to any tag that
    /// names it from the other side.
    #[serde(default)]
    pub tags: Vec<Namespaced>,
    /// The default component set for a stack of this item. A stack stores only
    /// the patch on top of this, exactly as Minecraft has done since 1.20.5.
    #[serde(default)]
    pub components: BTreeMap<Namespaced, Value>,
    /// An asset path for the icon, or `None` to bake one from a model.
    #[serde(default)]
    pub icon: Option<String>,
    /// Which colour token the name is drawn with.
    #[serde(default)]
    pub rarity: Rarity,
}

impl ItemDef {
    /// An item with every optional field left at its default.
    pub fn new(name: Namespaced) -> Self {
        Self {
            name,
            display_name: None,
            max_stack_size: DEFAULT_MAX_STACK_SIZE,
            tags: Vec::new(),
            components: BTreeMap::new(),
            icon: None,
            rarity: Rarity::Common,
        }
    }
}

/// A tag definition, the entry of the `tags` registry.
///
/// Tags are the one registry that merges rather than collides: two mods may
/// both add to `slotted:chests`, and the loader appends the second mod's values
/// to the first's unless the second sets [`replace`](Self::replace).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TagDef {
    /// The id this tag is registered under.
    pub name: Namespaced,
    /// Members, and `#`-prefixed tags to inherit from.
    #[serde(default)]
    pub values: Vec<TagEntry>,
    /// Discard everything earlier mods put in this tag instead of adding to it.
    #[serde(default)]
    pub replace: bool,
}

impl TagDef {
    /// An empty, additive tag.
    pub fn new(name: Namespaced) -> Self {
        Self {
            name,
            values: Vec::new(),
            replace: false,
        }
    }
}

/// A recipe *category*: the crafting grid, the furnace, the ore washer.
///
/// The browser groups recipes by this and draws one tab per entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecipeTypeDef {
    /// The id this recipe type is registered under.
    pub name: Namespaced,
    /// Localisation key for the tab title.
    #[serde(default)]
    pub title_key: Option<String>,
    /// An asset path for the tab icon.
    #[serde(default)]
    pub icon: Option<String>,
    /// Input grid size as `(columns, rows)`. Defaults to a single input.
    #[serde(default = "default_grid_size")]
    pub size: (u16, u16),
}

impl RecipeTypeDef {
    /// A recipe type with a one-by-one input grid and no artwork.
    pub fn new(name: Namespaced) -> Self {
        Self {
            name,
            title_key: None,
            icon: None,
            size: default_grid_size(),
        }
    }
}

/// What a recipe produces.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ItemResult {
    /// The item produced.
    pub item: Namespaced,
    /// How many. Defaults to one.
    #[serde(default = "default_count")]
    pub count: u32,
}

impl ItemResult {
    /// One of `item`.
    pub fn one(item: Namespaced) -> Self {
        Self { item, count: 1 }
    }
}

/// A recipe, the entry of the `recipes` registry.
///
/// Shapeless recipes list their inputs in [`ingredients`](Self::ingredients).
/// Shaped recipes give a [`shape`](Self::shape) of equal-length rows and a
/// [`key`](Self::key) from the characters in those rows to ingredients, the
/// way `crafting_shaped` does in a datapack. A space in the shape means an
/// empty cell.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecipeDef {
    /// The id this recipe is registered under.
    pub name: Namespaced,
    /// Which [`RecipeTypeDef`] this belongs to. Must exist at freeze time.
    pub recipe_type: Namespaced,
    /// Inputs for a shapeless recipe.
    #[serde(default)]
    pub ingredients: Vec<Ingredient>,
    /// The output.
    pub result: ItemResult,
    /// Rows of a shaped recipe, each character resolved through
    /// [`key`](Self::key).
    #[serde(default)]
    pub shape: Option<Vec<String>>,
    /// The shape's character-to-ingredient map.
    #[serde(default)]
    pub key: BTreeMap<char, Ingredient>,
    /// Anything a recipe type wants that this struct does not model, such as a
    /// furnace's cooking time. Opaque here; the recipe type interprets it.
    #[serde(default = "unit_value", skip_serializing_if = "is_unit_value")]
    pub extra: Value,
}

impl RecipeDef {
    /// A shapeless recipe.
    pub fn shapeless(
        name: Namespaced,
        recipe_type: Namespaced,
        ingredients: Vec<Ingredient>,
        result: ItemResult,
    ) -> Self {
        Self {
            name,
            recipe_type,
            ingredients,
            result,
            shape: None,
            key: BTreeMap::new(),
            extra: Value::Unit,
        }
    }

    /// Whether this recipe places its inputs on a grid.
    pub const fn is_shaped(&self) -> bool {
        self.shape.is_some()
    }

    /// Every ingredient the recipe can consume, shapeless list and shape key
    /// alike, in a stable order.
    pub fn all_ingredients(&self) -> impl Iterator<Item = &Ingredient> {
        self.ingredients.iter().chain(self.key.values())
    }
}

/// Generates one of the placeholder registry entry types.
///
/// These registries exist so that load order, patching and freezing are
/// exercised end to end before the crates that own the real types are written.
/// The entry keeps its id and an untouched [`Value`] payload, so a mod can
/// already ship a screen or widget definition and the ui crate can deserialise
/// it into a real type later without the data files changing.
macro_rules! opaque_def {
    ($name:ident, $doc:literal) => {
        #[doc = $doc]
        ///
        /// A placeholder: the payload is kept as an untyped [`Value`] until the
        /// crate that owns the real type exists. See `docs/PLAN.md` section 4.5.
        #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
        #[serde(deny_unknown_fields)]
        pub struct $name {
            /// The id this entry is registered under.
            pub name: Namespaced,
            /// The rest of the file, untouched.
            #[serde(default = "unit_value", skip_serializing_if = "is_unit_value")]
            pub payload: Value,
        }

        impl $name {
            /// An entry with an empty payload.
            pub fn new(name: Namespaced) -> Self {
                Self {
                    name,
                    payload: Value::Unit,
                }
            }
        }
    };
}

opaque_def!(
    ScreenDef,
    "A screen kind, the entry of the `screens` registry."
);
opaque_def!(
    WidgetDef,
    "A widget kind, the entry of the `widgets` registry."
);
opaque_def!(
    TooltipComponentDef,
    "A tooltip part, the entry of the `tooltip_components` registry."
);
opaque_def!(
    HudLayerDef,
    "A HUD layer, the entry of the `hud_layers` registry."
);
opaque_def!(
    FluidDef,
    "A fluid a tank can hold, the entry of the `fluids` registry (Phase 6). Typed by `slotted_ui::FluidDef`."
);
opaque_def!(
    IngredientTypeDef,
    "An ingredient kind the browser can display, the entry of the `ingredient_types` registry."
);

#[allow(clippy::unwrap_used)]
#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use pretty_assertions::assert_eq;
    use serde::Serialize;
    use serde::de::DeserializeOwned;
    use slotted_model::Namespaced;

    use super::{
        Ingredient, ItemDef, ItemResult, Rarity, RecipeDef, RecipeTypeDef, ScreenDef, TagDef,
        TagEntry,
    };
    use crate::Value;

    fn id(s: &str) -> Namespaced {
        Namespaced::parse(s).unwrap()
    }

    /// Parses `text` twice: straight to `T`, and through a [`Value`] the way
    /// the patch stage does. Both must agree, which is the rule the module
    /// documentation states.
    fn value_safe<T: DeserializeOwned + Serialize + PartialEq + core::fmt::Debug>(text: &str) -> T {
        let direct: T = ron::from_str(text).unwrap();
        let value: Value = ron::from_str(text).unwrap();
        let via_value: T = value.into_rust().unwrap();
        assert_eq!(direct, via_value, "def is not Value-safe: {text}");

        let reserialised = ron::to_string(&direct).unwrap();
        let round_tripped: T = ron::from_str(&reserialised).unwrap();
        assert_eq!(
            direct, round_tripped,
            "def does not re-parse: {reserialised}"
        );
        direct
    }

    #[test]
    fn rarity_is_a_lowercase_string() {
        assert_eq!(ron::to_string(&Rarity::Legendary).unwrap(), "\"legendary\"");
        assert_eq!(ron::from_str::<Rarity>("\"rare\"").unwrap(), Rarity::Rare);
        assert_eq!(Rarity::default(), Rarity::Common);
        let err = ron::from_str::<Rarity>("\"mythic\"").unwrap_err();
        assert!(err.to_string().contains("unknown rarity"), "{err}");
    }

    #[test]
    fn ingredients_use_the_hash_prefix_for_tags() {
        let shorthand = r##"["slotted:copper", "#slotted:iron", ["slotted:tin"]]"##;
        let parsed: Vec<Ingredient> = value_safe(shorthand);
        assert_eq!(
            parsed,
            vec![
                Ingredient::Item(id("slotted:copper")),
                Ingredient::Tag(id("slotted:iron")),
                Ingredient::AnyOf(vec![Ingredient::Item(id("slotted:tin"))]),
            ]
        );
        assert_eq!(parsed[1].to_string(), "#slotted:iron");
        assert_eq!(parsed[2].to_string(), "[slotted:tin]");
    }

    #[test]
    fn an_ingredient_walks_its_nested_options() {
        let nested = Ingredient::AnyOf(vec![
            Ingredient::Item(id("a:a")),
            Ingredient::AnyOf(vec![Ingredient::Tag(id("b:b"))]),
        ]);
        let mut seen = 0;
        nested.walk(&mut |_| seen += 1);
        assert_eq!(seen, 4);
    }

    #[test]
    fn a_bad_ingredient_id_names_itself() {
        let err = ron::from_str::<Ingredient>("\"NOPE\"").unwrap_err();
        assert!(err.to_string().contains("NOPE"), "{err}");
    }

    #[test]
    fn item_defs_fill_in_the_defaults() {
        let item: ItemDef = value_safe(r#"(name: "slotted:chest")"#);
        assert_eq!(item, ItemDef::new(id("slotted:chest")));
        assert_eq!(item.max_stack_size, 64);
        assert_eq!(item.rarity, Rarity::Common);
    }

    #[test]
    fn item_defs_carry_components_and_rarity() {
        let item: ItemDef = value_safe(
            r#"(
                name: "slotted:sword",
                display_name: Some("Sword"),
                max_stack_size: 1,
                tags: ["slotted:weapons"],
                components: { "slotted:damage": 0 },
                icon: Some("icons/sword.png"),
                rarity: "epic",
            )"#,
        );
        assert_eq!(item.max_stack_size, 1);
        assert_eq!(item.tags, vec![id("slotted:weapons")]);
        assert_eq!(item.rarity, Rarity::Epic);
        assert_eq!(item.components.len(), 1);
        assert_eq!(item.icon.as_deref(), Some("icons/sword.png"));
    }

    #[test]
    fn an_unknown_field_is_rejected() {
        let err = ron::from_str::<ItemDef>(r#"(name: "a:b", max_stack: 1)"#).unwrap_err();
        assert!(err.to_string().contains("max_stack"), "{err}");
    }

    #[test]
    fn tag_defs_mix_members_and_inherited_tags() {
        let tag: TagDef = value_safe(
            r##"(name: "slotted:storage", values: ["slotted:chest", "#slotted:barrels"], replace: true)"##,
        );
        assert_eq!(
            tag.values,
            vec![
                TagEntry::Item(id("slotted:chest")),
                TagEntry::Tag(id("slotted:barrels")),
            ]
        );
        assert!(tag.replace);
        assert!(tag.values[1].is_tag());
        assert_eq!(tag.values[1].id(), &id("slotted:barrels"));
        assert_eq!(TagDef::new(id("a:b")).values, vec![]);
    }

    #[test]
    fn recipe_types_default_to_a_single_input() {
        let recipe_type: RecipeTypeDef = value_safe(r#"(name: "slotted:smelting")"#);
        assert_eq!(recipe_type.size, (1, 1));
        assert_eq!(recipe_type, RecipeTypeDef::new(id("slotted:smelting")));

        let grid: RecipeTypeDef = value_safe(
            r#"(name: "slotted:crafting", title_key: Some("gui.crafting"), icon: Some("i.png"), size: (3, 3))"#,
        );
        assert_eq!(grid.size, (3, 3));
    }

    #[test]
    fn shaped_recipes_key_their_shape() {
        let recipe: RecipeDef = value_safe(
            r##"(
                name: "slotted:chest",
                recipe_type: "slotted:crafting",
                shape: Some(["PPP", "P P", "PPP"]),
                key: { 'P': "#slotted:planks" },
                result: (item: "slotted:chest"),
                extra: (cooking_time: 200),
            )"##,
        );
        assert!(recipe.is_shaped());
        assert_eq!(recipe.result.count, 1, "count defaults to one");
        assert_eq!(
            recipe.all_ingredients().collect::<Vec<_>>(),
            vec![&Ingredient::Tag(id("slotted:planks"))]
        );
        assert!(!matches!(recipe.extra, Value::Unit));
    }

    #[test]
    fn shapeless_recipes_list_their_inputs() {
        let recipe = RecipeDef::shapeless(
            id("slotted:dye"),
            id("slotted:crafting"),
            vec![Ingredient::Item(id("slotted:rose"))],
            ItemResult::one(id("slotted:red_dye")),
        );
        assert!(!recipe.is_shaped());
        assert_eq!(recipe.all_ingredients().count(), 1);
        assert_eq!(recipe.key, BTreeMap::new());
        let text = ron::to_string(&recipe).unwrap();
        assert!(
            !text.contains("extra"),
            "a unit `extra` is not written: {text}"
        );
        assert_eq!(ron::from_str::<RecipeDef>(&text).unwrap(), recipe);
    }

    #[test]
    fn placeholder_defs_keep_their_payload_untouched() {
        let screen: ScreenDef =
            value_safe(r#"(name: "copper_chest:sorter", payload: (root: (kind: "panel")))"#);
        assert_eq!(screen.name, id("copper_chest:sorter"));
        assert!(!matches!(screen.payload, Value::Unit));
        assert_eq!(ScreenDef::new(id("a:b")).payload, Value::Unit);
    }
}
