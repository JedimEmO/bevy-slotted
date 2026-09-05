//! The stdlib piccolo does not ship, and the read-only wrapper that stands in
//! for Luau's `sandbox(true)`.
//!
//! piccolo 0.3's `Lua::core()` is thin. It has `base` (minus `tonumber`,
//! `xpcall` and `print`), `coroutine`, `math`, five `string` functions
//! (`len`, `sub`, `upper`, `lower`, `reverse`) and two `table` ones (`pack`,
//! `unpack`). The prelude and the conformance suite need more than that, so
//! everything in [`POLYFILLED`] is implemented here as a Rust callback. The
//! list is deliberately small: a mod gets Lua, not a platform.
//!
//! Two deviations from PUC Lua are documented rather than fixed, because
//! nothing in the contract needs them:
//!
//! - `string.find` searches **plain text only**. Lua patterns are not
//!   implemented; a pattern containing a magic character raises. The prelude
//!   calls `string.find(id, ":", 1, true)` and the suite never uses a pattern.
//! - `table.sort` takes no comparator. Calling back into the VM from a
//!   callback needs a `Sequence` per comparison; the prelude sorts a list of
//!   strings and nothing else does.

use std::borrow::Cow;

use gc_arena::Collect;
use piccolo::{
    Callback, CallbackReturn, Context, Error, Execution, Function, IntoValue, Sequence,
    SequencePoll, Stack, Table, Value, Variadic,
};

/// Every function this module installs, for the record and for the test that
/// pins the list. Contract section 1.6.
pub const POLYFILLED: &[&str] = &[
    "_G",
    "tonumber",
    "xpcall",
    "getmetatable",
    "setmetatable",
    "string.format",
    "string.rep",
    "string.find",
    "string.byte",
    "string.char",
    "table.concat",
    "table.insert",
    "table.remove",
    "table.sort",
    "bit32.band",
    "bit32.bor",
    "bit32.bxor",
    "bit32.bnot",
    "bit32.lshift",
    "bit32.rshift",
    "bit32.arshift",
    "utf8.char",
    "utf8.codepoint",
    "utf8.len",
    "utf8.charpattern",
];

/// Marker every read-only violation carries, so the adapter can classify it as
/// [`slotted_script::ScriptError::Sandbox`] the way Luau's own wording does.
pub const READONLY_MARKER: &str = "readonly";

/// Sets `key` on `table`. The key is a string built here, so
/// `Table::set` cannot report an invalid key and the result is dropped.
fn set<'gc>(ctx: Context<'gc>, table: Table<'gc>, key: &str, value: impl IntoValue<'gc>) {
    // The key must be interned. piccolo holds table keys weakly and the
    // interned-string set is what keeps them alive; a key built with
    // `String::from_slice` can be collected while the table still holds it,
    // and the next growth of that table panics with "all keys must be live".
    let key = ctx.intern(key.as_bytes());
    let _ = table.set(ctx, key, value);
}

/// Reads a library table out of `env`, creating and installing one when it is
/// absent (`bit32` and `utf8` are absent: piccolo has neither).
fn library<'gc>(ctx: Context<'gc>, env: Table<'gc>, name: &str) -> Table<'gc> {
    if let Value::Table(t) = env.get(ctx, ctx.intern(name.as_bytes())) {
        t
    } else {
        let t = Table::new(&ctx);
        set(ctx, env, name, t);
        t
    }
}

