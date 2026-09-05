//! Adversarial review of `slotted-ui`, driven through the public harness.
//!
//! `tests/screens.rs` deliberately avoids `slotted-test` so the contract is
//! checked from both sides. This file is the other side: it uses the harness a
//! consumer would use, and each test tries to break a claim
//! `docs/design/phase2-contract.md` section 4 makes.

#![allow(clippy::unwrap_used)]

use std::time::Duration;

use bevy::prelude::*;
use pretty_assertions::assert_eq;
use slotted_model::{Button, MenuDef};
use slotted_test::prelude::*;
use slotted_theme::{ActiveMotions, Motion, Theme, Themed, Tween, TweenTarget, roles};
use slotted_ui::{
    AnchorId, ExclusionZone, Injection, Injections, ItemView, Layout, LocKey, ScreenDef,
    ScreenKind, Screens, Tags, TextRole, TooltipContent, TooltipTier, UiNodeDef, WidgetKind,
    close_screen,
};

const CHEST: &str = "demo:chest";
const GLASS: &str = include_str!("../../../assets/themes/glass.theme.ron");

fn grid(inventory: slotted_model::InventoryRef, rows: u16, first: u16, region: &str) -> UiNodeDef {
    UiNodeDef::SlotGrid {
        inventory,
        cols: 9,
        rows,
        first,
        tags: Tags::new().with("region", region),
    }
}

/// A chest screen with two anchors: one beside the title, one under the grid.
fn chest_screen() -> ScreenDef {
    ScreenDef {
        kind: ScreenKind::new(CHEST),
        inherits: None,
        root: UiNodeDef::Panel {
            role: roles::PANEL,
            layout: Layout {
                gap: 1.0,
                padding: 1.0,
                ..Default::default()
            },
            children: vec![
                UiNodeDef::Text {
                    key: LocKey("chest.title".to_owned()),
                    style: TextRole::Title,
                    tags: Tags::new().with("test_id", "title"),
                },
                UiNodeDef::Anchor {
                    id: AnchorId::new("title_end"),
                },
                grid(MenuDef::CONTAINER, 3, 0, "chest"),
                UiNodeDef::Anchor {
                    id: AnchorId::new("below_grid"),
                },
            ],
            tags: Tags::new().with("test_id", "chest_panel"),
        },
        listring: vec![MenuDef::CONTAINER, MenuDef::PLAYER_MAIN],
    }
}

fn harness() -> UiHarness {
    let mut h = UiHarness::builder()
        .plugins(SlottedPlugins::headless())
        .registries(TestRegistries::basic())
        .resolution(1280.0, 720.0)
        .theme("glass")
        .build();
    h.world_mut()
        .resource_mut::<Screens>()
        .register(chest_screen());
    h
}

fn open(h: &mut UiHarness) -> Opened {
    let opened = h.open_screen(ScreenKind::new(CHEST), ChestFixture::filled());
    h.settle();
    opened
}

fn label_node(text: &str, test_id: &str) -> UiNodeDef {
    UiNodeDef::Text {
        key: LocKey(text.to_owned()),
        style: TextRole::Body,
        tags: Tags::new().with("test_id", test_id),
    }
}

// ---------------------------------------------------------------- injection

/// Two injections at one anchor land in registration order, not in whatever
/// order the resource happens to iterate.
#[test]
fn two_injections_at_the_same_anchor_keep_registration_order() {
    let mut h = harness();
    {
        let mut injections = h.world_mut().resource_mut::<Injections>();
        for name in ["first", "second", "third"] {
            injections.0.push(Injection {
                target: ScreenKind::new(CHEST),
                anchor: AnchorId::new("title_end"),
                node: label_node(name, name),
                exclusion: false,
            });
        }
    }
    open(&mut h);

    let anchor = h.find(&by::anchor("title_end"));
    let children = h.world().get::<Children>(anchor).unwrap();
    let order: Vec<String> = children
        .iter()
        .filter_map(|c| h.world().get::<slotted_ui::TestId>(c).map(|t| t.0.clone()))
        .collect();
    assert_eq!(order, vec!["first", "second", "third"]);
}

/// An injection whose anchor no exists on the screen is reported, not
/// silently swallowed and not a panic.
#[test]
fn an_injection_at_a_missing_anchor_is_reported_and_not_fatal() {
    let mut h = harness();
    h.world_mut()
        .resource_mut::<Injections>()
        .0
        .push(Injection {
            target: ScreenKind::new(CHEST),
            anchor: AnchorId::new("nowhere"),
            node: label_node("stray", "stray"),
            exclusion: false,
        });
    let opened = open(&mut h);

    // The screen still opened and nothing from the stray injection appeared.
    assert!(h.try_find(&by::screen(ScreenKind::new(CHEST))).is_some());
    assert!(h.try_find(&by::test_id("stray")).is_none());

    // And the crate knows it dropped something: the unmatched injections are
    // recorded on the screen root for the game (and this test) to see.
    let missed = h
        .world()
        .get::<slotted_ui::UnmatchedInjections>(opened.screen)
        .expect("a screen with an injection it could not place records it");
    assert_eq!(missed.0, vec![AnchorId::new("nowhere")]);
}

