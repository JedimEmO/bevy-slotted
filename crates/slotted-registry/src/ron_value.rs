//! The bridge between the untyped value a `.ron` file parses to and the
//! untyped value a script produces.
//!
//! Data files parse to [`ron::Value`], because patches run against RON's own
//! tree and a patch has to be able to reach a `Some(...)`. A script produces a
//! [`slotted_model::Value`], which has no such wrapper. Two untyped trees would
//! mean two sets of deserialisation rules and, in Phase 4, two different
//! answers for an `Option` field. [`to_model`] converts one to the other so
//! that [`typed`](crate::loader) reads every definition, from either source,
//! through [`slotted_model::from_value`]. See that module for the rules.
//!
//! The conversion is lossy in one direction only, and deliberately: a RON
//! `Some(x)` and a bare `x` both become `x`, because "present" is the thing an
//! `Option` field cares about. [`from_model`] therefore writes a bare value
//! back, and reading it again gives the same typed def.

use std::collections::BTreeMap;

use ron::value::Number;
use slotted_model::Value;

/// Why a [`ron::Value`] has no [`Value`] equivalent.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ConvertError {
    /// A map key that is not a string, a char, a number or a bool. `Value`
    /// keys are strings, which is what both RON files and Lua tables use.
    #[error("a map key of type {0} cannot be a string key")]
    KeyKind(&'static str),
    /// A byte string. No definition uses one and there is no `Value` variant
    /// for it.
    #[error("a byte string cannot be a value")]
    Bytes,
}

/// Converts a parsed `.ron` value into the untyped value every definition is
/// deserialised from.
///
/// `Some(x)` becomes `x` and `None` becomes [`Value::Null`], so an optional
/// field reads the same whether it came from a file or from a script.
///
/// # Errors
///
/// [`ConvertError`] for a byte string or a map key that cannot be a string.
pub fn to_model(value: &ron::Value) -> Result<Value, ConvertError> {
    Ok(match value {
        ron::Value::Unit | ron::Value::Option(None) => Value::Null,
        ron::Value::Option(Some(inner)) => to_model(inner)?,
        ron::Value::Bool(b) => Value::Bool(*b),
        ron::Value::Char(c) => Value::Str(c.to_string()),
        ron::Value::String(s) => Value::Str(s.clone()),
        ron::Value::Number(n) => number(*n),
        ron::Value::Bytes(_) => return Err(ConvertError::Bytes),
        ron::Value::Seq(items) => {
            Value::List(items.iter().map(to_model).collect::<Result<Vec<_>, _>>()?)
        }
        ron::Value::Map(map) => {
            let mut out = BTreeMap::new();
            for (key, value) in map.iter() {
                out.insert(key_of(key)?, to_model(value)?);
            }
            Value::Map(out)
        }
    })
}

/// Converts an untyped value back into the [`ron::Value`] the registry stores
/// as a screen or widget payload.
///
/// [`Value::Null`] becomes `ron::Value::Unit`, which [`to_model`] reads back as
/// `Null`, so a payload survives the round trip.
pub fn from_model(value: &Value) -> ron::Value {
    match value {
        Value::Null => ron::Value::Unit,
        Value::Bool(b) => ron::Value::Bool(*b),
        Value::Int(i) => ron::Value::Number(Number::new(*i)),
        Value::Float(f) => ron::Value::Number(Number::new(*f)),
        Value::Str(s) => ron::Value::String(s.clone()),
        Value::List(items) => ron::Value::Seq(items.iter().map(from_model).collect()),
        Value::Map(map) => ron::Value::Map(
            map.iter()
                .map(|(k, v)| (ron::Value::String(k.clone()), from_model(v)))
                .collect(),
        ),
    }
}

/// A RON number as an `Int` when it is one and fits, else a `Float`. This is
/// the same rule the script bridge applies to a Lua number.
fn number(n: Number) -> Value {
    struct AsValue;

    // The visitor cannot fail, but `Number::visit` is generic over an error.
    impl serde::de::Visitor<'_> for AsValue {
        type Value = Value;

        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.write_str("a number")
        }

        fn visit_i64<E>(self, v: i64) -> Result<Value, E> {
            Ok(Value::Int(v))
        }

        fn visit_u64<E>(self, v: u64) -> Result<Value, E> {
            #[allow(clippy::cast_precision_loss)]
            Ok(i64::try_from(v).map_or_else(|_| Value::Float(v as f64), Value::Int))
        }

        fn visit_f64<E>(self, v: f64) -> Result<Value, E> {
            Ok(Value::Float(v))
        }
    }

    n.visit::<_, serde::de::value::Error>(AsValue)
        .unwrap_or_else(|_| Value::Float(n.into_f64()))
}

