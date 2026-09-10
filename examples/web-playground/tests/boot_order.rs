//! The order the two `Startup` schedules that both touch `ScreenHandlers` run in.
//!
//! Whether `demo:chest` has a [`ScreenHandler`] is whether the item browser
//! docks beside it, and two `Startup` systems decide it: the boot scene's
//! `enter` sets it to what that scene wants
//! (`SceneHandler::docks_the_item_browser`), and
//! `showcase::chest::register_browser_handler` registers one. Without an
//! explicit edge between them Bevy may run them in either order, and when
//! the boot scene was the chest without the browser, the plugin re-registered
//! the handler a moment after the scene removed it and the tab booted showing
//! the wrong scene.
//!
//! Since the showcase refresh (`docs/design/showcase-refresh-contract.md`
//! section 4.1) the boot scene docks the browser, so the two agree and the
//! race has nothing to break; the edge stays, because the next default scene
//! may not, and these two tests keep saying what the boot path does: the
//! same assertion with the mimic registered before `ShowcasePlugin` and after
//! it.

use std::sync::Arc;

use bevy::prelude::*;
use slotted::browser::{BrowserPhase, DefaultScreenHandler, ScreenHandlers};
use slotted::prelude::*;
use web_playground::bus::Bus;
use web_playground::showcase::{Scene, SceneHandler, ShowcasePlugin};

/// Stands in for `showcase::chest::register_browser_handler`, which is the
/// real system and lives in the same set. The real one also registers the
/// screen definition, which needs the pack registries this bare app has none
/// of; the handler is the half this ordering is about.
fn register_browser_handler(mut handlers: ResMut<ScreenHandlers>) {
    handlers.register(
        ScreenKind::new(showcase::chest::CHEST),
        Arc::new(DefaultScreenHandler),
    );
}

/// A world with both systems in `Startup`, run once.
///
/// `run_schedule(Startup)` rather than `update()`: this app has no
/// `MainSchedulePlugin`, and `Startup` is the whole question.
fn boot(mimic_first: bool) -> App {
    let mut app = App::new();
    app.insert_resource(Bus::new())
        .init_resource::<ScreenHandlers>()
        .configure_sets(Startup, BrowserPhase::ScreenHandlers);

    if mimic_first {
        app.add_systems(
            Startup,
            register_browser_handler.in_set(BrowserPhase::ScreenHandlers),
        )
        .add_plugins(ShowcasePlugin);
    } else {
        app.add_plugins(ShowcasePlugin).add_systems(
            Startup,
            register_browser_handler.in_set(BrowserPhase::ScreenHandlers),
        );
    }

    app.finish();
    app.cleanup();
    app.world_mut().run_schedule(Startup);
    app
}

/// The handler is in the state the boot scene asked for once `Startup` is
/// over: present, because the Chest scene docks the browser.
fn assert_the_boot_scene_won(app: &App, how: &str) {
    assert_eq!(
        web_playground::showcase::Scene::DEFAULT,
        Scene::Chest,
        "these tests are about the Chest scene being the one a tab lands on"
    );
    assert!(
        web_playground::scenes::chest::ChestScene.docks_the_item_browser(),
        "the boot scene docks the browser, which is what is asserted below"
    );
    assert!(
        app.world()
            .resource::<ScreenHandlers>()
            .contains(&ScreenKind::new(showcase::chest::CHEST)),
        "with the browser plugin registered {how}, `Startup` left the \
         `demo:chest` handler out and the Chest scene boots without the \
         browser panel it docks"
    );
}

#[test]
fn the_boot_scene_keeps_its_browser_denial_when_the_plugin_registers_first() {
    assert_the_boot_scene_won(&boot(true), "before ShowcasePlugin");
}

#[test]
fn the_boot_scene_keeps_its_browser_denial_when_the_plugin_registers_last() {
    assert_the_boot_scene_won(&boot(false), "after ShowcasePlugin");
}
