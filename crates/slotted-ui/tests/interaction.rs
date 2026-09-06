//! The four interaction reports from playing `just run-chest` and
//! `just run-machine` with a real mouse, each pinned by the behaviour that
//! was actually wrong.
//!
//! 1. The quick-move flight outlasted the gesture it decorated.
//! 2. Tooltips waited too long and then appeared at the corner of the screen
//!    before snapping into place.
//! 3. A click whose pointer drifted a few pixels was read as a drag paint and
//!    picked nothing up, and a click that put a stack down could not pick it
//!    straight back up.
//! 4. Opening a side tab moved the panel it sat beside.
//!
//! Everything here goes through the harness the way a player goes through a
//! mouse: real pointer input, real virtual time, no reduced motion.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::time::Duration;

use bevy::prelude::*;
use slotted_model::{ClickAction, InventoryRef, MenuDef, ToolbarAction};
use slotted_test::prelude::*;
use slotted_theme::{Motion, MotionPreset, Tween};
use slotted_ui::def::{Layout, Tags, UiNodeDef};
use slotted_ui::{
    FlyingItem, IconDef, LocKey, ScreenDef, Screens, SemanticRole, SideTabPanel, SideTabState,
    TextRole, TooltipContent, TooltipUnplaced,
};

const CHEST: &str = "interaction:chest";
const MACHINE: &str = "interaction:machine";
const WIDTH: f32 = 1280.0;
const HEIGHT: f32 = 720.0;

/// The motion budget a flight has to live inside, from the moodboard: a
/// single fly is 200-260 ms and the default is well under it.
const FLY_BUDGET: Duration = Duration::from_millis(260);

// ---------------------------------------------------------------------------
// Screens
// ---------------------------------------------------------------------------

fn grid(inventory: InventoryRef, rows: u16, first: u16, region: &str) -> UiNodeDef {
    UiNodeDef::SlotGrid {
        inventory,
        cols: 9,
        rows,
        first,
        tags: Tags::new().with("region", region),
    }
}

/// A chest and a player inventory on one screen, so a quick-move has a real
/// destination slot to fly to.
fn chest_screen() -> ScreenDef {
    ScreenDef {
        kind: ScreenKind::new(CHEST),
        inherits: None,
        root: UiNodeDef::Panel {
            role: slotted_theme::roles::PANEL,
            layout: Layout {
                gap: 6.0,
                padding: 8.0,
                ..Layout::default()
            },
            children: vec![
                grid(MenuDef::CONTAINER, 3, 0, "chest"),
                grid(MenuDef::PLAYER_MAIN, 3, 27, "player"),
            ],
            tags: Tags::new().with("test_id", "chest_panel"),
        },
        listring: vec![MenuDef::CONTAINER, MenuDef::PLAYER_MAIN],
    }
}

/// A panel with a tab rail beside it, the shape of the machine screen.
fn machine_screen() -> ScreenDef {
    ScreenDef {
        kind: ScreenKind::new(MACHINE),
        inherits: None,
        root: UiNodeDef::Panel {
            role: slotted_theme::roles::PANEL,
            layout: Layout {
                direction: slotted_ui::def::LayoutDirection::Row,
                gap: 0.0,
                ..Layout::default()
            },
            children: vec![
                UiNodeDef::Panel {
                    role: slotted_theme::roles::PANEL,
                    layout: Layout {
                        gap: 6.0,
                        padding: 8.0,
                        ..Layout::default()
                    },
                    children: vec![grid(MenuDef::CONTAINER, 3, 0, "chest")],
                    tags: Tags::new().with("test_id", "panel"),
                },
                UiNodeDef::Panel {
                    role: slotted_theme::roles::TAB_RAIL,
                    layout: Layout::default(),
                    children: vec![tab("tab_a"), tab("tab_b")],
                    tags: Tags::new().with("test_id", "rail"),
                },
            ],
            tags: Tags::new().with("test_id", "machine_root"),
        },
        listring: vec![MenuDef::CONTAINER],
    }
}

fn tab(test_id: &str) -> UiNodeDef {
    UiNodeDef::SideTab {
        icon: IconDef::Image("icons/tab.png".to_owned()),
        side: slotted_ui::Side::Right,
        label: Some(LocKey(format!("tab.{test_id}"))),
        open: false,
        children: vec![UiNodeDef::Text {
            key: LocKey("a body wide enough to notice".to_owned()),
            style: TextRole::Body,
            tags: Tags::new().with("test_id", &format!("{test_id}_body")),
        }],
        tags: Tags::new().with("test_id", test_id),
    }
}