/// Builds the environment table every chunk of one script runs in.
///
/// This is where the sandbox starts. piccolo has no `Lua::sandbox`, and
/// deleting a key from the real globals table is not an option: piccolo
/// tombstones a removed key and then panics ("all keys must be live when
/// table is grown") the next time that table grows, which on wasm is an abort
/// and exactly the failure mode this adapter exists to avoid. So nothing is
/// ever deleted. A fresh table is filled with the globals a mod may have, the
/// rest are simply never copied across, and the chunk runs with this table as
/// its `_ENV`.
pub fn build_env(ctx: Context<'_>) -> Table<'_> {
    let env = Table::new(&ctx);
    for (key, value) in ctx.globals() {
        let forbidden = matches!(key, Value::String(name)
            if slotted_script::FORBIDDEN_GLOBALS
                .iter()
                .any(|f| name.as_bytes() == f.as_bytes()));
        if forbidden {
            continue;
        }
        // The keys come out of the globals table, so they are already
        // interned and cannot be an invalid key.
        let _ = env.set_value(&ctx, key, value);
    }
    install(ctx, env);
    env
}

/// An error value with `msg` as its text.
fn err(ctx: Context<'_>, msg: impl Into<String>) -> Error<'_> {
    let msg: String = msg.into();
    piccolo::String::from_slice(&ctx, msg.as_bytes())
        .into_value(ctx)
        .into()
}

/// Installs everything in [`POLYFILLED`] into the state's globals.
///
/// Must run before [`freeze_stdlib`]: after that the tables reject writes.
fn install<'gc>(ctx: Context<'gc>, env: Table<'gc>) {
    install_base(ctx, env);
    install_string(ctx, env);
    install_table(ctx, env);
    install_bit32(ctx, env);
    install_utf8(ctx, env);
}

// ---------------------------------------------------------------------------
// base
// ---------------------------------------------------------------------------

fn install_base<'gc>(ctx: Context<'gc>, env: Table<'gc>) {
    // `_G` is the environment itself: `sandbox_globals.lua` reads `_G[name]`,
    // and the prelude ends with `_G.slotted = slotted`.
    set(ctx, env, "_G", env);

    set(
        ctx,
        env,
        "tonumber",
        Callback::from_fn(&ctx, |ctx, _, mut stack| {
            let (value, base): (Value, Option<i64>) = stack.consume(ctx)?;
            let out = match (value, base) {
                (v, None) => match v {
                    Value::Integer(i) => Value::Integer(i),
                    Value::Number(n) => Value::Number(n),
                    Value::String(s) => parse_number(&s.to_str_lossy()),
                    _ => Value::Nil,
                },
                (Value::String(s), Some(base)) => {
                    let text = s.to_str_lossy();
                    let text = text.trim();
                    u32::try_from(base)
                        .ok()
                        .filter(|b| (2..=36).contains(b))
                        .and_then(|b| i64::from_str_radix(text, b).ok())
                        .map_or(Value::Nil, Value::Integer)
                }
                _ => Value::Nil,
            };
            stack.replace(ctx, out);
            Ok(CallbackReturn::Return)
        }),
    );

    set(ctx, env, "xpcall", xpcall(ctx));

    // piccolo's own `getmetatable`/`setmetatable` ignore `__metatable`, which
    // Lua 5.4 uses to protect a metatable. The read-only wrappers below set
    // it, and without these two a mod could read `__index` back out of a
    // wrapper and write straight to the real library table.
    set(
        ctx,
        env,
        "getmetatable",
        Callback::from_fn(&ctx, |ctx, _, mut stack| {
            let out = match stack.get(0) {
                Value::Table(t) => t.metatable().map_or(Value::Nil, |meta| {
                    match meta.get(ctx, ctx.intern(b"__metatable")) {
                        Value::Nil => Value::Table(meta),
                        protected => protected,
                    }
                }),
                _ => Value::Nil,
            };
            stack.replace(ctx, out);
            Ok(CallbackReturn::Return)
        }),
    );
    set(
        ctx,
        env,
        "setmetatable",
        Callback::from_fn(&ctx, |ctx, _, mut stack| {
            let (t, meta): (Table, Option<Table>) = stack.consume(ctx)?;
            if let Some(current) = t.metatable()
                && !current.get(ctx, ctx.intern(b"__metatable")).is_nil()
            {
                return Err(err(ctx, "cannot change a protected metatable"));
            }
            t.set_metatable(&ctx, meta);
            stack.replace(ctx, t);
            Ok(CallbackReturn::Return)
        }),
    );
}

/// `tonumber("0x10")`, `tonumber(" 3.5 ")`, `tonumber("nope")`.
fn parse_number<'gc>(text: &str) -> Value<'gc> {
    let text = text.trim();
    if let Some(hex) = text.strip_prefix("0x").or_else(|| text.strip_prefix("0X"))
        && let Ok(i) = i64::from_str_radix(hex, 16)
    {
        return Value::Integer(i);
    }
    if let Ok(i) = text.parse::<i64>() {
        return Value::Integer(i);
    }
    text.parse::<f64>().map_or(Value::Nil, Value::Number)
}

