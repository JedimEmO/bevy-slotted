//! The browser half of the gate: the core conformance behaviours run in a
//! real wasm module, in a real tab.
//!
//! The suite proper reads `conformance/*.lua` off the filesystem, which
//! `wasm32-unknown-unknown` does not have, so the cases that matter for the
//! decision in ADR 0001 are inlined here. The one that matters most is
//! [`errors_do_not_abort_the_module`]: this is the exact test luaur fails, and
//! failing it is why piccolo is the web runtime.
//!
//! Run with:
//!
//! ```sh
//! PATH="$PWD/spikes/script-runtimes/chromedriver-shim:$PATH" \
//!   wasm-pack test --headless --chrome crates/slotted-script-piccolo
//! ```
//!
//! `.cargo/config.toml` already passes `--cfg getrandom_backend="wasm_js"`
//! for the target, so no `RUSTFLAGS` is needed.

#![cfg(target_arch = "wasm32")]

use slotted_script::{
    Limits, LogLevel, ModId, ScriptCommand, ScriptError, ScriptEvent, ScriptRuntime, Stage,
};
use slotted_script_piccolo::PiccoloRuntime;
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
fn control(rt: &mut PiccoloRuntime, source: &str) -> slotted_script::ScriptId {
    match rt.load(&mod_id(), "control.lua", source, Stage::Control) {
        Ok(id) => id,
        Err(err) => panic!("the chunk did not load: {err}"),
    }
}

#[wasm_bindgen_test]
fn a_data_chunk_registers_in_the_browser() {
    let mut rt = PiccoloRuntime::new(Limits::default());
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

/// The reason this adapter exists. On luaur every one of these three raises
/// aborts the wasm module; here each one comes back as a `Result` and the
/// state answers the next event.
#[wasm_bindgen_test]
fn errors_do_not_abort_the_module() {
    let mut rt = PiccoloRuntime::new(Limits::default());
    let id = control(
        &mut rt,
        r#"
        slotted.on("search_changed", function(ev)
            if ev.text == "raise" then error("boom") end
            if ev.text == "typeerror" then local t = nil return t.missing end
            if ev.text == "caught" then
                local ok, why = pcall(function() error("inner") end)
                return slotted.cmd.log("info", tostring(ok) .. ":" .. tostring(why))
            end
            return slotted.cmd.log("info", ev.text)
        end)
        "#,
    );

    let raised = rt.call(id, &search("raise")).unwrap_err();
    assert!(matches!(raised, ScriptError::Runtime { .. }), "{raised:?}");
    assert_eq!(rt.call(id, &search("ok")).unwrap(), vec![log("ok")]);

    let typed = rt.call(id, &search("typeerror")).unwrap_err();
    assert!(matches!(typed, ScriptError::Runtime { .. }), "{typed:?}");
    assert_eq!(rt.call(id, &search("ok")).unwrap(), vec![log("ok")]);

    assert_eq!(
        rt.call(id, &search("caught")).unwrap(),
        vec![log("false:inner")]
    );
    assert_eq!(rt.call(id, &search("ok")).unwrap(), vec![log("ok")]);
}

#[wasm_bindgen_test]
fn the_budget_stops_a_runaway_handler() {
    let mut rt = PiccoloRuntime::new(Limits {
        budget: 50_000,
        ..Limits::default()
    });
    let id = control(
        &mut rt,
        r#"
        slotted.on("search_changed", function(ev)
            if ev.text == "spin" then while true do end end
            return slotted.cmd.log("info", ev.text)
        end)
        "#,
    );
    let err = rt.call(id, &search("spin")).unwrap_err();
    assert!(matches!(err, ScriptError::BudgetExceeded { .. }), "{err:?}");
    assert_eq!(rt.call(id, &search("ok")).unwrap(), vec![log("ok")]);
}

#[wasm_bindgen_test]
fn the_sandbox_holds_in_the_browser() {
    let mut rt = PiccoloRuntime::new(Limits::default());
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

    let mut rt = PiccoloRuntime::new(Limits::default());
    let err = rt
        .load(
            &mod_id(),
            "data.lua",
            "string.format = function() return 'hijacked' end",
            Stage::Data,
        )
        .unwrap_err();
    assert!(matches!(err, ScriptError::Sandbox { .. }), "{err:?}");
}

#[wasm_bindgen_test]
fn unicode_crosses_the_bridge_in_the_browser() {
    let mut rt = PiccoloRuntime::new(Limits::default());
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
    let mut rt = PiccoloRuntime::new(Limits::default());
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

/// The tombstone case, in the browser, where a panic is an abort of the whole
/// module rather than a caught unwind. piccolo 0.3.3 aborts here; the fix
/// backported in `vendor/piccolo` is what makes these three return.
#[wasm_bindgen_test]
fn deleting_table_keys_does_not_abort_the_module() {
    let mut rt = PiccoloRuntime::new(Limits {
        budget: 20_000_000,
        ..Limits::default()
    });
    let id = rt
        .load(
            &mod_id(),
            "data.lua",
            r#"
            -- delete, then grow the same table
            local t = {}
            for i = 1, 64 do t["k" .. i] = i end
            t.k1 = nil
            for i = 65, 512 do t["k" .. i] = i end
            local grown = 0
            for _ in pairs(t) do grown = grown + 1 end

            -- ten thousand keys with a third removed as it goes
            local c = {}
            for i = 1, 10000 do
                c["c" .. i] = i
                if i % 3 == 0 then c["c" .. (i - 1)] = nil end
            end
            local churn = 0
            for _ in pairs(c) do churn = churn + 1 end

            -- empty a table completely, then refill it
            local r = {}
            for i = 1, 200 do r["a" .. i] = i end
            for i = 1, 200 do r["a" .. i] = nil end
            for i = 1, 400 do r["b" .. i] = i end
            local refilled = 0
            for _ in pairs(r) do refilled = refilled + 1 end

            slotted.info("%d %d %d", grown, churn, refilled)
            "#,
            Stage::Data,
        )
        .expect("the churn chunk loads");
    assert_eq!(
        rt.call(id, &ScriptEvent::DataStage { api_version: 1 })
            .unwrap(),
        vec![log("511 6667 400")]
    );
}

/// The same thing on the environment table, which is where the sandbox itself
/// would have tripped it: a mod removing a global and then declaring more.
#[wasm_bindgen_test]
fn deleting_a_global_does_not_abort_the_module() {
    let mut rt = PiccoloRuntime::new(Limits::default());
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
        .expect("the globals chunk loads");
    assert_eq!(
        rt.call(id, &ScriptEvent::DataStage { api_version: 1 })
            .unwrap(),
        vec![log("globals ok 2")]
    );
}