/// An injection marked `exclusion` becomes an `ExclusionZone`, and closing
/// the screen takes the zone with it.
#[test]
fn closing_a_screen_removes_its_exclusion_zones_and_the_carried_stack() {
    let mut h = harness();
    h.world_mut()
        .resource_mut::<Injections>()
        .0
        .push(Injection {
            target: ScreenKind::new(CHEST),
            anchor: AnchorId::new("below_grid"),
            node: UiNodeDef::Panel {
                role: roles::PANEL,
                layout: Layout {
                    width: Some(120.0),
                    height: Some(40.0),
                    ..Default::default()
                },
                children: vec![label_node("side", "side")],
                tags: Tags::new().with("test_id", "side_panel"),
            },
            exclusion: true,
        });
    let opened = open(&mut h);

    assert_eq!(h.exclusion_zones(opened.screen).len(), 1);

    // Pick a stack up so the carried layer is showing something.
    let slot = h.find(&by::role(SemanticRole::Slot).tag("region", "chest").index(0));
    h.click_slot(slot, Button::Left, slotted_ecs::Modifiers::NONE);
    h.settle();
    assert!(h.carried(opened.menu).is_some());
    let carried_node = h
        .world_mut()
        .query_filtered::<Entity, With<slotted_ui::CarriedItem>>()
        .single(h.world())
        .expect("the plugin spawned one carried node");
    assert!(h.displayed_stack(carried_node).is_some());

    // Close the screen without closing the menu, the way a game's escape key
    // would if it tore the UI down first.
    let screen = opened.screen;
    h.world_mut().commands().queue(move |world: &mut World| {
        let mut commands = world.commands();
        close_screen(&mut commands, screen);
    });
    h.settle();

    assert!(h.world().get_entity(screen).is_err(), "the root is gone");
    let zones = h
        .world_mut()
        .query_filtered::<Entity, With<ExclusionZone>>()
        .iter(h.world())
        .count();
    assert_eq!(zones, 0, "the injected zone went with the screen");
    assert_eq!(
        h.displayed_stack(carried_node),
        None,
        "the carried layer stopped showing a stack from a screen that is gone"
    );
}

/// A screen that says it inherits from another does not silently produce a
/// broken tree: Phase 2 falls back to the screen's own root and warns.
#[test]
fn an_inherits_chain_falls_back_to_the_screens_own_root() {
    let mut h = harness();
    let base = ScreenDef {
        kind: ScreenKind::new("demo:base"),
        inherits: None,
        root: UiNodeDef::Panel {
            role: roles::PANEL,
            layout: Layout::default(),
            children: vec![UiNodeDef::Anchor {
                id: AnchorId::new("base_anchor"),
            }],
            tags: Tags::new().with("test_id", "base_panel"),
        },
        listring: vec![],
    };
    let middle = ScreenDef {
        kind: ScreenKind::new("demo:middle"),
        inherits: Some(ScreenKind::new("demo:base")),
        root: UiNodeDef::Panel {
            role: roles::PANEL,
            layout: Layout::default(),
            children: vec![label_node("middle", "middle_text")],
            tags: Tags::new().with("test_id", "middle_panel"),
        },
        listring: vec![],
    };
    let leaf = ScreenDef {
        kind: ScreenKind::new("demo:leaf"),
        inherits: Some(ScreenKind::new("demo:middle")),
        root: UiNodeDef::Panel {
            role: roles::PANEL,
            layout: Layout::default(),
            children: vec![label_node("leaf", "leaf_text")],
            tags: Tags::new().with("test_id", "leaf_panel"),
        },
        listring: vec![],
    };
    {
        let mut screens = h.world_mut().resource_mut::<Screens>();
        screens.register(base);
        screens.register(middle);
        screens.register(leaf);
    }
    h.world_mut()
        .resource_mut::<Injections>()
        .0
        .push(Injection {
            target: ScreenKind::new("demo:base"),
            anchor: AnchorId::new("base_anchor"),
            node: label_node("injected", "injected"),
            exclusion: false,
        });

    h.open_screen(ScreenKind::new("demo:leaf"), ChestFixture::empty());
    h.settle();

    // Phase 2 does not flatten `inherits` (docs/FOLLOWUPS.md). The leaf's own
    // tree is what spawns, so the ancestor's anchor and the injection at it
    // are not there. This test pins the fallback so the day inheritance lands
    // it fails loudly rather than quietly changing behaviour.
    assert!(h.try_find(&by::test_id("leaf_panel")).is_some());
    assert!(h.try_find(&by::test_id("middle_panel")).is_none());
    assert!(h.try_find(&by::anchor("base_anchor")).is_none());
    assert!(h.try_find(&by::test_id("injected")).is_none());
}

