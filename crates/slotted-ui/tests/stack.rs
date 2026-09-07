//! Headless tests for the screen stack (menus contract 3.1 to 3.4): push and
//! pop order, `StackChanged`, menu close on pop with item conservation,
//! page and modal presentation, overlays, `Back`, focus scope and restore,
//! `close_screen` on a stacked root, `pop_to`, and HUD visibility.
//!
//! The harness only builds the app and steps frames; every stack operation
//! goes through the `slotted_ui` commands directly, and `Back` is written as
//! a `UiActionEvent` rather than pressed, so nothing here depends on the
//! action emitter.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::float_cmp)]

use std::collections::BTreeMap;
use std::sync::Arc;

use bevy::input_focus::{FocusCause, InputFocus};
use bevy::prelude::*;
use pretty_assertions::assert_eq;
use slotted_ecs::{Dropped, Inventory, MenuIdAllocator, OpenMenu, open_menu};
use slotted_model::{Actor, ItemId, ItemStack, MenuDef};
use slotted_test::prelude::*;
use slotted_theme::{Motion, Tween};
use slotted_ui::def::{
    BackPolicy, LocKey, Presentation, PresentationMode, ScreenDef, ScreenKind, Tags, TextRole,
    Transition, UiNodeDef,
};
use slotted_ui::zbands;
use slotted_ui::{
    InputDevice, Layout, ScreenRoot, ScreenStack, Screens, Scrim, SlottedUiSet, StackChanged,
    UiAction, UiActionClaims, UiActionEmit, UiActionEvent, clear_screens, close_screen, pop_screen,
    pop_to, push_screen, spawn_screen,
};

const WIDTH: f32 = 1280.0;
const HEIGHT: f32 = 720.0;

/// Whether the test's own `Input` system claims `Back` this frame.
#[derive(Resource, Default)]
struct ClaimBack(bool);

/// Every `StackChanged` the app wrote, in order.
#[derive(Resource, Default)]
struct Changes(Vec<Vec<ScreenKind>>);

fn claim_back(flag: Res<ClaimBack>, mut claims: ResMut<UiActionClaims>) {
    if flag.0 {
        claims.claim(UiAction::Back);
    } else {
        claims.clear();
    }
}

fn record_changes(mut events: MessageReader<StackChanged>, mut out: ResMut<Changes>) {
    for change in events.read() {
        out.0.push(change.kinds.clone());
    }
}

/// The test's own claim switch and `StackChanged` recorder.
struct StackTestPlugin;

impl Plugin for StackTestPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ClaimBack>()
            .init_resource::<Changes>()
            .add_systems(
                Update,
                (
                    claim_back.in_set(SlottedUiSet::Input).after(UiActionEmit),
                    record_changes.in_set(SlottedUiSet::Semantics),
                ),
            );
    }
}

fn harness_with(motion: Motion) -> UiHarness {
    UiHarness::builder()
        .plugins((SlottedPlugins::headless(), StackTestPlugin))
        .resolution(WIDTH, HEIGHT)
        .theme("glass")
        .motion(motion)
        .build()
}

fn harness() -> UiHarness {
    harness_with(Motion::REDUCED)
}

/// A screen of `kind`: a panel with one text node tagged `<kind>.label`,
/// so each screen has a distinct node focus can sit on.
fn screen(kind: &str, presentation: Presentation) -> ScreenDef {
    ScreenDef {
        kind: ScreenKind::new(kind),
        initial_focus: None,
        presentation,
        inherits: None,
        remove: vec![],
        listring: vec![],
        root: UiNodeDef::Panel {
            role: slotted_theme::roles::PANEL,
            layout: Layout {
                padding: 2.0.into(),
                ..Layout::default()
            },
            children: vec![UiNodeDef::Text {
                key: LocKey(format!("{kind}.label")),
                style: TextRole::Body,
                tags: Tags::new().with(Tags::TEST_ID, &format!("{kind}.label")),
            }],
            tags: Tags::new().with(Tags::TEST_ID, kind),
        },
    }
}