/// `xpcall(f, handler, ...)`: like `pcall`, but the handler sees the error
/// value before the stack is discarded. The prelude does not use it; it is
/// here because `sandbox_globals.lua` requires it and a mod may.
fn xpcall(ctx: Context<'_>) -> Callback<'_> {
    #[derive(Collect)]
    #[collect(no_drop)]
    struct XPCall<'gc> {
        handler: Function<'gc>,
        /// Set once the handler itself has been called, so the second poll
        /// knows it is looking at the handler's return values.
        handled: bool,
    }

    impl<'gc> Sequence<'gc> for XPCall<'gc> {
        fn poll(
            &mut self,
            _ctx: Context<'gc>,
            _exec: Execution<'gc, '_>,
            mut stack: Stack<'gc, '_>,
        ) -> Result<SequencePoll<'gc>, Error<'gc>> {
            stack.push_front(Value::Boolean(!self.handled));
            Ok(SequencePoll::Return)
        }

        fn error(
            &mut self,
            ctx: Context<'gc>,
            _exec: Execution<'gc, '_>,
            error: Error<'gc>,
            mut stack: Stack<'gc, '_>,
        ) -> Result<SequencePoll<'gc>, Error<'gc>> {
            if self.handled {
                // The handler itself raised; report that, unwrapped.
                return Err(error);
            }
            self.handled = true;
            stack.clear();
            stack.push_back(error.to_value(ctx));
            Ok(SequencePoll::Call {
                function: self.handler,
                is_tail: false,
            })
        }
    }

    Callback::from_fn(&ctx, |ctx, _, mut stack| {
        let function = piccolo::meta_ops::call(ctx, stack.get(0))?;
        let handler = piccolo::meta_ops::call(ctx, stack.get(1))?;
        stack.pop_front();
        stack.pop_front();
        Ok(CallbackReturn::Call {
            function,
            then: Some(piccolo::BoxSequence::new(
                &ctx,
                XPCall {
                    handler,
                    handled: false,
                },
            )),
        })
    })
}

// ---------------------------------------------------------------------------
// string
// ---------------------------------------------------------------------------

fn install_string<'gc>(ctx: Context<'gc>, env: Table<'gc>) {
    let string = library(ctx, env, "string");
    install_string_format(ctx, string);
    install_string_search(ctx, string);
    install_string_bytes(ctx, string);
}

/// `string.format`, the one directive-driven function.
fn install_string_format<'gc>(ctx: Context<'gc>, string: Table<'gc>) {
    set(
        ctx,
        string,
        "format",
        Callback::from_fn(&ctx, |ctx, _, mut stack| {
            let (fmt, args): (piccolo::String, Variadic<Vec<Value>>) = stack.consume(ctx)?;
            let out = format(&fmt.to_str_lossy(), &args).map_err(|e| err(ctx, e))?;
            stack.replace(ctx, piccolo::String::from_slice(&ctx, out.as_bytes()));
            Ok(CallbackReturn::Return)
        }),
    );
}

/// `string.rep` and `string.find`: build a string, or look one up.
fn install_string_search<'gc>(ctx: Context<'gc>, string: Table<'gc>) {
    set(
        ctx,
        string,
        "rep",
        Callback::from_fn(&ctx, |ctx, _, mut stack| {
            let (s, n, sep): (piccolo::String, i64, Option<piccolo::String>) =
                stack.consume(ctx)?;
            let n = usize::try_from(n).unwrap_or(0);
            let sep = sep.map_or_else(Vec::new, |s| s.as_bytes().to_vec());
            let mut out = Vec::with_capacity((s.as_bytes().len() + sep.len()) * n);
            for i in 0..n {
                if i > 0 {
                    out.extend_from_slice(&sep);
                }
                out.extend_from_slice(s.as_bytes());
            }
            stack.replace(ctx, piccolo::String::from_slice(&ctx, out));
            Ok(CallbackReturn::Return)
        }),
    );

    set(
        ctx,
        string,
        "find",
        Callback::from_fn(&ctx, |ctx, _, mut stack| {
            let (s, pattern, init, plain): (
                piccolo::String,
                piccolo::String,
                Option<i64>,
                Option<Value>,
            ) = stack.consume(ctx)?;
            let plain = plain.is_some_and(piccolo::Value::to_bool);
            let pattern_bytes = pattern.as_bytes();
            if !plain && pattern_bytes.iter().any(|b| is_magic(*b)) {
                return Err(err(
                    ctx,
                    "string.find: Lua patterns are not available in this runtime; \
                     pass plain = true",
                ));
            }
            let hay = s.as_bytes();
            let start = match init.unwrap_or(1) {
                i if i > 0 => usize::try_from(i - 1).unwrap_or(0),
                0 => 0,
                i => hay
                    .len()
                    .saturating_sub(usize::try_from(i.unsigned_abs()).unwrap_or(usize::MAX)),
            };
            let found = if start > hay.len() {
                None
            } else {
                hay[start..]
                    .windows(pattern_bytes.len().max(1))
                    .position(|w| pattern_bytes.is_empty() || w == pattern_bytes)
                    .map(|p| start + p)
            };
            match found {
                Some(at) => stack.replace(
                    ctx,
                    (
                        i64::try_from(at + 1).unwrap_or(i64::MAX),
                        i64::try_from(at + pattern_bytes.len()).unwrap_or(i64::MAX),
                    ),
                ),
                None => stack.replace(ctx, Value::Nil),
            }
            Ok(CallbackReturn::Return)
        }),
    );
}

