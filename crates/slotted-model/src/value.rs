//! The untyped [`Value`] tree and the one deserializer every typed definition
//! is read through.
//!
//! # Why this exists
//!
//! A definition reaches the host as an untyped tree from two directions: a
//! `.ron` data file, parsed to a `ron::Value` so patches can run against it,
//! and a Lua table returned by a mod's `data.lua`. Both used to be turned into
//! a typed def by `ron::Value::into_rust`, and that hop cannot express an
//! optional field coming from a script: RON writes `Some("x")`, `ron::Value`'s
//! deserializer accepts nothing but `ron::Value::Option` for
//! `deserialize_option`, and a Lua table has no `Some` wrapper for the prelude
//! to produce. Every `ItemDef::display_name`, `RecipeTypeDef::title_key` and
//! `RecipeDef::shape` failed with *expected option*, and Phase 2 answered the
//! same weakness by forbidding `Option` in widget parameters outright.
//!
//! [`ValueDeserializer`] replaces both hops. A `ron::Value` is converted to
//! [`Value`] first (`slotted_registry::ron_value::to_model`), a script's table
//! is already a [`Value`], and from there exactly one set of rules produces the
//! typed def.
//!
//! # The rules
//!
//! | Input | Reads as |
//! |---|---|
//! | absent map key | `None`, through `#[serde(default)]` on the field |
//! | [`Value::Null`] | `None` for an `Option`, `()` for a unit |
//! | any other present value | `Some(value)` for an `Option` |
//! | [`Value::Int`] | any integer type, and any float |
//! | [`Value::Float`] | `f32` and `f64` |
//! | [`Value::Str`] | a string, a `char` of one character, a unit enum variant |
//! | [`Value::List`] | a sequence, a tuple, a fixed-size array |
//! | [`Value::Map`] | a struct, a map, an internally tagged enum |
//! | one-entry [`Value::Map`] | an externally tagged enum variant |
//!
//! An `Option` field must carry `#[serde(default)]` for the absent-key rule to
//! apply; serde's derive does not default an `Option` on its own. Every
//! optional field of every def in `slotted-registry` and `slotted-ui` does.
//!
//! Internally tagged enums, the `(type: "slot_grid", ...)` convention the ui
//! tree and the script commands both use, need no special case: serde buffers
//! the map through `deserialize_any` and picks the variant off the tag itself.

use std::collections::BTreeMap;
use std::fmt;

use serde::de::{
    self, DeserializeOwned, Deserializer, EnumAccess, IntoDeserializer, MapAccess, SeqAccess,
    VariantAccess, Visitor,
};
use serde::forward_to_deserialize_any;
use serde::{Deserialize, Serialize};

/// A component value on a stack, and the untyped form of every definition: a
/// small JSON-like tree.
///
/// Equality is structural. `Float` compares by IEEE equality, so a `NaN`
/// never equals itself; do not store `NaN` in a patch if you want the stack to
/// merge with its own copy.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Value {
    /// The absent value: a RON `None`, a unit, or a Lua `nil` a host chose to
    /// keep. Reads back as `None` for an `Option` and as `()` for a unit.
    ///
    /// Scripts never produce one. A Lua table cannot hold a `nil` value, and
    /// the adapter's bridge rejects an explicit nil, so a mod expresses "no
    /// value" by leaving the key out.
    Null,
    /// A boolean.
    Bool(bool),
    /// A signed integer.
    Int(i64),
    /// A floating point number.
    Float(f64),
    /// A string.
    Str(String),
    /// An ordered list of values.
    List(Vec<Value>),
    /// A string-keyed map of values, ordered by key.
    Map(BTreeMap<String, Value>),
}

