//! Locators, failure messages and the screen tree, against a plain `bevy_ui`
//! tree this file spawns itself.
//!
//! Nothing here goes through `slotted-ui`'s widgets. The semantic components
//! are the whole contract between the widget layer and the harness, so a test
//! that attaches them by hand pins the harness half of that contract on its
//! own. `chest_screen.rs` covers the other half.
#![allow(clippy::unwrap_used)]

use bevy::input_focus::tab_navigation::{TabGroup, TabIndex};
use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use pretty_assertions::assert_eq;
use slotted_test::prelude::*;
use slotted_ui::{ItemView, ScreenRoot, SemanticLabel, Tags, TestId};

const CHEST: &str = "demo:chest";

/// Two "screens": a visible chest with a 2x2 grid of slots holding real
/// stacks, and a second screen whose panel is hidden. Enough shape for every
/// locator criterion without a single widget from `slotted-ui`.
fn spawn_tree(h: &mut UiHarness) {
    let world = h.world_mut();
    let root = world
        .spawn((
            Node {
                width: percent(100),
                height: percent(100),
                ..default()
            },
            TabGroup::new(0),
            ScreenRoot {
                kind: ScreenKind::new(CHEST),
                presentation: slotted_ui::Presentation::default(),
                initial_focus: None,
                menu: None,
            },
            SemanticRole::Screen,
        ))
        .id();
    let panel = world
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: px(100),
                top: px(100),
                ..default()
            },
            SemanticRole::Grid,
            Tags::new().with("region", "chest"),
            TestId::new("chest_grid"),
            ChildOf(root),
        ))
        .id();

    // A purely visual wrapper: no SemanticRole, so the tree must elide it and
    // lift the slots under it up to the grid.
    let wrapper = world.spawn((Node::default(), ChildOf(panel))).id();

    let contents = [
        ("minecraft:cobblestone", 64u32),
        ("minecraft:cobblestone", 23),
        ("minecraft:ender_pearl", 16),
        ("minecraft:diamond", 7),
    ];
    for (i, (item, count)) in contents.into_iter().enumerate() {
        let stack = TestRegistries::stack(item, count);
        let label = format!("{item} x{count}");
        world.spawn((
            Node {
                width: px(44),
                height: px(44),
                flex_shrink: 0.0,
                ..default()
            },
            bevy::ui_widgets::Button,
            Hovered::default(),
            TabIndex(i32::try_from(i).unwrap()),
            bevy::ui::auto_directional_navigation::AutoDirectionalNavigation::default(),
            SemanticRole::Slot,
            SemanticLabel(label),
            TestId::new(format!("chest:{i}")),
            Tags::new().with("region", "chest"),
            ItemView { stack: Some(stack) },
            ChildOf(wrapper),
        ));
    }

    // A second screen whose only slot is hidden: `.visible()` and the tree's
    // `(hidden)` marker both need something to look at.
    let other = world
        .spawn((
            Node::default(),
            ScreenRoot {
                kind: ScreenKind::new("demo:hidden"),
                presentation: slotted_ui::Presentation::default(),
                initial_focus: None,
                menu: None,
            },
            SemanticRole::Screen,
        ))
        .id();
    world.spawn((
        Node {
            width: px(44),
            height: px(44),
            ..default()
        },
        Visibility::Hidden,
        SemanticRole::Slot,
        SemanticLabel("hidden slot".to_owned()),
        Tags::new().with("region", "elsewhere"),
        ChildOf(other),
    ));
    h.step(2);
}

fn harness() -> UiHarness {
    let mut h = UiHarness::builder()
        .plugins(SlottedPlugins::headless())
        .registries(TestRegistries::basic())
        .resolution(1280.0, 720.0)
        .build();
    spawn_tree(&mut h);
    h
}

#[test]
fn every_locator_criterion_selects_what_it_says() {
    let h = harness();
    assert_eq!(h.find_all(&by::role(SemanticRole::Slot)).len(), 5);
    assert_eq!(h.find_all(&by::tag("region", "chest")).len(), 5, "grid too");
    assert_eq!(
        h.text_of(h.find(&by::test_id("chest:2"))).unwrap(),
        "minecraft:ender_pearl x16"
    );
    assert_eq!(
        h.find(&by::text("minecraft:diamond x7")),
        h.find(&by::test_id("chest:3"))
    );
    assert_eq!(h.find_all(&by::screen(ScreenKind::new(CHEST))).len(), 1);
    assert!(h.try_find(&by::anchor("title_end")).is_none());
    assert!(
        h.try_find(&by::widget_kind(WidgetKind::new("slotted:hotbar")))
            .is_none()
    );
}

