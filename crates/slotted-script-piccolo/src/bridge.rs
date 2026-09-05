//! The value bridge: [`slotted_model::Value`] to and from piccolo values.
//!
//! piccolo has no serde bridge, so this module is what
//! `mlua::LuaSerdeExt::to_value_with` and `from_value_with` do on the native
//! side, written out. The rules are contract section 1.3 and they match the
//! mlua adapter's `ser_options`/`de_options` exactly:
//!
//! - Rust to Lua: an `Int` is a Lua integer, a `Float` a Lua number, a `Map` a
//!   string-keyed table, a `List` a 1-based array, and a [`Value::Null`] (what
//!   an `Option::None` serialises to) is **absent**, not `nil` in the table.
//! - Lua to Rust: a table whose keys are exactly `1..n` is a `List`, an empty
//!   table is an empty `List`, anything else is a `Map` and its keys must be
//!   strings. A Lua integer is an `Int`; a Lua number with no fractional part
//!   inside `i64` is also an `Int`, so `{ columns = 9 }` reaches a `u32` field
//!   whichever way the script wrote it.

use std::collections::BTreeMap;

use piccolo::{Context, Table, Value as LuaValue};
use slotted_model::Value;

/// How deep a table may nest before the bridge gives up. A cyclic table is
/// the reason this exists: piccolo tables are `Gc` handles and a mod can make
/// `t.self = t`, which would otherwise recurse until the stack ends.
const MAX_DEPTH: usize = 64;

/// Writes `value` into the arena as a piccolo value.
///
/// A [`Value::Null`] becomes `nil`; when it is a map entry the caller drops
/// the key instead, which is what [`to_lua`] does for nested maps.
pub fn to_lua<'gc>(ctx: Context<'gc>, value: &Value) -> LuaValue<'gc> {
    match value {
        Value::Null => LuaValue::Nil,
        Value::Bool(b) => LuaValue::Boolean(*b),
        Value::Int(i) => LuaValue::Integer(*i),
        Value::Float(f) => LuaValue::Number(*f),
        Value::Str(s) => LuaValue::String(piccolo::String::from_slice(&ctx, s.as_bytes())),
        Value::List(items) => {
            let table = Table::new(&ctx);
            for (index, item) in items.iter().enumerate() {
                let lua = to_lua(ctx, item);
                // `Table::set` only fails on a NaN or nil key; the key here is
                // an integer, so the result cannot be an error.
                let _ = table.set(ctx, i64::try_from(index + 1).unwrap_or(i64::MAX), lua);
            }
            LuaValue::Table(table)
        }
        Value::Map(map) => {
            let table = Table::new(&ctx);
            for (key, item) in map {
                // A `None` field is an absent key, not a `nil` value: the
                // prelude tests `event.stack ~= nil` and the contract has no
                // null on the wire.
                if matches!(item, Value::Null) {
                    continue;
                }
                let lua = to_lua(ctx, item);
                // Interned, because piccolo holds table keys weakly.
                let key = ctx.intern(key.as_bytes());
                let _ = table.set(ctx, key, lua);
            }
            LuaValue::Table(table)
        }
    }
}

/// Reads a piccolo value back as a [`Value`].
///
/// # Errors
///
/// A message naming what could not cross: a function, a coroutine, userdata,
/// a `nil`, a non-string table key, or a table nested past 64 levels (the
/// depth limit that stops a cyclic table from recursing forever).
pub fn from_lua(value: LuaValue<'_>) -> Result<Value, String> {
    from_lua_at(value, 0)
}

fn from_lua_at(value: LuaValue<'_>, depth: usize) -> Result<Value, String> {
    if depth > MAX_DEPTH {
        return Err(format!("a table nested deeper than {MAX_DEPTH} levels"));
    }
    match value {
        LuaValue::Nil => Err("nil is not a value: omit the field instead".to_owned()),
        LuaValue::Boolean(b) => Ok(Value::Bool(b)),
        LuaValue::Integer(i) => Ok(Value::Int(i)),
        LuaValue::Number(n) => Ok(number(n)),
        LuaValue::String(s) => Ok(Value::Str(s.to_str_lossy().into_owned())),
        LuaValue::Table(t) => table(t, depth),
        LuaValue::Function(_) => Err("a function has no value form".to_owned()),
        LuaValue::Thread(_) => Err("a coroutine has no value form".to_owned()),
        LuaValue::UserData(_) => Err("userdata has no value form".to_owned()),
    }
}

/// Lua has one number type in practice: an integral double is an `Int` so a
/// `u32` field deserialises, and a fractional one is a `Float`.
fn number(n: f64) -> Value {
    #[allow(clippy::cast_possible_truncation)]
    if n.fract() == 0.0 && n.is_finite() && n.abs() < 9_007_199_254_740_992.0 {
        Value::Int(n as i64)
    } else {
        Value::Float(n)
    }
}

