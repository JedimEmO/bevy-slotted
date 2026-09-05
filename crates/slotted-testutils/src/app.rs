//! A headless [`App`] for exercising `slotted-ecs`.

use std::sync::Arc;

use bevy::prelude::*;
use slotted_ecs::{Authority, Registries, SlottedEcsPlugin};

use crate::builders::test_registries;

/// `MinimalPlugins` plus [`SlottedEcsPlugin`] and the test [`Registries`].
///
/// No authority is inserted, so the plugin's `Startup` system installs a
/// [`LocalAuthority`](slotted_ecs::LocalAuthority) on the first update unless
/// the test inserts its own first.
pub fn minimal_ecs_app() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_plugins(SlottedEcsPlugin)
        .insert_resource(Registries(test_registries()));
    app
}

/// The same app with `authority` already installed.
pub fn ecs_app_with(authority: Arc<dyn slotted_model::Authority>) -> App {
    let mut app = minimal_ecs_app();
    app.insert_resource(Authority(authority));
    app
}