fn page() -> Presentation {
    Presentation::default()
}

fn modal() -> Presentation {
    Presentation {
        mode: PresentationMode::Modal,
        ..Presentation::default()
    }
}

fn overlay() -> Presentation {
    Presentation {
        mode: PresentationMode::Overlay,
        ..Presentation::default()
    }
}

fn kind(name: &str) -> ScreenKind {
    ScreenKind::new(name)
}

fn kinds(names: &[&str]) -> Vec<ScreenKind> {
    names.iter().map(|n| kind(n)).collect()
}

fn register(h: &mut UiHarness, def: ScreenDef) -> Arc<ScreenDef> {
    h.world_mut().resource_mut::<Screens>().register(def)
}

fn push(h: &mut UiHarness, name: &str, presentation: Presentation) -> Entity {
    let def = register(h, screen(name, presentation));
    let root = {
        let world = h.world_mut();
        let mut commands = world.commands();
        push_screen(&mut commands, def, None)
    };
    h.world_mut().flush();
    h.step(1);
    root
}

fn pop(h: &mut UiHarness) {
    pop_screen(&mut h.world_mut().commands());
    h.world_mut().flush();
    h.step(1);
}

fn stack_kinds(h: &UiHarness) -> Vec<ScreenKind> {
    h.world().resource::<ScreenStack>().kinds()
}

fn press_back(h: &mut UiHarness) {
    h.world_mut().write_message(UiActionEvent {
        action: UiAction::Back,
        device: InputDevice::Keyboard,
        repeat: false,
    });
    h.step(1);
}

fn focus(h: &mut UiHarness, entity: Entity) {
    h.world_mut()
        .resource_mut::<InputFocus>()
        .set(entity, FocusCause::Navigated);
    h.step(1);
}

fn focused(h: &UiHarness) -> Option<Entity> {
    h.world().resource::<InputFocus>().get()
}

fn label_of(h: &UiHarness, name: &str) -> Entity {
    h.find(&by::test_id(&format!("{name}.label")))
}

fn visibility(h: &UiHarness, entity: Entity) -> Visibility {
    *h.world().get::<Visibility>(entity).expect("a node")
}

fn z_of(h: &UiHarness, entity: Entity) -> i32 {
    h.world().get::<GlobalZIndex>(entity).expect("a z index").0
}

fn scrims(h: &mut UiHarness) -> Vec<(Entity, Scrim)> {
    let mut q = h.world_mut().query::<(Entity, &Scrim)>();
    q.iter(h.world()).map(|(e, s)| (e, *s)).collect()
}

/// Every item in the world, per kind: inventories, carried stacks, dropped.
/// The same count `slotted-test` conserves over `open_screen`.
fn census(world: &mut World) -> BTreeMap<ItemId, u64> {
    let mut out: BTreeMap<ItemId, u64> = BTreeMap::new();
    let mut add = |stack: &ItemStack| {
        *out.entry(stack.id).or_default() += u64::from(stack.count);
    };
    for inventory in world.query::<&Inventory>().iter(world) {
        for i in 0..inventory.len() {
            if let Some(stack) = inventory.get(i) {
                add(stack);
            }
        }
    }
    for menu in world.query::<&OpenMenu>().iter(world) {
        if let Some(stack) = menu.state.carried.as_ref() {
            add(stack);
        }
    }
    if let Some(dropped) = world.get_resource::<Dropped>() {
        for entry in &dropped.0 {
            add(&entry.stack);
        }
    }
    out
}

#[test]
fn push_and_pop_keep_order_and_write_stack_changed() {
    let mut h = harness();
    push(&mut h, "t:a", page());
    push(&mut h, "t:b", page());
    assert_eq!(stack_kinds(&h), kinds(&["t:a", "t:b"]));
    assert!(h.world().resource::<ScreenStack>().is_open(&kind("t:a")));
    assert_eq!(
        h.world().resource::<ScreenStack>().top().map(|e| &e.kind),
        Some(&kind("t:b"))
    );

    pop(&mut h);
    assert_eq!(stack_kinds(&h), kinds(&["t:a"]));
    pop(&mut h);
    assert_eq!(stack_kinds(&h), Vec::<ScreenKind>::new());
    assert!(h.world().resource::<ScreenStack>().top().is_none());

    // A pop on an empty stack is silent.
    pop(&mut h);
    assert_eq!(
        h.world().resource::<Changes>().0,
        vec![
            kinds(&["t:a"]),
            kinds(&["t:a", "t:b"]),
            kinds(&["t:a"]),
            kinds(&[]),
        ]
    );
}