/// `string.byte` and `string.char`: the byte-level pair.
fn install_string_bytes<'gc>(ctx: Context<'gc>, string: Table<'gc>) {
    set(
        ctx,
        string,
        "byte",
        Callback::from_fn(&ctx, |ctx, _, mut stack| {
            let (s, i, j): (piccolo::String, Option<i64>, Option<i64>) = stack.consume(ctx)?;
            let bytes = s.as_bytes();
            let i = i.unwrap_or(1);
            let j = j.unwrap_or(i);
            let (from, to) = (index(i, bytes.len()), index(j, bytes.len()));
            let out: Vec<Value> = if from > to {
                Vec::new()
            } else {
                bytes[from - 1..to]
                    .iter()
                    .map(|b| Value::Integer(i64::from(*b)))
                    .collect()
            };
            stack.replace(ctx, Variadic(out));
            Ok(CallbackReturn::Return)
        }),
    );

    set(
        ctx,
        string,
        "char",
        Callback::from_fn(&ctx, |ctx, _, mut stack| {
            let args: Variadic<Vec<i64>> = stack.consume(ctx)?;
            let mut out = Vec::with_capacity(args.len());
            for code in &args {
                out.push(
                    u8::try_from(*code).map_err(|_| {
                        err(ctx, format!("string.char: {code} is not a byte value"))
                    })?,
                );
            }
            stack.replace(ctx, piccolo::String::from_slice(&ctx, out));
            Ok(CallbackReturn::Return)
        }),
    );
}

/// A 1-based Lua index, negative counting from the end, clamped to `1..=len`.
fn index(i: i64, len: usize) -> usize {
    let len = i64::try_from(len).unwrap_or(i64::MAX);
    let resolved = if i < 0 { len + i + 1 } else { i };
    usize::try_from(resolved.clamp(0, len)).unwrap_or(0)
}

/// Whether `b` is one of Lua's pattern metacharacters.
fn is_magic(b: u8) -> bool {
    matches!(
        b,
        b'^' | b'$' | b'*' | b'+' | b'?' | b'.' | b'(' | b')' | b'[' | b']' | b'%' | b'-'
    )
}

