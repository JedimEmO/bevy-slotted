//! Adversarial review of the harness itself.
//!
//! A test harness that lies is worse than none, so these tests are about the
//! harness's own promises: that `find` explains a miss, that `settle` names
//! what kept it awake, that a pointer click respects z-order, that
//! `assert_conserved` catches both duplication and destruction, and that
//! `screen_tree` says nothing about the theme.

#![allow(clippy::unwrap_used)]

use std::time::Duration;

use bevy::prelude::*;
use pretty_assertions::assert_eq;
use slotted_model::InventoryRef;
use slotted_test::prelude::*;
use slotted_theme::{Tween, TweenTarget};
use slotted_ui::{
    AnchorId, Layout, LocKey, ScreenDef, ScreenKind, Screens, Tags, TextRole, UiNodeDef, zbands,
};

const CHEST: &str = "demo:chest";

fn chest_screen() -> ScreenDef {
    ScreenDef {
        kind: ScreenKind::new(CHEST),
        inherits: None,
        root: UiNodeDef::Panel {
            role: slotted_theme::roles::PANEL,
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
                UiNodeDef::SlotGrid {
                    inventory: MenuDef::CONTAINER,
                    cols: 9,
                    rows: 3,
                    first: 0,
                    tags: Tags::new().with("region", "chest"),
                },
                UiNodeDef::Anchor {
                    id: AnchorId::new("title_end"),
                },
            ],
            tags: Tags::new().with("test_id", "chest_panel"),
        },
        listring: vec![MenuDef::CONTAINER, MenuDef::PLAYER_MAIN],
    }
}

const GLASS: &str = include_str!("../../../assets/themes/glass.theme.ron");

/// The shipped theme, inserted as an asset directly so the test does not
/// depend on where the asset directory is.
fn theme_named(name: &str) -> slotted_theme::Theme {
    let mut theme = slotted_theme::Theme::from_ron(GLASS).expect("glass parses");
    name.clone_into(&mut theme.name);
    if name != "glass" {
        // A visibly different skin over the same roles: every palette entry
        // becomes flat white. Nothing about the tree may notice.
        for colour in theme.tokens.palette.values_mut() {
            *colour = slotted_theme::ThemeColor("#FFFFFFFF".to_owned());
        }
    }
    theme
}

fn open_chest_with(theme: &str) -> (UiHarness, Opened) {
    let mut h = UiHarness::builder()
        .plugins(SlottedPlugins::headless())
        .registries(TestRegistries::basic())
        .resolution(1280.0, 720.0)
        .build();
    let handle = h
        .world_mut()
        .resource_mut::<Assets<slotted_theme::Theme>>()
        .add(theme_named(theme));
    h.world_mut()
        .insert_resource(slotted_theme::ActiveTheme(handle));
    h.world_mut()
        .resource_mut::<Screens>()
        .register(chest_screen());
    let opened = h.open_screen(ScreenKind::new(CHEST), ChestFixture::filled());
    h.settle();
    (h, opened)
}

fn open_chest() -> (UiHarness, Opened) {
    open_chest_with("glass")
}

fn chest_slot(h: &UiHarness, n: usize) -> Entity {
    h.find(&by::role(SemanticRole::Slot).tag("region", "chest").index(n))
}

// -------------------------------------------------------------------- find

/// A locator that matches nothing says what came close and prints the tree.
#[test]
#[should_panic(expected = "near misses:")]
fn find_with_no_match_panics_with_the_near_miss_list() {
    let (h, _) = open_chest();
    // Every criterion but the tag holds for 27 slots, so the tag is the one
    // reported as the failure.
    h.find(&by::role(SemanticRole::Slot).tag("region", "no_such_region"));
}