// ------------------------------------------------------------------ tooltips

/// A tooltip is clamped inside the window whichever corner its host sits in.
///
/// The host is parked directly under the full-window tooltip layer so its
/// `left`/`top` are window coordinates; that isolates `place_tooltips` from
/// the screen's own layout.
#[test]
fn a_tooltip_stays_inside_the_window_at_every_edge() {
    let window = Vec2::new(320.0, 240.0);
    let mut h = UiHarness::builder()
        .plugins(SlottedPlugins::headless())
        .registries(TestRegistries::basic())
        .resolution(window.x, window.y)
        .theme("glass")
        .build();
    h.settle();

    let layer = h
        .world_mut()
        .query_filtered::<Entity, With<slotted_ui::TooltipLayer>>()
        .single(h.world())
        .expect("the plugin spawned a tooltip layer");

    let host_size = Vec2::splat(40.0);
    let corners = [
        Vec2::new(0.0, 0.0),
        Vec2::new(window.x - host_size.x, 0.0),
        Vec2::new(0.0, window.y - host_size.y),
        window - host_size,
    ];

    for corner in corners {
        let host = h
            .world_mut()
            .spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(corner.x),
                    top: Val::Px(corner.y),
                    width: Val::Px(host_size.x),
                    height: Val::Px(host_size.y),
                    ..default()
                },
                ChildOf(layer),
            ))
            .id();
        let tooltip = h
            .world_mut()
            .spawn((
                Node {
                    position_type: PositionType::Absolute,
                    width: Val::Px(140.0),
                    height: Val::Px(60.0),
                    ..default()
                },
                SemanticRole::Tooltip,
                slotted_ui::TooltipHost(host),
                ChildOf(layer),
            ))
            .id();
        h.settle();

        let rect = h.rect_of(tooltip);
        assert!(
            rect.min.x >= -0.5 && rect.min.y >= -0.5,
            "tooltip for corner {corner:?} starts off-window: {rect:?}"
        );
        assert!(
            rect.max.x <= window.x + 0.5 && rect.max.y <= window.y + 0.5,
            "tooltip for corner {corner:?} runs off-window: {rect:?} in {window:?}"
        );

        h.world_mut().entity_mut(tooltip).despawn();
        h.world_mut().entity_mut(host).despawn();
        h.settle();
    }
}

/// A tooltip whose host is despawned must not stay on screen.
#[test]
fn a_tooltip_does_not_outlive_the_node_it_belongs_to() {
    let mut h = harness();
    let opened = open(&mut h);

    let slot = h.find(&by::role(SemanticRole::Slot).tag("region", "chest").index(0));
    h.hover(slot);
    h.advance(Duration::from_millis(600));
    h.settle();
    assert!(h.tooltip().is_some(), "the hovered slot composed a tooltip");

    let screen = opened.screen;
    h.world_mut().commands().queue(move |world: &mut World| {
        let mut commands = world.commands();
        close_screen(&mut commands, screen);
    });
    h.settle();

    let orphans = h
        .world_mut()
        .query_filtered::<Entity, With<slotted_ui::TooltipHost>>()
        .iter(h.world())
        .count();
    assert_eq!(orphans, 0, "the tooltip outlived its host node");
}

/// `request_tooltip` bypasses the hover delay by design and must survive a
/// `settle()`, which `tooltip_delay` would otherwise undo.
#[test]
fn a_requested_tooltip_survives_settling() {
    let mut h = harness();
    open(&mut h);
    let slot = h.find(&by::role(SemanticRole::Slot).tag("region", "chest").index(0));

    h.hover(slot);
    h.advance(Duration::from_millis(600));
    h.settle();
    assert_eq!(h.tooltip().map(|t| t.tier), Some(TooltipTier::Compact));

    // `tooltip_delay` must not re-assert the shift-derived tier every frame,
    // which would undo this request one frame later.
    h.request_tooltip(slot, TooltipTier::Expanded);
    h.settle();
    h.step(30);

    let content: TooltipContent = h.tooltip().expect("the request survived settling");
    assert_eq!(content.tier, TooltipTier::Expanded);
    assert!(!content.parts.is_empty(), "an occupied slot composes parts");
}

// ----------------------------------------------------------------- item view

