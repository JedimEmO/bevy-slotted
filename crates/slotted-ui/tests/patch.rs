//! `patch_screen`: an open screen brought to a new definition by a keyed
//! diff. A def patched to itself changes no entity, a text change touches
//! one `Text`, a shape change respawns that subtree only, focus survives.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use bevy::input_focus::{FocusCause, InputFocus};
use bevy::prelude::*;
use pretty_assertions::assert_eq;
use slotted_test::prelude::*;
use slotted_ui::def::{
    ButtonOpts, LocKey, Presentation, ScreenDef, ScreenKind, Tags, TextRole, UiNodeDef,
};
use slotted_ui::{
    Layout, PatchStats, ScreenRoot, ScreenStack, Screens, patch_screen, patch_screen_following,
    push_screen,
};

fn harness() -> UiHarness {
    UiHarness::builder()
        .plugins(SlottedPlugins::headless())
        .resolution(1280.0, 720.0)
        .theme("glass")
        .motion(slotted_theme::Motion::REDUCED)
        .build()
}

fn text(id: &str, words: &str) -> UiNodeDef {
    UiNodeDef::Text {
        key: LocKey(words.to_owned()),
        style: TextRole::Body,
        opts: slotted_ui::TextOpts::default(),
        tags: Tags::new().with(Tags::TEST_ID, id),
    }
}

fn button(id: &str, label: &str) -> UiNodeDef {
    UiNodeDef::Button {
        widget: None,
        opts: ButtonOpts {
            label: Some(LocKey(label.to_owned())),
            ..ButtonOpts::default()
        },
        tags: Tags::new().with(Tags::TEST_ID, id),
    }
}

fn panel(id: &str, children: Vec<UiNodeDef>) -> UiNodeDef {
    UiNodeDef::Panel {
        role: slotted_theme::roles::PANEL,
        layout: Layout::default(),
        children,
        tags: Tags::new().with(Tags::TEST_ID, id),
    }
}

fn screen(children: Vec<UiNodeDef>) -> ScreenDef {
    ScreenDef {
        kind: ScreenKind::new("t:patch"),
        initial_focus: None,
        presentation: Presentation::default(),
        inherits: None,
        remove: vec![],
        listring: vec![],
        root: panel("root", children),
    }
}

fn open(h: &mut UiHarness, def: &ScreenDef) -> Entity {
    let def = h
        .world_mut()
        .resource_mut::<Screens>()
        .register(def.clone());
    let root = push_screen(&mut h.world_mut().commands(), def, None);
    h.world_mut().flush();
    h.step(2);
    root
}

fn patch(h: &mut UiHarness, root: Entity, def: &ScreenDef) {
    patch_screen(&mut h.world_mut().commands(), root, Arc::new(def.clone()));
    h.world_mut().flush();
    h.step(2);
}

fn stats(h: &UiHarness, root: Entity) -> PatchStats {
    *h.world().get::<PatchStats>(root).unwrap()
}

fn id_entity(h: &UiHarness, id: &str) -> Entity {
    h.find(&by::test_id(id))
}

fn words(h: &UiHarness, id: &str) -> String {
    h.world().get::<Text>(id_entity(h, id)).unwrap().0.clone()
}

#[test]
fn a_def_patched_to_itself_changes_no_entity() {
    let mut h = harness();
    let def = screen(vec![
        text("a", "one"),
        panel("p", vec![text("b", "two"), button("go", "Go")]),
    ]);
    let root = open(&mut h, &def);
    let before: Vec<Entity> = h.world_mut().query::<Entity>().iter(h.world()).collect();
    patch(&mut h, root, &def);
    let after: Vec<Entity> = h.world_mut().query::<Entity>().iter(h.world()).collect();
    assert_eq!(before, after);
    assert_eq!(stats(&h, root), PatchStats::default());
}

#[test]
fn a_text_change_rewrites_one_text_and_nothing_else() {
    let mut h = harness();
    let root = open(
        &mut h,
        &screen(vec![
            text("a", "one"),
            panel("p", vec![text("b", "two"), button("go", "Go")]),
        ]),
    );
    let (a, b, go) = (id_entity(&h, "a"), id_entity(&h, "b"), id_entity(&h, "go"));
    patch(
        &mut h,
        root,
        &screen(vec![
            text("a", "one"),
            panel("p", vec![text("b", "three"), button("go", "Go")]),
        ]),
    );
    assert_eq!(words(&h, "b"), "three");
    assert_eq!(words(&h, "a"), "one");
    assert_eq!(
        (id_entity(&h, "a"), id_entity(&h, "b"), id_entity(&h, "go")),
        (a, b, go),
        "every entity is the one that was there"
    );
    assert_eq!(
        stats(&h, root),
        PatchStats {
            spawned: 0,
            despawned: 0,
            rewritten: 1
        }
    );
}

