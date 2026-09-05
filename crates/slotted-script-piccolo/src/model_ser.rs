//! A `serde::Serializer` that produces a [`slotted_model::Value`].
//!
//! The mlua adapter hands an event to Lua with `LuaSerdeExt::to_value_with`.
//! piccolo has no serde bridge at all, so an event takes one hop more here:
//! serde writes it into a [`Value`] tree first, and [`crate::bridge`] turns
//! that tree into piccolo tables. The rules are the contract's (section 1.3):
//! an `Option::None` becomes [`Value::Null`] and the bridge then omits the
//! key, which is what `serialize_none_to_null(false)` does on the mlua side.

use std::collections::BTreeMap;
use std::fmt;

use serde::{Serialize, ser};
use slotted_model::Value;

/// Why a value could not be written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SerError(String);

impl SerError {
    /// The message.
    pub fn message(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for SerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for SerError {}

impl ser::Error for SerError {
    fn custom<T: fmt::Display>(msg: T) -> Self {
        Self(msg.to_string())
    }
}

/// Serializes `value` into a [`Value`] tree.
///
/// # Errors
///
/// [`SerError`] for a type the tree cannot hold (a non-string map key, a
/// float that is not finite is fine, a byte buffer is not).
pub fn to_model<T: Serialize>(value: &T) -> Result<Value, SerError> {
    value.serialize(ModelSerializer)
}

/// The serializer itself: every method builds a [`Value`] directly.
struct ModelSerializer;

/// Collects sequence, tuple and tuple-variant elements.
struct SeqBuilder {
    items: Vec<Value>,
}

/// Collects map and struct entries.
struct MapBuilder {
    entries: BTreeMap<String, Value>,
    key: Option<String>,
}

/// Wraps a finished value in `{ variant: value }` for an externally tagged
/// enum, matching what `slotted_model::from_value` reads back.
struct VariantBuilder<B> {
    variant: &'static str,
    inner: B,
}

impl ser::Serializer for ModelSerializer {
    type Ok = Value;
    type Error = SerError;
    type SerializeSeq = SeqBuilder;
    type SerializeTuple = SeqBuilder;
    type SerializeTupleStruct = SeqBuilder;
    type SerializeTupleVariant = VariantBuilder<SeqBuilder>;
    type SerializeMap = MapBuilder;
    type SerializeStruct = MapBuilder;
    type SerializeStructVariant = VariantBuilder<MapBuilder>;

    fn serialize_bool(self, v: bool) -> Result<Value, SerError> {
        Ok(Value::Bool(v))
    }

    fn serialize_i8(self, v: i8) -> Result<Value, SerError> {
        Ok(Value::Int(i64::from(v)))
    }

    fn serialize_i16(self, v: i16) -> Result<Value, SerError> {
        Ok(Value::Int(i64::from(v)))
    }

    fn serialize_i32(self, v: i32) -> Result<Value, SerError> {
        Ok(Value::Int(i64::from(v)))
    }

    fn serialize_i64(self, v: i64) -> Result<Value, SerError> {
        Ok(Value::Int(v))
    }

    fn serialize_u8(self, v: u8) -> Result<Value, SerError> {
        Ok(Value::Int(i64::from(v)))
    }

    fn serialize_u16(self, v: u16) -> Result<Value, SerError> {
        Ok(Value::Int(i64::from(v)))
    }

    fn serialize_u32(self, v: u32) -> Result<Value, SerError> {
        Ok(Value::Int(i64::from(v)))
    }

    fn serialize_u64(self, v: u64) -> Result<Value, SerError> {
        i64::try_from(v).map_or_else(
            |_| {
                #[allow(clippy::cast_precision_loss)]
                Ok(Value::Float(v as f64))
            },
            |i| Ok(Value::Int(i)),
        )
    }

    fn serialize_f32(self, v: f32) -> Result<Value, SerError> {
        Ok(Value::Float(f64::from(v)))
    }

    fn serialize_f64(self, v: f64) -> Result<Value, SerError> {
        Ok(Value::Float(v))
    }

    fn serialize_char(self, v: char) -> Result<Value, SerError> {
        Ok(Value::Str(v.to_string()))
    }

    fn serialize_str(self, v: &str) -> Result<Value, SerError> {
        Ok(Value::Str(v.to_owned()))
    }

    fn serialize_bytes(self, _v: &[u8]) -> Result<Value, SerError> {
        Err(SerError("a byte buffer has no Lua form".to_owned()))
    }

    fn serialize_none(self) -> Result<Value, SerError> {
        Ok(Value::Null)
    }

    fn serialize_some<T: ?Sized + Serialize>(self, value: &T) -> Result<Value, SerError> {
        value.serialize(self)
    }

    fn serialize_unit(self) -> Result<Value, SerError> {
        Ok(Value::Null)
    }

    fn serialize_unit_struct(self, _name: &'static str) -> Result<Value, SerError> {
        Ok(Value::Null)
    }

    fn serialize_unit_variant(
        self,
        _name: &'static str,
        _index: u32,
        variant: &'static str,
    ) -> Result<Value, SerError> {
        Ok(Value::Str(variant.to_owned()))
    }

    fn serialize_newtype_struct<T: ?Sized + Serialize>(
        self,
        _name: &'static str,
        value: &T,
    ) -> Result<Value, SerError> {
        value.serialize(self)
    }

    fn serialize_newtype_variant<T: ?Sized + Serialize>(
        self,
        _name: &'static str,
        _index: u32,
        variant: &'static str,
        value: &T,
    ) -> Result<Value, SerError> {
        let inner = value.serialize(ModelSerializer)?;
        Ok(Value::Map(
            [(variant.to_owned(), inner)].into_iter().collect(),
        ))
    }

    fn serialize_seq(self, len: Option<usize>) -> Result<SeqBuilder, SerError> {
        Ok(SeqBuilder {
            items: Vec::with_capacity(len.unwrap_or(0)),
        })
    }

    fn serialize_tuple(self, len: usize) -> Result<SeqBuilder, SerError> {
        self.serialize_seq(Some(len))
    }

    fn serialize_tuple_struct(
        self,
        _name: &'static str,
        len: usize,
    ) -> Result<SeqBuilder, SerError> {
        self.serialize_seq(Some(len))
    }

    fn serialize_tuple_variant(
        self,
        _name: &'static str,
        _index: u32,
        variant: &'static str,
        len: usize,
    ) -> Result<VariantBuilder<SeqBuilder>, SerError> {
        Ok(VariantBuilder {
            variant,
            inner: self.serialize_seq(Some(len))?,
        })
    }

    fn serialize_map(self, _len: Option<usize>) -> Result<MapBuilder, SerError> {
        Ok(MapBuilder {
            entries: BTreeMap::new(),
            key: None,
        })
    }

    fn serialize_struct(self, _name: &'static str, len: usize) -> Result<MapBuilder, SerError> {
        self.serialize_map(Some(len))
    }

    fn serialize_struct_variant(
        self,
        _name: &'static str,
        _index: u32,
        variant: &'static str,
        len: usize,
    ) -> Result<VariantBuilder<MapBuilder>, SerError> {
        Ok(VariantBuilder {
            variant,
            inner: self.serialize_map(Some(len))?,
        })
    }
}

impl ser::SerializeSeq for SeqBuilder {
    type Ok = Value;
    type Error = SerError;

    fn serialize_element<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<(), SerError> {
        self.items.push(value.serialize(ModelSerializer)?);
        Ok(())
    }

    fn end(self) -> Result<Value, SerError> {
        Ok(Value::List(self.items))
    }
}

impl ser::SerializeTuple for SeqBuilder {
    type Ok = Value;
    type Error = SerError;

    fn serialize_element<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<(), SerError> {
        ser::SerializeSeq::serialize_element(self, value)
    }

    fn end(self) -> Result<Value, SerError> {
        ser::SerializeSeq::end(self)
    }
}

impl ser::SerializeTupleStruct for SeqBuilder {
    type Ok = Value;
    type Error = SerError;

    fn serialize_field<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<(), SerError> {
        ser::SerializeSeq::serialize_element(self, value)
    }

    fn end(self) -> Result<Value, SerError> {
        ser::SerializeSeq::end(self)
    }
}

impl ser::SerializeTupleVariant for VariantBuilder<SeqBuilder> {
    type Ok = Value;
    type Error = SerError;

    fn serialize_field<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<(), SerError> {
        ser::SerializeSeq::serialize_element(&mut self.inner, value)
    }

    fn end(self) -> Result<Value, SerError> {
        let inner = ser::SerializeSeq::end(self.inner)?;
        Ok(Value::Map(
            [(self.variant.to_owned(), inner)].into_iter().collect(),
        ))
    }
}

impl MapBuilder {
    fn take_key(&mut self) -> Result<String, SerError> {
        self.key
            .take()
            .ok_or_else(|| SerError("a map value arrived before its key".to_owned()))
    }
}

impl ser::SerializeMap for MapBuilder {
    type Ok = Value;
    type Error = SerError;

    fn serialize_key<T: ?Sized + Serialize>(&mut self, key: &T) -> Result<(), SerError> {
        match key.serialize(ModelSerializer)? {
            Value::Str(s) => {
                self.key = Some(s);
                Ok(())
            }
            other => Err(SerError(format!(
                "a Lua table key must be a string, got {other:?}"
            ))),
        }
    }

    fn serialize_value<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<(), SerError> {
        let key = self.take_key()?;
        let value = value.serialize(ModelSerializer)?;
        self.entries.insert(key, value);
        Ok(())
    }

    fn end(self) -> Result<Value, SerError> {
        Ok(Value::Map(self.entries))
    }
}

impl ser::SerializeStruct for MapBuilder {
    type Ok = Value;
    type Error = SerError;

    fn serialize_field<T: ?Sized + Serialize>(
        &mut self,
        key: &'static str,
        value: &T,
    ) -> Result<(), SerError> {
        let value = value.serialize(ModelSerializer)?;
        self.entries.insert(key.to_owned(), value);
        Ok(())
    }

    fn end(self) -> Result<Value, SerError> {
        Ok(Value::Map(self.entries))
    }
}

impl ser::SerializeStructVariant for VariantBuilder<MapBuilder> {
    type Ok = Value;
    type Error = SerError;

    fn serialize_field<T: ?Sized + Serialize>(
        &mut self,
        key: &'static str,
        value: &T,
    ) -> Result<(), SerError> {
        ser::SerializeStruct::serialize_field(&mut self.inner, key, value)
    }

    fn end(self) -> Result<Value, SerError> {
        let inner = ser::SerializeStruct::end(self.inner)?;
        Ok(Value::Map(
            [(self.variant.to_owned(), inner)].into_iter().collect(),
        ))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use pretty_assertions::assert_eq;
    use slotted_script::ScriptEvent;

    use super::*;

    #[test]
    fn an_event_becomes_a_string_keyed_map_with_its_type() {
        let value = to_model(&ScriptEvent::SearchChanged {
            text: "app".to_owned(),
        })
        .unwrap();
        let Value::Map(map) = &value else {
            panic!("expected a map, got {value:?}")
        };
        assert_eq!(map["type"], Value::Str("search_changed".to_owned()));
        assert_eq!(map["text"], Value::Str("app".to_owned()));
    }

    #[test]
    fn a_none_is_null_and_a_newtype_is_transparent() {
        let value = to_model(&ScriptEvent::ScreenClosed {
            menu: None,
            screen: "test:chest".to_owned(),
        })
        .unwrap();
        let Value::Map(map) = &value else {
            panic!("expected a map")
        };
        assert_eq!(map["menu"], Value::Null);

        let value = to_model(&ScriptEvent::ScreenOpened {
            menu: Some(slotted_model::MenuId(2)),
            screen: "test:chest".to_owned(),
        })
        .unwrap();
        let Value::Map(map) = &value else {
            panic!("expected a map")
        };
        assert_eq!(map["menu"], Value::Int(2));
    }

    #[test]
    fn a_unit_variant_is_its_name() {
        assert_eq!(
            to_model(&slotted_script::Button::Left).unwrap(),
            Value::Str("left".to_owned())
        );
    }
}
