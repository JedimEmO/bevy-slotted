//! The Phase 6 machine widgets through the headless harness.
//!
//! Every assertion goes through something a player or a machine simulation can
//! reach: a `SetProperty` from the host, a real pointer click, a key on the
//! focused node. What is asserted is the render-free contract of section 1.7,
//! never a colour on screen.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use bevy::camera::Camera3d;
use bevy::input::keyboard::KeyCode;
use bevy::prelude::*;
use bevy::ui::widget::ViewportNode;
use slotted_ecs::{MenuProperty, SetProperty};
use slotted_model::{Actor, Inventory, MenuDef, Namespaced, PropertyDef, PropertyId};
use slotted_test::prelude::*;
use slotted_ui::{
    DataSourceId, Direction, ExclusionZone, FillNode, FluidDef, Fluids, IconButtonState,
    IconButtonStateDef, IconDef, Layout, LocKey, Orientation, ScreenDef, SemanticLabel,
    SideTabContent, SideTabState, Tags, TextRole, UiNodeDef, ViewSubject, ViewportSubject,
    VirtualCell, VirtualGridSource, VirtualGridSources, VirtualGridState,
};

const SCREEN: &str = "machine:test";

// Properties the test screen binds to.
const TANK: PropertyId = PropertyId(0);
const TANK_CAP: PropertyId = PropertyId(1);
const VALUE: PropertyId = PropertyId(2);
const MAX: PropertyId = PropertyId(3);
const MODE: PropertyId = PropertyId(4);
const FLUID: PropertyId = PropertyId(5);

/// A three-slot machine menu with the six properties the screen binds to.
struct MachineFixture;

impl MenuFixture for MachineFixture {
    fn def(&self) -> Arc<MenuDef> {
        let mut def = MenuDef::generic(3);
        def.properties = vec![
            PropertyDef {
                id: TANK,
                initial: 0,
            },
            PropertyDef {
                id: TANK_CAP,
                initial: 8000,
            },
            PropertyDef {
                id: VALUE,
                initial: 0,
            },
            PropertyDef {
                id: MAX,
                initial: 100,
            },
            PropertyDef {
                id: MODE,
                initial: 0,
            },
            PropertyDef {
                id: FLUID,
                initial: -1,
            },
        ];
        Arc::new(def)
    }

    fn inventories(&self) -> Vec<Inventory> {
        self.def()
            .inventory_sizes()
            .into_iter()
            .map(Inventory::new)
            .collect()
    }

    fn actor(&self) -> Actor {
        Actor::SURVIVAL
    }
}

fn tags(test_id: &str) -> Tags {
    Tags::new().with(Tags::TEST_ID, test_id)
}

fn bar(test_id: &str, direction: Direction, text: bool) -> UiNodeDef {
    UiNodeDef::Bar {
        property: VALUE,
        max: MAX,
        direction,
        text,
        tags: tags(test_id),
    }
}

fn icon_state(id: &str) -> IconButtonStateDef {
    IconButtonStateDef {
        id: id.to_owned(),
        icon: IconDef::Image(format!("icons/{id}.png")),
        label: LocKey(format!("mode.{id}")),
    }
}