#[test]
fn pop_closes_the_menu_and_the_carried_stack_returns() {
    let mut h = harness();
    let def = Arc::new(MenuDef::generic(9));
    let sizes = def.inventory_sizes();
    let screen_def = register(&mut h, screen("t:chest", page()));
    let world = h.world_mut();
    let inventories: Vec<Entity> = sizes
        .iter()
        .map(|n| world.spawn(Inventory::new(*n)).id())
        .collect();
    world
        .get_mut::<Inventory>(inventories[0])
        .unwrap()
        .set(0, Some(ItemStack::new(ItemId(1), 5)));
    let mut ids = world
        .remove_resource::<MenuIdAllocator>()
        .unwrap_or_default();
    let (menu, root) = {
        let mut commands = world.commands();
        let menu = open_menu(&mut commands, &mut ids, def, inventories, Actor::SURVIVAL);
        let root = push_screen(&mut commands, screen_def, Some(menu));
        (menu, root)
    };
    world.insert_resource(ids);
    world.flush();
    // Something on the cursor, as if the player picked it up and pressed
    // Escape mid-gesture.
    world.get_mut::<OpenMenu>(menu).unwrap().state.carried = Some(ItemStack::new(ItemId(2), 3));
    h.step(1);
    let before = census(h.world_mut());
    assert_eq!(before.get(&ItemId(2)), Some(&3));
    assert_eq!(
        h.world()
            .resource::<ScreenStack>()
            .entry(root)
            .map(|e| e.menu),
        Some(Some(menu))
    );

    pop(&mut h);

    assert!(
        h.world().get_entity(root).is_err(),
        "the screen root is gone"
    );
    assert!(h.world().get_entity(menu).is_err(), "the menu is closed");
    let dropped: Vec<ItemStack> = h.world().resource::<Dropped>().of(menu).cloned().collect();
    assert_eq!(dropped, vec![ItemStack::new(ItemId(2), 3)]);
    assert_eq!(
        census(h.world_mut()),
        before,
        "items are conserved over a pop"
    );
}

#[test]
fn a_page_hides_the_entry_below_and_pop_reveals_it() {
    let mut h = harness();
    let a = push(&mut h, "t:a", page());
    assert_eq!(visibility(&h, a), Visibility::Inherited);
    assert_eq!(z_of(&h, a), zbands::SCREEN);

    let b = push(&mut h, "t:b", page());
    assert_eq!(visibility(&h, a), Visibility::Hidden);
    assert_eq!(visibility(&h, b), Visibility::Inherited);
    assert_eq!(z_of(&h, b), zbands::SCREEN + 2);
    assert!(
        h.world().get_entity(a).is_ok(),
        "a hidden entry keeps its state"
    );
    assert!(scrims(&mut h).is_empty(), "a page draws no scrim");

    pop(&mut h);
    assert_eq!(visibility(&h, a), Visibility::Inherited);
}

