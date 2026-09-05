//! The piccolo 0.3 fallback, kept because luaur cannot report a Lua runtime
//! error on wasm. Same three operations, plus fuel limits and error behaviour.
//!
//! Note how much longer this is than `luaur_backend`/`mlua_backend` for the
//! same work: piccolo has no serde bridge, no `Function::call`, GC-arena
//! lifetimes that stop values crossing an `enter` boundary, and untyped
//! `Table::get_value`. That gap is the adapter cost.

use piccolo::{
    Callback, CallbackReturn, Closure, Context, Executor, Function, IntoValue, Lua, StaticError,
    Table, Value,
};

pub const SCRIPT: &str = r#"
loaded_marker = "chunk-ran"
function use_rust(a, b) return rust_add(a, b) end
function transform(spec)
    spec.title = spec.title .. " (modded)"
    spec.meta.api_version = spec.meta.api_version + 1
    for _, slot in ipairs(spec.slots) do slot.role = "lua_" .. slot.role end
    return spec
end
"#;

/// Every string key has to be interned into the GC arena first; a plain `&str`
/// borrows for too short a lifetime to be used as a global key.
fn func<'gc>(ctx: Context<'gc>, name: &str) -> Function<'gc> {
    let key = piccolo::String::from_slice(&ctx, name);
    match ctx.get_global(key) {
        Value::Function(f) => f,
        other => panic!("{name} is {other:?}"),
    }
}

pub fn prepare() -> Result<Lua, StaticError> {
    let mut lua = Lua::core();
    let ex = lua.try_enter(|ctx| {
        let add = Callback::from_fn(&ctx, |ctx, _, mut stack| {
            let (a, b): (i64, i64) = stack.consume(ctx)?;
            stack.replace(ctx, a + b);
            Ok(CallbackReturn::Return)
        });
        ctx.set_global("rust_add", add)?;
        let closure = Closure::load(ctx, None, SCRIPT.as_bytes())?;
        Ok(ctx.stash(Executor::start(ctx, closure.into(), ())))
    })?;
    lua.execute::<()>(&ex)?;
    Ok(lua)
}

pub fn marker(lua: &mut Lua) -> Result<String, StaticError> {
    lua.try_enter(|ctx| match ctx.get_global("loaded_marker") {
        Value::String(s) => Ok(s.to_str().unwrap().to_string()),
        other => panic!("marker is {other:?}"),
    })
}

pub fn call_rust_from_lua(lua: &mut Lua, a: i64, b: i64) -> Result<i64, StaticError> {
    let ex = lua.try_enter(|ctx| {
        let f = func(ctx, "use_rust");
        Ok(ctx.stash(Executor::start(ctx, f, (a, b))))
    })?;
    lua.execute::<i64>(&ex)
}

/// A nested table round trip, written by hand because piccolo has no serde
/// bridge. This is the whole cost story for the adapter.
pub fn round_trip(lua: &mut Lua) -> Result<(String, i64, Vec<String>), StaticError> {
    let ex = lua.try_enter(|ctx| {
        let spec = Table::new(&ctx);
        spec.set(ctx, "id", "copper_chest")?;
        spec.set(ctx, "title", "Copper Chest")?;
        let meta = Table::new(&ctx);
        meta.set(ctx, "api_version", 1)?;
        spec.set(ctx, "meta", meta)?;
        let slots = Table::new(&ctx);
        for i in 0..3i64 {
            let slot = Table::new(&ctx);
            slot.set(ctx, "index", i)?;
            let role = piccolo::String::from_slice(&ctx, format!("storage_{i}"));
            slot.set(ctx, "role", role)?;
            slot.set(ctx, "locked", i == 2)?;
            slots.set(ctx, i + 1, slot)?;
        }
        spec.set(ctx, "slots", slots)?;
        // Stash the result in a global: `Table` cannot cross the `execute`
        // boundary, because the GC arena branding does not outlive `enter`.
        let store = Callback::from_fn(&ctx, |ctx, _, mut stack| {
            let v: Value = stack.consume(ctx)?;
            ctx.set_global("result", v)?;
            Ok(CallbackReturn::Return)
        });
        ctx.set_global("store", store)?;
        let f = func(ctx, "transform");
        let wrapper = Closure::load(ctx, None, b"store(transform(...))".as_slice())?;
        let _ = f;
        Ok(ctx.stash(Executor::start(ctx, wrapper.into(), (spec,))))
    })?;
    lua.execute::<()>(&ex)?;

    lua.try_enter(|ctx| {
        let t = match ctx.get_global("result") {
            Value::Table(t) => t,
            v => panic!("result is {v:?}"),
        };
        let title = match t.get_value("title".into_value(ctx)) {
            Value::String(s) => s.to_str().unwrap().to_string(),
            v => panic!("title is {v:?}"),
        };
        let meta = match t.get_value("meta".into_value(ctx)) {
            Value::Table(m) => m,
            v => panic!("meta is {v:?}"),
        };
        let api = match meta.get_value("api_version".into_value(ctx)) {
            Value::Integer(i) => i,
            Value::Number(n) => n as i64,
            v => panic!("api_version is {v:?}"),
        };
        let slots = match t.get_value("slots".into_value(ctx)) {
            Value::Table(s) => s,
            v => panic!("slots is {v:?}"),
        };
        let mut roles = Vec::new();
        for i in 1..=3i64 {
            let slot = match slots.get_value(Value::Integer(i)) {
                Value::Table(s) => s,
                v => panic!("slot is {v:?}"),
            };
            match slot.get_value("role".into_value(ctx)) {
                Value::String(s) => roles.push(s.to_str().unwrap().to_string()),
                v => panic!("role is {v:?}"),
            }
        }
        Ok((title, api, roles))
    })
}

/// A Lua runtime error: `Result`, or panic?
pub fn runtime_error_is_a_result() -> bool {
    let mut lua = Lua::core();
    let r = lua
        .try_enter(|ctx| {
            let c = Closure::load(ctx, None, b"local x = nil return x + 1".as_slice())?;
            Ok(ctx.stash(Executor::start(ctx, c.into(), ())))
        })
        .and_then(|ex| lua.execute::<()>(&ex));
    r.is_err()
}

/// Fuel: piccolo's cooperative instruction budget.
pub fn fuel_stops_a_runaway_loop() -> bool {
    let mut lua = Lua::core();
    let ex = lua
        .try_enter(|ctx| {
            let c = Closure::load(
                ctx,
                None,
                b"local s = 0 for i = 1, 100000000 do s = s + i end return s".as_slice(),
            )?;
            Ok(ctx.stash(Executor::start(ctx, c.into(), ())))
        })
        .unwrap();
    // Step with a fixed fuel budget instead of running to completion.
    let mut fuel = piccolo::Fuel::with(10_000);
    let finished = lua.enter(|ctx| ctx.fetch(&ex).step(ctx, &mut fuel));
    !finished
}
