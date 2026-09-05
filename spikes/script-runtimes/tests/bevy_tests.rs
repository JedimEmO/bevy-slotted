//! luaur inside a Bevy 0.19 `MinimalPlugins` app.
//!
//! Run: cargo test --features bevy-native --test bevy_tests
#![cfg(feature = "bevy-native")]

use script_runtimes_spike::bevy_app;

#[test]
fn runs_lua_from_a_bevy_system() {
    let mut app = bevy_app::build_app();
    app.update();

    let out = app
        .world()
        .resource::<bevy_app::ScriptOutput>();
    assert_eq!(out.error, None, "script errored inside the Bevy system");
    assert_eq!(out.marker.as_deref(), Some("chunk-ran"));
    assert_eq!(out.sum, Some(42));
    let spec = out.spec.as_ref().expect("spec");
    assert_eq!(spec.title, "Copper Chest (modded)");
    assert_eq!(spec.meta.api_version, 2);
    assert_eq!(spec.slots[0].role, "lua_storage_0");
}

#[test]
fn survives_many_frames() {
    let mut app = bevy_app::build_app();
    for _ in 0..100 {
        app.update();
    }
    let out = app.world().resource::<bevy_app::ScriptOutput>();
    assert_eq!(out.error, None);
    assert_eq!(out.sum, Some(42));
}
