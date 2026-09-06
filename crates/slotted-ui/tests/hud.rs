//! HUD layers, the position editor and script-driven HUD updates.
//! Phase 6 contract section 2.
//!
//! These run through `slotted-test`, so they assert on the same components a
//! game's own tests would: the layer order, the wrapper rects the anchors
//! resolve to, and the stacks a hotbar layer shows.

#![allow(clippy::unwrap_used)]

use bevy::prelude::*;
use pretty_assertions::assert_eq;
use slotted_ecs::{Inventory, SlotChanged, SlotRef};
use slotted_model::{ItemId, ItemStack, MenuDef, SlotIx};
use slotted_test::prelude::*;
use slotted_ui::def::{Layout, Tags, UiNodeDef};
use slotted_ui::hud::builtin;
use slotted_ui::{
    HudAnchor, HudHotbar, HudLayerDef, HudLayerId, HudLayerPayload, HudLayers, HudUpdate, HudValue,
    ItemView, NineAnchor, SemanticRole,
};

const WIDTH: f32 = 1280.0;
const HEIGHT: f32 = 720.0;

fn harness_at(width: f32, height: f32) -> UiHarness {
    UiHarness::builder()
        .plugins(SlottedPlugins::headless())
        .resolution(width, height)
        .build()
}

fn harness() -> UiHarness {
    harness_at(WIDTH, HEIGHT)
}

/// A layer whose whole tree is one `width x height` panel tagged `test_id`.
fn panel_layer(id: &str, anchor: HudAnchor, size: Vec2) -> HudLayerDef {
    HudLayerDef {
        id: HudLayerId::new(id),
        anchor,
        tree: UiNodeDef::Panel {
            role: slotted_theme::roles::HUD_PANEL,
            layout: Layout {
                width: Some(size.x),
                height: Some(size.y),
                ..Layout::default()
            },
            children: Vec::new(),
            tags: Tags::new().with(Tags::TEST_ID, id),
        },
        visible: true,
        hide_with_screen: false,
    }
}

fn at(anchor: NineAnchor, offset: Vec2) -> HudAnchor {
    HudAnchor {
        anchor,
        offset,
        scale: 1.0,
    }
}

fn ids(layers: &HudLayers) -> Vec<String> {
    layers.order().iter().map(ToString::to_string).collect()
}

// ---------------------------------------------------------------------------
// The registry
// ---------------------------------------------------------------------------

#[test]
fn builtin_layers_are_registered_bottom_to_top() {
    let h = harness();
    let layers = h.world().resource::<HudLayers>();
    assert_eq!(
        ids(layers),
        vec![
            "crosshair",
            "hotbar",
            "health",
            "hunger",
            "air",
            "experience",
            "boss_bar",
            "chat",
            "debug",
        ]
    );
}

#[test]
fn insert_replace_and_remove_keep_the_order_stable() {
    let mut layers = HudLayers::default();
    slotted_ui::hud::register_builtin_layers(&mut layers, &HudHotbar::default());

    layers
        .insert_above(
            &builtin::HEALTH,
            panel_layer(
                "mod:mana",
                at(NineAnchor::TopLeft, Vec2::ZERO),
                Vec2::splat(8.0),
            ),
        )
        .unwrap();
    layers
        .insert_below(
            &builtin::CHAT,
            panel_layer(
                "mod:quests",
                at(NineAnchor::TopLeft, Vec2::ZERO),
                Vec2::splat(8.0),
            ),
        )
        .unwrap();
    assert_eq!(
        ids(&layers),
        vec![
            "crosshair",
            "hotbar",
            "health",
            "mod:mana",
            "hunger",
            "air",
            "experience",
            "boss_bar",
            "mod:quests",
            "chat",
            "debug",
        ]
    );

    // A replacement keeps the slot and takes the replaced layer's id.
    let mut replacement = panel_layer(
        "ignored",
        at(NineAnchor::Bottom, Vec2::ZERO),
        Vec2::splat(4.0),
    );
    replacement.hide_with_screen = true;
    layers.replace(&builtin::HUNGER, replacement).unwrap();
    assert_eq!(ids(&layers)[4], "hunger");
    assert!(layers.get(&builtin::HUNGER).unwrap().hide_with_screen);

    assert!(layers.remove(&builtin::AIR).is_some());
    assert!(layers.remove(&builtin::AIR).is_none());
    assert_eq!(
        ids(&layers),
        vec![
            "crosshair",
            "hotbar",
            "health",
            "mod:mana",
            "hunger",
            "experience",
            "boss_bar",
            "mod:quests",
            "chat",
            "debug",
        ]
    );

    // Re-placing a layer that already exists moves it; it never duplicates.
    layers
        .insert_below(
            &builtin::CROSSHAIR,
            panel_layer(
                "mod:mana",
                at(NineAnchor::TopLeft, Vec2::ZERO),
                Vec2::splat(8.0),
            ),
        )
        .unwrap();
    assert_eq!(ids(&layers)[0], "mod:mana");
    assert_eq!(
        ids(&layers).iter().filter(|id| *id == "mod:mana").count(),
        1
    );

    let unknown = HudLayerId::new("mod:nope");
    assert!(
        layers
            .insert_above(
                &unknown,
                panel_layer("mod:x", HudAnchor::default(), Vec2::ZERO)
            )
            .is_err()
    );
}