#[test]
fn a_modal_keeps_the_entry_below_visible_behind_a_scrim() {
    let mut h = harness();
    let a = push(&mut h, "t:a", page());
    let b = push(&mut h, "t:b", modal());
    assert_eq!(visibility(&h, a), Visibility::Inherited);
    assert_eq!(visibility(&h, b), Visibility::Inherited);

    let scrims = scrims(&mut h);
    assert_eq!(scrims.len(), 1);
    let (scrim, marker) = scrims[0];
    assert_eq!(marker.for_root, b);
    assert!(
        z_of(&h, a) < z_of(&h, scrim) && z_of(&h, scrim) < z_of(&h, b),
        "the scrim sits between the page and the modal"
    );
    assert_eq!(z_of(&h, scrim), zbands::SCREEN + 1);
    assert!(
        h.world()
            .get::<Pickable>(scrim)
            .is_some_and(|p| p.should_block_lower),
        "the scrim blocks the pointer"
    );
    let rect = h.rect_of(scrim);
    assert_eq!(rect.size(), Vec2::new(WIDTH, HEIGHT), "full window");

    pop(&mut h);
    assert!(
        h.world().get_entity(scrim).is_err(),
        "the scrim goes with its entry"
    );
    assert!(h.world().get_entity(b).is_err());
}

#[test]
fn an_overlay_takes_no_focus_and_back_pops_the_screen_under_it() {
    let mut h = harness();
    push(&mut h, "t:a", page());
    let label = label_of(&h, "t:a");
    focus(&mut h, label);
    assert_eq!(focused(&h), Some(label));

    let overlay_root = push(&mut h, "t:toast", overlay());
    assert_eq!(focused(&h), Some(label), "focus stays on the page");
    assert_eq!(
        h.world().resource::<ScreenStack>().top().map(|e| e.root),
        Some(h.find(&by::screen(kind("t:a")))),
        "top() skips the overlay"
    );
    assert_eq!(
        h.world()
            .resource::<ScreenStack>()
            .top_any()
            .map(|e| e.root),
        Some(overlay_root)
    );

    press_back(&mut h);
    assert_eq!(stack_kinds(&h), kinds(&["t:toast"]));
    assert!(h.world().get_entity(overlay_root).is_ok());
}

#[test]
fn pop_on_back_respects_ignore_and_a_claim() {
    let mut h = harness();
    push(
        &mut h,
        "t:root",
        Presentation {
            back: BackPolicy::Ignore,
            ..Presentation::default()
        },
    );
    press_back(&mut h);
    assert_eq!(stack_kinds(&h), kinds(&["t:root"]), "`ignore` keeps it");

    push(&mut h, "t:dialog", page());
    h.world_mut().resource_mut::<ClaimBack>().0 = true;
    press_back(&mut h);
    assert_eq!(
        stack_kinds(&h),
        kinds(&["t:root", "t:dialog"]),
        "a claimed Back is somebody else's"
    );

    h.world_mut().resource_mut::<ClaimBack>().0 = false;
    press_back(&mut h);
    assert_eq!(stack_kinds(&h), kinds(&["t:root"]));

    // Two Backs in one frame pop once.
    push(&mut h, "t:one", page());
    push(&mut h, "t:two", page());
    h.world_mut().write_message(UiActionEvent {
        action: UiAction::Back,
        device: InputDevice::Keyboard,
        repeat: false,
    });
    press_back(&mut h);
    assert_eq!(stack_kinds(&h), kinds(&["t:root", "t:one"]));
}

#[test]
fn back_leaves_a_screen_outside_the_stack_alone() {
    let mut h = harness();
    let def = register(&mut h, screen("t:direct", page()));
    let root = spawn_screen(&mut h.world_mut().commands(), def, None);
    h.world_mut().flush();
    h.step(1);
    assert_eq!(stack_kinds(&h), kinds(&[]));
    press_back(&mut h);
    assert!(h.world().get_entity(root).is_ok());
}

#[test]
fn enforce_focus_scope_bounces_focus_back_to_the_top() {
    let mut h = harness();
    push(&mut h, "t:a", page());
    let a_label = label_of(&h, "t:a");
    push(&mut h, "t:b", page());
    let b_label = label_of(&h, "t:b");
    focus(&mut h, b_label);
    assert_eq!(
        h.world()
            .resource::<ScreenStack>()
            .top()
            .and_then(|e| e.focus),
        Some(b_label),
        "record_stack_focus wrote the entry"
    );

    // Something (a stray click, a stale navigator) moves focus into the
    // hidden page.
    focus(&mut h, a_label);
    assert_eq!(focused(&h), Some(b_label), "bounced to the top's focus");
}

