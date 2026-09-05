//! The `luaur` (pure-Rust Luau) backend. Compiles for both native and
//! wasm32-unknown-unknown from the same source.

use crate::{ScreenSpec, SCRIPT};
use luaur::LuaSerdeExt;
use luaur::{Function, Lua, Result, Table, Value, VmState};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// Names a mod script must not see. Luau already omits `io`, `package`,
/// `require`, `load`, `loadstring`, `dofile` and `loadfile` from its stdlib, so
/// only the rest need removing.
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
/// Two things matter here and neither is obvious from the API:
///
/// * `StdLib` flags are all-or-nothing in luaur 0.1.8. Anything other than
///   `StdLib::NONE` opens the whole safe stdlib, so the unwanted globals have to
///   be deleted by hand.
/// * Deletions must happen **before** `sandbox(true)`. Afterwards a write to
///   `globals()` returns `Ok(())` and silently does nothing.
pub fn sandboxed_lua() -> Result<Lua> {
    let lua = Lua::new();
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

/// Install an instruction budget. The interrupt fires periodically; raising
/// from it aborts the running chunk.
pub fn set_instruction_budget(lua: &Lua, budget: u64) -> Arc<AtomicU64> {
    let counter = Arc::new(AtomicU64::new(0));
    let seen = counter.clone();
    lua.set_interrupt(move |_| {
        let n = seen.fetch_add(1, Ordering::Relaxed) + 1;
        if n > budget {
            Err(luaur::Error::runtime("instruction budget exhausted"))
        } else {
            Ok(VmState::Continue)
        }
    });
    counter
}

/// Register a Rust function callable from Lua.
pub fn register_rust_fn(lua: &Lua) -> Result<()> {
    let add = lua.create_function(|_, (a, b): (i64, i64)| Ok(a + b))?;
    lua.globals().set("rust_add", add)?;
    Ok(())
}

/// Load the shared chunk and register the host function.
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

/// Serde round trip: Rust struct -> Lua table -> Lua mutates it -> Rust struct.
pub fn round_trip(lua: &Lua, spec: &ScreenSpec) -> Result<ScreenSpec> {
    let value = lua.to_value(spec)?;
    let transform: Function = lua.globals().get("transform")?;
    let out: Value = transform.call(value)?;
    lua.from_value(out)
}

/// The hot path used for the per-call cost measurement.
pub fn hot_fn(lua: &Lua) -> Result<(Function, Table)> {
    let f: Function = lua.globals().get("hot")?;
    let arg = lua.create_table();
    arg.set("a", 1i64)?;
    arg.set("b", 2i64)?;
    Ok((f, arg))
}

/// One call with a freshly built small table (what an event dispatch costs).
pub fn call_with_fresh_table(lua: &Lua, f: &Function, a: i64, b: i64) -> Result<i64> {
    let t = lua.create_table();
    t.set("a", a)?;
    t.set("b", b)?;
    f.call(t)
}
