//! Regression test for the SLOTTED PATCH in `src/table/raw.rs`: a table that
//! has had a key removed must survive being grown afterwards.
//!
//! Before the patch, `RawTable::set_value` rehashed the map on growth and
//! called `live_key().expect("all keys must be live when table is grown")` on
//! the tombstone a removal leaves behind. Every case below panicked. Upstream
//! fixed this after 0.3.3 by filtering dead keys in `RawTable::reserve_map`.

use piccolo::{Closure, Executor, Lua, StaticError, Table, Value};

/// Runs `source` and returns what it returns.
fn run<R: for<'gc> piccolo::FromMultiValue<'gc>>(source: &str) -> Result<R, StaticError> {
    let mut lua = Lua::core();
    let executor = lua.try_enter(|ctx| {
        let closure = Closure::load(ctx, None, source.as_bytes())?;
        Ok(ctx.stash(Executor::start(ctx, closure.into(), ())))
    })?;
    lua.execute::<R>(&executor)
}

#[test]
fn delete_then_grow_the_map_part() {
    let count: i64 = run(
        r#"
        local t = {}
        for i = 1, 64 do t["k" .. i] = i end
        t.k1 = nil
        for i = 65, 512 do t["k" .. i] = i end
        local n = 0
        for _ in pairs(t) do n = n + 1 end
        return n
        "#,
    )
    .expect("the chunk runs");
    assert_eq!(count, 511);
}

#[test]
fn insert_and_delete_churn() {
    let (live, sum): (i64, i64) = run(
        r#"
        local t = {}
        for i = 1, 10000 do
            t["c" .. i] = i
            if i % 3 == 0 then t["c" .. (i - 1)] = nil end
        end
        local n, s = 0, 0
        for _, v in pairs(t) do n = n + 1 s = s + v end
        return n, s
        "#,
    )
    .expect("the chunk runs");
    assert_eq!(live, 6667);
    assert!(sum > 0);
}

#[test]
fn every_key_is_removable_and_the_table_is_reusable() {
    let count: i64 = run(
        r#"
        local t = {}
        for i = 1, 200 do t["a" .. i] = i end
        for i = 1, 200 do t["a" .. i] = nil end
        for i = 1, 400 do t["b" .. i] = i end
        local n = 0
        for _ in pairs(t) do n = n + 1 end
        return n
        "#,
    )
    .expect("the chunk runs");
    assert_eq!(count, 400);
}

/// The same thing straight through the Rust API, without the compiler in the
/// way, so a failure points at the table rather than at codegen.
#[test]
fn delete_then_grow_through_the_rust_api() {
    let mut lua = Lua::core();
    lua.enter(|ctx| {
        let t = Table::new(&ctx);
        for i in 0..64 {
            let key = ctx.intern(format!("k{i}").as_bytes());
            t.set(ctx, key, i as i64).expect("a string key is valid");
        }
        let first = ctx.intern(b"k0");
        t.set(ctx, first, Value::Nil).expect("removal works");
        for i in 64..512 {
            let key = ctx.intern(format!("k{i}").as_bytes());
            t.set(ctx, key, i as i64).expect("a string key is valid");
        }
        assert_eq!(t.iter().count(), 511);
    });
}
