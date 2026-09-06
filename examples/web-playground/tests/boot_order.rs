//! The order the two `Startup` schedules that both touch `ScreenHandlers` run in.
//!
//! Scene 1 is the chest on its own and scene 2 is the same chest with the item
//! browser docked beside it, and the only thing that separates them is whether
//! `demo:chest` has a [`ScreenHandler`]. `ChestScene::enter` takes the handler
//! away; `showcase::chest::register_browser_handler` puts it there. Both run in
//! `Startup`, so without an explicit edge between them Bevy may run them in
//! either order, and in one of the two the browser plugin re-registers the
//! handler a moment after the scene removed it. The tab then boots on scene 1
//! showing scene 2.
//!
//! Every other test in this crate switches scenes after startup, where the two
//! are frames apart and the race cannot happen, so nothing caught it until a
//! screenshot did. These two tests are the boot path: the same assertion with
//! the mimic registered before `ShowcasePlugin` and after it. With the edge in
//! place both pass; without it, whichever order Bevy takes from the insertion
//! order fails one of them.

use std::sync::Arc;

use bevy::prelude::*;
use slotted::browser::{BrowserPhase, DefaultScreenHandler, ScreenHandlers};
use slotted::prelude::*;
use web_playground::bus::Bus;
use web_playground::showcase::{Scene, ShowcasePlugin};

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

/// The handler the boot scene removed is still gone once `Startup` is over.
fn assert_the_boot_scene_won(app: &App, how: &str) {
    assert_eq!(
        web_playground::showcase::Scene::DEFAULT,
        Scene::Chest,
        "these tests are about the Chest scene being the one a tab lands on"
    );
    assert!(
        !app.world()
            .resource::<ScreenHandlers>()
            .contains(&ScreenKind::new(showcase::chest::CHEST)),
        "with the browser plugin registered {how}, `Startup` left the \
         `demo:chest` handler in place and the Chest scene boots with the \
         browser panel docked beside it"
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
