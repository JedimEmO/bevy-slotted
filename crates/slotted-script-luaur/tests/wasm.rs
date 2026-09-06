//! The browser half of the gate: the core conformance behaviours run in a
//! real wasm module, in a real tab.
//!
//! The suite proper reads `conformance/*.lua` off the filesystem, which
//! `wasm32-unknown-unknown` does not have, so the cases that matter are
//! inlined here.
//!
//! **What is deliberately absent**: every case whose assertion depends on a
//! Lua error being *raised* and caught. luaur raises by panicking and
//! `wasm32-unknown-unknown` cannot unwind, so on the web `error("boom")`, a
//! runtime type error, an exhausted budget and a refused allocation each abort
//! the whole module. A test for them would not fail here, it would kill the
//! runner. ADR 0004 accepts that trade and puts the recovery on the host: see
//! `examples/web-playground`, which restarts the module and restores its
//! state. The cfg is the finding, not a bug to fix.
//!
//! Run with:
//!
//! ```sh
//! PATH="$PWD/spikes/script-runtimes/chromedriver-shim:$PATH" \
//!   wasm-pack test --headless --chrome crates/slotted-script-luaur --test wasm
//! ```

#![cfg(target_arch = "wasm32")]

use slotted_script::{
    Limits, LogLevel, ModId, ScriptCommand, ScriptError, ScriptEvent, ScriptRuntime, Stage,
};
use slotted_script_luaur::LuaurRuntime;
use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};

wasm_bindgen_test_configure!(run_in_browser);

fn mod_id() -> ModId {
    ModId::new("test").expect("a valid mod id")
}

fn search(text: &str) -> ScriptEvent {
    ScriptEvent::SearchChanged {
        text: text.to_owned(),
    }
}

fn log(message: &str) -> ScriptCommand {
    ScriptCommand::Log {
        level: LogLevel::Info,
        message: message.to_owned(),
    }
}

/// Loads a control chunk, panicking with the adapter's own message on failure.
fn control(rt: &mut LuaurRuntime, source: &str) -> slotted_script::ScriptId {
    match rt.load(&mod_id(), "control.lua", source, Stage::Control) {
        Ok(id) => id,
        Err(err) => panic!("the chunk did not load: {err}"),
    }
}

#[wasm_bindgen_test]
fn a_data_chunk_registers_in_the_browser() {
    let mut rt = LuaurRuntime::new(Limits::default());
    let id = rt
        .load(
            &mod_id(),
            "data.lua",
            r#"slotted.register_item("apple", { max_stack_size = 16 })"#,
            Stage::Data,
        )
        .expect("the data chunk loads");
    let out = rt
        .call(id, &ScriptEvent::DataStage { api_version: 1 })
        .expect("the data stage dispatches");
    assert_eq!(out.len(), 1);
    let ScriptCommand::RegisterItem { id: item, def } = &out[0] else {
        panic!("expected a register_item, got {:?}", out[0]);
    };
    assert_eq!(item, "test:apple");
    assert_eq!(
        def.get("max_stack_size"),
        Some(&slotted_model::Value::Int(16))
    );
}

/// A control script subscribes, answers events and keeps its own state across
/// calls. This is the per-click path the page exercises on every slot.
#[wasm_bindgen_test]
fn a_control_script_answers_events_in_the_browser() {
    let mut rt = LuaurRuntime::new(Limits::default());
    let id = control(
        &mut rt,
        r#"
        local seen = 0
        slotted.on("search_changed", function(ev)
            seen = seen + 1
            return slotted.cmd.log("info", ev.text .. " " .. tostring(seen))
        end)
        "#,
    );
    assert_eq!(rt.call(id, &search("a")).unwrap(), vec![log("a 1")]);
    assert_eq!(rt.call(id, &search("b")).unwrap(), vec![log("b 2")]);
}