/// `string.format` for the directives the prelude and mods actually use:
/// `%%`, `%s`, `%q`, `%d`/`%i`, `%u`, `%c`, `%x`/`%X`, `%o`, `%f`/`%e`/`%g`,
/// each with optional `-`, `0`, width and `.precision`.
///
/// # Errors
///
/// A message naming the directive when it is unknown, or the argument when it
/// is the wrong type or missing.
#[allow(clippy::too_many_lines)]
// Splitting this further would mean threading the parse cursor, the flags, the
// width and the precision through three signatures to save nine lines; the
// directive table reads better in one place.
fn format(fmt: &str, args: &[Value<'_>]) -> Result<String, String> {
    let bytes = fmt.as_bytes();
    let mut out = String::with_capacity(fmt.len());
    let mut next = 0usize;
    let mut i = 0usize;

    while i < bytes.len() {
        if bytes[i] != b'%' {
            let start = i;
            while i < bytes.len() && bytes[i] != b'%' {
                i += 1;
            }
            out.push_str(&fmt[start..i]);
            continue;
        }
        i += 1;
        if i >= bytes.len() {
            return Err("string.format: the format ends with a lone `%`".to_owned());
        }
        if bytes[i] == b'%' {
            out.push('%');
            i += 1;
            continue;
        }

        let mut left = false;
        let mut zero = false;
        while i < bytes.len() && matches!(bytes[i], b'-' | b'0' | b'+' | b' ' | b'#') {
            left |= bytes[i] == b'-';
            zero |= bytes[i] == b'0';
            i += 1;
        }
        let mut width = 0usize;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            width = width * 10 + usize::from(bytes[i] - b'0');
            i += 1;
        }
        let mut precision: Option<usize> = None;
        if i < bytes.len() && bytes[i] == b'.' {
            i += 1;
            let mut p = 0usize;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                p = p * 10 + usize::from(bytes[i] - b'0');
                i += 1;
            }
            precision = Some(p);
        }
        if i >= bytes.len() {
            return Err("string.format: the format ends inside a directive".to_owned());
        }
        let conversion = bytes[i];
        i += 1;

        let arg = args.get(next).copied().ok_or_else(|| {
            format!(
                "string.format: no value for directive #{} (%{})",
                next + 1,
                conversion as char
            )
        })?;
        next += 1;

        let mut piece = match conversion {
            b'd' | b'i' | b'u' => integer(arg, conversion)?.to_string(),
            b'c' => {
                let code = integer(arg, conversion)?;
                u8::try_from(code)
                    .map(|b| (b as char).to_string())
                    .map_err(|_| format!("string.format: %c wants a byte, got {code}"))?
            }
            b'x' => format!("{:x}", integer(arg, conversion)?),
            b'X' => format!("{:X}", integer(arg, conversion)?),
            b'o' => format!("{:o}", integer(arg, conversion)?),
            b'f' | b'F' => format!("{:.*}", precision.unwrap_or(6), float(arg, conversion)?),
            b'e' => format!("{:.*e}", precision.unwrap_or(6), float(arg, conversion)?),
            b'g' | b'G' => {
                let v = float(arg, conversion)?;
                let mut text = format!("{v}");
                if let Some(p) = precision {
                    text = format!("{v:.p$}");
                }
                text
            }
            b's' => {
                let mut text = text_of(arg);
                if let Some(p) = precision
                    && text.len() > p
                {
                    text.truncate(p);
                }
                text
            }
            b'q' => quoted(&text_of(arg)),
            other => {
                return Err(format!(
                    "string.format: %{} is not supported by this runtime",
                    other as char
                ));
            }
        };

        if piece.chars().count() < width {
            let pad = width - piece.chars().count();
            if left {
                piece.push_str(&" ".repeat(pad));
            } else if zero && matches!(conversion, b'd' | b'i' | b'u' | b'x' | b'X' | b'o' | b'f') {
                let at = usize::from(piece.starts_with('-'));
                piece.insert_str(at, &"0".repeat(pad));
            } else {
                piece.insert_str(0, &" ".repeat(pad));
            }
        }
        out.push_str(&piece);
    }
    Ok(out)
}

/// The integer a `%d`-family directive wants.
fn integer(value: Value<'_>, conversion: u8) -> Result<i64, String> {
    #[allow(clippy::cast_possible_truncation)]
    match value {
        Value::Integer(i) => Ok(i),
        Value::Number(n) if n.fract() == 0.0 && n.is_finite() => Ok(n as i64),
        other => Err(format!(
            "string.format: %{} wants a number, got {}",
            conversion as char,
            other.type_name()
        )),
    }
}

/// The float a `%f`-family directive wants.
fn float(value: Value<'_>, conversion: u8) -> Result<f64, String> {
    #[allow(clippy::cast_precision_loss)]
    match value {
        Value::Integer(i) => Ok(i as f64),
        Value::Number(n) => Ok(n),
        other => Err(format!(
            "string.format: %{} wants a number, got {}",
            conversion as char,
            other.type_name()
        )),
    }
}

/// `%s` text: the same rendering piccolo's own `tostring` uses, without
/// consulting a `__tostring` metamethod (a callback cannot call back into the
/// VM without a `Sequence`, and no case needs it).
fn text_of(value: Value<'_>) -> String {
    match value {
        Value::String(s) => s.to_str_lossy().into_owned(),
        other => other.to_string(),
    }
}

/// `%q`: a Lua-readable quoted string.
fn quoted(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\0' => out.push_str("\\0"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

// ---------------------------------------------------------------------------
// table
// ---------------------------------------------------------------------------

fn install_table<'gc>(ctx: Context<'gc>, env: Table<'gc>) {
    let table = library(ctx, env, "table");
    install_table_concat(ctx, table);
    install_table_edit(ctx, table);
    install_table_sort(ctx, table);
}

