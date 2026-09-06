//! `*.screen.ron` through the asset server, and what happens when the file
//! changes under a screen that is open.
//!
//! The test uses a temporary asset root so it can rewrite the file, and drives
//! the reload with `AssetServer::reload` rather than `bevy/file_watcher`: the
//! watcher is a notify thread and a debounce window, which would make this a
//! sleep-and-hope test, and `reload` is the exact call the watcher makes when
//! it sees the write. Everything after that -- re-read, re-parse,
//! `AssetEvent::Modified`, `apply_screen_assets`, respawn -- is the real path.

#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};

use bevy::asset::AssetPlugin;
use bevy::prelude::*;
use pretty_assertions::assert_eq;
use slotted_test::prelude::*;
use slotted_ui::{ScreenAssets, ScreenRoot, Screens};

const GLASS: &str = include_str!("../../../assets/themes/glass.theme.ron");
const SCREEN: &str = "screens/demo_chest.screen.ron";

/// A one-panel chest screen whose only marked node is `label`.
fn screen_ron(label: &str) -> String {
    format!(
        r#"#![enable(implicit_some)]
(
    kind: "demo:chest",
    root: (
        type: "panel",
        role: "panel",
        layout: (direction: "column", gap: 2.0, padding: 2.0),
        tags: {{"test_id": "chest_panel"}},
        children: [
            (type: "text", key: "chest.title", style: "title", tags: {{"test_id": "{label}"}}),
            (type: "slot_grid", inventory: 0, cols: 9, rows: 3, first: 0, tags: {{"region": "chest"}}),
        ],
    ),
)
"#
    )
}

/// A private asset root that goes away with the test.
struct AssetRoot(PathBuf);

impl AssetRoot {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "slotted-screen-assets-{}-{name}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("screens")).unwrap();
        std::fs::create_dir_all(dir.join("themes")).unwrap();
        // The harness's `theme("glass")` loads from this root too.
        std::fs::write(dir.join("themes/glass.theme.ron"), GLASS).unwrap();
        Self(dir)
    }

    fn path(&self) -> &Path {
        &self.0
    }

    fn write_screen(&self, label: &str) {
        std::fs::write(self.0.join(SCREEN), screen_ron(label)).unwrap();
    }
}

impl Drop for AssetRoot {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn harness(root: &AssetRoot) -> UiHarness {
    UiHarness::builder()
        .plugins(SlottedPlugins::headless().set(AssetPlugin {
            file_path: root.path().to_string_lossy().into_owned(),
            ..default()
        }))
        .registries(TestRegistries::basic())
        .theme("glass")
        .build()
}

/// Runs frames until `done`, or fails naming what never happened. Asset loads
/// finish on the IO pool, so the frame they land on is not fixed.
fn pump_until(h: &mut UiHarness, what: &str, mut done: impl FnMut(&UiHarness) -> bool) {
    for _ in 0..600 {
        if done(h) {
            return;
        }
        h.step(1);
    }
    panic!("{what} did not happen within 600 frames");
}

fn registered(h: &UiHarness) -> bool {
    h.world()
        .resource::<Screens>()
        .get(&ScreenKind::new("demo:chest"))
        .is_some()
}

/// The loader turns a `*.screen.ron` into a registered `ScreenDef`, and
/// rewriting the file respawns the screen that is open on it, on the same
/// menu entity.
#[test]
fn editing_a_screen_asset_respawns_the_open_screen() {
    let root = AssetRoot::new("reload");
    root.write_screen("first");
    let mut h = harness(&root);

    let server = h.world().resource::<AssetServer>().clone();
    h.world_mut()
        .resource_mut::<ScreenAssets>()
        .load(&server, SCREEN);
    pump_until(&mut h, "the screen asset loaded", registered);

    let opened = h.open_screen(ScreenKind::new("demo:chest"), ChestFixture::empty());
    h.settle();
    assert!(h.try_find(&by::test_id("first")).is_some());
    assert!(h.try_find(&by::test_id("second")).is_none());

    // What the file watcher does when it sees the write.
    root.write_screen("second");
    server.reload(SCREEN);
    pump_until(&mut h, "the edited screen respawned", |h| {
        h.try_find(&by::test_id("second")).is_some()
    });
    h.settle();

    assert!(
        h.try_find(&by::test_id("first")).is_none(),
        "the old tree went with the old screen root"
    );
    let roots: Vec<&ScreenRoot> = h
        .world()
        .iter_entities()
        .filter_map(|e| e.get::<ScreenRoot>())
        .collect();
    assert_eq!(roots.len(), 1, "one screen, not two");
    assert_eq!(
        roots[0].menu,
        Some(opened.menu),
        "the respawn re-used the menu, so the slots did not need re-seeding"
    );
    assert!(
        h.try_find(&by::test_id("chest_panel")).is_some(),
        "the rest of the tree came back with it"
    );
}

/// A file that does not parse leaves the registered screen alone rather than
/// blanking it.
#[test]
fn a_broken_edit_leaves_the_previous_definition_registered() {
    let root = AssetRoot::new("broken");
    root.write_screen("first");
    let mut h = harness(&root);

    let server = h.world().resource::<AssetServer>().clone();
    h.world_mut()
        .resource_mut::<ScreenAssets>()
        .load(&server, SCREEN);
    pump_until(&mut h, "the screen asset loaded", registered);
    h.open_screen(ScreenKind::new("demo:chest"), ChestFixture::empty());
    h.settle();

    std::fs::write(root.path().join(SCREEN), "( kind: \"demo:chest\"").unwrap();
    server.reload(SCREEN);
    h.step(30);

    assert!(registered(&h), "the good definition is still registered");
    assert!(
        h.try_find(&by::test_id("first")).is_some(),
        "and the screen on screen is untouched"
    );
}