/// A syntax error is the one error class that is recoverable on wasm: the
/// compiler never enters the VM, so nothing is raised and nothing aborts.
#[wasm_bindgen_test]
fn a_compile_error_is_still_recoverable_in_the_browser() {
    let mut rt = LuaurRuntime::new(Limits::default());
    let err = rt
        .load(&mod_id(), "data.lua", "this is not lua ((", Stage::Data)
        .unwrap_err();
    assert!(matches!(err, ScriptError::Compile { .. }), "{err:?}");
    // The runtime is still usable afterwards, which is what lets the page
    // report a typo and carry on without a restart.
    let id = rt
        .load(
            &mod_id(),
            "data.lua",
            r#"slotted.info("fine")"#,
            Stage::Data,
        )
        .expect("a good chunk still loads");
    assert_eq!(
        rt.call(id, &ScriptEvent::DataStage { api_version: 1 })
            .unwrap(),
        vec![log("fine")]
    );
}

#[wasm_bindgen_test]
fn the_sandbox_holds_in_the_browser() {
    let mut rt = LuaurRuntime::new(Limits::default());
    let id = rt
        .load(
            &mod_id(),
            "data.lua",
            r#"
            local forbidden = {
                "io", "os", "package", "require", "dofile", "loadfile", "load",
                "loadstring", "debug", "collectgarbage", "getfenv", "setfenv", "newproxy",
            }
            for _, name in ipairs(forbidden) do
                if _G[name] ~= nil then error("global " .. name .. " is still reachable") end
            end
            slotted.info("all absent")
            "#,
            Stage::Data,
        )
        .expect("the sandbox chunk loads");
    assert_eq!(
        rt.call(id, &ScriptEvent::DataStage { api_version: 1 })
            .unwrap(),
        vec![log("all absent")]
    );
}

#[wasm_bindgen_test]
fn unicode_crosses_the_bridge_in_the_browser() {
    let mut rt = LuaurRuntime::new(Limits::default());
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
        .expect("the unicode chunk loads");
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
    assert_eq!(out[1], log("bytes 20"));
}

#[wasm_bindgen_test]
fn the_value_bridge_keeps_arrays_maps_and_number_kinds() {
    let mut rt = LuaurRuntime::new(Limits::default());
    let id = rt
        .load(
            &mod_id(),
            "data.lua",
            r#"
            slotted.register_item("t", {
                array = { 1, 2, 3 },
                map = { a = 1 },
                empty = {},
                integral_float = 3.0,
                float = 0.5,
            })
            "#,
            Stage::Data,
        )
        .expect("the bridge chunk loads");
    let out = rt
        .call(id, &ScriptEvent::DataStage { api_version: 1 })
        .unwrap();
    let ScriptCommand::RegisterItem { def, .. } = &out[0] else {
        panic!("expected a register_item, got {:?}", out[0]);
    };
    use slotted_model::Value;
    assert_eq!(
        def.get("array"),
        Some(&Value::List(vec![
            Value::Int(1),
            Value::Int(2),
            Value::Int(3)
        ]))
    );
    assert_eq!(def.get("empty"), Some(&Value::List(Vec::new())));
    assert_eq!(def.get("integral_float"), Some(&Value::Int(3)));
    assert_eq!(def.get("float"), Some(&Value::Float(0.5)));
}

/// Deleting table keys and then growing the same table. This is the case that
/// aborted the module on piccolo 0.3.3 and needed a vendored fix; luaur is
/// Luau's own table implementation and does not have the bug. Cheap insurance
/// that the swap did not reintroduce it.
#[wasm_bindgen_test]
fn deleting_table_keys_does_not_abort_the_module() {
    let mut rt = LuaurRuntime::new(Limits {
        budget: 20_000_000,
        ..Limits::default()
    });
    let id = rt
        .load(
            &mod_id(),
            "data.lua",
            r#"
            local t = {}
            for i = 1, 64 do t["k" .. i] = i end
            t.k1 = nil
            for i = 65, 512 do t["k" .. i] = i end
            local grown = 0
            for _ in pairs(t) do grown = grown + 1 end

            local c = {}
            for i = 1, 10000 do
                c["c" .. i] = i
                if i % 3 == 0 then c["c" .. (i - 1)] = nil end
            end
            local churn = 0
            for _ in pairs(c) do churn = churn + 1 end

            slotted.info("%d %d", grown, churn)
            "#,
            Stage::Data,
        )
        .expect("the churn chunk loads");
    assert_eq!(
        rt.call(id, &ScriptEvent::DataStage { api_version: 1 })
            .unwrap(),
        vec![log("511 6667")]
    );
}