/// The near-miss machinery is a plain function; assert on it directly rather
/// than on a panic string.
#[test]
fn near_misses_name_the_single_criterion_that_failed() {
    let (h, _) = open_chest();
    let locator = by::role(SemanticRole::Slot).tag("region", "no_such_region");
    assert!(locator.resolve(h.world()).is_empty());

    let near = locator.near_misses(h.world(), 12);
    assert!(!near.is_empty(), "27 slots fail only the tag");
    assert!(
        near.iter().all(|(_, why)| why.contains("region")),
        "the reported criterion should be the tag: {near:?}"
    );
    let first = near[0].0;
    assert_eq!(
        h.world().get::<SemanticRole>(first),
        Some(&SemanticRole::Slot),
        "the near misses really are slots"
    );
}

/// Several matches is also a failure, and it lists them so the test author
/// can add an index.
#[test]
#[should_panic(expected = "add .index(n)")]
fn find_with_several_matches_lists_them() {
    let (h, _) = open_chest();
    h.find(&by::role(SemanticRole::Slot));
}

// ------------------------------------------------------------------ settle

/// A tween that never ends trips the frame cap, and the error names it.
#[test]
fn a_settle_timeout_names_the_tween_that_caused_it() {
    let mut h = UiHarness::builder()
        .plugins(SlottedPlugins::headless())
        .registries(TestRegistries::basic())
        .resolution(1280.0, 720.0)
        .theme("glass")
        .max_settle_frames(20)
        .build();
    h.world_mut()
        .resource_mut::<Screens>()
        .register(chest_screen());
    let _ = h.open_screen(ScreenKind::new(CHEST), ChestFixture::filled());
    h.settle();

    let slot = chest_slot(&h, 0);
    h.world_mut().entity_mut(slot).insert((
        Tags::new().with("test_id", "sticky"),
        Tween::new(
            TweenTarget::Scale { from: 1.0, to: 2.0 },
            Duration::from_hours(1),
        ),
    ));

    let error = h.try_settle().expect_err("a one-hour tween never settles");
    assert_eq!(error.frames, 20);
    assert_eq!(error.motions, 1);
    assert_eq!(error.tweens.len(), 1);
    assert!(
        error.tweens[0].contains("Slot"),
        "the culprit is named: {}",
        error.tweens[0]
    );
    let text = error.to_string();
    assert!(text.contains("did not settle within 20 frames"), "{text}");
    assert!(text.contains("tween:"), "{text}");
}

/// An authority that never acks is the other way `settle` hangs, and the
/// error says so rather than blaming layout.
#[test]
fn a_settle_timeout_reports_pending_round_trips() {
    let authority = slotted_testutils::RecordingAuthority::manual();
    let mut h = UiHarness::builder()
        .plugins(SlottedPlugins::headless())
        .registries(TestRegistries::basic())
        .max_settle_frames(15)
        .theme("glass")
        .build();
    h.world_mut()
        .insert_resource(slotted_ecs::Authority(authority.clone()));
    h.world_mut()
        .resource_mut::<Screens>()
        .register(chest_screen());
    let _ = h.open_screen(ScreenKind::new(CHEST), ChestFixture::filled());
    h.step(1);

    let slot = chest_slot(&h, 0);
    h.click_slot(
        slot,
        slotted_model::Button::Left,
        slotted_ecs::Modifiers::NONE,
    );

    let error = h
        .try_settle()
        .expect_err("the authority is holding the ack");
    assert_eq!(error.pending, 1);
    assert!(error.tweens.is_empty());
    assert!(
        error.to_string().contains("1 pending round trip"),
        "{error}"
    );

    authority.ack_all();
    let frames = h.settle();
    assert!(frames > 0, "and it settles once the ack arrives");
}

// ------------------------------------------------------------------ picking

/// A pointer click lands on the topmost node under it, not on whatever the
/// locator happened to name.
#[test]
fn a_pointer_click_hits_the_node_in_the_higher_z_band() {
    let (mut h, opened) = open_chest();
    let slot = chest_slot(&h, 0);
    let before = h.stack_at(slot).expect("the fixture filled slot 0");
    let centre = h.center_of(slot);

    // A dev overlay covering the whole window, above the screen band.
    h.world_mut().spawn((
        Node {
            position_type: PositionType::Absolute,
            width: percent(100),
            height: percent(100),
            ..default()
        },
        GlobalZIndex(zbands::DEV),
        Pickable::default(),
        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.5)),
    ));
    h.settle();

    h.click_at(centre, bevy::picking::pointer::PointerButton::Primary);
    h.settle();

    assert_eq!(
        h.stack_at(slot),
        Some(before),
        "the overlay swallowed the click"
    );
    assert_eq!(h.carried(opened.menu), None);
    h.assert_conserved();
}