/// A table is a list when its keys are exactly `1..n`, otherwise a map. An
/// empty table is an empty list.
fn table(t: Table<'_>, depth: usize) -> Result<Value, String> {
    let len = t.length();
    let mut count = 0usize;
    let mut array_keys = 0usize;
    for (key, _) in t {
        count += 1;
        if let Some(index) = integer_key(key)
            && index >= 1
            && index <= len
        {
            array_keys += 1;
        }
    }

    if count == 0 {
        return Ok(Value::List(Vec::new()));
    }

    if count == array_keys && usize::try_from(len).is_ok_and(|l| l == count) {
        let mut items = Vec::with_capacity(count);
        for index in 1..=len {
            items.push(from_lua_at(
                t.get_value(LuaValue::Integer(index)),
                depth + 1,
            )?);
        }
        return Ok(Value::List(items));
    }

    let mut map = BTreeMap::new();
    for (key, value) in t {
        let LuaValue::String(name) = key else {
            return Err(format!(
                "a table key must be a string, got {}",
                key.type_name()
            ));
        };
        map.insert(
            name.to_str_lossy().into_owned(),
            from_lua_at(value, depth + 1)?,
        );
    }
    Ok(Value::Map(map))
}

/// The integer a key holds, if it is one; `2.0` counts, `2.5` does not.
fn integer_key(key: LuaValue<'_>) -> Option<i64> {
    match key {
        LuaValue::Integer(i) => Some(i),
        #[allow(clippy::cast_possible_truncation)]
        LuaValue::Number(n) if n.fract() == 0.0 && n.is_finite() => Some(n as i64),
        _ => None,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use piccolo::Lua;
    use pretty_assertions::assert_eq;

    use super::*;

    /// Round trips `value` through the arena.
    fn round_trip(value: &Value) -> Result<Value, String> {
        let mut lua = Lua::empty();
        lua.enter(|ctx| from_lua(to_lua(ctx, value)))
    }

    fn map(entries: &[(&str, Value)]) -> Value {
        Value::Map(
            entries
                .iter()
                .map(|(k, v)| ((*k).to_owned(), v.clone()))
                .collect(),
        )
    }

    #[test]
    fn scalars_and_containers_round_trip() {
        let value = map(&[
            ("int", Value::Int(16)),
            ("float", Value::Float(0.5)),
            ("integral_float", Value::Float(3.0)),
            ("yes", Value::Bool(true)),
            ("text", Value::Str("a:b".to_owned())),
            (
                "array",
                Value::List(vec![Value::Int(1), Value::Int(2), Value::Int(3)]),
            ),
            ("empty", Value::List(Vec::new())),
            ("nested", Value::List(vec![map(&[("a", Value::Int(1))])])),
        ]);
        let back = round_trip(&value).unwrap();
        // An integral float comes back as an `Int`, which is the one rule
        // that is not a plain identity.
        let expected = map(&[
            ("int", Value::Int(16)),
            ("float", Value::Float(0.5)),
            ("integral_float", Value::Int(3)),
            ("yes", Value::Bool(true)),
            ("text", Value::Str("a:b".to_owned())),
            (
                "array",
                Value::List(vec![Value::Int(1), Value::Int(2), Value::Int(3)]),
            ),
            ("empty", Value::List(Vec::new())),
            ("nested", Value::List(vec![map(&[("a", Value::Int(1))])])),
        ]);
        assert_eq!(back, expected);
    }

    #[test]
    fn a_null_entry_is_absent_and_a_bare_null_is_nil() {
        let value = map(&[("here", Value::Int(1)), ("gone", Value::Null)]);
        assert_eq!(round_trip(&value).unwrap(), map(&[("here", Value::Int(1))]));
        assert!(round_trip(&Value::Null).is_err());
    }

    #[test]
    fn strings_cross_as_bytes() {
        let value = Value::Str("Kupferkiste 家 🧰".to_owned());
        assert_eq!(round_trip(&value).unwrap(), value);
    }

    #[test]
    fn a_non_string_key_is_named() {
        let mut lua = Lua::empty();
        let err = lua
            .enter(|ctx| {
                let t = Table::new(&ctx);
                let _ = t.set(ctx, 2, 1);
                let _ = t.set(ctx, "a", 1);
                from_lua(LuaValue::Table(t))
            })
            .unwrap_err();
        assert!(err.contains("must be a string"), "{err}");
    }

    #[test]
    fn a_cycle_stops_at_the_depth_limit() {
        let mut lua = Lua::empty();
        let err = lua
            .enter(|ctx| {
                let t = Table::new(&ctx);
                let _ = t.set(ctx, "self", t);
                from_lua(LuaValue::Table(t))
            })
            .unwrap_err();
        assert!(err.contains("nested deeper"), "{err}");
    }
}
