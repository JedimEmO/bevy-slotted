//! Reload invalidation: which open screens an edit reaches, and what happens
//! to a screen whose definition goes away.
//!
//! The bug these are written against is that respawning matched only the exact
//! kind whose definition changed. Every test here is a way an edit reaches a
//! screen without changing that screen's own definition: inheritance, a widget
//! template, an injection added, an injection removed, and the `slotted:any`
//! wildcard. The last two cover the removal half of ownership -- a screen a
//! mod stops registering has to leave the world, and a game's own registration
//! has to survive that.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use bevy::prelude::*;
use pretty_assertions::assert_eq;
use slotted_test::prelude::*;
use slotted_ui::def::{AnchorId, LocKey, ScreenDef, ScreenKind, Tags, TextRole, UiNodeDef};
use slotted_ui::{
    ChangeSet, Injection, Injections, Owner, ScreenDependencies, ScreenDropped, ScreenRoot,
    Screens, SpawnCtx, TestId, Widget, WidgetKind, WidgetRegistry, invalidate_and_respawn,
};

// --------------------------------------------------------------- fixtures

/// A base screen with a rail anchor and a titled header.
const BASE: &str = r#"#![enable(implicit_some)]
(
    kind: "inv:base",
    root: (
        type: "panel",
        role: "panel",
        tags: {"test_id": "root"},
        children: [
            (type: "text", key: "base.title", style: "title", tags: {"test_id": "title"}),
            (type: "anchor", id: "rail"),
        ],
    ),
)"#;

/// A screen that inherits the base and adds nothing of its own.
const DERIVED: &str = r#"#![enable(implicit_some)]
(
    kind: "inv:derived",
    inherits: "inv:base",
    root: (
        type: "panel",
        role: "panel",
        tags: {"test_id": "root"},
        children: [
            (type: "text", key: "derived.extra", style: "body", tags: {"test_id": "extra"}),
        ],
    ),
)"#;

/// The base again, with the title text changed. Registering this replaces the
/// base without touching the derived screen's own definition.
const BASE_EDITED: &str = r#"#![enable(implicit_some)]
(
    kind: "inv:base",
    root: (
        type: "panel",
        role: "panel",
        tags: {"test_id": "root"},
        children: [
            (type: "text", key: "base.edited", style: "title", tags: {"test_id": "edited"}),
            (type: "anchor", id: "rail"),
        ],
    ),
)"#;

/// A screen whose tree spawns a mod-registered widget kind.
const USES_TEMPLATE: &str = r#"#![enable(implicit_some)]
(
    kind: "inv:uses_template",
    root: (
        type: "panel",
        role: "panel",
        tags: {"test_id": "root"},
        children: [
            (type: "custom", kind: "inv:badge", tags: {"test_id": "badge_host"}),
        ],
    ),
)"#;