/// `table.concat`, which the prelude's `print` needs.
fn install_table_concat<'gc>(ctx: Context<'gc>, table: Table<'gc>) {
    set(
        ctx,
        table,
        "concat",
        Callback::from_fn(&ctx, |ctx, _, mut stack| {
            let (t, sep, from, to): (Table, Option<piccolo::String>, Option<i64>, Option<i64>) =
                stack.consume(ctx)?;
            let sep = sep.map_or_else(Vec::new, |s| s.as_bytes().to_vec());
            let from = from.unwrap_or(1);
            let to = to.unwrap_or_else(|| t.length());
            let mut out: Vec<u8> = Vec::new();
            let mut index = from;
            while index <= to {
                if index > from {
                    out.extend_from_slice(&sep);
                }
                match t.get_value(Value::Integer(index)) {
                    Value::String(s) => out.extend_from_slice(s.as_bytes()),
                    v @ (Value::Integer(_) | Value::Number(_)) => {
                        out.extend_from_slice(v.to_string().as_bytes());
                    }
                    other => {
                        return Err(err(
                            ctx,
                            format!(
                                "table.concat: element #{index} is a {}, not a string or number",
                                other.type_name()
                            ),
                        ));
                    }
                }
                index += 1;
            }
            stack.replace(ctx, piccolo::String::from_slice(&ctx, out));
            Ok(CallbackReturn::Return)
        }),
    );
}

/// `table.insert` and `table.remove`.
fn install_table_edit<'gc>(ctx: Context<'gc>, table: Table<'gc>) {
    set(
        ctx,
        table,
        "insert",
        Callback::from_fn(&ctx, |ctx, _, mut stack| {
            let args: Variadic<Vec<Value>> = stack.consume(ctx)?;
            let Some(Value::Table(t)) = args.first().copied() else {
                return Err(err(ctx, "table.insert: first argument must be a table"));
            };
            let len = t.length();
            match args.len() {
                2 => {
                    let _ = t.set(ctx, len + 1, args[1]);
                }
                3 => {
                    let Value::Integer(pos) = args[1] else {
                        return Err(err(ctx, "table.insert: position must be an integer"));
                    };
                    if pos < 1 || pos > len + 1 {
                        return Err(err(ctx, "table.insert: position out of bounds"));
                    }
                    let mut i = len;
                    while i >= pos {
                        let moved = t.get_value(Value::Integer(i));
                        let _ = t.set(ctx, i + 1, moved);
                        i -= 1;
                    }
                    let _ = t.set(ctx, pos, args[2]);
                }
                _ => return Err(err(ctx, "table.insert: wrong number of arguments")),
            }
            Ok(CallbackReturn::Return)
        }),
    );

    set(
        ctx,
        table,
        "remove",
        Callback::from_fn(&ctx, |ctx, _, mut stack| {
            let (t, pos): (Table, Option<i64>) = stack.consume(ctx)?;
            let len = t.length();
            let pos = pos.unwrap_or(len);
            if len == 0 {
                stack.replace(ctx, Value::Nil);
                return Ok(CallbackReturn::Return);
            }
            if pos < 1 || pos > len {
                return Err(err(ctx, "table.remove: position out of bounds"));
            }
            let removed = t.get_value(Value::Integer(pos));
            let mut i = pos;
            while i < len {
                let moved = t.get_value(Value::Integer(i + 1));
                let _ = t.set(ctx, i, moved);
                i += 1;
            }
            let _ = t.set(ctx, len, Value::Nil);
            stack.replace(ctx, removed);
            Ok(CallbackReturn::Return)
        }),
    );
}

/// `table.sort`, comparator-free; the prelude sorts event names.
fn install_table_sort<'gc>(ctx: Context<'gc>, table: Table<'gc>) {
    set(
        ctx,
        table,
        "sort",
        Callback::from_fn(&ctx, |ctx, _, mut stack| {
            let (t, comparator): (Table, Option<Value>) = stack.consume(ctx)?;
            if comparator.is_some_and(|c| !c.is_nil()) {
                return Err(err(
                    ctx,
                    "table.sort: a comparator is not supported by this runtime",
                ));
            }
            let len = t.length();
            let mut items: Vec<Value> = (1..=len).map(|i| t.get_value(Value::Integer(i))).collect();
            let mut failure: Option<String> = None;
            items.sort_by(|a, b| {
                order(*a, *b).unwrap_or_else(|| {
                    failure.get_or_insert_with(|| {
                        format!(
                            "table.sort: cannot compare a {} with a {}",
                            a.type_name(),
                            b.type_name()
                        )
                    });
                    std::cmp::Ordering::Equal
                })
            });
            if let Some(message) = failure {
                return Err(err(ctx, message));
            }
            for (i, item) in items.into_iter().enumerate() {
                let _ = t.set(ctx, i64::try_from(i + 1).unwrap_or(i64::MAX), item);
            }
            Ok(CallbackReturn::Return)
        }),
    );
}