/// A slot that empties and fills again tracks the model both ways.
#[test]
fn an_item_view_follows_a_slot_from_some_to_none_and_back() {
    let mut h = harness();
    let opened = open(&mut h);
    let slot = h.find(&by::role(SemanticRole::Slot).tag("region", "chest").index(0));

    let before = h.displayed_stack(slot).expect("the fixture filled slot 0");
    assert_eq!(h.stack_at(slot), Some(before.clone()));

    h.click_slot(slot, Button::Left, slotted_ecs::Modifiers::NONE);
    h.settle();
    assert_eq!(h.stack_at(slot), None);
    assert_eq!(h.displayed_stack(slot), None, "the widget emptied too");
    // Far enough apart that the second click is not a double click.
    h.advance(Duration::from_millis(500));
    assert_eq!(
        h.world().get::<slotted_ui::SemanticLabel>(slot).unwrap().0,
        "",
        "an empty slot announces nothing"
    );

    h.click_slot(slot, Button::Left, slotted_ecs::Modifiers::NONE);
    h.settle();
    assert_eq!(h.stack_at(slot), Some(before.clone()));
    assert_eq!(h.displayed_stack(slot), Some(before));
    assert!(
        !h.world()
            .get::<slotted_ui::SemanticLabel>(slot)
            .unwrap()
            .0
            .is_empty(),
        "and a full one announces the item again"
    );
    h.assert_conserved();
    let _ = opened;
}

// --------------------------------------------------------------------- theme

/// A `Themed` role the theme has never heard of falls back through its dotted
/// parents, and a role with no parent at all is skipped rather than fatal.
#[test]
fn an_unknown_theme_role_falls_back_and_never_panics() {
    let mut h = harness();
    open(&mut h);

    let node = h
        .world_mut()
        .spawn((
            Node::default(),
            Themed(slotted_theme::Role::new("slot.hover.deeply.nested")),
        ))
        .id();
    let orphan = h
        .world_mut()
        .spawn((
            Node::default(),
            Themed(slotted_theme::Role::new("no_such_root_role")),
        ))
        .id();
    h.settle();

    // Both nodes still exist and the app kept running: an unknown role is a
    // logged warning, never a panic.
    assert!(h.world().get_entity(node).is_ok());
    assert!(h.world().get_entity(orphan).is_ok());

    // The shipped theme resolves the same way, without an app: a dotted role
    // walks up to `slot`, a rootless one resolves to nothing.
    let theme = Theme::from_ron(GLASS).expect("the glass theme parses");
    assert!(
        theme
            .material(&slotted_theme::Role::new("slot.hover.deeply.nested"))
            .is_some(),
        "a dotted role falls back to `slot`"
    );
    assert!(
        theme
            .material(&slotted_theme::Role::new("no_such_root_role"))
            .is_none(),
        "a role with no parent resolves to nothing rather than guessing"
    );
}

// -------------------------------------------------------------------- motion

/// Under reduced motion every tween lands on its end value on its first frame,
/// whatever duration it was authored with.
#[test]
fn reduced_motion_finishes_every_tween_in_one_frame() {
    let mut h = UiHarness::builder()
        .plugins(SlottedPlugins::headless())
        .registries(TestRegistries::basic())
        .resolution(1280.0, 720.0)
        .theme("glass")
        .motion(Motion::REDUCED)
        .build();
    h.world_mut()
        .resource_mut::<Screens>()
        .register(chest_screen());
    open(&mut h);

    let slot = h.find(&by::role(SemanticRole::Slot).tag("region", "chest").index(0));
    h.world_mut().entity_mut(slot).insert(Tween::new(
        TweenTarget::Scale { from: 1.0, to: 1.2 },
        Duration::from_secs(10),
    ));
    let other = h
        .world_mut()
        .spawn((
            Node::default(),
            BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.0)),
            Tween::new(
                TweenTarget::Alpha { from: 0.0, to: 1.0 },
                Duration::from_secs(10),
            ),
        ))
        .id();

    h.step(1);
    assert_eq!(h.world().resource::<ActiveMotions>().0, 0);
    assert!(h.world().get::<Tween>(slot).is_none());
    assert!(h.world().get::<Tween>(other).is_none());
    let scale = h
        .world()
        .get::<bevy::ui::ui_transform::UiTransform>(slot)
        .expect("the tween wrote a transform")
        .scale;
    assert!((scale.x - 1.2).abs() < 1e-6, "{scale:?}");
}