/// A widget that spawns one text node whose `test_id` is its own label, so a
/// test can tell one registration of the kind from the next.
#[derive(Debug, Clone)]
struct Badge(&'static str);

impl Widget for Badge {
    fn spawn(
        &self,
        ctx: &mut SpawnCtx<'_>,
        _params: &slotted_registry::Value,
        _children: &[UiNodeDef],
    ) -> Entity {
        let root = ctx.spawn_node((
            Node::default(),
            slotted_ui::SemanticRole::Panel,
            slotted_ui::WidgetNode(WidgetKind::new("inv:badge")),
        ));
        // The label goes on a child: the `Custom` node's own `test_id` tag
        // lands on the widget root, so the root cannot carry the version.
        ctx.world.spawn((
            Node::default(),
            slotted_ui::SemanticRole::Text,
            TestId(self.0.to_owned()),
            ChildOf(root),
        ));
        root
    }
}

fn text_injection(target: &str, anchor: &str, test_id: &str, owner: Owner) -> Injection {
    Injection {
        target: ScreenKind::new(target),
        anchor: AnchorId::new(anchor),
        node: UiNodeDef::Text {
            key: LocKey("mod.rail".to_owned()),
            style: TextRole::Body,
            tags: Tags::new().with(Tags::TEST_ID, test_id),
        },
        exclusion: false,
        owner,
    }
}

fn harness(screens: &[&str]) -> UiHarness {
    let mut h = UiHarness::builder()
        .plugins(SlottedPlugins::headless())
        .registries(TestRegistries::basic())
        .build();
    {
        let mut registry = h.world_mut().resource_mut::<Screens>();
        for text in screens {
            registry.register(ScreenDef::from_ron(text).unwrap());
        }
    }
    h
}

fn respawn(h: &mut UiHarness, change: ChangeSet) -> Vec<ScreenKind> {
    let affected = invalidate_and_respawn(h.world_mut(), &change);
    h.settle();
    affected
}

fn open_kinds(h: &UiHarness) -> Vec<String> {
    let mut kinds: Vec<String> = h
        .world()
        .iter_entities()
        .filter_map(|e| e.get::<ScreenRoot>())
        .map(|root| root.kind.0.to_string())
        .collect();
    kinds.sort();
    kinds
}

// ------------------------------------------------------------ inheritance

/// The headline case: an edit to a base screen has to reach the screens that
/// inherit it. Matching only the changed kind left the derived screen drawing
/// the tree it was spawned with.
#[test]
fn editing_a_base_respawns_the_derived_open_screen() {
    let mut h = harness(&[BASE, DERIVED]);
    h.open_screen(ScreenKind::new("inv:derived"), ChestFixture::empty());
    h.settle();
    assert!(
        h.try_find(&by::test_id("title")).is_some(),
        "the derived screen starts with the base's title"
    );

    h.world_mut()
        .resource_mut::<Screens>()
        .register(ScreenDef::from_ron(BASE_EDITED).unwrap());
    let affected = respawn(&mut h, ChangeSet::screens([ScreenKind::new("inv:base")]));

    assert!(
        affected.contains(&ScreenKind::new("inv:derived")),
        "the derived screen is affected by its base changing: {affected:?}"
    );
    assert!(
        h.try_find(&by::test_id("edited")).is_some(),
        "the open derived screen was respawned onto the edited base"
    );
    assert!(
        h.try_find(&by::test_id("title")).is_none(),
        "and the node the edit removed is gone"
    );
}

/// A screen that inherits nothing and shares no template with the edit is left
/// alone, so the test above is measuring the dependency and not a blanket
/// respawn of everything.
#[test]
fn an_unrelated_screen_is_not_invalidated() {
    let h = harness(&[BASE, DERIVED, USES_TEMPLATE]);
    let deps = ScreenDependencies::build(h.world().resource::<Screens>());
    let affected = deps.invalidate(&ChangeSet::screens([ScreenKind::new("inv:base")]));
    assert_eq!(
        affected,
        vec![ScreenKind::new("inv:base"), ScreenKind::new("inv:derived")],
        "only the base and what inherits it"
    );
}

// --------------------------------------------------------------- templates

/// A screen does not change when a widget template it uses does, but what it
/// draws does. The template's kind is the dependency.
#[test]
fn changing_a_widget_template_respawns_its_users() {
    let mut h = harness(&[USES_TEMPLATE]);
    h.world_mut()
        .resource_mut::<WidgetRegistry>()
        .register_owned(
            WidgetKind::new("inv:badge"),
            Badge("badge_v1"),
            Owner::Mod("inv".to_owned()),
        );
    h.open_screen(ScreenKind::new("inv:uses_template"), ChestFixture::empty());
    h.settle();
    assert!(h.try_find(&by::test_id("badge_v1")).is_some());

    let removed = h
        .world_mut()
        .resource_mut::<WidgetRegistry>()
        .reconcile_mods(vec![(
            WidgetKind::new("inv:badge"),
            Arc::new(Badge("badge_v2")) as Arc<dyn Widget>,
            Owner::Mod("inv".to_owned()),
        )]);
    assert!(removed.removed.is_empty(), "the kind is still registered");

    let affected = respawn(
        &mut h,
        ChangeSet {
            templates: removed.changed.into_iter().collect(),
            ..ChangeSet::default()
        },
    );
    assert_eq!(affected, vec![ScreenKind::new("inv:uses_template")]);
    assert!(
        h.try_find(&by::test_id("badge_v2")).is_some(),
        "the screen redrew with the new template"
    );
}

// -------------------------------------------------------------- injections

/// An injection removed is as much a change as one added: the node it spliced
/// in is still on screen until the screen respawns.
#[test]
fn removing_an_injection_respawns_its_former_target() {
    let mut h = harness(&[BASE]);
    let injection = text_injection("inv:base", "rail", "injected", Owner::Mod("inv".to_owned()));
    h.world_mut()
        .insert_resource(Injections(vec![injection.clone()]));
    h.open_screen(ScreenKind::new("inv:base"), ChestFixture::empty());
    h.settle();
    assert!(
        h.try_find(&by::test_id("injected")).is_some(),
        "the injection is spliced in to start with"
    );

    let diff = slotted_ui::reconcile_mod_injections(
        &mut h.world_mut().resource_mut::<Injections>(),
        Vec::new(),
    );
    assert_eq!(diff.removed, vec![injection]);

    let affected = respawn(
        &mut h,
        ChangeSet {
            injections_removed: diff.removed,
            ..ChangeSet::default()
        },
    );
    assert_eq!(affected, vec![ScreenKind::new("inv:base")]);
    assert!(
        h.try_find(&by::test_id("injected")).is_none(),
        "the node the mod stopped injecting is gone from the open screen"
    );
}

/// The wildcard target used not to match anything during invalidation, so an
/// `slotted:any` injection changed nothing until the player re-opened the
/// screen by hand. It matches every open screen that has the anchor.
#[test]
fn an_any_injection_respawns_ordinary_open_screens() {
    let mut h = harness(&[BASE, DERIVED, USES_TEMPLATE]);
    h.open_screen(ScreenKind::new("inv:base"), ChestFixture::empty());
    h.settle();

    let injection = text_injection(
        ScreenKind::any().0.to_string().as_str(),
        "rail",
        "everywhere",
        Owner::Mod("inv".to_owned()),
    );
    h.world_mut()
        .resource_mut::<Injections>()
        .0
        .push(injection.clone());

    let affected = respawn(
        &mut h,
        ChangeSet {
            injections_added: vec![injection],
            ..ChangeSet::default()
        },
    );
    assert_eq!(
        affected,
        vec![ScreenKind::new("inv:base"), ScreenKind::new("inv:derived")],
        "every screen with a `rail` anchor, and only those: `inv:uses_template` has none"
    );
    assert!(
        h.try_find(&by::test_id("everywhere")).is_some(),
        "the open screen picked the wildcard injection up"
    );
}

// ---------------------------------------------------------------- removal

/// A mod that stops registering a screen unregisters it, and an instance the
/// player has open closes rather than drawing from a definition nothing holds.
#[test]
fn a_removed_screen_closes_the_open_instance_with_a_report() {
    let mut h = UiHarness::builder()
        .plugins(SlottedPlugins::headless())
        .registries(TestRegistries::basic())
        .build();
    {
        let mut screens = h.world_mut().resource_mut::<Screens>();
        screens.register_owned(
            ScreenDef::from_ron(BASE).unwrap(),
            Owner::Mod("inv".to_owned()),
        );
    }
    h.open_screen(ScreenKind::new("inv:base"), ChestFixture::empty());
    h.settle();
    assert_eq!(open_kinds(&h), vec!["inv:base".to_owned()]);

    let diff = h
        .world_mut()
        .resource_mut::<Screens>()
        .reconcile_mods(Vec::new());
    assert_eq!(diff.removed, vec![ScreenKind::new("inv:base")]);
    assert!(
        h.world()
            .resource::<Screens>()
            .get(&ScreenKind::new("inv:base"))
            .is_none(),
        "the kind is gone from Screens"
    );

    invalidate_and_respawn(
        h.world_mut(),
        &ChangeSet {
            removed_screens: diff.removed.into_iter().collect(),
            ..ChangeSet::default()
        },
    );
    // Read the report before settling: `Messages` is double-buffered and a
    // handful of frames would drop it.
    let reports: Vec<ScreenKind> = h
        .world()
        .resource::<Messages<ScreenDropped>>()
        .iter_current_update_messages()
        .map(|dropped| dropped.kind.clone())
        .collect();
    h.settle();
    assert!(open_kinds(&h).is_empty(), "the open instance closed");
    assert_eq!(
        reports,
        vec![ScreenKind::new("inv:base")],
        "and the close was reported"
    );
}

/// Reconciling the mod-owned set never removes what the game registered in
/// Rust, and a mod that shadowed a game screen hands it back when it stops
/// registering.
#[test]
fn reconciling_mods_leaves_game_registrations_alone() {
    let mut screens = Screens::default();
    screens.register(ScreenDef::from_ron(BASE).unwrap());
    screens.register(ScreenDef::from_ron(USES_TEMPLATE).unwrap());
    screens.register_owned(
        ScreenDef::from_ron(DERIVED).unwrap(),
        Owner::Mod("inv".to_owned()),
    );

    // A mod takes the base over, then stops shipping both screens.
    let diff = screens.reconcile_mods(vec![(
        Owner::Mod("inv".to_owned()),
        ScreenDef::from_ron(BASE_EDITED).unwrap(),
    )]);
    assert_eq!(diff.removed, vec![ScreenKind::new("inv:derived")]);
    assert_eq!(diff.changed, vec![ScreenKind::new("inv:base")]);

    let diff = screens.reconcile_mods(Vec::new());
    assert!(
        diff.removed.is_empty(),
        "the game's base was restored rather than removed: {diff:?}"
    );
    assert_eq!(
        screens.owner(&ScreenKind::new("inv:base")),
        Some(&Owner::Game)
    );
    assert_eq!(
        **screens.get(&ScreenKind::new("inv:base")).unwrap(),
        ScreenDef::from_ron(BASE).unwrap(),
        "and it is the game's definition again"
    );
    assert!(
        screens.get(&ScreenKind::new("inv:uses_template")).is_some(),
        "a game screen no mod ever touched is untouched"
    );
}

// ---------------------------------------------------------------- the stack

/// A hot reload of a screen the stack holds puts the new tree back in the
/// same stack position (menus M0, package B's follow-up). Before this, the
/// respawn went through `spawn_screen`, so the edited screen was open but no
/// longer an entry and `Back` stopped closing it.
#[test]
fn respawning_a_stacked_screen_keeps_it_in_the_stack() {
    let mut h = harness(&[BASE, DERIVED]);
    let opened = h.open_screen(ScreenKind::new("inv:derived"), ChestFixture::empty());
    h.settle();
    assert_eq!(h.stack(), vec![ScreenKind::new("inv:derived")]);

    h.world_mut()
        .resource_mut::<Screens>()
        .register(ScreenDef::from_ron(BASE_EDITED).unwrap());
    respawn(&mut h, ChangeSet::screens([ScreenKind::new("inv:base")]));

    let root = h.find(&by::screen(ScreenKind::new("inv:derived")));
    assert_ne!(root, opened.screen, "a fresh tree");
    assert!(h.try_find(&by::test_id("edited")).is_some());
    assert_eq!(
        h.stack(),
        vec![ScreenKind::new("inv:derived")],
        "still one entry, the same kind"
    );
    let entry = h
        .world()
        .resource::<slotted_ui::ScreenStack>()
        .top()
        .cloned()
        .expect("the respawned screen is the top entry");
    assert_eq!(entry.root, root, "the entry names the new root");
    assert_eq!(entry.menu, Some(opened.menu), "on the same menu");

    // And `Back` still closes it, menu and all.
    h.action(slotted_ui::UiAction::Back);
    h.settle();
    assert!(h.stack().is_empty());
    assert!(open_kinds(&h).is_empty());
    assert!(h.world().get_entity(opened.menu).is_err());
}

/// The position is kept, not just the membership: a screen under a page is
/// respawned under it, hidden, and the page stays on top with focus.
#[test]
fn respawning_a_screen_under_a_page_keeps_it_under_the_page() {
    let mut h = harness(&[BASE, DERIVED, USES_TEMPLATE]);
    h.world_mut()
        .resource_mut::<WidgetRegistry>()
        .register_owned(
            WidgetKind::new("inv:badge"),
            Badge("badge_v1"),
            Owner::Mod("inv".to_owned()),
        );
    let below = h.open_screen(ScreenKind::new("inv:derived"), ChestFixture::empty());
    let above = h.open(ScreenKind::new("inv:uses_template"));
    h.settle();
    assert_eq!(
        h.stack(),
        vec![
            ScreenKind::new("inv:derived"),
            ScreenKind::new("inv:uses_template")
        ]
    );
    assert!(!h.is_visible(below.screen), "a page hides the entry below");

    h.world_mut()
        .resource_mut::<Screens>()
        .register(ScreenDef::from_ron(BASE_EDITED).unwrap());
    respawn(&mut h, ChangeSet::screens([ScreenKind::new("inv:base")]));

    assert_eq!(
        h.stack(),
        vec![
            ScreenKind::new("inv:derived"),
            ScreenKind::new("inv:uses_template")
        ],
        "same order"
    );
    let respawned = h.find(&by::screen(ScreenKind::new("inv:derived")));
    assert_ne!(respawned, below.screen);
    assert!(
        !h.is_visible(respawned),
        "still under the page, still hidden"
    );
    assert!(h.is_visible(above), "the page above is untouched");
    let top = h
        .world()
        .resource::<slotted_ui::ScreenStack>()
        .top()
        .map(|e| e.root);
    assert_eq!(top, Some(above));
    // Neither screen has a focusable node (Bevy parks focus on the window),
    // so what matters is that the respawn did not hand it to the hidden
    // screen.
    let focused = h.focused().expect("Bevy's InputFocus starts on the window");
    let mut e = focused;
    let under_respawned = loop {
        if e == respawned {
            break true;
        }
        match h.world().get::<ChildOf>(e) {
            Some(parent) => e = parent.parent(),
            None => break false,
        }
    };
    assert!(
        !under_respawned,
        "focus did not move into the hidden screen"
    );
}