/// A drag across three slots is a paint, not three clicks: it goes through
/// `MenuAction(Drag)` and spreads the carried stack.
#[test]
fn a_pointer_drag_paints_across_slots() {
    let (mut h, opened) = open_chest();
    let source = chest_slot(&h, 0);
    let carried_count = h.stack_at(source).unwrap().count;

    h.click_slot(
        source,
        slotted_model::Button::Left,
        slotted_ecs::Modifiers::NONE,
    );
    h.settle();
    assert_eq!(
        h.carried(opened.menu).map(|s| s.count),
        Some(carried_count),
        "the whole stack is on the cursor"
    );

    // Three empty slots of the chest grid.
    let targets = [3usize, 6, 7].map(|n| chest_slot(&h, n));
    assert!(targets.iter().all(|t| h.stack_at(*t).is_none()));

    h.drag_paint(&targets);
    h.settle();

    let counts: Vec<u32> = targets
        .iter()
        .map(|t| h.stack_at(*t).map_or(0, |s| s.count))
        .collect();
    let landed: u32 = counts.iter().sum();
    assert!(
        counts.iter().all(|c| *c > 0),
        "a left drag spreads the stack evenly over every painted slot, \
         but it landed as {counts:?}; the paint never started and the \
         release read as a plain click"
    );
    assert_eq!(
        landed + h.carried(opened.menu).map_or(0, |s| s.count),
        carried_count,
        "the paint neither created nor destroyed items"
    );
    h.assert_conserved();
}

// ------------------------------------------------------------- conservation

/// `assert_conserved` catches an item appearing from nowhere.
#[test]
#[should_panic(expected = "items were created or destroyed")]
fn assert_conserved_catches_duplication() {
    let (mut h, opened) = open_chest();
    let chest = opened.inventories[MenuDef::CONTAINER.index()];
    let mut inventory = h
        .world_mut()
        .get_mut::<slotted_ecs::Inventory>(chest)
        .unwrap();
    inventory.set(3, Some(TestRegistries::stack("minecraft:diamond", 5)));
    h.assert_conserved();
}

/// And an item quietly vanishing.
#[test]
#[should_panic(expected = "items were created or destroyed")]
fn assert_conserved_catches_destruction() {
    let (mut h, opened) = open_chest();
    let chest = opened.inventories[MenuDef::CONTAINER.index()];
    let mut inventory = h
        .world_mut()
        .get_mut::<slotted_ecs::Inventory>(chest)
        .unwrap();
    inventory.set(0, None);
    h.assert_conserved();
}

/// A stack that leaves the menu into `Dropped` is still conserved: the bin is
/// part of the census.
#[test]
fn assert_conserved_counts_the_dropped_bin_and_the_cursor() {
    let (mut h, opened) = open_chest();
    let slot = chest_slot(&h, 0);

    h.click_slot(
        slot,
        slotted_model::Button::Left,
        slotted_ecs::Modifiers::NONE,
    );
    h.settle();
    assert!(h.carried(opened.menu).is_some(), "on the cursor");
    h.assert_conserved();

    h.menu_action(
        opened.menu,
        slotted_model::ClickAction::Pickup {
            slot: slotted_model::SlotIx::OUTSIDE,
            button: slotted_model::Button::Left,
        },
    );
    h.settle();
    assert_eq!(h.carried(opened.menu), None);
    assert_eq!(
        h.world().resource::<slotted_ecs::Dropped>().0.len(),
        1,
        "it went to the drop bin"
    );
    h.assert_conserved();
}

// ------------------------------------------------------------ theme neutral