/// A tween interrupted halfway must not leave the node at a scale that is
/// neither its start nor its end. Removing the component is the interruption a
/// pointer leaving mid-hover would cause.
#[test]
fn an_interrupted_scale_tween_can_be_returned_to_identity() {
    let mut h = harness();
    open(&mut h);
    let slot = h.find(&by::role(SemanticRole::Slot).tag("region", "chest").index(0));

    h.world_mut().entity_mut(slot).insert(Tween::new(
        TweenTarget::Scale { from: 1.0, to: 1.2 },
        Duration::from_millis(200),
    ));
    h.step(3);
    let mid = h
        .world()
        .get::<bevy::ui::ui_transform::UiTransform>(slot)
        .unwrap()
        .scale;
    assert!(
        mid.x > 1.0 && mid.x < 1.2,
        "the tween is mid-flight: {mid:?}"
    );

    // The pointer leaves: the hover tween is replaced by one back to rest.
    h.world_mut().entity_mut(slot).insert(Tween::new(
        TweenTarget::Scale {
            from: mid.x,
            to: 1.0,
        },
        Duration::from_millis(200),
    ));
    h.settle();

    assert!(
        h.world().get::<Tween>(slot).is_none(),
        "settle waits for tweens"
    );
    let rest = h
        .world()
        .get::<bevy::ui::ui_transform::UiTransform>(slot)
        .unwrap()
        .scale;
    assert!(
        (rest.x - 1.0).abs() < 1e-6 && (rest.y - 1.0).abs() < 1e-6,
        "the slot was left at {rest:?}, not identity"
    );
    assert_eq!(h.world().resource::<ActiveMotions>().0, 0);
}

// ---------------------------------------------------------------- navigation

/// The arrow keys move focus; the digit keys are a slot gesture and must not
/// also move focus. Neither reads the keyboard while something else owns it,
/// which is all Phase 2 can assert: there is no text input widget yet, so the
/// guard is `InputFocus` pointing at a node that is not a slot.
#[test]
fn arrow_and_digit_keys_do_not_overlap() {
    let mut h = harness();
    let opened = open(&mut h);

    let first = h.find(&by::role(SemanticRole::Slot).tag("region", "chest").index(0));
    let second = h.find(&by::role(SemanticRole::Slot).tag("region", "chest").index(1));
    h.set_focus(Some(first));
    h.focus_dir(bevy::math::CompassOctant::East);
    assert_eq!(h.focused(), Some(second), "the arrow key moved focus east");

    // A digit is a slot gesture, taken from the hovered slot, and it leaves
    // focus where it was.
    let before = h.stack_at(first);
    h.hover(first);
    h.key(KeyCode::Digit1);
    h.settle();
    assert_eq!(
        h.focused(),
        Some(second),
        "a digit key must not move keyboard focus"
    );
    assert_ne!(h.stack_at(first), before, "but it did swap the slot");
    h.assert_conserved();
    let _ = opened;
}

// ------------------------------------------------------------------ carried

/// The carried layer mirrors the menu the screen drives, and drops it again
/// when the stack goes back.
#[test]
fn the_carried_layer_mirrors_and_clears() {
    let mut h = harness();
    let opened = open(&mut h);
    let slot = h.find(&by::role(SemanticRole::Slot).tag("region", "chest").index(0));
    let carried_node = h
        .world_mut()
        .query_filtered::<Entity, With<slotted_ui::CarriedItem>>()
        .single(h.world())
        .unwrap();

    assert_eq!(h.displayed_stack(carried_node), None);
    h.click_slot(slot, Button::Left, slotted_ecs::Modifiers::NONE);
    h.settle();
    assert_eq!(h.displayed_stack(carried_node), h.carried(opened.menu));
    assert!(h.world().get::<ItemView>(carried_node).is_some());

    h.advance(Duration::from_millis(500));
    h.click_slot(slot, Button::Left, slotted_ecs::Modifiers::NONE);
    h.settle();
    assert_eq!(h.displayed_stack(carried_node), None);
    h.assert_conserved();
}

// ------------------------------------------------------------------ semantics