/// Default `<` ordering, for numbers and strings only.
fn order(a: Value<'_>, b: Value<'_>) -> Option<std::cmp::Ordering> {
    #[allow(clippy::cast_precision_loss)]
    match (a, b) {
        (Value::Integer(a), Value::Integer(b)) => Some(a.cmp(&b)),
        (Value::Integer(a), Value::Number(b)) => (a as f64).partial_cmp(&b),
        (Value::Number(a), Value::Integer(b)) => a.partial_cmp(&(b as f64)),
        (Value::Number(a), Value::Number(b)) => a.partial_cmp(&b),
        (Value::String(a), Value::String(b)) => Some(a.as_bytes().cmp(b.as_bytes())),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// bit32 and utf8
// ---------------------------------------------------------------------------

fn install_bit32<'gc>(ctx: Context<'gc>, env: Table<'gc>) {
    let bit32 = library(ctx, env, "bit32");

    /// `f` over every argument, folded, as a `bit32` callback.
    macro_rules! fold {
        ($name:literal, $init:expr, $f:expr) => {
            set(
                ctx,
                bit32,
                $name,
                Callback::from_fn(&ctx, |ctx, _, mut stack| {
                    let args: Variadic<Vec<i64>> = stack.consume(ctx)?;
                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    let out = args
                        .iter()
                        .map(|v| *v as u32)
                        .fold($init, |acc: u32, v: u32| $f(acc, v));
                    stack.replace(ctx, i64::from(out));
                    Ok(CallbackReturn::Return)
                }),
            );
        };
    }

    fold!("band", u32::MAX, |a: u32, b: u32| a & b);
    fold!("bor", 0u32, |a: u32, b: u32| a | b);
    fold!("bxor", 0u32, |a: u32, b: u32| a ^ b);

    /// A one-argument-plus-shift callback.
    macro_rules! shift {
        ($name:literal, $f:expr) => {
            set(
                ctx,
                bit32,
                $name,
                Callback::from_fn(&ctx, |ctx, _, mut stack| {
                    let (v, n): (i64, i64) = stack.consume(ctx)?;
                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    let out: u32 = $f(v as u32, n.clamp(0, 32) as u32);
                    stack.replace(ctx, i64::from(out));
                    Ok(CallbackReturn::Return)
                }),
            );
        };
    }

    shift!("lshift", |v: u32, n: u32| v.checked_shl(n).unwrap_or(0));
    shift!("rshift", |v: u32, n: u32| v.checked_shr(n).unwrap_or(0));
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_wrap)]
    {
        shift!(
            "arshift",
            |v: u32, n: u32| ((v as i32).checked_shr(n).unwrap_or(if v as i32 >= 0 {
                0
            } else {
                -1
            })) as u32
        );
    }

    set(
        ctx,
        bit32,
        "bnot",
        Callback::from_fn(&ctx, |ctx, _, mut stack| {
            let v: i64 = stack.consume(ctx)?;
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let out = !(v as u32);
            stack.replace(ctx, i64::from(out));
            Ok(CallbackReturn::Return)
        }),
    );
}