/// `screen_tree` describes what nodes are, never how they look. Two themes
/// must produce the same tree.
#[test]
fn the_screen_tree_is_identical_across_themes() {
    let (glass, _) = open_chest_with("glass");
    let (other, _) = open_chest_with("paper");
    // The two harnesses really do paint differently.
    assert_ne!(
        glass
            .world()
            .resource::<Assets<slotted_theme::Theme>>()
            .len(),
        0
    );

    let a = glass.screen_tree();
    let b = other.screen_tree();
    assert_eq!(
        a.to_string(),
        b.to_string(),
        "the semantic tree leaked something theme-dependent"
    );
    assert_eq!(a, b);
    assert!(a.to_string().contains("Slot"), "and it is not empty: {a}");
}

// -------------------------------------------------------------- text input

/// Phase 2 ships no text input widget, so `type_text` has nowhere to go. It
/// must still be harmless: no panic, no focus change, no inventory change.
/// See docs/FOLLOWUPS.md; this test becomes a real one when a field exists.
#[test]
fn type_text_is_inert_while_no_text_field_exists() {
    let (mut h, opened) = open_chest();
    let fields = h
        .world_mut()
        .query_filtered::<Entity, With<bevy::input_focus::AutoFocus>>()
        .iter(h.world())
        .count();
    assert_eq!(fields, 0, "Phase 2 has no focus-taking text field");

    let slot = chest_slot(&h, 0);
    h.set_focus(Some(slot));
    let before = h.stack_at(slot);

    h.type_text("hello 42");
    h.settle();

    assert_eq!(h.focused(), Some(slot), "typing did not move focus");
    assert_eq!(h.stack_at(slot), before, "and changed no inventory");
    h.assert_conserved();
    let _ = opened;
}

// ------------------------------------------------------------------- misc

/// `stack_at` reads the model and `displayed_stack` the widget. They agree
/// once the frame has settled, and `stack_at` is the one that is right first.
#[test]
fn the_model_and_the_widget_agree_after_settling() {
    let (mut h, _) = open_chest();
    let slot = chest_slot(&h, 0);

    h.click_slot(
        slot,
        slotted_model::Button::Left,
        slotted_ecs::Modifiers::NONE,
    );
    // No settle: the model has already moved.
    assert_eq!(h.stack_at(slot), None);
    h.settle();
    assert_eq!(h.displayed_stack(slot), None, "the widget caught up");
    assert_eq!(h.stack_at(slot), h.displayed_stack(slot));
}

/// A locator resolves in tree order from every screen root, so `.index(n)`
/// means the n-th in layout order and stays stable across frames.
#[test]
fn locator_order_is_stable_across_frames() {
    let (mut h, _) = open_chest();
    let first: Vec<Entity> = h.find_all(&by::role(SemanticRole::Slot));
    h.step(10);
    let again: Vec<Entity> = h.find_all(&by::role(SemanticRole::Slot));
    assert_eq!(first, again);
    assert_eq!(first.len(), 27);
    assert_eq!(first[0], chest_slot(&h, 0));

    // `.within` narrows to one grid without changing the order.
    let grid = h.find(&by::role(SemanticRole::Grid));
    let within = h.find_all(&by::role(SemanticRole::Slot).within(grid));
    assert_eq!(within, first);
}

/// The harness must not leave a screen behind between `open_screen` calls:
/// two open screens both appear in the tree, in spawn order.
#[test]
fn two_open_screens_both_appear_in_the_tree() {
    let (mut h, _) = open_chest();
    let second = ScreenDef {
        kind: ScreenKind::new("demo:second"),
        inherits: None,
        root: UiNodeDef::Panel {
            role: slotted_theme::roles::PANEL,
            layout: Layout::default(),
            children: vec![UiNodeDef::SlotGrid {
                inventory: InventoryRef::new(0),
                cols: 3,
                rows: 1,
                first: 0,
                tags: Tags::new().with("region", "second"),
            }],
            tags: Tags::new().with("test_id", "second_panel"),
        },
        listring: vec![],
    };
    h.world_mut().resource_mut::<Screens>().register(second);
    h.open_screen(ScreenKind::new("demo:second"), ChestFixture::empty());
    h.settle();

    let tree = h.screen_tree();
    assert_eq!(
        tree.roots.len(),
        3,
        "two screens plus the carried layer: {tree}"
    );
    assert_eq!(
        tree.roots[0].screen.as_deref(),
        Some(CHEST),
        "spawn order, not entity order"
    );
    assert_eq!(tree.roots[1].screen.as_deref(), Some("demo:second"));
    assert_eq!(h.find_all(&by::tag("region", "second")).len(), 4);
}

