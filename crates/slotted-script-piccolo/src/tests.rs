//! Adapter tests. The conformance suite is the contract; these pin the
//! things it cannot see: which error class a failure lands in, that the state
//! is still usable afterwards, and that the sandbox holds.

#![allow(clippy::unwrap_used)]

use pretty_assertions::assert_eq;
use slotted_script::{
    FORBIDDEN_GLOBALS, Limits, LogLevel, ModId, ScriptCommand, ScriptError, ScriptEvent,
    ScriptRuntime, Stage,
};

use super::*;

fn mod_id() -> ModId {
    ModId::new("test").unwrap()
}

fn runtime() -> PiccoloRuntime {
    PiccoloRuntime::new(Limits::default())
}

/// Loads `source` as a control chunk and returns the runtime and its id.
fn control(source: &str) -> (PiccoloRuntime, slotted_script::ScriptId) {
    let mut rt = runtime();
    let id = rt
        .load(&mod_id(), "control.lua", source, Stage::Control)
        .unwrap();
    (rt, id)
}

fn search(text: &str) -> ScriptEvent {
    ScriptEvent::SearchChanged {
        text: text.to_owned(),
    }
}

fn log(level: LogLevel, message: &str) -> ScriptCommand {
    ScriptCommand::Log {
        level,
        message: message.to_owned(),
    }
}

#[test]
fn the_prelude_installs_and_the_state_is_sealed() {
    let SandboxedState { mut lua, env } =
        PiccoloRuntime::sandboxed_state(&mod_id(), Stage::Data, &Limits::default())
            .expect("a sandboxed state");
    lua.enter(|ctx| {
        let env = ctx.fetch(&env);
        for name in FORBIDDEN_GLOBALS {
            let key = ctx.intern(name.as_bytes());
            assert!(
                env.get(ctx, key).is_nil(),
                "{name} is still reachable in a sandboxed state"
            );
        }
        for name in [
            "string", "table", "math", "slotted", "_G", "tonumber", "pcall", "print",
        ] {
            let key = ctx.intern(name.as_bytes());
            assert!(!env.get(ctx, key).is_nil(), "{name} should be available");
        }
    });
}

#[test]
fn every_polyfilled_function_exists() {
    let SandboxedState { mut lua, env } =
        PiccoloRuntime::sandboxed_state(&mod_id(), Stage::Data, &Limits::default())
            .expect("a sandboxed state");
    lua.enter(|ctx| {
        let env = ctx.fetch(&env);
        for path in stdlib::POLYFILLED {
            let mut parts = path.split('.');
            let head = parts.next().unwrap_or_default();
            let mut value = env.get(ctx, ctx.intern(head.as_bytes()));
            for part in parts {
                let piccolo::Value::Table(t) = value else {
                    panic!("{path}: {head} is not a table");
                };
                let key = ctx.intern(part.as_bytes());
                // A frozen library is an empty wrapper, so the lookup has to
                // go through `__index` the way the VM does.
                value = match t.get(ctx, key) {
                    piccolo::Value::Nil => t
                        .metatable()
                        .map(|meta| meta.get(ctx, ctx.intern(b"__index")))
                        .and_then(|inner| match inner {
                            piccolo::Value::Table(inner) => Some(inner.get(ctx, key)),
                            _ => None,
                        })
                        .unwrap_or(piccolo::Value::Nil),
                    found => found,
                };
            }
            assert!(!value.is_nil(), "{path} is missing");
        }
    });
}

#[test]
fn compile_errors_are_compile_errors() {
    let mut rt = runtime();
    let err = rt
        .load(&mod_id(), "case.lua", "this is not lua", Stage::Data)
        .unwrap_err();
    assert!(matches!(err, ScriptError::Compile { .. }), "{err:?}");
}