#[test]
fn a_layer_root_carries_its_id_and_a_locator_finds_it() {
    let mut h = harness();
    h.world_mut()
        .resource_mut::<HudLayers>()
        .register(panel_layer(
            "mod:mana",
            at(NineAnchor::TopLeft, Vec2::new(16.0, 16.0)),
            Vec2::new(40.0, 20.0),
        ));
    h.step(2);

    let root = h.find(&by::hud_layer("mod:mana"));
    assert_eq!(h.hud_layer("mod:mana"), Some(root));
    assert_eq!(
        h.world().get::<SemanticRole>(root),
        Some(&SemanticRole::HudLayer)
    );
    // Bottom to top, and the mod layer went on top of the nine built-ins.
    assert_eq!(h.hud_layers().last().copied(), Some(root));
    assert!(h.hud_tree().to_string().contains("mod:mana"));
}

// ---------------------------------------------------------------------------
// Anchors
// ---------------------------------------------------------------------------

/// Every anchor, at two resolutions: the offset is measured from the anchor
/// point, and a centred axis centres the content rather than its left edge.
#[test]
fn anchors_resolve_to_the_expected_rects_at_two_resolutions() {
    let size = Vec2::new(40.0, 20.0);
    let offset = Vec2::new(10.0, 6.0);
    let cases = [
        NineAnchor::TopLeft,
        NineAnchor::Top,
        NineAnchor::TopRight,
        NineAnchor::Left,
        NineAnchor::Center,
        NineAnchor::Right,
        NineAnchor::BottomLeft,
        NineAnchor::Bottom,
        NineAnchor::BottomRight,
    ];
    for (width, height) in [(WIDTH, HEIGHT), (1920.0, 1080.0)] {
        let mut h = harness_at(width, height);
        {
            let mut layers = h.world_mut().resource_mut::<HudLayers>();
            for anchor in cases {
                layers.register(panel_layer(
                    &format!("mod:{anchor:?}"),
                    at(anchor, offset),
                    size,
                ));
            }
        }
        h.step(2);
        let window = Vec2::new(width, height);
        for anchor in cases {
            let node = h.find(&by::test_id(&format!("mod:{anchor:?}")));
            let rect = h.rect_of(node);
            let expected = expected_center(anchor, window, size, offset);
            assert!(
                (rect.center() - expected).length() < 0.5,
                "{anchor:?} at {width}x{height}: {:?} != {expected:?}",
                rect.center()
            );
            assert!((rect.size() - size).length() < 0.5, "{anchor:?} size");
        }
    }
}

/// Where the anchored panel's centre must land, spelled out independently of
/// the implementation.
fn expected_center(anchor: NineAnchor, window: Vec2, size: Vec2, offset: Vec2) -> Vec2 {
    let x = match anchor {
        NineAnchor::TopLeft | NineAnchor::Left | NineAnchor::BottomLeft => offset.x + size.x * 0.5,
        NineAnchor::Top | NineAnchor::Center | NineAnchor::Bottom => window.x * 0.5 + offset.x,
        NineAnchor::TopRight | NineAnchor::Right | NineAnchor::BottomRight => {
            window.x + offset.x - size.x * 0.5
        }
    };
    let y = match anchor {
        NineAnchor::TopLeft | NineAnchor::Top | NineAnchor::TopRight => offset.y + size.y * 0.5,
        NineAnchor::Left | NineAnchor::Center | NineAnchor::Right => window.y * 0.5 + offset.y,
        NineAnchor::BottomLeft | NineAnchor::Bottom | NineAnchor::BottomRight => {
            window.y + offset.y - size.y * 0.5
        }
    };
    Vec2::new(x, y)
}

#[test]
fn the_crosshair_hides_while_a_screen_is_open() {
    let mut h = harness();
    h.step(2);
    let crosshair = h.find(&by::hud_layer("crosshair"));
    assert_eq!(
        h.world().get::<Visibility>(crosshair),
        Some(&Visibility::Inherited)
    );
    h.open_screen(one_slot_screen(), MenuDef::generic(9));
    h.step(1);
    assert_eq!(
        h.world().get::<Visibility>(crosshair),
        Some(&Visibility::Hidden)
    );
}

