//! luaur living inside a Bevy 0.19 `MinimalPlugins` app.
//!
//! The question this answers is whether a Lua state can be held in a Bevy
//! `Resource` (which requires `Send + Sync + 'static`) and driven from a normal
//! system, which is exactly how `slotted-script` would hold a runtime.

use bevy::prelude::*;

use crate::luaur_backend as be;
use crate::ScreenSpec;

/// The script runtime as a Bevy resource. `luaur::Lua` is `Send` only with the
/// crate's `send` feature; it is never `Sync`, so the resource wraps it in a
/// `Mutex` the way a real adapter would.
#[derive(Resource)]
pub struct ScriptRuntime(pub std::sync::Mutex<luaur::Lua>);

/// What the script produced this frame, so a test can assert on it.
#[derive(Resource, Default, Debug)]
pub struct ScriptOutput {
    pub marker: Option<String>,
    pub sum: Option<i64>,
    pub spec: Option<ScreenSpec>,
    pub error: Option<String>,
}

pub fn insert_runtime(mut commands: Commands) {
    match be::prepare() {
        Ok(lua) => commands.insert_resource(ScriptRuntime(std::sync::Mutex::new(lua))),
        Err(e) => panic!("script runtime failed to start: {e}"),
    }
}

/// One system doing all three operations, standing in for an event dispatch.
pub fn run_script(runtime: Res<ScriptRuntime>, mut out: ResMut<ScriptOutput>) {
    let lua = runtime.0.lock().unwrap();
    let result = (|| -> luaur::Result<()> {
        out.marker = Some(be::chunk_ran(&lua)?);
        out.sum = Some(be::call_rust_from_lua(&lua, 20, 22)?);
        out.spec = Some(be::round_trip(&lua, &ScreenSpec::sample())?);
        Ok(())
    })();
    if let Err(e) = result {
        out.error = Some(e.to_string());
    }
}

/// A `MinimalPlugins` app with the runtime wired in.
pub fn build_app() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<ScriptOutput>()
        .add_systems(Startup, insert_runtime)
        .add_systems(Update, run_script);
    app
}