/// The test screen: one of every Phase 6 widget, each with a `test_id`.
fn screen() -> ScreenDef {
    ScreenDef {
        kind: ScreenKind::new(SCREEN),
        initial_focus: None,
        presentation: slotted_ui::Presentation::default(),
        inherits: None,
        remove: vec![],
        root: UiNodeDef::Panel {
            role: slotted_theme::roles::PANEL,
            layout: Layout::default(),
            children: vec![
                UiNodeDef::Tank {
                    property: TANK,
                    capacity: TANK_CAP,
                    orientation: Orientation::Vertical,
                    fluid: None,
                    fluid_property: Some(FLUID),
                    unit: "mB".to_owned(),
                    tags: tags("tank"),
                },
                bar("right", Direction::Right, true),
                bar("left", Direction::Left, false),
                bar("up", Direction::Up, false),
                bar("down", Direction::Down, false),
                UiNodeDef::Progress {
                    property: VALUE,
                    max: MAX,
                    direction: Direction::Right,
                    tags: tags("cook"),
                },
                UiNodeDef::IconButton {
                    states: vec![icon_state("ignore"), icon_state("low"), icon_state("high")],
                    property: Some(MODE),
                    tags: tags("mode"),
                },
                UiNodeDef::SideTab {
                    icon: IconDef::Image("icons/tab.png".to_owned()),
                    side: slotted_ui::Side::Right,
                    label: Some(LocKey("tab.redstone".to_owned())),
                    open: false,
                    children: vec![UiNodeDef::Text {
                        key: LocKey("tab.body".to_owned()),
                        style: TextRole::Body,
                        tags: tags("tab_body"),
                    }],
                    tags: tags("tab"),
                },
                UiNodeDef::VirtualGrid {
                    source: source_id(),
                    cols: 9,
                    rows: 3,
                    tags: tags("grid"),
                },
                UiNodeDef::Viewport {
                    subject: ViewSubject::Block(Namespaced::parse("machine:furnace").expect("id")),
                    size: 96.0,
                    tags: tags("view"),
                },
            ],
            tags: Tags::new(),
        },
        listring: vec![],
    }
}

fn source_id() -> DataSourceId {
    DataSourceId(Namespaced::parse("machine:many").expect("id"))
}

/// Ten thousand cells, each a text node naming its own index.
struct ManyCells;

impl VirtualGridSource for ManyCells {
    fn len(&self) -> usize {
        10_000
    }
    fn version(&self) -> u64 {
        1
    }
    fn cell(&self, index: usize) -> UiNodeDef {
        UiNodeDef::Text {
            key: LocKey(format!("cell {index}")),
            style: TextRole::Body,
            tags: Tags::new(),
        }
    }
}

fn water() -> FluidDef {
    FluidDef {
        name: Namespaced::parse("machine:water").expect("id"),
        color: "#3B7DD8B0".to_owned(),
        texture: None,
        unit: "mB".to_owned(),
    }
}

fn harness() -> (UiHarness, Opened) {
    let mut h = UiHarness::builder()
        .plugins(SlottedPlugins::headless())
        .registries(TestRegistries::basic())
        .theme("glass")
        .build();
    h.world_mut()
        .resource_mut::<VirtualGridSources>()
        .register(source_id(), ManyCells);
    h.world_mut().insert_resource(Fluids(vec![water()]));
    let opened = h.open_screen(screen(), MachineFixture);
    h.settle();
    (h, opened)
}

/// What a machine simulation does: write a property on the menu.
fn set(h: &mut UiHarness, menu: Entity, id: PropertyId, value: i32) {
    h.world_mut().trigger(SetProperty {
        entity: menu,
        id,
        value,
    });
    h.settle();
}

fn find(h: &UiHarness, test_id: &str) -> Entity {
    h.find(&by::test_id(test_id))
}

// ---------------------------------------------------------------------------
// Tank
// ---------------------------------------------------------------------------

#[test]
fn a_tank_binds_to_its_two_properties_when_the_screen_opens() {
    let (h, _) = harness();
    let tank = find(&h, "tank");
    let fill = h.fill_of(tank).expect("a tank has a FillValue");
    assert!(
        (fill.max - 8000.0).abs() < f32::EPSILON,
        "capacity is bound"
    );
    assert!((h.tank_fill(tank)).abs() < f32::EPSILON, "starts empty");
}