/// Every interactive node has a `SemanticRole`, and AccessKit sees exactly the
/// semantic nodes.
#[test]
fn every_interactive_node_is_semantic_and_reaches_accesskit() {
    let mut h = harness();
    open(&mut h);

    let mut interactive = h
        .world_mut()
        .query_filtered::<Entity, With<bevy::ui_widgets::Button>>();
    let missing: Vec<Entity> = interactive
        .iter(h.world())
        .filter(|e| h.world().get::<SemanticRole>(*e).is_none())
        .collect();
    assert!(
        missing.is_empty(),
        "interactive nodes without a SemanticRole: {missing:?}"
    );

    let semantic: Vec<Entity> = h
        .world_mut()
        .query_filtered::<Entity, With<SemanticRole>>()
        .iter(h.world())
        .collect();
    let unsynced: Vec<Entity> = semantic
        .iter()
        .copied()
        .filter(|e| h.world().get::<bevy::a11y::AccessibilityNode>(*e).is_none())
        .collect();
    assert!(
        unsynced.is_empty(),
        "semantic nodes AccessKit never heard about: {unsynced:?}"
    );
    assert!(semantic.len() > 27, "the chest grid alone is 27 slots");

    // The extra AccessKit nodes are Bevy's own, one per `Text`, and never a
    // node this crate spawned with a role.
    let extra: Vec<Entity> = h
        .world_mut()
        .query_filtered::<Entity, With<bevy::a11y::AccessibilityNode>>()
        .iter(h.world())
        .filter(|e| h.world().get::<SemanticRole>(*e).is_none())
        .collect();
    // Bevy gives every `ImageNode` an `AccessibilityNode` of its own, so the
    // icon child of each slot shows up in the AccessKit tree even though the
    // contract calls it purely visual and gives it no role. Pin that this is
    // the only source of extra nodes; see docs/FOLLOWUPS.md.
    let unexplained: Vec<Entity> = extra
        .iter()
        .copied()
        .filter(|e| h.world().get::<slotted_ui::ItemIcon>(*e).is_none())
        .filter(|e| h.world().get::<Text>(*e).is_none())
        .collect();
    assert!(
        unexplained.is_empty(),
        "an AccessKit node that is neither semantic, an item icon nor text: {unexplained:?}"
    );
}

/// The `WidgetKind` mapping is what a locator relies on; a `Custom` node with
/// no registered widget still spawns something findable rather than nothing.
#[test]
fn an_unknown_custom_widget_kind_still_spawns_a_findable_node() {
    let mut h = harness();
    let def = ScreenDef {
        kind: ScreenKind::new("demo:unknown"),
        inherits: None,
        root: UiNodeDef::Panel {
            role: roles::PANEL,
            layout: Layout::default(),
            children: vec![UiNodeDef::Custom {
                kind: WidgetKind::new("mod:does_not_exist"),
                params: slotted_registry::Value::Unit,
                children: vec![],
                tags: Tags::new().with("test_id", "mystery"),
            }],
            tags: Tags::new(),
        },
        listring: vec![],
    };
    h.world_mut().resource_mut::<Screens>().register(def);
    h.open_screen(ScreenKind::new("demo:unknown"), ChestFixture::empty());
    h.settle();

    let node = h.find(&by::test_id("mystery"));
    assert_eq!(
        h.world().get::<SemanticRole>(node),
        Some(&SemanticRole::Custom("mod:does_not_exist".to_owned()))
    );
    assert_eq!(
        h.find(&by::widget_kind(WidgetKind::new("mod:does_not_exist"))),
        node,
        "it still carries its WidgetNode, so a locator can name it"
    );
}

// ------------------------------------------------------------ widget motion

/// A chest screen with both grids, so a quick-move has a destination slot
/// entity to fly to.
fn two_grid_screen() -> ScreenDef {
    ScreenDef {
        kind: ScreenKind::new("demo:chest_full"),
        inherits: None,
        root: UiNodeDef::Panel {
            role: roles::PANEL,
            layout: Layout {
                direction: slotted_ui::LayoutDirection::Column,
                gap: 1.0,
                padding: 1.0,
                ..Default::default()
            },
            children: vec![
                grid(MenuDef::CONTAINER, 3, 0, "chest"),
                grid(MenuDef::PLAYER_MAIN, 3, 27, "player"),
            ],
            tags: Tags::new().with("test_id", "full_panel"),
        },
        listring: vec![MenuDef::CONTAINER, MenuDef::PLAYER_MAIN],
    }
}

fn scale_of(h: &UiHarness, entity: Entity) -> f32 {
    h.world()
        .get::<bevy::ui::ui_transform::UiTransform>(entity)
        .map_or(1.0, |t| t.scale.x)
}

/// Hovering a slot starts a hover tween, `settle` waits for it, and it lands
/// exactly on the hover scale.
#[test]
fn hovering_a_slot_tweens_it_to_the_hover_scale() {
    let mut h = harness();
    open(&mut h);
    let slot = h.find(&by::role(SemanticRole::Slot).tag("region", "chest").index(0));
    assert_eq!(
        h.world().get::<slotted_ui::MotionTarget>(slot).map(|t| t.0),
        Some(slotted_ui::REST_SCALE),
        "a slot at rest records its target without spending a tween"
    );
    assert!(h.world().get::<Tween>(slot).is_none());

    h.hover(slot);
    h.step(1);
    assert!(
        h.world().get::<Tween>(slot).is_some(),
        "hovering starts a tween"
    );
    assert_eq!(
        h.world().get::<slotted_ui::MotionTarget>(slot).map(|t| t.0),
        Some(slotted_ui::HOVER_SCALE)
    );
    assert!(h.world().resource::<ActiveMotions>().0 > 0);

    let frames = h.settle();
    assert!(frames > 1, "settle waited for the tween, not one frame");
    assert!(h.world().get::<Tween>(slot).is_none());
    assert_eq!(h.world().resource::<ActiveMotions>().0, 0);
    let scale = scale_of(&h, slot);
    assert!(
        (scale - slotted_ui::HOVER_SCALE).abs() < 1e-4,
        "hovered slot rests at {scale}"
    );
}

