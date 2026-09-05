//! Phase 0 spike: compare `luaur` (pure-Rust Luau) and `mlua` (C Luau) as the
//! backing runtime for the `slotted-script` port, natively and on wasm32.
//!
//! The three operations every adapter must support are exercised identically in
//! both backends: load a chunk, expose a Rust function to Lua, call a Lua
//! function from Rust, and round-trip a nested table through serde.

use serde::{Deserialize, Serialize};

/// Stand-in for a `ScriptCommand`-shaped payload: nested, mixed types, a
/// sequence of structs, and an optional field.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScreenSpec {
    pub id: String,
    pub title: String,
    pub size: [u32; 2],
    pub slots: Vec<SlotSpec>,
    pub meta: Meta,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SlotSpec {
    pub index: u32,
    pub role: String,
    pub locked: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Meta {
    pub api_version: u32,
    pub tags: Vec<String>,
}

impl ScreenSpec {
    pub fn sample() -> Self {
        ScreenSpec {
            id: "copper_chest".into(),
            title: "Copper Chest".into(),
            size: [9, 3],
            slots: (0..3)
                .map(|i| SlotSpec {
                    index: i,
                    role: format!("storage_{i}"),
                    locked: i == 2,
                })
                .collect(),
            meta: Meta {
                api_version: 1,
                tags: vec!["storage".into(), "copper".into()],
            },
        }
    }
}

/// The Lua source used by every backend, so the comparison is apples to apples.
pub const SCRIPT: &str = r#"
-- 1. a chunk that runs at load time and defines globals
loaded_marker = "chunk-ran"

-- 2. call a Rust function registered as a global
function use_rust(a, b)
    return rust_add(a, b)
end

-- 3. a Lua function Rust calls, taking and returning a nested table
function transform(spec)
    spec.title = spec.title .. " (modded)"
    spec.meta.api_version = spec.meta.api_version + 1
    for _, slot in ipairs(spec.slots) do
        slot.role = "lua_" .. slot.role
    end
    return spec
end

-- 4. the hot path for the call-cost measurement: small table in, number out
function hot(t)
    return t.a + t.b
end
"#;

pub mod luaur_backend;

#[cfg(feature = "bevy-native")]
pub mod bevy_app;

#[cfg(feature = "mlua-native")]
pub mod mlua_backend;

#[cfg(feature = "piccolo")]
pub mod piccolo_backend;