impl Value {
    /// The variant name, for an error message.
    pub const fn describe(&self) -> &'static str {
        match self {
            Self::Null => "nothing",
            Self::Bool(_) => "a boolean",
            Self::Int(_) => "an integer",
            Self::Float(_) => "a number",
            Self::Str(_) => "a string",
            Self::List(_) => "a list",
            Self::Map(_) => "a table",
        }
    }

    /// The value at `key`, if this is a map that has one.
    pub fn get(&self, key: &str) -> Option<&Self> {
        match self {
            Self::Map(map) => map.get(key),
            _ => None,
        }
    }

    /// Inserts `key = value` if this is a map that does not have that key.
    ///
    /// This is how `register_item(id, def)` fills `def.name` from the id when
    /// the script left it out.
    pub fn or_insert(&mut self, key: &str, value: impl Into<Self>) {
        if let Self::Map(map) = self {
            map.entry(key.to_owned()).or_insert_with(|| value.into());
        }
    }
}

impl From<bool> for Value {
    fn from(v: bool) -> Self {
        Self::Bool(v)
    }
}

impl From<i64> for Value {
    fn from(v: i64) -> Self {
        Self::Int(v)
    }
}

impl From<f64> for Value {
    fn from(v: f64) -> Self {
        Self::Float(v)
    }
}

impl From<&str> for Value {
    fn from(v: &str) -> Self {
        Self::Str(v.to_owned())
    }
}

impl From<String> for Value {
    fn from(v: String) -> Self {
        Self::Str(v)
    }
}

/// Why a [`Value`] is not the type it was asked to be.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct ValueError(String);

impl ValueError {
    /// The message, without the wrapper.
    pub fn message(&self) -> &str {
        &self.0
    }
}

impl de::Error for ValueError {
    fn custom<T: fmt::Display>(message: T) -> Self {
        Self(message.to_string())
    }
}

/// Reads `value` as `T` under the rules at the top of this module.
///
/// # Errors
///
/// [`ValueError`] describing the first field that did not fit.
pub fn from_value<T: DeserializeOwned>(value: Value) -> Result<T, ValueError> {
    T::deserialize(ValueDeserializer(value))
}

/// A [`Deserializer`] over one [`Value`].
#[derive(Debug, Clone)]
pub struct ValueDeserializer(pub Value);

impl<'de> Deserializer<'de> for ValueDeserializer {
    type Error = ValueError;

    fn deserialize_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, ValueError> {
        match self.0 {
            Value::Null => visitor.visit_unit(),
            Value::Bool(value) => visitor.visit_bool(value),
            Value::Int(value) => visitor.visit_i64(value),
            Value::Float(value) => visitor.visit_f64(value),
            Value::Str(value) => visitor.visit_string(value),
            Value::List(items) => visitor.visit_seq(Seq(items.into_iter())),
            Value::Map(map) => visitor.visit_map(Map {
                entries: map.into_iter(),
                value: None,
            }),
        }
    }

    /// The whole point: a value that is there is a `Some`, and only an explicit
    /// [`Value::Null`] is a `None`. An absent map key never reaches here at
    /// all; serde fills it from the field's `#[serde(default)]`.
    fn deserialize_option<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, ValueError> {
        match self.0 {
            Value::Null => visitor.visit_none(),
            value => visitor.visit_some(Self(value)),
        }
    }

    fn deserialize_newtype_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value, ValueError> {
        visitor.visit_newtype_struct(self)
    }

    fn deserialize_enum<V: Visitor<'de>>(
        self,
        _name: &'static str,
        _variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, ValueError> {
        match self.0 {
            // `"left"` is a unit variant, the shape a Lua string maps onto and
            // the one every lowercase-string enum in the port uses.
            Value::Str(name) => visitor.visit_enum(name.into_deserializer()),
            // `{ pickup = {...} }` is an externally tagged variant. An
            // internally tagged one never gets here: serde reads its tag
            // through `deserialize_any` before it knows the variant.
            Value::Map(map) if map.len() == 1 => {
                let (name, value) = map.into_iter().next().expect("exactly one entry");
                visitor.visit_enum(Enum { name, value })
            }
            other => Err(de::Error::custom(format!(
                "expected a variant name or a one-entry table, found {}",
                other.describe()
            ))),
        }
    }

    forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string
        bytes byte_buf unit unit_struct seq tuple tuple_struct map struct
        identifier ignored_any
    }
}

impl IntoDeserializer<'_, ValueError> for ValueDeserializer {
    type Deserializer = Self;

    fn into_deserializer(self) -> Self {
        self
    }
}

impl IntoDeserializer<'_, ValueError> for Value {
    type Deserializer = ValueDeserializer;

    fn into_deserializer(self) -> ValueDeserializer {
        ValueDeserializer(self)
    }
}

/// A `Value::List` as a sequence.
struct Seq(std::vec::IntoIter<Value>);

impl<'de> SeqAccess<'de> for Seq {
    type Error = ValueError;

    fn next_element_seed<T: de::DeserializeSeed<'de>>(
        &mut self,
        seed: T,
    ) -> Result<Option<T::Value>, ValueError> {
        match self.0.next() {
            Some(value) => seed.deserialize(ValueDeserializer(value)).map(Some),
            None => Ok(None),
        }
    }

    fn size_hint(&self) -> Option<usize> {
        Some(self.0.len())
    }
}

/// A `Value::Map` as a map, with string keys.
struct Map {
    entries: std::collections::btree_map::IntoIter<String, Value>,
    value: Option<Value>,
}

impl<'de> MapAccess<'de> for Map {
    type Error = ValueError;

    fn next_key_seed<K: de::DeserializeSeed<'de>>(
        &mut self,
        seed: K,
    ) -> Result<Option<K::Value>, ValueError> {
        match self.entries.next() {
            Some((key, value)) => {
                self.value = Some(value);
                seed.deserialize(key.into_deserializer()).map(Some)
            }
            None => Ok(None),
        }
    }

    fn next_value_seed<V: de::DeserializeSeed<'de>>(
        &mut self,
        seed: V,
    ) -> Result<V::Value, ValueError> {
        let value = self
            .value
            .take()
            .expect("serde calls next_value_seed after next_key_seed");
        seed.deserialize(ValueDeserializer(value))
    }

    fn size_hint(&self) -> Option<usize> {
        Some(self.entries.len())
    }
}

/// A one-entry table as an externally tagged enum.
struct Enum {
    name: String,
    value: Value,
}

impl<'de> EnumAccess<'de> for Enum {
    type Error = ValueError;
    type Variant = ValueDeserializer;

    fn variant_seed<V: de::DeserializeSeed<'de>>(
        self,
        seed: V,
    ) -> Result<(V::Value, Self::Variant), ValueError> {
        let name = seed.deserialize(self.name.into_deserializer())?;
        Ok((name, ValueDeserializer(self.value)))
    }
}

impl<'de> VariantAccess<'de> for ValueDeserializer {
    type Error = ValueError;

    fn unit_variant(self) -> Result<(), ValueError> {
        Ok(())
    }

    fn newtype_variant_seed<T: de::DeserializeSeed<'de>>(
        self,
        seed: T,
    ) -> Result<T::Value, ValueError> {
        seed.deserialize(self)
    }

    fn tuple_variant<V: Visitor<'de>>(
        self,
        _len: usize,
        visitor: V,
    ) -> Result<V::Value, ValueError> {
        self.deserialize_any(visitor)
    }

    fn struct_variant<V: Visitor<'de>>(
        self,
        _fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, ValueError> {
        self.deserialize_any(visitor)
    }
}

#[allow(clippy::unwrap_used)]
#[cfg(test)]
mod tests {
    use super::{Value, from_value};
    use pretty_assertions::assert_eq;
    use serde::Deserialize;
    use std::collections::BTreeMap;

    fn map(pairs: &[(&str, Value)]) -> Value {
        Value::Map(
            pairs
                .iter()
                .map(|(k, v)| ((*k).to_owned(), v.clone()))
                .collect::<BTreeMap<_, _>>(),
        )
    }

    fn list(items: &[Value]) -> Value {
        Value::List(items.to_vec())
    }

    fn s(text: &str) -> Value {
        Value::Str(text.to_owned())
    }