#[test]
fn focus_outside_the_stack_is_never_corrected() {
    let mut h = harness();
    push(&mut h, "t:a", page());
    let def = register(&mut h, screen("t:direct", page()));
    spawn_screen(&mut h.world_mut().commands(), def, None);
    h.world_mut().flush();
    h.step(1);
    let direct = label_of(&h, "t:direct");
    focus(&mut h, direct);
    assert_eq!(focused(&h), Some(direct));
}

#[test]
fn focus_is_restored_on_pop() {
    let mut h = harness();
    push(&mut h, "t:a", page());
    let a_label = label_of(&h, "t:a");
    focus(&mut h, a_label);

    push(&mut h, "t:b", page());
    let b_label = label_of(&h, "t:b");
    focus(&mut h, b_label);
    assert_eq!(focused(&h), Some(b_label));

    pop(&mut h);
    assert_eq!(focused(&h), Some(a_label));
}

#[test]
fn a_pop_with_nothing_left_clears_a_dead_focus() {
    let mut h = harness();
    push(&mut h, "t:a", page());
    let a_label = label_of(&h, "t:a");
    focus(&mut h, a_label);
    pop(&mut h);
    assert_eq!(focused(&h), None);
}

#[test]
fn close_screen_on_a_stacked_root_removes_the_entry() {
    let mut h = harness();
    let a = push(&mut h, "t:a", page());
    let b = push(&mut h, "t:b", modal());
    assert_eq!(scrims(&mut h).len(), 1);

    close_screen(&mut h.world_mut().commands(), b);
    h.world_mut().flush();
    h.step(1);
    assert_eq!(stack_kinds(&h), kinds(&["t:a"]));
    assert!(scrims(&mut h).is_empty(), "the scrim went with the entry");
    assert_eq!(visibility(&h, a), Visibility::Inherited);
    assert_eq!(
        h.world().resource::<Changes>().0.last(),
        Some(&kinds(&["t:a"]))
    );
}

#[test]
fn pop_to_pops_until_the_kind_is_on_top_and_ignores_an_absent_kind() {
    let mut h = harness();
    push(&mut h, "t:a", page());
    push(&mut h, "t:b", page());
    push(&mut h, "t:c", overlay());
    push(&mut h, "t:d", page());
    let changes_before = h.world().resource::<Changes>().0.len();

    pop_to(&mut h.world_mut().commands(), &kind("t:missing"));
    h.world_mut().flush();
    h.step(1);
    assert_eq!(stack_kinds(&h), kinds(&["t:a", "t:b", "t:c", "t:d"]));
    assert_eq!(
        h.world().resource::<Changes>().0.len(),
        changes_before,
        "a no-op writes nothing"
    );

    pop_to(&mut h.world_mut().commands(), &kind("t:b"));
    h.world_mut().flush();
    h.step(1);
    assert_eq!(stack_kinds(&h), kinds(&["t:a", "t:b"]));
    assert_eq!(
        h.world().resource::<Changes>().0.len(),
        changes_before + 1,
        "one message for the whole pop_to"
    );
}

#[test]
fn clear_screens_empties_the_stack() {
    let mut h = harness();
    let a = push(&mut h, "t:a", page());
    let b = push(&mut h, "t:b", modal());
    clear_screens(&mut h.world_mut().commands());
    h.world_mut().flush();
    h.step(1);
    assert_eq!(stack_kinds(&h), kinds(&[]));
    assert!(h.world().get_entity(a).is_err());
    assert!(h.world().get_entity(b).is_err());
    assert!(scrims(&mut h).is_empty());
    let mut roots = h.world_mut().query::<&ScreenRoot>();
    assert_eq!(roots.iter(h.world()).count(), 0);
}