/// A minimal screen: one slot, so opening it costs nothing but a `ScreenRoot`.
fn one_slot_screen() -> slotted_ui::ScreenDef {
    slotted_ui::ScreenDef {
        kind: slotted_ui::ScreenKind::new("test:hud"),
        inherits: None,
        remove: vec![],
        root: UiNodeDef::Slot {
            slot: SlotIx(0),
            tags: Tags::new(),
        },
        listring: Vec::new(),
    }
}

// ---------------------------------------------------------------------------
// The hotbar layer
// ---------------------------------------------------------------------------

#[test]
fn the_hotbar_layer_shows_the_player_hotbar_and_follows_slot_changes() {
    let mut h = harness();
    // No menu, no hotbar: the layer has nothing to bind to.
    h.step(2);
    assert_eq!(h.hud_layer("hotbar"), None);

    let opened = h.open_screen(one_slot_screen(), MenuDef::generic(9));
    // `generic` puts the player's hotbar row at menu slot 36.
    h.world_mut().insert_resource(HudHotbar { first: 36 });
    h.set_hud_menu(Some(opened.menu));
    h.step(2);

    let root = h.hud_layer("hotbar").expect("the hotbar layer is spawned");
    let slots = h.find_all(&by::role(SemanticRole::Slot).within(root));
    assert_eq!(slots.len(), 9);
    let first = slots[0];
    assert_eq!(
        h.world().get::<SlotRef>(first).map(|r| r.slot),
        Some(SlotIx(36))
    );
    assert_eq!(h.stack_at(first), None);

    // The authority fills the slot and syncs it.
    let stack = ItemStack::new(ItemId(1), 5);
    let inventory = opened.inventories[2];
    h.world_mut()
        .get_mut::<Inventory>(inventory)
        .unwrap()
        .set(0, Some(stack.clone()));
    h.world_mut().trigger(SlotChanged {
        entity: first,
        menu: opened.menu,
        slot: SlotIx(36),
        stack: Some(stack.clone()),
    });
    h.step(1);
    assert_eq!(
        h.world()
            .get::<ItemView>(first)
            .and_then(|v| v.stack.clone()),
        Some(stack.clone())
    );
    assert_eq!(h.stack_at(first), Some(stack));
}

// ---------------------------------------------------------------------------
// Script-driven layers
// ---------------------------------------------------------------------------

/// What a mod's `set_hud` and `hud_update` amount to on this side: a payload
/// creates a layer on top, and an update writes into it by `test_id`.
#[test]
fn a_mod_layer_is_created_on_top_and_hud_update_writes_into_it() {
    let mut h = harness();
    let payload: HudLayerPayload = ron::from_str(
        r#"(
            anchor: (anchor: top_right, offset: (-8.0, 8.0)),
            tree: (
                type: "panel",
                role: "hud.panel",
                children: [
                    (type: "text", key: "mana", style: "body", tags: {"test_id": "mana"}),
                ],
            ),
        )"#,
    )
    .expect("the payload parses");
    let (def, placement) = payload.into_def(HudLayerId::new("mod:mana"));
    assert_eq!(placement, slotted_ui::HudPlacement::Top);
    h.world_mut()
        .resource_mut::<HudLayers>()
        .place(&placement, def)
        .unwrap();
    h.step(2);

    let root = h.find(&by::hud_layer("mod:mana"));
    assert_eq!(h.hud_layers().last().copied(), Some(root));
    let label = h.find(&by::test_id("mana"));
    assert_eq!(h.text_of(label).as_deref(), Some("mana"));

    h.world_mut().write_message(HudUpdate {
        layer: HudLayerId::new("mod:mana"),
        path: "mana".to_owned(),
        value: HudValue::Text("40 / 100".to_owned()),
    });
    h.step(1);
    assert_eq!(h.text_of(label).as_deref(), Some("40 / 100"));

    h.world_mut().write_message(HudUpdate {
        layer: HudLayerId::new("mod:mana"),
        path: "mana".to_owned(),
        value: HudValue::Fill {
            value: 40.0,
            max: 100.0,
        },
    });
    h.step(1);
    assert!((h.tank_fill(label) - 0.4).abs() < 1e-6);

    h.world_mut().write_message(HudUpdate {
        layer: HudLayerId::new("mod:mana"),
        path: "mana".to_owned(),
        value: HudValue::Visible(false),
    });
    h.step(1);
    assert_eq!(
        h.world().get::<Visibility>(label),
        Some(&Visibility::Hidden)
    );
}