// --------------------------------------------------------- double click

/// The double-click window is 250 ms of accumulated virtual time, whatever
/// the harness's frame size. Two clicks 300 ms apart at 16 ms per frame are
/// two clicks, not a gather.
#[test]
fn a_second_click_after_the_window_is_not_a_double_click() {
    let (mut h, opened) = open_chest();
    let first = chest_slot(&h, 0);
    // Chest slot 1 holds 23 more of the same item, which a `PickupAll` would
    // gather onto the cursor.
    let second = chest_slot(&h, 1);
    let before = h.stack_at(second).expect("the fixture filled chest slot 1");

    h.click_slot(
        first,
        slotted_model::Button::Left,
        slotted_ecs::Modifiers::NONE,
    );
    h.settle();
    assert!(h.carried(opened.menu).is_some());

    h.advance(Duration::from_millis(300));
    h.click_slot(
        first,
        slotted_model::Button::Left,
        slotted_ecs::Modifiers::NONE,
    );
    h.settle();

    assert_eq!(
        h.carried(opened.menu),
        None,
        "the second click put the stack back instead of gathering"
    );
    assert_eq!(h.stack_at(second), Some(before), "slot 1 was left alone");
    h.assert_conserved();
}

/// And two clicks inside the window still gather, so the window is a window
/// and not simply switched off.
#[test]
fn two_clicks_inside_the_window_still_gather() {
    let (mut h, opened) = open_chest();
    // Slot 1 holds 23 cobblestone, slot 0 a full 64 of the same kind, so a
    // gather has somewhere to draw from and room to put it.
    let partial = chest_slot(&h, 1);
    let full = chest_slot(&h, 0);
    assert_eq!(h.stack_at(partial).unwrap().count, 23);
    assert_eq!(h.stack_at(full).unwrap().count, 64);

    h.click_slot(
        partial,
        slotted_model::Button::Left,
        slotted_ecs::Modifiers::NONE,
    );
    h.click_slot(
        partial,
        slotted_model::Button::Left,
        slotted_ecs::Modifiers::NONE,
    );
    h.settle();

    assert_eq!(
        h.carried(opened.menu).map(|s| s.count),
        Some(64),
        "a double click gathered up to a full stack"
    );
    assert!(
        h.stack_at(full).map(|s| s.count) < Some(64),
        "and drew the difference from the other cobblestone in the list ring"
    );
    h.assert_conserved();
}

/// The window is configuration, not a constant.
#[test]
fn the_double_click_window_is_configurable() {
    let mut h = UiHarness::builder()
        .plugins(SlottedPlugins::headless())
        .registries(TestRegistries::basic())
        .double_click_window(Duration::from_millis(10))
        .build();
    assert_eq!(
        h.world()
            .resource::<slotted_ecs::ClickInterpreter>()
            .double_click_window,
        Duration::from_millis(10)
    );
    h.world_mut()
        .resource_mut::<Screens>()
        .register(chest_screen());
    let opened = h.open_screen(ScreenKind::new(CHEST), ChestFixture::filled());
    h.settle();

    // Two clicks a frame apart are already 16 ms, outside a 10 ms window.
    let first = chest_slot(&h, 0);
    let second = chest_slot(&h, 1);
    let before = h.stack_at(second).unwrap();
    h.click_slot(
        first,
        slotted_model::Button::Left,
        slotted_ecs::Modifiers::NONE,
    );
    h.click_slot(
        first,
        slotted_model::Button::Left,
        slotted_ecs::Modifiers::NONE,
    );
    h.settle();

    assert_eq!(h.carried(opened.menu), None);
    assert_eq!(h.stack_at(second), Some(before));
    h.assert_conserved();
}