#[test]
fn a_runtime_error_carries_a_message_and_a_traceback() {
    let (mut rt, id) = control(r#"slotted.on("search_changed", function() error("boom") end)"#);
    let err = rt.call(id, &search("x")).unwrap_err();
    let ScriptError::Runtime {
        message, traceback, ..
    } = &err
    else {
        panic!("expected a runtime error, got {err:?}");
    };
    assert!(message.contains("boom"), "{message}");
    assert!(!traceback.is_empty(), "the traceback was empty");
}

#[test]
fn the_state_survives_an_explicit_error() {
    let (mut rt, id) = control(
        r#"
        slotted.on("search_changed", function(ev)
            if ev.text == "boom" then error("no") end
            return slotted.cmd.log("info", ev.text)
        end)
        "#,
    );
    assert!(rt.call(id, &search("boom")).is_err());
    assert_eq!(
        rt.call(id, &search("ok")).unwrap(),
        vec![log(LogLevel::Info, "ok")]
    );
}

#[test]
fn the_state_survives_a_runtime_type_error() {
    let (mut rt, id) = control(
        r#"
        slotted.on("search_changed", function(ev)
            if ev.text == "boom" then
                local t = nil
                return t.missing
            end
            return slotted.cmd.log("info", ev.text)
        end)
        "#,
    );
    let err = rt.call(id, &search("boom")).unwrap_err();
    assert!(matches!(err, ScriptError::Runtime { .. }), "{err:?}");
    assert_eq!(
        rt.call(id, &search("ok")).unwrap(),
        vec![log(LogLevel::Info, "ok")]
    );
}

#[test]
fn a_pcall_caught_error_does_not_reach_the_host() {
    // This is the case luaur cannot do on wasm at all: an in-VM `pcall`
    // around a raise, with the module still alive afterwards.
    let (mut rt, id) = control(
        r#"
        slotted.on("search_changed", function()
            local ok, err = pcall(function() error("inner") end)
            return slotted.cmd.log("info", tostring(ok) .. ":" .. tostring(err))
        end)
        "#,
    );
    let out = rt.call(id, &search("x")).unwrap();
    assert_eq!(out, vec![log(LogLevel::Info, "false:inner")]);
    // And again, to show nothing was consumed by the raise.
    assert_eq!(rt.call(id, &search("x")).unwrap().len(), 1);
}

#[test]
fn the_state_survives_budget_exhaustion() {
    let mut rt = PiccoloRuntime::new(Limits {
        budget: 50_000,
        ..Limits::default()
    });
    let id = rt
        .load(
            &mod_id(),
            "control.lua",
            r#"
            slotted.on("search_changed", function(ev)
                if ev.text == "spin" then while true do end end
                return slotted.cmd.log("info", ev.text)
            end)
            "#,
            Stage::Control,
        )
        .unwrap();
    let err = rt.call(id, &search("spin")).unwrap_err();
    assert!(matches!(err, ScriptError::BudgetExceeded { .. }), "{err:?}");
    // The budget is re-armed per call, and the abandoned frames are gone.
    assert_eq!(
        rt.call(id, &search("ok")).unwrap(),
        vec![log(LogLevel::Info, "ok")]
    );
}

#[test]
fn a_memory_bomb_hits_the_limit() {
    let mut rt = PiccoloRuntime::new(Limits {
        budget: u64::MAX / FUEL_PER_TICK,
        memory_bytes: 8 * 1024 * 1024,
    });
    let err = rt
        .load(
            &mod_id(),
            "data.lua",
            "local t = {} for i = 1, 100000 do t[i] = string.rep(tostring(i) .. 'y', 100000) end",
            Stage::Data,
        )
        .unwrap_err();
    assert!(matches!(err, ScriptError::Memory { .. }), "{err:?}");
}

#[test]
fn set_limits_reaches_a_loaded_state() {
    let (mut rt, id) = control(r#"slotted.on("search_changed", function() while true do end end)"#);
    rt.set_limits(Limits {
        budget: 1_000,
        memory_bytes: 64 * 1024 * 1024,
    });
    let err = rt.call(id, &search("x")).unwrap_err();
    assert!(matches!(err, ScriptError::BudgetExceeded { .. }), "{err:?}");
}

#[test]
fn a_write_to_a_frozen_table_is_a_sandbox_error() {
    let mut rt = runtime();
    let err = rt
        .load(
            &mod_id(),
            "data.lua",
            "string.format = function() return 'hijacked' end",
            Stage::Data,
        )
        .unwrap_err();
    assert!(matches!(err, ScriptError::Sandbox { .. }), "{err:?}");

    let mut rt = runtime();
    let err = rt
        .load(
            &mod_id(),
            "data.lua",
            "slotted.register_item = function() end",
            Stage::Data,
        )
        .unwrap_err();
    assert!(matches!(err, ScriptError::Sandbox { .. }), "{err:?}");
}

#[test]
fn the_frozen_wrapper_cannot_be_unwrapped() {
    let mut rt = runtime();
    let outcome = rt.load(
        &mod_id(),
        "data.lua",
        "local meta = getmetatable(string) if type(meta) == 'table' then error('exposed') end\n\
         local ok = pcall(setmetatable, string, {}) if ok then error('exposed') end",
        Stage::Data,
    );
    // `__metatable` makes `getmetatable` return `false`, so the chunk must not
    // find a table to write through; reaching `error('exposed')` would mean it
    // did.
    assert!(
        !matches!(&outcome, Err(ScriptError::Runtime { message, .. }) if message.contains("exposed")),
        "{outcome:?}"
    );
}

#[test]
fn two_states_do_not_share_globals() {
    let mut rt = runtime();
    let a = rt
        .load(&mod_id(), "a.lua", "shared_marker = 1", Stage::Data)
        .unwrap();
    let b = rt
        .load(
            &ModId::new("other").unwrap(),
            "b.lua",
            r#"
            if shared_marker ~= nil then error("leaked") end
            slotted.info("mod is %s", slotted.mod_id)
            "#,
            Stage::Data,
        )
        .unwrap();
    assert!(
        rt.call(a, &ScriptEvent::DataStage { api_version: 1 })
            .is_ok()
    );
    assert_eq!(
        rt.call(b, &ScriptEvent::DataStage { api_version: 1 })
            .unwrap(),
        vec![log(LogLevel::Info, "mod is other")]
    );
}

#[test]
fn unicode_survives_the_bridge() {
    let mut rt = runtime();
    let id = rt
        .load(
            &mod_id(),
            "data.lua",
            r#"
            local label = "Kupferkiste 家 🧰"
            slotted.register_item("kupfer", { label = label })
            slotted.info("bytes %d", #label)
            "#,
            Stage::Data,
        )
        .unwrap();
    let out = rt
        .call(id, &ScriptEvent::DataStage { api_version: 1 })
        .unwrap();
    let ScriptCommand::RegisterItem { def, .. } = &out[0] else {
        panic!("expected a register_item, got {:?}", out[0]);
    };
    assert_eq!(
        def.get("label"),
        Some(&slotted_model::Value::Str("Kupferkiste 家 🧰".to_owned()))
    );
    assert_eq!(out[1], log(LogLevel::Info, "bytes 20"));
}

#[test]
fn unknown_ids_and_unload() {
    let mut rt = runtime();
    let id = rt.load(&mod_id(), "a.lua", "", Stage::Data).unwrap();
    assert_eq!(rt.len(), 1);
    rt.unload(id);
    assert!(rt.is_empty());
    let err = rt
        .call(id, &ScriptEvent::DataStage { api_version: 1 })
        .unwrap_err();
    assert_eq!(err, ScriptError::UnknownScript(id));
    rt.unload(id);
}

#[test]
fn a_malformed_command_names_its_position_and_type() {
    let (mut rt, id) = control(
        r#"
        slotted.on("search_changed", function()
            return { { type = "sort", menu = "not a number", inventory = 0 } }
        end)
        "#,
    );
    let err = rt.call(id, &search("x")).unwrap_err();
    let ScriptError::Protocol { message, .. } = &err else {
        panic!("expected a protocol error, got {err:?}");
    };
    assert!(message.contains("command #1"), "{message}");
    assert!(message.contains("sort"), "{message}");
    assert!(message.contains("menu"), "{message}");
}

#[test]
fn a_wrong_argument_names_the_function_that_rejected_it() {
    let mut rt = runtime();
    let err = rt
        .load(
            &mod_id(),
            "data.lua",
            r#"slotted.register_item("apple", "not a table")"#,
            Stage::Data,
        )
        .unwrap_err();
    let ScriptError::Runtime { message, .. } = &err else {
        panic!("expected a runtime error, got {err:?}");
    };
    assert!(message.contains("def must be a table"), "{message}");
}

#[test]
fn a_slot_click_round_trip_is_measured() {
    let (mut rt, id) = control(
        r#"
        slotted.on("slot_click", function(ev)
            if ev.modifiers.alt then
                return slotted.cmd.toggle_favorite(ev.menu, ev.slot)
            end
        end)
        "#,
    );
    let event = ScriptEvent::SlotClick {
        menu: slotted_model::MenuId(1),
        screen: "test:chest".to_owned(),
        slot: 3,
        button: slotted_script::Button::Left,
        modifiers: slotted_script::Modifiers {
            alt: true,
            ..slotted_script::Modifiers::default()
        },
        stack: Some(slotted_script::StackInfo {
            item: "demo:apple".to_owned(),
            count: 3,
            components: slotted_model::Value::Map(std::collections::BTreeMap::new()),
        }),
    };
    for _ in 0..200 {
        rt.call(id, &event).unwrap();
    }
    let runs = 2_000;
    let start = std::time::Instant::now();
    for _ in 0..runs {
        rt.call(id, &event).unwrap();
    }
    let per_call = start.elapsed() / runs;
    // ADR 0001 left piccolo's call cost unmeasured; this is the number, and
    // the bound is generous enough for a debug build on a loaded machine.
    println!("piccolo slot_click round trip: {per_call:?} per call");
    assert!(
        per_call < std::time::Duration::from_millis(2),
        "a slot_click round trip took {per_call:?}"
    );
}

/// A mod that removes a table key and then grows the same table must come
/// back as `Ok`, not as an abort.
///
/// piccolo 0.3.3 tombstones a removed key and then panics rehashing it when
/// the map grows ("all keys must be live when table is grown",
/// `table/raw.rs`). On `wasm32-unknown-unknown` a panic is an abort of the
/// whole module, so one mod writing `t[k] = nil` would take down the page,
/// which is the failure ADR 0001 rejected luaur for. `vendor/piccolo` carries
/// upstream's own fix; this test is what says the patch is wired in.
#[test]
fn deleting_a_table_key_then_growing_it_survives() {
    let mut rt = runtime();
    let id = rt
        .load(
            &mod_id(),
            "data.lua",
            r#"
            local t = {}
            for i = 1, 64 do t["k" .. i] = i end
            t.k1 = nil
            for i = 65, 512 do t["k" .. i] = i end
            local n = 0
            for _ in pairs(t) do n = n + 1 end
            slotted.info("live %d", n)
            "#,
            Stage::Data,
        )
        .expect("the chunk loads");
    assert_eq!(
        rt.call(id, &ScriptEvent::DataStage { api_version: 1 })
            .unwrap(),
        vec![log(LogLevel::Info, "live 511")]
    );
}

/// The same hazard through the environment table: a mod removing one of its
/// own globals and then declaring more of them.
#[test]
fn deleting_a_global_then_adding_more_survives() {
    let mut rt = runtime();
    let id = rt
        .load(
            &mod_id(),
            "data.lua",
            r#"
            marker = 1
            helper = function() return 2 end
            marker = nil
            for i = 1, 400 do _G["g" .. i] = i end
            if marker ~= nil then error("the global came back") end
            slotted.info("globals ok %d", helper())
            "#,
            Stage::Data,
        )
        .expect("the chunk loads");
    assert_eq!(
        rt.call(id, &ScriptEvent::DataStage { api_version: 1 })
            .unwrap(),
        vec![log(LogLevel::Info, "globals ok 2")]
    );
}

/// Sustained churn, the shape a real mod's cache has: ten thousand inserts
/// with a third of them removed again as it goes.
#[test]
fn ten_thousand_key_insert_delete_churn_survives() {
    let mut rt = PiccoloRuntime::new(Limits {
        budget: 20_000_000,
        ..Limits::default()
    });
    let id = rt
        .load(
            &mod_id(),
            "data.lua",
            r#"
            local t = {}
            for i = 1, 10000 do
                t["c" .. i] = i
                if i % 3 == 0 then t["c" .. (i - 1)] = nil end
            end
            local n = 0
            for _ in pairs(t) do n = n + 1 end
            slotted.info("churn %d", n)
            "#,
            Stage::Data,
        )
        .expect("the chunk loads");
    assert_eq!(
        rt.call(id, &ScriptEvent::DataStage { api_version: 1 })
            .unwrap(),
        vec![log(LogLevel::Info, "churn 6667")]
    );
}

/// Removing every key and refilling, which exercises the tombstone path from
/// the other side: the map is all dead before it grows again.
#[test]
fn emptying_and_refilling_a_table_survives() {
    let mut rt = runtime();
    let id = rt
        .load(
            &mod_id(),
            "data.lua",
            r#"
            local t = {}
            for i = 1, 200 do t["a" .. i] = i end
            for i = 1, 200 do t["a" .. i] = nil end
            for i = 1, 400 do t["b" .. i] = i end
            local n = 0
            for _ in pairs(t) do n = n + 1 end
            slotted.info("refill %d", n)
            "#,
            Stage::Data,
        )
        .expect("the chunk loads");
    assert_eq!(
        rt.call(id, &ScriptEvent::DataStage { api_version: 1 })
            .unwrap(),
        vec![log(LogLevel::Info, "refill 400")]
    );
}