    #[derive(Debug, Deserialize, PartialEq)]
    #[serde(deny_unknown_fields)]
    struct Def {
        name: String,
        #[serde(default)]
        display_name: Option<String>,
        #[serde(default)]
        shape: Option<Vec<String>>,
        #[serde(default)]
        max_stack_size: u32,
        #[serde(default)]
        scale: f32,
        #[serde(default)]
        size: (u16, u16),
        #[serde(default)]
        tags: Vec<String>,
        #[serde(default)]
        key: BTreeMap<char, String>,
    }

    // --- the Option rules ------------------------------------------------

    #[test]
    fn a_present_string_fills_an_option() {
        let def: Def = from_value(map(&[
            ("name", s("copper_chest:chest")),
            ("display_name", s("copper_chest.item.chest")),
        ]))
        .unwrap();
        assert_eq!(
            def.display_name,
            Some("copper_chest.item.chest".to_owned()),
            "a bare string is Some, which is the bug this module exists for"
        );
    }

    #[test]
    fn an_absent_key_is_none() {
        let def: Def = from_value(map(&[("name", s("a:b"))])).unwrap();
        assert_eq!(def.display_name, None);
        assert_eq!(def.shape, None);
    }

    #[test]
    fn an_explicit_null_is_none() {
        let def: Def =
            from_value(map(&[("name", s("a:b")), ("display_name", Value::Null)])).unwrap();
        assert_eq!(def.display_name, None);
    }

    #[test]
    fn a_nested_option_of_a_list_is_some_of_the_list() {
        let def: Def = from_value(map(&[
            ("name", s("a:b")),
            ("shape", list(&[s("iii"), s("i i"), s("iii")])),
        ]))
        .unwrap();
        assert_eq!(
            def.shape,
            Some(vec!["iii".to_owned(), "i i".to_owned(), "iii".to_owned()])
        );
    }

    #[test]
    fn an_option_of_an_option_keeps_both_layers() {
        let outer: Option<Option<u32>> = from_value(Value::Int(3)).unwrap();
        assert_eq!(outer, Some(Some(3)));
        let null: Option<Option<u32>> = from_value(Value::Null).unwrap();
        assert_eq!(null, None);
    }

    // --- scalars ---------------------------------------------------------

    #[test]
    fn an_integer_reads_as_an_integer_and_as_a_float() {
        let def: Def = from_value(map(&[
            ("name", s("a:b")),
            ("max_stack_size", Value::Int(16)),
            ("scale", Value::Int(2)),
        ]))
        .unwrap();
        assert_eq!(def.max_stack_size, 16);
        assert!((def.scale - 2.0).abs() < f32::EPSILON);
    }

    #[test]
    fn a_float_does_not_read_as_an_integer() {
        let err = from_value::<Def>(map(&[
            ("name", s("a:b")),
            ("max_stack_size", Value::Float(2.5)),
        ]))
        .unwrap_err();
        assert!(err.message().contains("invalid type"), "{err}");
    }