/// A map key as a string. RON allows any value as a key; every key in a
/// definition is a string, a char (`RecipeDef::key`) or, at worst, a number.
fn key_of(key: &ron::Value) -> Result<String, ConvertError> {
    Ok(match key {
        ron::Value::String(s) => s.clone(),
        ron::Value::Char(c) => c.to_string(),
        ron::Value::Bool(b) => b.to_string(),
        ron::Value::Number(n) => match number(*n) {
            Value::Int(i) => i.to_string(),
            Value::Float(f) => f.to_string(),
            _ => unreachable!("number() returns an Int or a Float"),
        },
        ron::Value::Unit | ron::Value::Option(_) => return Err(ConvertError::KeyKind("an option")),
        ron::Value::Bytes(_) => return Err(ConvertError::KeyKind("a byte string")),
        ron::Value::Seq(_) => return Err(ConvertError::KeyKind("a list")),
        ron::Value::Map(_) => return Err(ConvertError::KeyKind("a map")),
    })
}

#[allow(clippy::unwrap_used)]
#[cfg(test)]
mod tests {
    use super::{ConvertError, from_model, to_model};
    use pretty_assertions::assert_eq;
    use serde::Deserialize;
    use slotted_model::{Value, from_value};

    fn ron(text: &str) -> ron::Value {
        ron::from_str(text).unwrap()
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
        size: (u16, u16),
    }

    #[test]
    fn a_ron_some_and_a_bare_value_reach_the_same_typed_def() {
        let from_file: Def =
            from_value(to_model(&ron(r#"(name: "a:b", display_name: Some("a.b"))"#)).unwrap())
                .unwrap();
        let from_script: Def =
            from_value(to_model(&ron(r#"(name: "a:b", display_name: "a.b")"#)).unwrap()).unwrap();
        assert_eq!(from_file, from_script);
        assert_eq!(from_file.display_name, Some("a.b".to_owned()));
    }

    #[test]
    fn a_ron_none_is_null_and_reads_as_none() {
        assert_eq!(to_model(&ron("None")).unwrap(), Value::Null);
        let def: Def =
            from_value(to_model(&ron(r#"(name: "a:b", shape: None)"#)).unwrap()).unwrap();
        assert_eq!(def.shape, None);
    }

    #[test]
    fn an_absent_field_is_none() {
        let def: Def = from_value(to_model(&ron(r#"(name: "a:b")"#)).unwrap()).unwrap();
        assert_eq!(def.display_name, None);
        assert_eq!(def.shape, None);
    }

    #[test]
    fn a_nested_option_of_a_list_survives() {
        let def: Def =
            from_value(to_model(&ron(r#"(name: "a:b", shape: Some(["ii", "ii"]))"#)).unwrap())
                .unwrap();
        assert_eq!(def.shape, Some(vec!["ii".to_owned(), "ii".to_owned()]));
    }

    #[test]
    fn a_tuple_is_a_list() {
        let def: Def =
            from_value(to_model(&ron(r#"(name: "a:b", size: (3, 3))"#)).unwrap()).unwrap();
        assert_eq!(def.size, (3, 3));
    }

    #[test]
    fn integers_stay_integers_and_floats_stay_floats() {
        assert_eq!(to_model(&ron("7")).unwrap(), Value::Int(7));
        assert_eq!(to_model(&ron("-7")).unwrap(), Value::Int(-7));
        assert_eq!(to_model(&ron("0.5")).unwrap(), Value::Float(0.5));
    }

    #[test]
    fn a_char_key_becomes_a_one_character_string_key() {
        let value = to_model(&ron("{'i': \"demo:ingot\"}")).unwrap();
        assert_eq!(value.get("i"), Some(&Value::Str("demo:ingot".to_owned())));
    }

    #[test]
    fn a_list_key_is_rejected() {
        assert_eq!(
            to_model(&ron("{[1, 2]: 3}")).unwrap_err(),
            ConvertError::KeyKind("a list")
        );
    }

    #[test]
    fn a_payload_round_trips_back_to_ron() {
        let original = to_model(&ron(
            r#"(type: "text", key: "a.b", size: [1, 2.5], gone: None)"#,
        ))
        .unwrap();
        assert_eq!(to_model(&from_model(&original)).unwrap(), original);
    }
}