/// The pointer leaving halfway through the hover-in tween must not park the
/// slot at some intermediate scale.
#[test]
fn a_pointer_leaving_mid_tween_returns_the_slot_to_identity() {
    let mut h = harness();
    open(&mut h);
    let slot = h.find(&by::role(SemanticRole::Slot).tag("region", "chest").index(0));

    h.hover(slot);
    h.step(2);
    let mid = scale_of(&h, slot);
    assert!(
        mid > slotted_ui::REST_SCALE && mid < slotted_ui::HOVER_SCALE,
        "the hover tween is mid-flight at {mid}"
    );

    // Away, while the tween is still running.
    h.pointer_move_to(Vec2::new(5.0, 5.0));
    h.settle();

    assert!(h.world().get::<Tween>(slot).is_none());
    let rest = scale_of(&h, slot);
    assert!(
        (rest - slotted_ui::REST_SCALE).abs() < 1e-4,
        "the slot was left at {rest}, not identity"
    );
}

/// A pointer button held on a slot scales it down; releasing restores it.
#[test]
fn a_press_scales_the_slot_down_and_release_restores_it() {
    let mut h = harness();
    open(&mut h);
    let slot = h.find(&by::role(SemanticRole::Slot).tag("region", "chest").index(0));
    let centre = h.center_of(slot);

    h.pointer_move_to(centre);
    h.pointer_press(bevy::picking::pointer::PointerButton::Primary);
    h.step(1);
    assert!(h.world().get::<slotted_ui::SlotPressed>(slot).is_some());
    assert_eq!(
        h.world().get::<slotted_ui::MotionTarget>(slot).map(|t| t.0),
        Some(slotted_ui::PRESS_SCALE)
    );
    h.settle();
    assert!((scale_of(&h, slot) - slotted_ui::PRESS_SCALE).abs() < 1e-4);

    h.pointer_release(bevy::picking::pointer::PointerButton::Primary);
    h.settle();
    assert!(h.world().get::<slotted_ui::SlotPressed>(slot).is_none());
    // The pointer is still over it, so it springs back to the hover scale.
    assert_eq!(
        h.world().get::<slotted_ui::MotionTarget>(slot).map(|t| t.0),
        Some(slotted_ui::HOVER_SCALE)
    );
    assert!((scale_of(&h, slot) - slotted_ui::HOVER_SCALE).abs() < 1e-4);
}

/// A stack landing in the slot the player aimed at squashes it, and the
/// squash springs back to the slot's resting scale.
#[test]
fn a_placement_squashes_the_slot_it_landed_in() {
    let mut h = harness();
    open(&mut h);
    let slot = h.find(&by::role(SemanticRole::Slot).tag("region", "chest").index(0));

    h.click_slot(slot, Button::Left, slotted_ecs::Modifiers::NONE);
    h.settle();
    assert_eq!(h.stack_at(slot), None, "the stack is on the cursor");
    h.advance(Duration::from_millis(500));

    h.click_slot(slot, Button::Left, slotted_ecs::Modifiers::NONE);
    h.step(1);
    let tween = h
        .world()
        .get::<Tween>(slot)
        .expect("a placement squashes the slot")
        .clone();
    assert_eq!(
        tween.target,
        TweenTarget::Scale {
            from: slotted_ui::SQUASH_SCALE,
            to: slotted_ui::REST_SCALE,
        }
    );

    h.settle();
    assert!((scale_of(&h, slot) - slotted_ui::REST_SCALE).abs() < 1e-4);
    h.assert_conserved();
}