#[test]
fn combinators_narrow_the_match_set() {
    let h = harness();
    let grid = h.find(&by::test_id("chest_grid"));

    // .within
    let in_grid = h.find_all(&by::role(SemanticRole::Slot).within(grid));
    assert_eq!(in_grid.len(), 4, "the hidden screen's slot is elsewhere");

    // .index picks one of several.
    assert_eq!(
        h.find(&by::role(SemanticRole::Slot).within(grid).index(2)),
        h.find(&by::test_id("chest:2"))
    );

    // .visible is a filter; the hidden screen's slot drops out.
    assert_eq!(h.find_all(&by::role(SemanticRole::Slot).visible()).len(), 4);
    assert_eq!(
        h.find_all(&by::tag("region", "elsewhere").visible()).len(),
        0
    );

    // .nth_visible is a selector over the visible matches.
    assert_eq!(
        h.find(&by::role(SemanticRole::Slot).nth_visible(0)),
        h.find(&by::test_id("chest:0"))
    );

    // .with_item reads ItemView through the registries.
    let cobble = h.find_all(&by::role(SemanticRole::Slot).with_item("minecraft:cobblestone"));
    assert_eq!(cobble.len(), 2);
    assert_eq!(
        h.find(
            &by::role(SemanticRole::Slot)
                .with_item("minecraft:cobblestone")
                .index(1)
        ),
        h.find(&by::test_id("chest:1"))
    );
}

#[test]
fn locators_resolve_in_tree_order() {
    let h = harness();
    let slots = h.find_all(&by::role(SemanticRole::Slot));
    let ids: Vec<String> = slots
        .iter()
        .map(|e| {
            h.world()
                .get::<TestId>(*e)
                .map_or_else(|| "-".to_owned(), |t| t.0.clone())
        })
        .collect();
    assert_eq!(ids, ["chest:0", "chest:1", "chest:2", "chest:3", "-"]);
}

#[test]
fn a_locator_reads_as_a_sentence() {
    assert_eq!(
        by::role(SemanticRole::Slot)
            .tag("region", "chest")
            .visible()
            .index(1)
            .to_string(),
        r#"role == Slot and tag region="chest" and visible, index 1"#
    );
    assert_eq!(Locator::default().to_string(), "any node");
}

#[test]
fn finding_nothing_names_the_criterion_that_failed() {
    let h = harness();
    let err = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        h.find(&by::role(SemanticRole::Slot).tag("region", "hotbar"))
    }))
    .expect_err("no slot is tagged region=hotbar");
    let msg = message(&err);

    assert!(msg.contains("no node matches"), "{msg}");
    assert!(msg.contains(r#"tag region="hotbar""#), "{msg}");
    // Every chest slot is a near miss on exactly the tag, and the message has
    // to say so rather than just reporting a count.
    assert!(msg.contains("near misses:"), "{msg}");
    assert!(msg.contains(r#"<- fails tag region="hotbar""#), "{msg}");
    assert!(msg.contains(r#"test_id="chest:0""#), "{msg}");
    // And the tree, so the reader can see what does exist.
    assert!(msg.contains("semantic tree:"), "{msg}");
    assert!(msg.contains("Grid #chest_grid"), "{msg}");
}

#[test]
fn finding_several_lists_them_with_their_index() {
    let h = harness();
    let err = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        h.find(&by::role(SemanticRole::Slot))
    }))
    .expect_err("there are five slots");
    let msg = message(&err);

    assert!(msg.starts_with("5 nodes match"), "{msg}");
    assert!(msg.contains(".index(n)"), "{msg}");
    assert!(msg.contains("[0] "), "{msg}");
    assert!(msg.contains("[4] "), "{msg}");
    assert!(
        msg.contains("(not visible)"),
        "the hidden slot is flagged: {msg}"
    );
}

#[test]
fn a_locator_that_matches_nothing_at_all_still_says_something() {
    let h = harness();
    let err = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        h.find(&by::test_id("no_such_node"))
    }))
    .expect_err("no such test id");
    let msg = message(&err);
    assert!(msg.contains(r#"test_id == "no_such_node""#), "{msg}");
    // One criterion: relaxing it explains nothing, so the message lists the
    // nodes that carry a TestId at all.
    assert!(msg.contains("chest_grid"), "{msg}");
}

/// The text tree is what a reader sees in a failure and in a snapshot diff.
#[test]
fn the_screen_tree_renders_as_an_indented_text_tree() {
    let mut h = harness();
    h.set_focus(Some(h.find(&by::test_id("chest:1"))));
    let tree = h.screen_tree();

    // Purely visual nodes are elided: the wrapper is not a level.
    assert_eq!(tree.roots.len(), 3, "two screens plus the carried layer");
    assert_eq!(tree.roots[0].children.len(), 1, "the grid");
    assert_eq!(tree.roots[0].children[0].children.len(), 4, "four slots");

    slotted_test::insta::assert_snapshot!(tree.to_string());
}

/// The RON form carries the same data and is what `assert_tree_snapshot!`
/// writes. A theme swap must not move either of them.
#[test]
fn the_screen_tree_is_serialisable() {
    let h = harness();
    assert_tree_snapshot!(h.screen_tree());
}

fn message(err: &Box<dyn std::any::Any + Send>) -> String {
    err.downcast_ref::<String>()
        .cloned()
        .or_else(|| err.downcast_ref::<&str>().map(ToString::to_string))
        .expect("a panic carries a message")
}
