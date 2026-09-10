//! The order the two `Startup` schedules that both touch `ScreenHandlers` run in.
//!
//! Whether `demo:chest` has a [`ScreenHandler`] is whether the item browser
//! docks beside it, and two `Startup` systems decide it: the boot scene's
//! `enter` sets it to what that scene wants
//! (`SceneHandler::docks_the_item_browser`), and the browser plugin's own
//! `BrowserPhase::ScreenHandlers` set registers what it registers. Without
//! an explicit edge between them Bevy may run them in either order, and
//! when the boot scene was the chest without the browser, the plugin
//! re-registered the handler a moment after the scene removed it and the tab
//! booted showing the wrong scene.
//!
//! Since the showcase refresh (`docs/design/showcase-refresh-contract.md`
//! section 4.1) the boot scene docks the browser, so a mimic that registers
//! the handler agrees with it and proves nothing. The mimic here does the
//! opposite, removing the handler inside the set, so the assertion is again
//! that the scene's `enter` has the last word: the same assertion with the
//! mimic registered before `ShowcasePlugin` and after it.

use std::sync::Arc;

use bevy::prelude::*;
use slotted::browser::{BrowserPhase, DefaultScreenHandler, ScreenHandlers};
use slotted::prelude::*;
use web_playground::bus::Bus;
use web_playground::showcase::{Scene, SceneHandler, ShowcasePlugin};

/// Stands in for whatever the browser plugin's `ScreenHandlers` set does to
/// the `demo:chest` handler; today that is `register_browser_handler`,
/// which agrees with the boot scene. This mimic disagrees with it, so an
/// edge that is missing shows up as a boot without the panel.
fn unregister_browser_handler(mut handlers: ResMut<ScreenHandlers>) {
    handlers.remove(&ScreenKind::new(showcase::chest::CHEST));
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
    // What the real plugin leaves registered before its set runs, so the
    // mimic has something to remove.
    app.world_mut().resource_mut::<ScreenHandlers>().register(
        ScreenKind::new(showcase::chest::CHEST),
        Arc::new(DefaultScreenHandler),
    );

    if mimic_first {
        app.add_systems(
            Startup,
            unregister_browser_handler.in_set(BrowserPhase::ScreenHandlers),
        )
        .add_plugins(ShowcasePlugin);
    } else {
        app.add_plugins(ShowcasePlugin).add_systems(
            Startup,
            unregister_browser_handler.in_set(BrowserPhase::ScreenHandlers),
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
fn the_boot_scene_has_the_last_word_when_the_plugin_registers_first() {
    assert_the_boot_scene_won(&boot(true), "before ShowcasePlugin");
}

#[test]
fn the_boot_scene_has_the_last_word_when_the_plugin_registers_last() {
    assert_the_boot_scene_won(&boot(false), "after ShowcasePlugin");
}