/// The end of the chain the review found broken: an authority snapshot lands
/// on the model, the model write goes through the one property path, the path
/// moves the `MenuProperty` child, and the tank bound to that child redraws.
/// Before, the snapshot moved only `MenuState.properties` and the tank kept
/// drawing the value it was spawned with.
#[test]
fn a_tank_follows_a_property_delivered_by_a_snapshot() {
    let (mut h, opened) = harness();
    let tank = find(&h, "tank");
    assert!(h.tank_fill(tank).abs() < f32::EPSILON, "starts empty");

    let (id, def, inventories) = {
        let menu = h.world().get::<slotted_ecs::OpenMenu>(opened.menu).unwrap();
        (menu.id, menu.def.clone(), menu.inventories.clone())
    };
    let mut snapshot_inventories = slotted_model::Inventories::new();
    for entity in &inventories {
        let inventory = h.world().get::<slotted_ecs::Inventory>(*entity).unwrap();
        snapshot_inventories.push(inventory.0.clone());
    }
    let mut state = slotted_model::MenuState::new(&def);
    state.properties[usize::from(TANK.0)] = 2000;
    let authority = Arc::new(SnapshotAuthority::new(
        slotted_model::AuthorityEvent::Resync {
            menu: id,
            snapshot: slotted_model::MenuSnapshot {
                inventories: snapshot_inventories,
                state,
            },
        },
    ));
    h.world_mut()
        .insert_resource(slotted_ecs::Authority(authority));
    h.settle();

    assert!(
        (h.tank_fill(tank) - 0.25).abs() < 0.001,
        "the tank redrew from the snapshot: 2000 of 8000"
    );
    assert_eq!(
        h.world()
            .get::<SemanticLabel>(tank)
            .map(|l| l.0.clone())
            .as_deref(),
        Some("2000 / 8000 mB"),
    );
}

/// An authority that delivers one event and then nothing, so a test can put a
/// snapshot on the wire without a server.
#[derive(Debug)]
struct SnapshotAuthority(std::sync::Mutex<Option<slotted_model::AuthorityEvent>>);

impl SnapshotAuthority {
    fn new(event: slotted_model::AuthorityEvent) -> Self {
        Self(std::sync::Mutex::new(Some(event)))
    }
}

impl slotted_model::Authority for SnapshotAuthority {
    fn submit(
        &self,
        _menu: slotted_model::MenuId,
        _action: slotted_model::ClickAction,
        _predicted: &slotted_model::Delta,
    ) -> Result<(), slotted_model::AuthorityError> {
        Ok(())
    }

    fn poll(&self) -> Vec<slotted_model::AuthorityEvent> {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()
            .into_iter()
            .collect()
    }
}

#[test]
fn a_tank_fill_and_label_follow_a_property_change() {
    let (mut h, opened) = harness();
    let tank = find(&h, "tank");
    set(&mut h, opened.menu, TANK, 2000);
    assert!((h.tank_fill(tank) - 0.25).abs() < 0.001, "2000 of 8000");
    assert_eq!(
        h.world().get::<SemanticLabel>(tank).map(|l| l.0.clone()),
        Some("2000 / 8000 mB".to_owned()),
    );

    // The fill child grows to the same fraction; that is the whole render.
    let node = fill_child(&h, tank);
    assert_percent(node.height, 25.0);
    assert_eq!(node.bottom, px(0), "a vertical tank fills from the bottom");
}

#[test]
fn a_tank_resolves_its_fluid_from_the_bound_property() {
    let (mut h, opened) = harness();
    let tank = find(&h, "tank");
    assert_eq!(
        h.world().get::<slotted_ui::TankFluid>(tank).map(|f| f.0),
        Some(None),
        "the fluid property starts at -1: no fluid"
    );
    set(&mut h, opened.menu, FLUID, 0);
    assert_eq!(
        h.world().get::<slotted_ui::TankFluid>(tank).map(|f| f.0),
        Some(Some(slotted_ui::FluidId(0)))
    );
    // A fluid tints the fill; the tint is a component, not a draw call.
    let child = fill_child_entity(&h, tank);
    let color = h.world().get::<BackgroundColor>(child).map(|c| c.0);
    assert_eq!(color, Some(water().color()));
}

#[test]
fn a_tank_tooltip_carries_the_amount_line() {
    let (mut h, opened) = harness();
    let tank = find(&h, "tank");
    set(&mut h, opened.menu, TANK, 1200);
    h.request_tooltip(tank, slotted_ui::TooltipTier::Compact);
    let tooltip = h.tooltip().expect("the tank composed a tooltip");
    let lines: Vec<String> = tooltip
        .parts
        .iter()
        .filter_map(|p| match p {
            UiNodeDef::Text { key, .. } => Some(key.0.clone()),
            _ => None,
        })
        .collect();
    assert!(
        lines.contains(&"1200 / 8000 mB".to_owned()),
        "widget tooltip line missing from {lines:?}"
    );
}

