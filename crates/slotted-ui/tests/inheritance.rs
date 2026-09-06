//! Screen inheritance end to end: two `*.screen.ron` texts, one `inherits`
//! link, and the tree that actually spawns.
//!
//! The unit tests in `src/screen.rs` cover the merge rules; this file checks
//! that the RON round trip carries `inherits` and `remove`, and that
//! `spawn_screen` puts the merged tree on screen in the right order.

#![allow(clippy::unwrap_used)]

use bevy::prelude::*;
use pretty_assertions::assert_eq;
use slotted_test::prelude::*;
use slotted_ui::{ScreenDef, Screens};

/// The screen a game ships: a title, an anchor where a variant can put its own
/// controls, and a row of slots.
const BASE: &str = r#"#![enable(implicit_some)]
(
    kind: "demo:base",
    root: (
        type: "panel",
        role: "panel",
        layout: (direction: "column"),
        tags: {"test_id": "base_panel"},
        children: [
            (type: "text", key: "chest.title", style: "title", tags: {"test_id": "title"}),
            (type: "panel", role: "panel", tags: {"test_id": "header"}, children: [
                (type: "anchor", id: "controls"),
            ]),
            (type: "text", key: "chest.hint", style: "muted", tags: {"test_id": "hint"}),
        ],
    ),
)
"#;

/// The variant: fills the ancestor's `controls` anchor, drops the hint, adds a
/// footer of its own.
const VARIANT: &str = r#"#![enable(implicit_some)]
(
    kind: "demo:variant",
    inherits: "demo:base",
    remove: ["hint"],
    root: (
        type: "panel",
        role: "panel",
        layout: (direction: "column"),
        tags: {"test_id": "variant_panel"},
        children: [
            (type: "panel", role: "panel", tags: {"test_id": "controls"}, children: [
                (type: "text", key: "chest.sort", style: "body", tags: {"test_id": "sort"}),
            ]),
            (type: "text", key: "chest.footer", style: "muted", tags: {"test_id": "footer"}),
        ],
    ),
)
"#;

fn harness() -> UiHarness {
    let mut h = UiHarness::builder()
        .plugins(SlottedPlugins::headless())
        .registries(TestRegistries::basic())
        .build();
    let mut screens = h.world_mut().resource_mut::<Screens>();
    screens.register(ScreenDef::from_ron(BASE).unwrap());
    screens.register(ScreenDef::from_ron(VARIANT).unwrap());
    h
}

/// `inherits` and `remove` survive the RON round trip, and a screen with
/// neither still parses.
#[test]
fn a_screen_ron_carries_inherits_and_remove() {
    let base = ScreenDef::from_ron(BASE).unwrap();
    assert_eq!(base.inherits, None);
    assert!(base.remove.is_empty(), "`remove` defaults to nothing");

    let variant = ScreenDef::from_ron(VARIANT).unwrap();
    assert_eq!(variant.inherits, Some(ScreenKind::new("demo:base")));
    assert_eq!(variant.remove, ["hint"]);
}

/// The spawned tree is the merged one: the child's panel filled the ancestor's
/// anchor where the anchor stood, the removed node is gone, and the ancestor's
/// own children are still there.
#[test]
fn spawning_an_inheriting_screen_puts_the_merged_tree_on_screen() {
    let mut h = harness();
    h.open_screen(ScreenKind::new("demo:variant"), ChestFixture::empty());
    h.settle();

    assert!(h.try_find(&by::test_id("variant_panel")).is_some());
    assert!(h.try_find(&by::test_id("title")).is_some(), "from the base");
    assert!(
        h.try_find(&by::test_id("sort")).is_some(),
        "the child's controls landed inside the ancestor's `header`"
    );
    assert!(
        h.try_find(&by::test_id("hint")).is_none(),
        "`remove` deleted the base's hint"
    );
    assert!(h.try_find(&by::test_id("footer")).is_some());

    // The anchor is gone as a node because the child replaced it, and what
    // replaced it kept the anchor's place under `header`.
    let controls = h.find(&by::test_id("controls"));
    let header = h.find(&by::test_id("header"));
    assert_eq!(
        h.world().get::<ChildOf>(controls).map(ChildOf::parent),
        Some(header),
        "the child node stands where the anchor stood, not appended at the root"
    );
}
