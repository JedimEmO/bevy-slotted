//! The untagged serde form of [`slotted_model::Value`].
//!
//! `Value` derives serde as an externally tagged enum (`Int(3)`), which is the
//! right thing for a stack component patch on the wire and the wrong thing
//! for a Lua table. Every `Value` field on a [`crate::ScriptCommand`] or
//! [`crate::ScriptEvent`] therefore uses [`untagged`], which reads and writes
//! the JSON-like shape a table maps onto. Contract section 1.3 has the rules;
//! `tests` below pins them.

use std::collections::BTreeMap;
use std::fmt;

use serde::de::{self, MapAccess, SeqAccess, Visitor};
use serde::ser::{SerializeMap, SerializeSeq};
use serde::{Deserializer, Serializer};
use slotted_model::Value;

/// `#[serde(with = "slotted_script::value::untagged")]`.
pub mod untagged {
    use super::{Deserializer, Serializer, Value};

    /// Writes `value` in its untagged form.
    ///
    /// # Errors
    ///
    /// Whatever the serializer reports.
    pub fn serialize<S: Serializer>(value: &Value, serializer: S) -> Result<S::Ok, S::Error> {
        super::serialize(value, serializer)
    }

    /// Reads an untagged value.
    ///
    /// # Errors
    ///
    /// A `null`, a non-string map key, or whatever the deserializer reports.
    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Value, D::Error> {
        super::deserialize(deserializer)
    }
}

/// Writes `value` untagged: scalars as themselves, lists as sequences, maps as
/// string-keyed maps.
///
/// # Errors
///
/// Whatever the serializer reports.
pub fn serialize<S: Serializer>(value: &Value, serializer: S) -> Result<S::Ok, S::Error> {
    match value {
        Value::Bool(b) => serializer.serialize_bool(*b),
        Value::Int(i) => serializer.serialize_i64(*i),
        Value::Float(f) => serializer.serialize_f64(*f),
        Value::Str(s) => serializer.serialize_str(s),
        Value::List(items) => {
            let mut seq = serializer.serialize_seq(Some(items.len()))?;
            for item in items {
                seq.serialize_element(&Untagged(item))?;
            }
            seq.end()
        }
        Value::Map(map) => {
            let mut m = serializer.serialize_map(Some(map.len()))?;
            for (k, v) in map {
                m.serialize_entry(k, &Untagged(v))?;
            }
            m.end()
        }
    }
}

/// Reads an untagged value. A number with no fractional part inside `i64`
/// is an `Int`; anything else numeric is a `Float`.
///
/// # Errors
///
/// A `null`, a non-string map key, or whatever the deserializer reports.
pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Value, D::Error> {
    deserializer.deserialize_any(ValueVisitor)
}

/// Borrowed wrapper so nested values serialize through this module.
struct Untagged<'a>(&'a Value);

impl serde::Serialize for Untagged<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serialize(self.0, serializer)
    }
}

/// Owned wrapper so nested values deserialize through this module.
struct Owned(Value);

impl<'de> serde::Deserialize<'de> for Owned {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserialize(deserializer).map(Owned)
    }
}

struct ValueVisitor;

impl<'de> Visitor<'de> for ValueVisitor {
    type Value = Value;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a bool, number, string, list or string-keyed map")
    }

    fn visit_bool<E: de::Error>(self, v: bool) -> Result<Value, E> {
        Ok(Value::Bool(v))
    }

    fn visit_i64<E: de::Error>(self, v: i64) -> Result<Value, E> {
        Ok(Value::Int(v))
    }

    fn visit_u64<E: de::Error>(self, v: u64) -> Result<Value, E> {
        i64::try_from(v).map_or_else(
            |_| {
                #[allow(clippy::cast_precision_loss)]
                Ok(Value::Float(v as f64))
            },
            |i| Ok(Value::Int(i)),
        )
    }

    fn visit_f64<E: de::Error>(self, v: f64) -> Result<Value, E> {
        // Luau numbers are doubles; an integral double is an Int so that
        // `{ max_stack_size = 16 }` deserialises into a `u32` field.
        #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
        if v.fract() == 0.0 && v.is_finite() && v.abs() < 9_007_199_254_740_992.0 {
            Ok(Value::Int(v as i64))
        } else {
            Ok(Value::Float(v))
        }
    }

    fn visit_str<E: de::Error>(self, v: &str) -> Result<Value, E> {
        Ok(Value::Str(v.to_owned()))
    }

    fn visit_string<E: de::Error>(self, v: String) -> Result<Value, E> {
        Ok(Value::Str(v))
    }

    fn visit_unit<E: de::Error>(self) -> Result<Value, E> {
        Err(E::custom("nil is not a value: omit the field instead"))
    }

    fn visit_none<E: de::Error>(self) -> Result<Value, E> {
        self.visit_unit()
    }

    fn visit_some<D: Deserializer<'de>>(self, d: D) -> Result<Value, D::Error> {
        deserialize(d)
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Value, A::Error> {
        let mut out = Vec::with_capacity(seq.size_hint().unwrap_or(0));
        while let Some(Owned(v)) = seq.next_element()? {
            out.push(v);
        }
        Ok(Value::List(out))
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Value, A::Error> {
        let mut out = BTreeMap::new();
        while let Some((k, Owned(v))) = map.next_entry::<String, Owned>()? {
            out.insert(k, v);
        }
        Ok(Value::Map(out))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use pretty_assertions::assert_eq;
    use serde::{Deserialize, Serialize};

    use super::*;

    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    #[serde(transparent)]
    struct Holder(#[serde(with = "super::untagged")] Value);

    fn parse(text: &str) -> Value {
        ron::from_str::<Holder>(text).unwrap().0
    }

    #[test]
    fn integral_numbers_are_ints_and_the_rest_floats() {
        assert_eq!(parse("16"), Value::Int(16));
        assert_eq!(parse("16.0"), Value::Int(16));
        assert_eq!(parse("-2"), Value::Int(-2));
        assert_eq!(parse("0.5"), Value::Float(0.5));
    }

    #[test]
    fn lists_maps_and_round_trip() {
        let v = parse(r#"{"name": "a:b", "tags": ["x", "y"], "nested": {"ok": true}}"#);
        let Value::Map(map) = &v else { panic!("map") };
        assert_eq!(map["name"], Value::Str("a:b".into()));
        assert_eq!(
            map["tags"],
            Value::List(vec![Value::Str("x".into()), Value::Str("y".into())])
        );
        let text = ron::to_string(&Holder(v.clone())).unwrap();
        assert_eq!(parse(&text), v);
        assert_eq!(parse("[]"), Value::List(vec![]));
    }

    #[test]
    fn nil_is_rejected() {
        assert!(ron::from_str::<Holder>("None").is_err());
    }
}