/// The fill node under a tank or bar. A tank's sits inside its well, one
/// level below the root, so this walks the subtree.
fn fill_child_entity(h: &UiHarness, root: Entity) -> Entity {
    let mut queue = vec![root];
    while let Some(next) = queue.pop() {
        if next != root && h.world().get::<FillNode>(next).is_some() {
            return next;
        }
        if let Some(children) = h.world().get::<Children>(next) {
            queue.extend(children.iter());
        }
    }
    panic!("a tank or bar has a fill child")
}

/// A percentage `Val`, to a tenth of a percent: a fill fraction is computed
/// in `f32`, so `30%` may arrive as `30.000002`.
fn assert_percent(value: Val, expected: f32) {
    match value {
        Val::Percent(got) => assert!(
            (got - expected).abs() < 0.1,
            "expected {expected}%, got {got}%"
        ),
        other => panic!("expected a percentage, got {other:?}"),
    }
}

fn fill_child(h: &UiHarness, root: Entity) -> Node {
    let child = fill_child_entity(h, root);
    h.world()
        .get::<Node>(child)
        .expect("the fill is a node")
        .clone()
}

// ---------------------------------------------------------------------------
// Bar
// ---------------------------------------------------------------------------

#[test]
fn a_bar_fills_from_the_edge_its_direction_names() {
    let (mut h, opened) = harness();
    set(&mut h, opened.menu, VALUE, 30);
    for (test_id, direction) in [
        ("right", Direction::Right),
        ("left", Direction::Left),
        ("up", Direction::Up),
        ("down", Direction::Down),
    ] {
        let bar = find(&h, test_id);
        assert!(
            (h.tank_fill(bar) - 0.3).abs() < 0.001,
            "{test_id} is 30 of 100"
        );
        let node = fill_child(&h, bar);
        match direction {
            Direction::Right => {
                assert_eq!(node.left, px(0));
                assert_percent(node.width, 30.0);
            }
            Direction::Left => {
                assert_eq!(node.right, px(0));
                assert_percent(node.width, 30.0);
            }
            Direction::Up => {
                assert_eq!(node.bottom, px(0));
                assert_percent(node.height, 30.0);
            }
            Direction::Down => {
                assert_eq!(node.top, px(0));
                assert_percent(node.height, 30.0);
            }
        }
    }
}

#[test]
fn a_bar_with_text_writes_the_value_over_itself() {
    let (mut h, opened) = harness();
    set(&mut h, opened.menu, VALUE, 42);
    let bar = find(&h, "right");
    let text = h
        .world()
        .get::<Children>(bar)
        .expect("children")
        .iter()
        .filter_map(|c| h.world().get::<Text>(c))
        .map(|t| t.0.clone())
        .next();
    assert_eq!(text, Some("42 / 100".to_owned()));
}

#[test]
fn a_progress_arrow_is_a_bar_with_the_progress_roles() {
    let (mut h, opened) = harness();
    let cook = find(&h, "cook");
    set(&mut h, opened.menu, VALUE, 50);
    assert!((h.tank_fill(cook) - 0.5).abs() < 0.001);
    assert_eq!(
        h.world()
            .get::<slotted_theme::Themed>(cook)
            .map(|t| t.0.clone()),
        Some(slotted_theme::roles::PROGRESS)
    );
}

// ---------------------------------------------------------------------------
// Side tab
// ---------------------------------------------------------------------------

#[test]
fn a_side_tab_opens_on_a_click_and_closes_on_the_next() {
    let (mut h, _) = harness();
    let tab = find(&h, "tab");
    let header = h.find(&by::tag("side_tab", "header"));
    assert_eq!(h.side_tab_open(tab), Some(false));

    let closed_root_width = h.world().get::<Node>(tab).expect("node").width;

    h.click(header);
    h.settle();
    assert_eq!(h.side_tab_open(tab), Some(true), "a click opens it");
    let state = h.world().get::<SideTabState>(tab).copied().expect("state");
    assert_eq!(
        h.world().get::<Node>(tab).expect("node").width,
        closed_root_width,
        "the root keeps its width; only the absolute box grows"
    );
    let box_width = panel_width(&mut h, tab);
    assert!(
        (box_width - (state.open_width - state.closed_width)).abs() < 0.5,
        "the tween settled at the content width: {box_width} vs {state:?}"
    );

    h.click(header);
    h.settle();
    assert_eq!(
        h.side_tab_open(tab),
        Some(false),
        "the next click closes it"
    );
}