#[test]
fn the_hud_hides_for_pages_and_modals_but_not_overlays() {
    let mut h = harness();
    h.step(2);
    let crosshair = h.find(&by::hud_layer("crosshair"));
    assert_eq!(visibility(&h, crosshair), Visibility::Inherited);

    push(&mut h, "t:toast", overlay());
    h.step(1);
    assert_eq!(
        visibility(&h, crosshair),
        Visibility::Inherited,
        "an overlay leaves the HUD alone"
    );

    push(&mut h, "t:pause", modal());
    h.step(1);
    assert_eq!(visibility(&h, crosshair), Visibility::Hidden);
    pop(&mut h);
    h.step(1);
    assert_eq!(visibility(&h, crosshair), Visibility::Inherited);

    // A screen spawned outside the stack still counts as a page.
    let def = register(&mut h, screen("t:direct", page()));
    let direct = spawn_screen(&mut h.world_mut().commands(), def, None);
    h.world_mut().flush();
    h.step(2);
    assert_eq!(visibility(&h, crosshair), Visibility::Hidden);
    close_screen(&mut h.world_mut().commands(), direct);
    h.world_mut().flush();
    h.step(2);
    assert_eq!(visibility(&h, crosshair), Visibility::Inherited);
}

#[test]
fn a_fade_push_tweens_the_panel_and_settles() {
    let mut h = harness_with(Motion::default());
    let def = register(&mut h, screen("t:a", page()));
    let root = push_screen(&mut h.world_mut().commands(), def, None);
    h.world_mut().flush();
    h.step(1);
    let panel = h.find(&by::test_id("t:a"));
    let tween = h.world().get::<Tween>(panel).expect("the panel fades in");
    assert!(
        matches!(
            tween.target,
            slotted_theme::TweenTarget::Alpha { from, .. } if from == 0.0
        ),
        "{tween:?}"
    );
    assert!(
        h.world().get::<Tween>(root).is_none(),
        "a fade does not move the root"
    );
    let frames = h.settle();
    assert!(frames < 60, "the push converged in {frames} frames");
    assert!(h.world().get::<Tween>(panel).is_none());
}

#[test]
fn a_slide_push_moves_the_root_from_spacing_xl_and_reduced_motion_collapses_it() {
    let mut h = harness_with(Motion::default());
    let def = register(
        &mut h,
        screen(
            "t:a",
            Presentation {
                transition: Transition::SlideUp,
                ..Presentation::default()
            },
        ),
    );
    let root = push_screen(&mut h.world_mut().commands(), def, None);
    h.world_mut().flush();
    h.step(1);
    let tween = h.world().get::<Tween>(root).expect("the root slides");
    let slotted_theme::TweenTarget::Translate { from, to } = tween.target else {
        panic!("{tween:?}");
    };
    assert_eq!(to, Vec2::ZERO);
    assert!(
        from.x == 0.0 && from.y > 0.0,
        "slide_up starts below: {from}"
    );
    h.settle();
    let transform = h
        .world()
        .get::<bevy::ui::ui_transform::UiTransform>(root)
        .expect("the tween wrote a transform");
    assert_eq!(
        transform.translation,
        bevy::ui::ui_transform::Val2::px(0.0, 0.0)
    );

    let mut reduced = harness();
    let def = register(
        &mut reduced,
        screen(
            "t:b",
            Presentation {
                transition: Transition::SlideLeft,
                ..Presentation::default()
            },
        ),
    );
    let root = push_screen(&mut reduced.world_mut().commands(), def, None);
    reduced.world_mut().flush();
    reduced.step(1);
    assert!(
        reduced.world().get::<Tween>(root).is_none(),
        "reduced motion collapses a slide to a fade"
    );
    let frames = reduced.settle();
    assert!(frames <= 3, "reduced motion settles at once, took {frames}");
}

#[test]
fn a_transition_of_none_starts_nothing() {
    let mut h = harness_with(Motion::default());
    let def = register(
        &mut h,
        screen(
            "t:a",
            Presentation {
                transition: Transition::None,
                ..Presentation::default()
            },
        ),
    );
    let root = push_screen(&mut h.world_mut().commands(), def, None);
    h.world_mut().flush();
    h.step(1);
    let mut tweens = h.world_mut().query::<&Tween>();
    assert_eq!(tweens.iter(h.world()).count(), 0);
    assert!(h.world().get_entity(root).is_ok());
}