/// A quick-move sends a transient icon from the slot it left to the slot it
/// landed in, and the icon is gone once the flight is over.
#[test]
fn a_quick_move_flies_an_icon_between_the_slots() {
    let mut h = harness();
    h.world_mut()
        .resource_mut::<Screens>()
        .register(two_grid_screen());
    let opened = h.open_screen(ScreenKind::new("demo:chest_full"), ChestFixture::filled());
    h.settle();

    let source = h.find(&by::role(SemanticRole::Slot).tag("region", "chest").index(0));
    let moved = h.stack_at(source).expect("the fixture filled chest slot 0");

    h.click_slot(source, Button::Left, slotted_ecs::Modifiers::SHIFT);
    h.step(1);

    let flights: Vec<Entity> = h
        .world_mut()
        .query_filtered::<Entity, With<slotted_ui::FlyingItem>>()
        .iter(h.world())
        .collect();
    assert_eq!(flights.len(), 1, "one icon is in flight");
    let flight = flights[0];
    assert_eq!(
        h.displayed_stack(flight).map(|s| s.id),
        Some(moved.id),
        "the flying icon shows the stack that moved"
    );
    let tween = h.world().get::<Tween>(flight).expect("it is animating");
    assert!(
        matches!(tween.target, TweenTarget::Translate { from, to } if from == Vec2::ZERO && to != Vec2::ZERO),
        "it travels somewhere: {:?}",
        tween.target
    );
    assert!(h.world().resource::<ActiveMotions>().0 > 0);

    h.settle();
    assert!(
        h.world().get_entity(flight).is_err(),
        "the flight despawned when it finished"
    );
    assert_eq!(h.stack_at(source), None, "and the model really moved");
    h.assert_conserved();
    let _ = opened;
}

/// Under reduced motion every widget animation lands on its first frame, and
/// the quick-move flight is skipped rather than flashed.
#[test]
fn reduced_motion_finishes_widget_motion_in_one_frame() {
    let mut h = UiHarness::builder()
        .plugins(SlottedPlugins::headless())
        .registries(TestRegistries::basic())
        .resolution(1280.0, 720.0)
        .motion(Motion::REDUCED)
        .build();
    h.world_mut()
        .resource_mut::<Screens>()
        .register(two_grid_screen());
    h.open_screen(ScreenKind::new("demo:chest_full"), ChestFixture::filled());
    h.settle();

    let source = h.find(&by::role(SemanticRole::Slot).tag("region", "chest").index(0));
    h.hover(source);
    h.step(1);
    assert!(
        h.world().get::<Tween>(source).is_none(),
        "the hover tween completed on its first frame"
    );
    assert_eq!(h.world().resource::<ActiveMotions>().0, 0);
    assert!((scale_of(&h, source) - slotted_ui::HOVER_SCALE).abs() < 1e-4);

    h.click_slot(source, Button::Left, slotted_ecs::Modifiers::SHIFT);
    h.step(1);
    let flights = h
        .world_mut()
        .query_filtered::<Entity, With<slotted_ui::FlyingItem>>()
        .iter(h.world())
        .count();
    assert_eq!(flights, 0, "no flight is spawned under reduced motion");
    assert_eq!(h.world().resource::<ActiveMotions>().0, 0);
    assert_eq!(
        h.stack_at(source),
        None,
        "but the quick-move still happened"
    );
    h.assert_conserved();
}

/// An explicit request on a node the pointer never touched stands until it is
/// cleared. Contract 4.4: teardown is the pointer *leaving*, and a node that
/// was never hovered has not left anything.
#[test]
fn a_request_without_hover_stands_until_it_is_cleared() {
    let mut h = harness();
    open(&mut h);
    let slot = h.find(&by::role(SemanticRole::Slot).tag("region", "chest").index(0));
    assert!(h.tooltip().is_none());

    h.request_tooltip(slot, TooltipTier::Expanded);
    h.settle();
    h.step(60);

    let content: TooltipContent = h.tooltip().expect("the request still stands");
    assert_eq!(content.tier, TooltipTier::Expanded);
    assert!(
        h.world().get::<TooltipContent>(slot).is_some(),
        "and it is recorded on the host, not floating"
    );

    h.clear_tooltip(slot);
    h.settle();
    assert!(h.tooltip().is_none(), "clearing it takes it down");
    let hosted = h
        .world_mut()
        .query_filtered::<Entity, With<slotted_ui::TooltipHost>>()
        .iter(h.world())
        .count();
    assert_eq!(hosted, 0, "and takes the spawned node with it");
}

/// A request made while the pointer is over the host still ends when the
/// pointer leaves, which is the other half of the same rule.
#[test]
fn a_requested_tooltip_ends_when_the_pointer_leaves_the_host() {
    let mut h = harness();
    open(&mut h);
    let slot = h.find(&by::role(SemanticRole::Slot).tag("region", "chest").index(0));

    h.hover(slot);
    h.advance(Duration::from_millis(600));
    h.settle();
    h.request_tooltip(slot, TooltipTier::Expanded);
    h.settle();
    assert_eq!(h.tooltip().map(|t| t.tier), Some(TooltipTier::Expanded));

    h.pointer_move_to(Vec2::new(5.0, 5.0));
    h.settle();
    assert!(h.tooltip().is_none(), "the pointer left the host");
}