#[test]
fn a_hidden_layer_stays_hidden_without_a_respawn() {
    let mut h = harness();
    h.world_mut()
        .resource_mut::<HudLayers>()
        .register(panel_layer(
            "mod:mana",
            HudAnchor::default(),
            Vec2::splat(8.0),
        ));
    h.step(2);
    let root = h.find(&by::hud_layer("mod:mana"));

    h.world_mut()
        .resource_mut::<HudLayers>()
        .set_visible(&HudLayerId::new("mod:mana"), false);
    h.step(2);
    assert_eq!(h.hud_layer("mod:mana"), Some(root), "no respawn");
    assert_eq!(h.world().get::<Visibility>(root), Some(&Visibility::Hidden));
}

// ---------------------------------------------------------------------------
// The position editor
// ---------------------------------------------------------------------------

#[cfg(feature = "dev")]
#[test]
fn edit_mode_drags_a_layer_and_the_layout_round_trips() {
    use slotted_ui::hud_editor::{HudEditMode, SNAP};
    use slotted_ui::{HudAnchored, HudLayout};

    let mut h = harness();
    h.world_mut()
        .resource_mut::<HudLayers>()
        .register(panel_layer(
            "mod:mana",
            at(NineAnchor::TopLeft, Vec2::new(100.0, 100.0)),
            Vec2::new(60.0, 40.0),
        ));
    h.step(2);
    let panel = h.find(&by::test_id("mod:mana"));
    let before = h.rect_of(panel);

    h.world_mut().insert_resource(HudEditMode(true));
    h.step(2);

    // The wrapper is the panel's parent; edit mode makes it pickable and
    // outlines it.
    let wrapper = h
        .world()
        .get::<ChildOf>(panel)
        .expect("the panel hangs under the anchor wrapper")
        .parent();
    assert!(h.world().get::<HudAnchored>(wrapper).is_some());
    assert!(
        h.world()
            .get::<Children>(wrapper)
            .expect("the wrapper has children")
            .iter()
            .any(|child| h
                .world()
                .get::<slotted_ui::hud_editor::HudEditFrame>(child)
                .is_some()),
        "edit mode outlines the wrapper"
    );

    // Drag it 13 px right and 9 px down: both snap to the 4 px grid.
    let start = before.center();
    h.pointer_move_to(start);
    h.pointer_press(bevy::picking::pointer::PointerButton::Primary);
    h.pointer_move_to(start + Vec2::new(13.0, 9.0));
    h.pointer_release(bevy::picking::pointer::PointerButton::Primary);
    h.step(2);

    let anchor = *h.world().get::<HudAnchor>(wrapper).expect("wrapper anchor");
    assert_eq!(anchor.offset, Vec2::new(112.0, 108.0));
    assert!(anchor.offset.x % SNAP < f32::EPSILON, "the offset snapped");
    let moved = h.rect_of(panel);
    assert!((moved.center() - (before.center() + Vec2::new(12.0, 8.0))).length() < 0.5);

    // The layout the editor wrote round-trips through RON.
    let layout = h.world().resource::<HudLayout>().clone();
    assert_eq!(
        layout.anchors.get(&HudLayerId::new("mod:mana")).copied(),
        Some(anchor)
    );
    let text = ron::ser::to_string_pretty(&layout, ron::ser::PrettyConfig::default()).unwrap();
    let back: HudLayout = ron::from_str(&text).unwrap();
    assert_eq!(back, layout);

    // Esc during a drag puts the layer back where it started.
    h.pointer_move_to(moved.center());
    h.pointer_press(bevy::picking::pointer::PointerButton::Primary);
    h.pointer_move_to(moved.center() + Vec2::new(40.0, 0.0));
    h.key(KeyCode::Escape);
    h.pointer_release(bevy::picking::pointer::PointerButton::Primary);
    h.step(2);
    assert_eq!(
        h.world().get::<HudAnchor>(wrapper).map(|a| a.offset),
        Some(Vec2::new(112.0, 108.0))
    );
}

/// A layout file written by the editor overrides the def's anchor at spawn.
#[test]
fn a_saved_layout_overrides_the_defs_anchor() {
    let mut h = harness();
    h.world_mut()
        .resource_mut::<HudLayers>()
        .register(panel_layer(
            "mod:mana",
            at(NineAnchor::TopLeft, Vec2::ZERO),
            Vec2::new(20.0, 20.0),
        ));
    let mut layout = slotted_ui::HudLayout::default();
    layout.anchors.insert(
        HudLayerId::new("mod:mana"),
        at(NineAnchor::TopLeft, Vec2::new(64.0, 32.0)),
    );
    h.world_mut().insert_resource(layout);
    h.step(2);
    let panel = h.find(&by::test_id("mod:mana"));
    assert!((h.rect_of(panel).min - Vec2::new(64.0, 32.0)).length() < 0.5);
}