#[test]
fn an_open_side_tab_publishes_an_exclusion_zone() {
    let (mut h, _) = harness();
    let tab = find(&h, "tab");
    assert!(content_excluded(&mut h, tab).is_none(), "closed: no zone");

    h.toggle_side_tab(tab);
    h.settle();
    let content = content_excluded(&mut h, tab).expect("an open tab excludes its content");
    assert_eq!(
        h.world().get::<Visibility>(content),
        Some(&Visibility::Inherited)
    );

    h.toggle_side_tab(tab);
    h.settle();
    assert!(
        content_excluded(&mut h, tab).is_none(),
        "closed again: no zone"
    );
}

/// Width of the absolute box a tab opens into.
fn panel_width(h: &mut UiHarness, tab: Entity) -> f32 {
    let world = h.world_mut();
    let mut query = world.query::<(&slotted_ui::SideTabPanel, &Node)>();
    for (panel, node) in query.iter(world) {
        if panel.tab == tab {
            return match node.width {
                bevy::prelude::Val::Px(w) => w,
                _ => 0.0,
            };
        }
    }
    panic!("no side tab panel for {tab:?}");
}

fn content_excluded(h: &mut UiHarness, tab: Entity) -> Option<Entity> {
    let world = h.world_mut();
    let mut query = world.query_filtered::<(Entity, &SideTabContent), With<ExclusionZone>>();
    query
        .iter(world)
        .find(|(_, content)| content.tab == tab)
        .map(|(e, _)| e)
}

#[test]
fn a_side_tab_header_is_keyboard_reachable() {
    let (mut h, _) = harness();
    let tab = find(&h, "tab");
    let header = h.find(&by::tag("side_tab", "header"));
    h.set_focus(Some(header));
    h.key(KeyCode::Enter);
    h.settle();
    assert_eq!(h.side_tab_open(tab), Some(true), "Enter toggles the tab");
}

// ---------------------------------------------------------------------------
// Icon button
// ---------------------------------------------------------------------------

#[test]
fn an_icon_button_cycles_forward_on_a_click_and_back_on_shift_click() {
    let (mut h, _) = harness();
    let button = find(&h, "mode");
    assert_eq!(state_tag(&h, button), Some("ignore".to_owned()));

    h.click(button);
    h.settle();
    assert_eq!(state_tag(&h, button), Some("low".to_owned()));

    h.shift_click(button);
    h.settle();
    assert_eq!(state_tag(&h, button), Some("ignore".to_owned()));

    // Backwards past the start wraps to the last state.
    h.shift_click(button);
    h.settle();
    assert_eq!(state_tag(&h, button), Some("high".to_owned()));
}

#[test]
fn an_icon_button_writes_its_bound_property() {
    let (mut h, opened) = harness();
    let button = find(&h, "mode");
    assert_eq!(h.property_of(button), Some((MODE, 0)));

    h.cycle(button, true);
    h.settle();
    assert_eq!(h.property_of(button), Some((MODE, 1)), "the host sees it");
    assert_eq!(menu_property(&h, opened.menu, MODE), Some(1));

    h.cycle(button, false);
    h.settle();
    assert_eq!(h.property_of(button), Some((MODE, 0)));
}

#[test]
fn an_icon_button_follows_a_property_written_by_the_host() {
    let (mut h, opened) = harness();
    let button = find(&h, "mode");
    set(&mut h, opened.menu, MODE, 2);
    assert_eq!(state_tag(&h, button), Some("high".to_owned()));
    assert_eq!(
        h.world().get::<IconButtonState>(button).map(|s| s.current),
        Some(2)
    );
    assert_eq!(
        h.world().get::<SemanticLabel>(button).map(|l| l.0.clone()),
        Some("mode.high".to_owned())
    );
}

