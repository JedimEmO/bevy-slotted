//! The `mlua` (C Luau, vendored) backend. Native only.

use crate::{ScreenSpec, SCRIPT};
use mlua::{Function, Lua, LuaOptions, LuaSerdeExt, Result, StdLib, Table, Value, VmState};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// Names a mod script must not see.
pub const FORBIDDEN_GLOBALS: &[&str] = &[
    "io",
    "os",
    "package",
    "require",
    "dofile",
    "loadfile",
    "load",
    "loadstring",
    "debug",
    "collectgarbage",
    "getfenv",
    "setfenv",
    "newproxy",
];

/// Build a state a mod script may run in.
///
/// `StdLib` flags are honoured here (unlike luaur), but mlua injects `require`
/// and `loadstring` of its own, so a manual pass is still needed. As with
/// luaur, deletions must happen **before** `sandbox(true)`: afterwards a write
/// to `globals()` returns `Ok(())` and does nothing.
pub fn sandboxed_lua() -> Result<Lua> {
    let libs = StdLib::TABLE | StdLib::STRING | StdLib::MATH | StdLib::BIT;
    let lua = Lua::new_with(libs, LuaOptions::new())?;
    let globals = lua.globals();
    for name in FORBIDDEN_GLOBALS {
        globals.set(*name, Value::Nil)?;
    }
    lua.sandbox(true)?;
    Ok(lua)
}

pub fn missing_globals(lua: &Lua) -> Result<Vec<String>> {
    let globals = lua.globals();
    let mut absent = Vec::new();
    for name in FORBIDDEN_GLOBALS {
        if globals.get::<Value>(*name)? == Value::Nil {
            absent.push((*name).to_string());
        }
    }
    Ok(absent)
}

pub fn register_rust_fn(lua: &Lua) -> Result<()> {
    let add = lua.create_function(|_, (a, b): (i64, i64)| Ok(a + b))?;
    lua.globals().set("rust_add", add)?;
    Ok(())
}

pub fn prepare() -> Result<Lua> {
    let lua = sandboxed_lua()?;
    register_rust_fn(&lua)?;
    lua.load(SCRIPT).set_name("spike").exec()?;
    Ok(lua)
}

pub fn chunk_ran(lua: &Lua) -> Result<String> {
    lua.globals().get("loaded_marker")
}

pub fn call_rust_from_lua(lua: &Lua, a: i64, b: i64) -> Result<i64> {
    let f: Function = lua.globals().get("use_rust")?;
    f.call((a, b))
}

pub fn round_trip(lua: &Lua, spec: &ScreenSpec) -> Result<ScreenSpec> {
    let value = lua.to_value(spec)?;
    let transform: Function = lua.globals().get("transform")?;
    let out: Value = transform.call(value)?;
    lua.from_value(out)
}

pub fn hot_fn(lua: &Lua) -> Result<(Function, Table)> {
    let f: Function = lua.globals().get("hot")?;
    let arg = lua.create_table()?;
    arg.set("a", 1i64)?;
    arg.set("b", 2i64)?;
    Ok((f, arg))
}

pub fn call_with_fresh_table(lua: &Lua, f: &Function, a: i64, b: i64) -> Result<i64> {
    let t = lua.create_table()?;
    t.set("a", a)?;
    t.set("b", b)?;
    f.call(t)
}

/// Install an instruction budget. The interrupt fires every N VM instructions;
/// returning `VmState::Yield` is not valid outside a coroutine, so a blown
/// budget must raise instead.
pub fn set_instruction_budget(lua: &Lua, budget: u64) -> Arc<AtomicU64> {
    let counter = Arc::new(AtomicU64::new(0));
    let seen = counter.clone();
    lua.set_interrupt(move |_| {
        let n = seen.fetch_add(1, Ordering::Relaxed) + 1;
        if n > budget {
            Err(mlua::Error::runtime("instruction budget exhausted"))
        } else {
            Ok(VmState::Continue)
        }
    });
    counter
}