// ---------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------

fn harness() -> UiHarness {
    let mut h = UiHarness::builder()
        .plugins(SlottedPlugins::headless())
        .registries(TestRegistries::basic())
        .resolution(WIDTH, HEIGHT)
        .theme("glass")
        .build();
    {
        let mut screens = h.world_mut().resource_mut::<Screens>();
        screens.register(chest_screen());
        screens.register(machine_screen());
    }
    // Full motion: every one of these reports is about how long something
    // takes, so reduced motion would test nothing.
    h.world_mut().insert_resource(Motion::default());
    h
}

fn open_chest(h: &mut UiHarness, fixture: ChestFixture) -> Opened {
    let opened = h.open_screen(ScreenKind::new(CHEST), fixture);
    h.settle();
    opened
}

fn slot(h: &UiHarness, region: &str, index: usize) -> Entity {
    h.find(
        &by::role(SemanticRole::Slot)
            .tag("region", region)
            .index(index),
    )
}

fn slots(h: &UiHarness, region: &str) -> Vec<Entity> {
    h.find_all(&by::role(SemanticRole::Slot).tag("region", region))
}

fn elapsed(h: &UiHarness) -> Duration {
    h.world().resource::<Time<Virtual>>().elapsed()
}

/// The tokens actually in force, which is the theme asset when it has loaded
/// and [`slotted_theme::Tokens::default`] when it has not. Exactly what the
/// widgets read.
fn tokens(h: &UiHarness) -> slotted_theme::Tokens {
    let handle = h
        .world()
        .get_resource::<slotted_theme::ActiveTheme>()
        .map(|a| a.0.clone());
    handle
        .and_then(|handle| {
            h.world()
                .get_resource::<Assets<slotted_theme::Theme>>()
                .and_then(|themes| themes.get(&handle).map(|t| t.tokens.clone()))
        })
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------
// 1. The quick-move flight
// ---------------------------------------------------------------------------

/// The theme's flight is inside the moodboard's budget. This is the number the
/// "nice but slow, it feels very annoying" report was about: it used to come
/// off the `slow` tier.
#[test]
fn the_flight_duration_is_inside_the_motion_budget() {
    let h = harness();
    let tokens = tokens(&h);
    let fly = Motion::default().preset_duration(MotionPreset::FlyToSlot, &tokens);
    assert!(
        fly <= FLY_BUDGET,
        "a single flight is {fly:?}, over the {FLY_BUDGET:?} budget"
    );
    assert!(fly >= Duration::from_millis(100), "and still readable");
}

/// And every shipped theme keeps its own flight and hover delay inside the
/// budget, not just the built-in defaults.
#[test]
fn every_shipped_theme_keeps_the_flight_and_the_hover_delay_short() {
    const THEMES: [(&str, &str); 3] = [
        (
            "glass",
            include_str!("../../../assets/themes/glass.theme.ron"),
        ),
        (
            "paper",
            include_str!("../../../assets/themes/paper.theme.ron"),
        ),
        (
            "neon",
            include_str!("../../../assets/themes/neon.theme.ron"),
        ),
    ];
    for (name, source) in THEMES {
        let theme =
            slotted_theme::Theme::from_ron(source).unwrap_or_else(|e| panic!("{name} parses: {e}"));
        let fly = Motion::default().preset_duration(MotionPreset::FlyToSlot, &theme.tokens);
        assert!(fly <= FLY_BUDGET, "{name} flies for {fly:?}");
        let delay = theme.tokens.durations.hover_delay_ms();
        assert!(delay <= 150, "{name} waits {delay} ms before a tooltip");
    }
}

/// The model lands first and the flight is decoration over the top: the
/// destination slot shows its stack on the very frame of the shift-click,
/// long before the icon finishes travelling.
#[test]
fn a_quick_move_shows_the_result_before_the_flight_lands() {
    let mut h = harness();
    open_chest(&mut h, ChestFixture::filled());
    let source = slot(&h, "chest", 0);
    let moved = h.stack_at(source).expect("the first chest slot is filled");

    h.shift_click(source);
    h.step(1);

    assert!(
        h.stack_at(source).is_none(),
        "the source emptied on the click, not when the flight ended"
    );
    let landed: u32 = slots(&h, "player")
        .into_iter()
        .filter_map(|e| h.stack_at(e))
        .filter(|s| s.id == moved.id)
        .map(|s| s.count)
        .sum();
    assert_eq!(landed, moved.count, "the whole stack is already in place");

    // And the flight is a separate, transient node that cleans itself up.
    h.settle();
    let mut flights = h.world_mut().query_filtered::<Entity, With<FlyingItem>>();
    let world = h.world();
    assert_eq!(
        flights.iter(world).count(),
        0,
        "every flight despawned once its tween finished"
    );
}

/// One gesture that lands in several slots flies to all of them at once. The
/// flights start on the same frame, so a bulk move can never serialise into a
/// queue of animations.
#[test]
fn a_quick_move_into_several_slots_flies_to_all_of_them_at_once() {
    let mut h = harness();
    // Nine cobblestone spread one per player slot, and a full stack in the
    // chest: quick-moving it tops up every partial stack in one gesture.
    let mut fixture = ChestFixture::empty();
    fixture.chest = vec![(0, "minecraft:cobblestone".to_owned(), 40)];
    fixture.main = (0..9)
        .map(|i| (i, "minecraft:cobblestone".to_owned(), 60))
        .collect();
    open_chest(&mut h, fixture);

    h.shift_click(slot(&h, "chest", 0));
    h.step(1);

    let mut flights = h
        .world_mut()
        .query_filtered::<&Tween, With<FlyingItem>>()
        .iter(h.world())
        .cloned()
        .collect::<Vec<_>>();
    assert!(
        flights.len() > 1,
        "a gesture that filled several slots flies to each of them: {} flight(s)",
        flights.len()
    );
    flights.sort_by_key(|t| t.duration);
    assert_eq!(
        flights.first().map(|t| t.duration),
        flights.last().map(|t| t.duration),
        "no flight is given a longer run than another: there is no stagger"
    );
    let first = flights[0].elapsed;
    for tween in &flights {
        assert_eq!(
            tween.elapsed, first,
            "the flights are all the same age: they started on one frame"
        );
        assert!(
            tween.duration <= FLY_BUDGET,
            "a flight in a bulk move runs for {:?}",
            tween.duration
        );
    }
    // The whole bulk move is over inside the budget, not one flight after
    // another.
    let start = elapsed(&h);
    h.settle();
    let spent = elapsed(&h).saturating_sub(start);
    assert!(
        spent < Duration::from_millis(350),
        "the flights took {spent:?} to finish; they serialised"
    );
}

/// A twenty-seven slot "deposit all" is the worst case a player can reach
/// from the action rail. It has to land in the model at once and be over
/// inside the frame budget.
#[test]
fn a_twenty_seven_slot_deposit_all_settles_inside_four_hundred_milliseconds() {
    let mut h = harness();
    let mut fixture = ChestFixture::empty();
    fixture.main = (0..27)
        .map(|i| (i, "minecraft:cobblestone".to_owned(), 64))
        .collect();
    let opened = open_chest(&mut h, fixture);

    let start = elapsed(&h);
    h.menu_action(
        opened.menu,
        ClickAction::Toolbar(ToolbarAction::DepositAll {
            from: MenuDef::PLAYER_MAIN,
            to: MenuDef::CONTAINER,
        }),
    );
    h.step(1);

    // Every destination shows its stack on the first frame after the action.
    let chest = slots(&h, "chest");
    let filled = chest.iter().filter(|e| h.stack_at(**e).is_some()).count();
    assert_eq!(
        filled, 27,
        "all twenty-seven slots show their stacks at once"
    );
    assert!(
        slots(&h, "player").iter().all(|e| h.stack_at(*e).is_none()),
        "and the player inventory emptied in the same frame"
    );

    h.settle();
    let spent = elapsed(&h).saturating_sub(start);
    assert!(
        spent < Duration::from_millis(400),
        "a bulk deposit settled in {spent:?}, past the 400 ms budget"
    );
}

// ---------------------------------------------------------------------------
// 2. Tooltips
// ---------------------------------------------------------------------------

/// The delay is a token now, it is short, and the clock starts when the
/// pointer arrives rather than when some other animation finishes.
#[test]
fn a_tooltip_appears_exactly_after_the_theme_delay() {
    let mut h = harness();
    open_chest(&mut h, ChestFixture::filled());
    let delay = Duration::from_millis(u64::from(tokens(&h).durations.hover_delay_ms()));
    assert!(
        delay <= Duration::from_millis(150),
        "the compact delay is {delay:?}, long enough to read as lag"
    );

    let target = slot(&h, "chest", 0);
    h.hover(target);
    let start = elapsed(&h);

    // Nothing before the delay, however many frames pass.
    while elapsed(&h).saturating_sub(start) < delay {
        assert!(
            h.world().get::<TooltipContent>(target).is_none(),
            "a tooltip appeared after {:?}, before the {delay:?} delay",
            elapsed(&h).saturating_sub(start)
        );
        h.step(1);
    }
    // And one frame's grace after it.
    h.step(2);
    assert!(
        h.world().get::<TooltipContent>(target).is_some(),
        "no tooltip {:?} after the pointer arrived",
        elapsed(&h).saturating_sub(start)
    );
}

/// The report was "they spawn top-left and then snap into position". A
/// tooltip is now hidden until it has been placed, and the frame it becomes
/// visible it is already beside its slot and inside the window.
#[test]
fn a_tooltip_is_never_visible_at_the_corner_of_the_window() {
    let mut h = harness();
    open_chest(&mut h, ChestFixture::filled());
    let target = slot(&h, "chest", 0);
    let target_rect = h.rect_of(target);
    h.hover(target);

    let mut seen = false;
    for _ in 0..120 {
        h.step(1);
        let Some(tooltip) = tooltip_entity(&mut h) else {
            continue;
        };
        if !h.is_visible(tooltip) {
            assert!(
                h.world().get::<TooltipUnplaced>(tooltip).is_some(),
                "a hidden tooltip is one that has not been placed yet"
            );
            continue;
        }
        let rect = h.rect_of(tooltip);
        assert!(
            rect.min.length() > 1.0,
            "the tooltip became visible at the window corner: {rect:?}"
        );
        assert!(
            rect.min.x >= -0.5
                && rect.min.y >= -0.5
                && rect.max.x <= WIDTH + 0.5
                && rect.max.y <= HEIGHT + 0.5,
            "the tooltip became visible outside the window: {rect:?}"
        );
        // Adjacent to the slot the pointer is on, not somewhere else.
        assert!(
            (rect.min.x - target_rect.max.x).abs() < 24.0
                && (rect.min.y - target_rect.min.y).abs() < 24.0,
            "the tooltip became visible away from its slot: {rect:?} vs {target_rect:?}"
        );
        seen = true;
        break;
    }
    assert!(seen, "the tooltip never became visible");
}

fn tooltip_entity(h: &mut UiHarness) -> Option<Entity> {
    let world = h.world_mut();
    let mut query = world.query_filtered::<Entity, With<slotted_ui::TooltipHost>>();
    query.iter(world).next()
}

// ---------------------------------------------------------------------------
// 3. Picking things up
// ---------------------------------------------------------------------------

/// The headline case. `bevy_picking` calls any movement between press and
/// release a drag; a hand that moves three pixels is still clicking.
#[test]
fn a_click_that_drifts_three_pixels_is_still_a_pickup() {
    let mut h = harness();
    let opened = open_chest(&mut h, ChestFixture::filled());
    let target = slot(&h, "chest", 0);
    let expected = h.stack_at(target).expect("a filled slot");
    let centre = h.center_of(target);

    h.pointer_move_to(centre);
    h.pointer_press(bevy::picking::pointer::PointerButton::Primary);
    h.pointer_move_to(centre + Vec2::new(3.0, -2.0));
    h.pointer_release(bevy::picking::pointer::PointerButton::Primary);
    h.settle();

    assert_eq!(
        h.carried(opened.menu),
        Some(expected),
        "a click with a shaky hand picked nothing up"
    );
    assert!(h.stack_at(target).is_none());
}

/// The same gesture at the far edge of the slot, where the hover scale has
/// already grown the node under the pointer.
#[test]
fn a_click_near_the_edge_of_a_hovered_slot_still_picks_up() {
    let mut h = harness();
    let opened = open_chest(&mut h, ChestFixture::filled());
    let target = slot(&h, "chest", 0);
    let expected = h.stack_at(target).expect("a filled slot");
    let rect = h.rect_of(target);
    // Two pixels inside the top-left corner, then hold still long enough for
    // the hover tween to grow the node before releasing.
    let edge = rect.min + Vec2::splat(2.0);

    h.pointer_move_to(edge);
    h.step(6);
    h.pointer_press(bevy::picking::pointer::PointerButton::Primary);
    h.pointer_move_to(edge + Vec2::new(1.0, 1.0));
    h.pointer_release(bevy::picking::pointer::PointerButton::Primary);
    h.settle();

    assert_eq!(h.carried(opened.menu), Some(expected));
}

/// A drag has to leave its slot to become a paint. One that comes back and
/// releases where it started is the click it always was.
#[test]
fn a_drag_that_ends_on_its_own_slot_is_a_click() {
    let mut h = harness();
    let opened = open_chest(&mut h, ChestFixture::filled());
    let source = slot(&h, "chest", 0);
    let expected = h.stack_at(source).expect("a filled slot");
    let a = h.center_of(source);

    h.pointer_move_to(a);
    h.pointer_press(bevy::picking::pointer::PointerButton::Primary);
    h.pointer_move_to(a + Vec2::new(6.0, 6.0));
    h.pointer_move_to(a);
    h.pointer_release(bevy::picking::pointer::PointerButton::Primary);
    h.settle();

    assert_eq!(h.carried(opened.menu), Some(expected));
}

/// And a drag that does reach a second slot is still a paint: the origin is
/// painted retroactively, so the distribution is the same one it always was.
#[test]
fn a_drag_across_two_slots_still_paints_both() {
    let mut h = harness();
    let mut fixture = ChestFixture::empty();
    fixture.main = vec![(0, "minecraft:cobblestone".to_owned(), 8)];
    let opened = open_chest(&mut h, fixture);

    // Pick the stack up, then paint it across two empty chest slots.
    let carrier = slot(&h, "player", 0);
    h.click(carrier);
    h.settle();
    assert!(h.carried(opened.menu).is_some(), "holding the stack");

    let a = slot(&h, "chest", 0);
    let b = slot(&h, "chest", 1);
    h.drag_paint(&[a, b]);
    h.settle();

    assert!(
        h.stack_at(a).is_some() && h.stack_at(b).is_some(),
        "both painted slots got a share: {:?} and {:?}",
        h.stack_at(a),
        h.stack_at(b)
    );
    assert_eq!(
        h.stack_at(a).map(|s| s.count),
        h.stack_at(b).map(|s| s.count),
        "an even left-drag splits evenly"
    );
}

/// Put a stack down and take it straight back up. The double-click window
/// used to turn the second click into a collect-all with an empty cursor,
/// which the model rejects as nothing to do, so the click did nothing at all.
#[test]
fn putting_a_stack_down_and_picking_it_back_up_is_two_pickups() {
    let mut h = harness();
    let opened = open_chest(&mut h, ChestFixture::filled());
    let source = slot(&h, "chest", 0);
    let expected = h.stack_at(source).expect("a filled slot");

    // Pick it up, and let the double-click window lapse.
    h.click(source);
    h.settle();
    h.advance(Duration::from_millis(400));
    assert_eq!(h.carried(opened.menu), Some(expected.clone()));

    // Put it down, then take it straight back: two deliberate clicks well
    // inside the window, and both of them have to land.
    h.click(source);
    h.step(1);
    assert!(h.carried(opened.menu).is_none(), "the stack went down");

    h.click(source);
    h.settle();
    assert_eq!(
        h.carried(opened.menu),
        Some(expected),
        "the second click did nothing: it was eaten by the double-click window"
    );
    assert!(h.stack_at(source).is_none());
}

/// The vanilla gesture still works: a second click with a full cursor gathers
/// the kind. That is the behaviour the window exists for.
#[test]
fn a_second_click_with_a_full_cursor_still_collects_the_kind() {
    let mut h = harness();
    let mut fixture = ChestFixture::empty();
    fixture.chest = vec![
        (0, "minecraft:cobblestone".to_owned(), 10),
        (2, "minecraft:cobblestone".to_owned(), 20),
    ];
    let opened = open_chest(&mut h, fixture);
    let source = slot(&h, "chest", 0);

    h.click(source);
    h.step(1);
    h.click(source);
    h.settle();

    assert_eq!(
        h.carried(opened.menu).map(|s| s.count),
        Some(30),
        "the double click gathered both stacks"
    );
}

// ---------------------------------------------------------------------------
// 4. Side tabs
// ---------------------------------------------------------------------------

fn open_machine(h: &mut UiHarness) -> Opened {
    let opened = h.open_screen(ScreenKind::new(MACHINE), ChestFixture::filled());
    h.settle();
    opened
}

/// The report: "the side tabs of the machine UI shift the main UI layout".
/// The panel's rect is identical before and after a tab opens.
#[test]
fn opening_a_side_tab_does_not_move_the_panel() {
    let mut h = harness();
    open_machine(&mut h);
    let panel = h.find(&by::test_id("panel"));
    let before = h.rect_of(panel);
    let grid_before = h.rect_of(slot(&h, "chest", 0));

    let tab = h.find(&by::test_id("tab_a"));
    h.toggle_side_tab(tab);
    h.settle();
    assert_eq!(h.side_tab_open(tab), Some(true), "the tab opened");

    assert_eq!(
        h.rect_of(panel),
        before,
        "the panel moved when a tab opened"
    );
    assert_eq!(
        h.rect_of(slot(&h, "chest", 0)),
        grid_before,
        "the slots under the panel moved too"
    );

    // Closing it again leaves everything exactly where it was.
    h.toggle_side_tab(tab);
    h.settle();
    assert_eq!(h.rect_of(panel), before);
}

/// The tab's own rect stays outside the panel: it is a rail beside it, not an
/// overlay on top of it.
#[test]
fn an_open_side_tab_sits_outside_the_panel() {
    let mut h = harness();
    open_machine(&mut h);
    let panel = h.rect_of(h.find(&by::test_id("panel")));
    let tab = h.find(&by::test_id("tab_a"));

    h.toggle_side_tab(tab);
    h.settle();

    let tab_rect = h.rect_of(tab);
    assert!(
        tab_rect.min.x >= panel.max.x - 1.0,
        "the tab overlaps the panel: {tab_rect:?} against {panel:?}"
    );
    let body = h.rect_of(h.find(&by::test_id("tab_a_body")));
    assert!(
        body.min.x >= panel.max.x - 1.0,
        "the tab's body overlaps the panel: {body:?} against {panel:?}"
    );
    assert!(body.max.x > body.min.x, "the body has a width to show");
}

/// The rail keeps its width whatever the tabs do, which is the mechanism
/// behind the two tests above.
#[test]
fn the_rail_keeps_its_width_when_a_tab_opens() {
    let mut h = harness();
    open_machine(&mut h);
    let rail = h.find(&by::test_id("rail"));
    let before = h.rect_of(rail);

    let tab = h.find(&by::test_id("tab_a"));
    h.toggle_side_tab(tab);
    h.settle();

    assert_eq!(h.rect_of(rail), before, "the rail grew with its tab");
    // The open box is the thing that grew, and it is a separate node.
    let state = h.world().get::<SideTabState>(tab).copied().expect("state");
    let world = h.world_mut();
    let mut panels = world.query::<(&SideTabPanel, &Node)>();
    let opened_width = panels
        .iter(world)
        .find(|(p, _)| p.tab == tab)
        .map(|(_, n)| n.width)
        .expect("a side tab panel");
    assert_eq!(
        opened_width,
        Val::Px(state.open_width - state.closed_width),
        "the box holds the content and the rail holds the header"
    );
}

/// And the zone the browser dodges is still published while the tab is open.
#[test]
fn an_open_side_tab_still_publishes_its_exclusion_zone() {
    let mut h = harness();
    let opened = open_machine(&mut h);
    let before = h.exclusion_zones(opened.screen).len();

    let tab = h.find(&by::test_id("tab_a"));
    h.toggle_side_tab(tab);
    h.settle();

    let zones = h.exclusion_zones(opened.screen);
    assert_eq!(zones.len(), before + 1, "the open tab published a zone");
    let panel = h.rect_of(h.find(&by::test_id("panel")));
    assert!(
        zones
            .iter()
            .any(|z| z.min.x >= panel.max.x - 1.0 && z.max.x > z.min.x),
        "the zone covers the box beside the panel: {zones:?}"
    );
}

/// A closed tab has no width to give away and no zone to publish.
#[test]
fn a_closed_side_tab_is_exactly_one_header_wide() {
    let mut h = harness();
    open_machine(&mut h);
    let tab = h.find(&by::test_id("tab_a"));
    let rect = h.rect_of(tab);
    let state = h.world().get::<SideTabState>(tab).copied().expect("state");
    assert!(
        (rect.width() - state.closed_width).abs() < 1.0,
        "a closed tab is {} wide, not {}",
        rect.width(),
        state.closed_width
    );
}