#[test]
fn an_icon_button_cycles_from_the_keyboard() {
    let (mut h, _) = harness();
    let button = find(&h, "mode");
    h.set_focus(Some(button));
    h.key(KeyCode::Space);
    h.settle();
    assert_eq!(state_tag(&h, button), Some("low".to_owned()));
}

fn state_tag(h: &UiHarness, entity: Entity) -> Option<String> {
    h.world()
        .get::<Tags>(entity)
        .and_then(|t| t.get("state"))
        .map(ToOwned::to_owned)
}

fn menu_property(h: &UiHarness, menu: Entity, id: PropertyId) -> Option<i32> {
    h.world()
        .get::<Children>(menu)?
        .iter()
        .filter_map(|c| h.world().get::<MenuProperty>(c))
        .find(|p| p.id == id)
        .map(|p| p.value)
}

// ---------------------------------------------------------------------------
// Virtual grid
// ---------------------------------------------------------------------------

#[test]
fn a_virtual_grid_spawns_only_the_visible_window_of_a_huge_source() {
    let (mut h, _) = harness();
    let grid = find(&h, "grid");
    let state = h
        .world()
        .get::<VirtualGridState>(grid)
        .cloned()
        .expect("grid state");
    assert_eq!(state.total, 10_000, "the source is huge");
    assert_eq!(cells(&mut h).len(), 27, "9 columns by 3 visible rows");
    assert_eq!(first_cell(&mut h), Some(0));
}

#[test]
fn scrolling_a_virtual_grid_moves_the_window_by_one_row() {
    let (mut h, _) = harness();
    let grid = find(&h, "grid");
    h.scroll(grid, Vec2::new(0.0, -1.0));
    h.settle();
    assert_eq!(first_cell(&mut h), Some(9), "one row further in");
    assert_eq!(cells(&mut h).len(), 27, "still only one window of cells");

    h.scroll(grid, Vec2::new(0.0, 1.0));
    h.settle();
    assert_eq!(first_cell(&mut h), Some(0), "and back");
}

#[test]
fn paging_a_virtual_grid_from_the_keyboard_moves_a_whole_window() {
    let (mut h, _) = harness();
    let grid = find(&h, "grid");
    h.set_focus(Some(grid));
    h.key(KeyCode::PageDown);
    h.settle();
    assert_eq!(first_cell(&mut h), Some(27), "three rows of nine");
    h.key(KeyCode::PageUp);
    h.settle();
    assert_eq!(first_cell(&mut h), Some(0));
}

fn cells(h: &mut UiHarness) -> Vec<usize> {
    let world = h.world_mut();
    let mut query = world.query::<&VirtualCell>();
    let mut out: Vec<usize> = query.iter(world).map(|c| c.0).collect();
    out.sort_unstable();
    out
}

fn first_cell(h: &mut UiHarness) -> Option<usize> {
    cells(h).first().copied()
}

// ---------------------------------------------------------------------------
// Viewport
// ---------------------------------------------------------------------------

#[test]
fn a_headless_viewport_has_no_camera_but_still_names_its_subject() {
    let (mut h, _) = harness();
    let view = find(&h, "view");
    assert_eq!(
        h.viewport_subject(view),
        Some(ViewportSubject::Block(
            Namespaced::parse("machine:furnace").expect("id")
        ))
    );
    assert_eq!(
        h.world().get::<ViewportNode>(view).map(|v| v.camera),
        Some(None),
        "headless spawns no camera"
    );

    let world = h.world_mut();
    let mut cameras = world.query_filtered::<Entity, With<Camera3d>>();
    assert_eq!(
        cameras.iter(world).count(),
        0,
        "no 3D camera exists in a headless app"
    );
}

#[test]
fn the_screen_tree_shows_the_new_roles() {
    let (h, _) = harness();
    let tree = h.screen_tree().to_string();
    for role in ["tank", "bar", "side_tab", "viewport", "grid"] {
        assert!(
            tree.to_lowercase().contains(role),
            "{role} missing from the tree:\n{tree}"
        );
    }
}