    #[test]
    fn a_float_reads_as_a_float() {
        let def: Def =
            from_value(map(&[("name", s("a:b")), ("scale", Value::Float(0.5))])).unwrap();
        assert!((def.scale - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn a_one_character_string_reads_as_a_char_key() {
        let def: Def = from_value(map(&[
            ("name", s("a:b")),
            ("key", map(&[("i", s("demo:ingot"))])),
        ]))
        .unwrap();
        assert_eq!(def.key.get(&'i'), Some(&"demo:ingot".to_owned()));
    }

    #[test]
    fn a_null_reads_as_a_unit() {
        let unit: () = from_value(Value::Null).unwrap();
        assert_eq!(unit, ());
    }

    // --- sequences and maps ----------------------------------------------

    #[test]
    fn a_list_reads_as_a_sequence_and_as_a_tuple() {
        let def: Def = from_value(map(&[
            ("name", s("a:b")),
            ("tags", list(&[s("c:storage"), s("c:chests")])),
            ("size", list(&[Value::Int(3), Value::Int(3)])),
        ]))
        .unwrap();
        assert_eq!(def.tags, ["c:storage", "c:chests"]);
        assert_eq!(def.size, (3, 3));
    }

    #[test]
    fn an_empty_list_reads_as_an_empty_sequence() {
        let def: Def = from_value(map(&[("name", s("a:b")), ("tags", list(&[]))])).unwrap();
        assert!(def.tags.is_empty());
    }

    #[test]
    fn an_unknown_key_is_still_rejected() {
        let err =
            from_value::<Def>(map(&[("name", s("a:b")), ("nonsense", Value::Int(1))])).unwrap_err();
        assert!(err.message().contains("unknown field"), "{err}");
    }

    // --- enums -----------------------------------------------------------

    #[derive(Debug, Deserialize, PartialEq)]
    #[serde(rename_all = "snake_case")]
    enum Rarity {
        Common,
        Uncommon,
    }

    #[derive(Debug, Deserialize, PartialEq)]
    #[serde(tag = "type", rename_all = "snake_case")]
    enum Node {
        Text {
            key: String,
            #[serde(default)]
            style: Option<String>,
        },
        SlotGrid {
            cols: u16,
            rows: u16,
            #[serde(default)]
            children: Vec<Node>,
        },
    }

    #[derive(Debug, Deserialize, PartialEq)]
    #[serde(rename_all = "snake_case")]
    enum Action {
        Pickup { slot: u16 },
        Drop(u16),
    }

    #[test]
    fn a_string_reads_as_a_unit_variant() {
        assert_eq!(
            from_value::<Rarity>(s("uncommon")).unwrap(),
            Rarity::Uncommon
        );
    }

    #[test]
    fn an_internally_tagged_variant_reads_from_a_table() {
        let node: Node = from_value(map(&[
            ("type", s("text")),
            ("key", s("appleskin_like.food")),
            ("style", s("muted")),
        ]))
        .unwrap();
        assert_eq!(
            node,
            Node::Text {
                key: "appleskin_like.food".to_owned(),
                style: Some("muted".to_owned()),
            },
            "the (type: \"...\") convention the ui tree and the commands share"
        );
    }

    #[test]
    fn an_internally_tagged_variant_nests() {
        let node: Node = from_value(map(&[
            ("type", s("slot_grid")),
            ("cols", Value::Int(9)),
            ("rows", Value::Int(3)),
            (
                "children",
                list(&[map(&[("type", s("text")), ("key", s("a"))])]),
            ),
        ]))
        .unwrap();
        assert_eq!(
            node,
            Node::SlotGrid {
                cols: 9,
                rows: 3,
                children: vec![Node::Text {
                    key: "a".to_owned(),
                    style: None,
                }],
            }
        );
    }

    #[test]
    fn a_one_entry_table_reads_as_an_externally_tagged_variant() {
        let action: Action =
            from_value(map(&[("pickup", map(&[("slot", Value::Int(4))]))])).unwrap();
        assert_eq!(action, Action::Pickup { slot: 4 });
        let drop: Action = from_value(map(&[("drop", Value::Int(2))])).unwrap();
        assert_eq!(drop, Action::Drop(2));
    }

    #[test]
    fn a_two_entry_table_is_not_an_externally_tagged_variant() {
        let err = from_value::<Action>(map(&[("pickup", Value::Int(1)), ("drop", Value::Int(2))]))
            .unwrap_err();
        assert!(err.message().contains("one-entry table"), "{err}");
    }

    // --- errors ----------------------------------------------------------

    #[test]
    fn a_missing_required_field_names_itself() {
        let err = from_value::<Def>(map(&[("display_name", s("x"))])).unwrap_err();
        assert!(err.message().contains("missing field `name`"), "{err}");
    }

    #[test]
    fn a_wrong_type_names_what_it_found() {
        let err = from_value::<Def>(map(&[("name", Value::Int(1))])).unwrap_err();
        assert!(err.message().contains("invalid type"), "{err}");
    }
}