#[test]
fn a_shape_change_respawns_that_subtree_only() {
    let mut h = harness();
    let root = open(
        &mut h,
        &screen(vec![
            text("a", "one"),
            panel("p", vec![text("b", "two"), button("go", "Go")]),
        ]),
    );
    let (a, p, b) = (id_entity(&h, "a"), id_entity(&h, "p"), id_entity(&h, "b"));
    let go = id_entity(&h, "go");
    // The button's label is part of its widget: it is respawned where it
    // stood, its siblings and its parent are not.
    patch(
        &mut h,
        root,
        &screen(vec![
            text("a", "one"),
            panel("p", vec![text("b", "two"), button("go", "Stop")]),
        ]),
    );
    assert_eq!(id_entity(&h, "a"), a);
    assert_eq!(id_entity(&h, "p"), p);
    assert_eq!(id_entity(&h, "b"), b);
    assert!(h.world().get_entity(go).is_err());
    assert_eq!(
        stats(&h, root),
        PatchStats {
            spawned: 1,
            despawned: 1,
            rewritten: 0
        }
    );
    let kids: Vec<Entity> = h.world().get::<Children>(p).unwrap().to_vec();
    assert_eq!(kids, vec![b, id_entity(&h, "go")], "in the same place");
}

#[test]
fn a_list_that_grows_shrinks_and_reorders_keeps_the_rows_that_stay() {
    let mut h = harness();
    let rows = |ids: &[&str]| screen(ids.iter().map(|id| text(id, id)).collect());
    let root = open(&mut h, &rows(&["r1", "r2", "r3"]));
    let (r1, r3) = (id_entity(&h, "r1"), id_entity(&h, "r3"));
    patch(&mut h, root, &rows(&["r3", "r4", "r1"]));
    assert_eq!(id_entity(&h, "r1"), r1);
    assert_eq!(id_entity(&h, "r3"), r3);
    let panel_node = id_entity(&h, "root");
    let order: Vec<String> = h
        .world()
        .get::<Children>(panel_node)
        .unwrap()
        .iter()
        .map(|c| h.world().get::<slotted_ui::TestId>(c).unwrap().0.clone())
        .collect();
    assert_eq!(order, ["r3", "r4", "r1"]);
    assert_eq!(
        stats(&h, root),
        PatchStats {
            spawned: 1,
            despawned: 1,
            rewritten: 0
        }
    );
}

#[test]
fn focus_stays_where_its_id_survives_a_respawn() {
    let mut h = harness();
    let root = open(
        &mut h,
        &screen(vec![button("one", "One"), button("go", "Go")]),
    );
    let go = id_entity(&h, "go");
    h.world_mut()
        .resource_mut::<InputFocus>()
        .set(go, FocusCause::Navigated);
    h.step(1);
    patch(
        &mut h,
        root,
        &screen(vec![button("one", "One"), button("go", "Go on")]),
    );
    let now = id_entity(&h, "go");
    assert_ne!(now, go, "the button was respawned");
    assert_eq!(h.world().resource::<InputFocus>().get(), Some(now));

    // And a focused node that stays is simply still focused.
    patch(
        &mut h,
        root,
        &screen(vec![button("one", "Uno"), button("go", "Go on")]),
    );
    assert_eq!(
        h.world().resource::<InputFocus>().get(),
        Some(id_entity(&h, "go"))
    );
}

#[test]
fn a_named_unchanged_hint_does_not_pull_the_focus_back() {
    let mut h = harness();
    let mut def = screen(vec![
        button("one", "One"),
        button("two", "Two"),
        text("clock", "10"),
    ]);
    def.initial_focus = Some("one".to_owned());
    let root = open(&mut h, &def);
    let two = id_entity(&h, "two");
    h.world_mut()
        .resource_mut::<InputFocus>()
        .set(two, FocusCause::Navigated);
    h.step(1);
    let mut next = def.clone();
    next.root = panel(
        "root",
        vec![
            button("one", "One"),
            button("two", "Two"),
            text("clock", "9"),
        ],
    );
    patch(&mut h, root, &next);
    assert_eq!(h.world().resource::<InputFocus>().get(), Some(two));

    // A hint that moves is obeyed.
    let one = id_entity(&h, "one");
    h.world_mut()
        .resource_mut::<InputFocus>()
        .set(one, FocusCause::Navigated);
    h.step(1);
    let mut moved = next.clone();
    moved.initial_focus = Some("two".to_owned());
    patch(&mut h, root, &moved);
    assert_eq!(
        h.world().resource::<InputFocus>().get(),
        Some(id_entity(&h, "two"))
    );

    // A following patch obeys even a hint that repeats.
    let one = id_entity(&h, "one");
    h.world_mut()
        .resource_mut::<InputFocus>()
        .set(one, FocusCause::Navigated);
    h.step(1);
    patch_screen_following(&mut h.world_mut().commands(), root, Arc::new(moved));
    h.world_mut().flush();
    h.step(2);
    assert_eq!(
        h.world().resource::<InputFocus>().get(),
        Some(id_entity(&h, "two"))
    );
}

#[test]
fn another_kind_or_presentation_is_a_replace() {
    let mut h = harness();
    let root = open(&mut h, &screen(vec![text("a", "one")]));
    let mut other = screen(vec![text("a", "one")]);
    other.presentation.mode = slotted_ui::def::PresentationMode::Modal;
    h.world_mut()
        .resource_mut::<Screens>()
        .register(other.clone());
    patch(&mut h, root, &other);
    assert!(h.world().get_entity(root).is_err(), "the old root is gone");
    let top = h.world().resource::<ScreenStack>().top().unwrap().root;
    assert!(h.world().get::<ScreenRoot>(top).is_some());
}