fn install_utf8<'gc>(ctx: Context<'gc>, env: Table<'gc>) {
    let utf8 = library(ctx, env, "utf8");

    set(
        ctx,
        utf8,
        "charpattern",
        piccolo::String::from_slice(&ctx, b"[\0-\x7F\xC2-\xFD][\x80-\xBF]*".as_slice()),
    );

    set(
        ctx,
        utf8,
        "char",
        Callback::from_fn(&ctx, |ctx, _, mut stack| {
            let args: Variadic<Vec<i64>> = stack.consume(ctx)?;
            let mut out = String::new();
            for code in &args {
                let ch = u32::try_from(*code)
                    .ok()
                    .and_then(char::from_u32)
                    .ok_or_else(|| err(ctx, format!("utf8.char: {code} is not a code point")))?;
                out.push(ch);
            }
            stack.replace(ctx, piccolo::String::from_slice(&ctx, out.as_bytes()));
            Ok(CallbackReturn::Return)
        }),
    );

    set(
        ctx,
        utf8,
        "codepoint",
        Callback::from_fn(&ctx, |ctx, _, mut stack| {
            let (s, i, j): (piccolo::String, Option<i64>, Option<i64>) = stack.consume(ctx)?;
            let text = s.to_str_lossy();
            let bytes = text.as_bytes();
            let i = i.unwrap_or(1);
            let j = j.unwrap_or(i);
            let (from, to) = (index(i, bytes.len()), index(j, bytes.len()));
            let mut out = Vec::new();
            for (at, ch) in text.char_indices() {
                if at + 1 >= from && at < to {
                    out.push(Value::Integer(i64::from(u32::from(ch))));
                }
            }
            stack.replace(ctx, Variadic(out));
            Ok(CallbackReturn::Return)
        }),
    );

    set(
        ctx,
        utf8,
        "len",
        Callback::from_fn(&ctx, |ctx, _, mut stack| {
            let s: piccolo::String = stack.consume(ctx)?;
            match std::str::from_utf8(s.as_bytes()) {
                Ok(text) => stack.replace(ctx, i64::try_from(text.chars().count()).unwrap_or(0)),
                Err(e) => stack.replace(
                    ctx,
                    (
                        Value::Nil,
                        i64::try_from(e.valid_up_to() + 1).unwrap_or(i64::MAX),
                    ),
                ),
            }
            Ok(CallbackReturn::Return)
        }),
    );
}

// ---------------------------------------------------------------------------
// the read-only wrapper
// ---------------------------------------------------------------------------

/// Wraps `inner` in an empty table whose `__index` reads through and whose
/// `__newindex` raises, and returns the wrapper.
///
/// Luau gets this from `sandbox(true)`, which marks a table read-only.
/// piccolo has no such flag, and a metatable alone is not enough: `__newindex`
/// only fires for a key the table does not already have, so
/// `string.format = f` would silently succeed on the real table. The empty
/// wrapper has no keys at all, so every write goes through `__newindex`.
pub fn readonly<'gc>(ctx: Context<'gc>, name: &str, inner: Table<'gc>) -> Table<'gc> {
    let proxy = Table::new(&ctx);
    let meta = Table::new(&ctx);
    set(ctx, meta, "__index", inner);
    let owner = piccolo::String::from_slice(&ctx, name.as_bytes());
    set(
        ctx,
        meta,
        "__newindex",
        Callback::from_fn_with(&ctx, owner, |owner, ctx, _, mut stack| {
            let (_, key, _): (Value, Value, Value) = stack.consume(ctx)?;
            Err(err(
                ctx,
                format!(
                    "{}.{} is {READONLY_MARKER}",
                    owner.to_str_lossy(),
                    text_of(key)
                ),
            ))
        }),
    );
    // `__metatable` hides the wrapper from `getmetatable`, so a mod cannot
    // read `__index` back out and write to the real table through it.
    set(ctx, meta, "__metatable", Value::Boolean(false));
    proxy.set_metatable(&ctx, Some(meta));
    proxy
}

/// Replaces every library table in `env` with a read-only wrapper. Nothing may
/// be installed into them afterwards.
pub fn freeze_stdlib<'gc>(ctx: Context<'gc>, env: Table<'gc>) {
    for name in ["string", "table", "math", "coroutine", "bit32", "utf8"] {
        if let Value::Table(inner) = env.get(ctx, ctx.intern(name.as_bytes())) {
            let proxy = readonly(ctx, name, inner);
            set(ctx, env, name, proxy);
        }
    }
}

/// The text of an error value, for the adapter's messages.
pub fn error_text(value: Value<'_>) -> Cow<'_, str> {
    match value {
        Value::String(s) => s.to_str_lossy(),
        other => Cow::Owned(other.to_string()),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn padding_and_precision() {
        assert_eq!(format("%5d", &[Value::Integer(42)]).unwrap(), "   42");
        assert_eq!(format("%-5d|", &[Value::Integer(42)]).unwrap(), "42   |");
        assert_eq!(format("%05d", &[Value::Integer(-42)]).unwrap(), "-0042");
        assert_eq!(format("%.2f", &[Value::Number(1.5)]).unwrap(), "1.50");
        assert_eq!(format("100%%", &[]).unwrap(), "100%");
    }

    #[test]
    fn an_unknown_directive_is_named() {
        let err = format("%p", &[Value::Integer(1)]).unwrap_err();
        assert!(err.contains("%p"), "{err}");
    }
}
