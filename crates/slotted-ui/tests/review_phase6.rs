//! Phase 6 under abuse: the values, ids and orderings a screen author, a mod
//! or a game system can get wrong.
//!
//! `machine_widgets.rs` and `hud.rs` prove the widgets do what the contract
//! says when they are used correctly. This file is the other half, and every
//! case here is something that reached a real screen or a real mod: a
//! property that ran past its maximum, a maximum of zero, a fluid id nobody
//! registered, a theme reload in the middle of an open tab, two tabs on one
//! rail, a one-state icon button, a data source that emptied under a scrolled
//! grid, a HUD layer id that does not exist, and a recording replayed into a
//! window it was never made in.
//!
//! The bar is the same throughout: no panic, a fallback a player can see, and
//! a message that names what went wrong.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use bevy::prelude::*;
use pretty_assertions::assert_eq;
use slotted_ecs::SetProperty;
use slotted_model::{Actor, Inventory, MenuDef, Namespaced, PropertyDef, PropertyId};
use slotted_test::prelude::*;
use slotted_ui::def::{Layout, Tags, UiNodeDef};
use slotted_ui::hud::builtin;
use slotted_ui::{
    DataSourceId, Direction, ExclusionZone, FillValue, FluidDef, Fluids, HudLayerDef, HudLayerId,
    HudLayers, HudUpdate, HudValue, IconButtonStateDef, IconDef, LocKey, Orientation, ScreenDef,
    SideTabState, TankFluid, TextRole, UnknownFluidWarned, VirtualGridSource, VirtualGridSources,
    VirtualGridState,
};

const SCREEN: &str = "review:phase6";

const VALUE: PropertyId = PropertyId(0);
const MAX: PropertyId = PropertyId(1);
const FLUID: PropertyId = PropertyId(2);
const MODE: PropertyId = PropertyId(3);

/// A menu with the four properties this file's screen binds to. `MAX` starts
/// at 100 so a test can drive `VALUE` past it, and `FLUID` at -1, which is how
/// an empty tank reports itself.
struct Fixture;

impl MenuFixture for Fixture {
    fn def(&self) -> Arc<MenuDef> {
        let mut def = MenuDef::generic(1);
        def.properties = vec![
            PropertyDef {
                id: VALUE,
                initial: 0,
            },
            PropertyDef {
                id: MAX,
                initial: 100,
            },
            PropertyDef {
                id: FLUID,
                initial: -1,
            },
            PropertyDef {
                id: MODE,
                initial: 0,
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

fn state(id: &str) -> IconButtonStateDef {
    IconButtonStateDef {
        id: id.to_owned(),
        icon: IconDef::Image(format!("icons/{id}.png")),
        label: LocKey(format!("mode.{id}")),
    }
}

fn side_tab(test_id: &str, open: bool) -> UiNodeDef {
    UiNodeDef::SideTab {
        icon: IconDef::Image("icons/tab.png".to_owned()),
        side: slotted_ui::Side::Right,
        label: Some(LocKey(format!("tab.{test_id}"))),
        open,
        children: vec![UiNodeDef::Text {
            opts: slotted_ui::TextOpts::default(),
            key: LocKey(format!("body.{test_id}")),
            style: TextRole::Body,
            tags: tags(&format!("{test_id}_body")),
        }],
        tags: tags(test_id),
    }
}

fn source_id() -> DataSourceId {
    DataSourceId(Namespaced::parse("review:cells").expect("id"))
}

/// A source whose length and version a test moves under the grid, the way a
/// mod's data source does when its backing list is refiltered.
#[derive(Debug, Clone, Copy, Default)]
struct Cells {
    len: usize,
    version: u64,
}

impl VirtualGridSource for Cells {
    fn len(&self) -> usize {
        self.len
    }
    fn version(&self) -> u64 {
        self.version
    }
    fn cell(&self, index: usize) -> UiNodeDef {
        UiNodeDef::Text {
            opts: slotted_ui::TextOpts::default(),
            key: LocKey(format!("cell {index}")),
            style: TextRole::Body,
            tags: Tags::new(),
        }
    }
}

/// One of every widget this file abuses, each reachable by `test_id`.
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
                    property: VALUE,
                    capacity: MAX,
                    orientation: Orientation::Vertical,
                    fluid: None,
                    fluid_property: Some(FLUID),
                    unit: "mB".to_owned(),
                    tags: tags("tank"),
                },
                UiNodeDef::Bar {
                    property: VALUE,
                    max: MAX,
                    direction: Direction::Right,
                    text: true,
                    tags: tags("bar"),
                },
                UiNodeDef::Progress {
                    property: VALUE,
                    max: MAX,
                    direction: Direction::Up,
                    tags: tags("progress"),
                },
                UiNodeDef::IconButton {
                    states: vec![state("only")],
                    property: Some(MODE),
                    tags: tags("one_state"),
                },
                UiNodeDef::Panel {
                    role: slotted_theme::roles::TAB_RAIL,
                    layout: Layout::default(),
                    children: vec![side_tab("tab_a", false), side_tab("tab_b", false)],
                    tags: Tags::new(),
                },
                UiNodeDef::VirtualGrid {
                    source: source_id(),
                    cols: 3,
                    rows: 3,
                    tags: tags("grid"),
                },
            ],
            tags: Tags::new(),
        },
        listring: vec![],
    }
}

fn water() -> FluidDef {
    FluidDef {
        name: Namespaced::parse("review:water").expect("id"),
        color: "#3B7DD8B0".to_owned(),
        texture: None,
        unit: "mB".to_owned(),
    }
}

fn harness_with(cells: Cells) -> (UiHarness, Opened) {
    let mut h = UiHarness::builder()
        .plugins(SlottedPlugins::headless())
        .registries(TestRegistries::basic())
        .theme("glass")
        .build();
    h.world_mut()
        .resource_mut::<VirtualGridSources>()
        .register(source_id(), cells);
    h.world_mut().insert_resource(Fluids(vec![water()]));
    let opened = h.open_screen(screen(), Fixture);
    h.settle();
    (h, opened)
}

fn harness() -> (UiHarness, Opened) {
    harness_with(Cells {
        len: 100,
        version: 1,
    })
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

/// Float equality with a tolerance. Every number here comes out of a divide,
/// so `assert_eq!` on one would be asserting the rounding as well as the
/// value.
#[track_caller]
fn close(got: f32, want: f32, what: &str) {
    assert!(
        (got - want).abs() < 1e-4,
        "{what}: expected {want}, got {got}"
    );
}

fn find(h: &UiHarness, test_id: &str) -> Entity {
    h.find(&by::test_id(test_id))
}

/// A theme hot reload, the way the asset loader announces one: `apply_theme`
/// listens for `AssetEvent::Modified` on the active handle and repaints every
/// `Themed` node from its role.
fn reload_theme(h: &mut UiHarness) {
    let id = h.world().resource::<slotted_theme::ActiveTheme>().0.id();
    h.world_mut()
        .write_message(bevy::asset::AssetEvent::Modified { id });
    h.step(2);
}

// ---------------------------------------------------------------------------
// Fill values out of range
// ---------------------------------------------------------------------------

/// A simulation that overshoots leaves the fill full, not overflowing, and the
/// label still reads the real numbers so the overshoot is visible.
#[test]
fn a_value_past_the_maximum_clamps_the_fill_and_keeps_the_real_label() {
    let (mut h, opened) = harness();
    set(&mut h, opened.menu, VALUE, 100_000);

    for id in ["tank", "bar", "progress"] {
        let entity = find(&h, id);
        let fill = h.fill_of(entity).expect("a FillValue");
        close(
            fill.value,
            100_000.0,
            &format!("{id} keeps the value it was given"),
        );
        close(
            fill.fraction(),
            1.0,
            &format!("{id} clamps its fraction to full"),
        );
    }
    close(h.tank_fill(find(&h, "tank")), 1.0, "the tank reads full");
    assert!(
        h.text_of(find(&h, "bar"))
            .is_some_and(|t| t.contains("100000")),
        "the readout still says what the value actually is"
    );
}

/// A value below zero is empty, not negative, and does not invert the fill.
#[test]
fn a_negative_value_reads_as_empty() {
    let (mut h, opened) = harness();
    set(&mut h, opened.menu, VALUE, -5_000);

    for id in ["tank", "bar", "progress"] {
        let fill = h.fill_of(find(&h, id)).expect("a FillValue");
        close(
            fill.fraction(),
            0.0,
            &format!("{id} is empty, not inverted"),
        );
    }
}

/// A maximum of zero is a machine that has not told the screen its capacity
/// yet. Dividing by it must not produce a NaN width.
#[test]
fn a_zero_maximum_is_an_empty_fill_and_not_a_nan() {
    let (mut h, opened) = harness();
    set(&mut h, opened.menu, MAX, 0);
    set(&mut h, opened.menu, VALUE, 50);

    for id in ["tank", "bar", "progress"] {
        let fill = h.fill_of(find(&h, id)).expect("a FillValue");
        close(fill.max, 0.0, &format!("{id} was told a zero maximum"));
        assert!(fill.fraction().is_finite(), "{id} has no NaN fraction");
        close(fill.fraction(), 0.0, &format!("{id} is empty"));
    }
}

/// A negative maximum is nonsense a property write can still produce. It reads
/// the same as zero rather than flipping the fill inside out.
#[test]
fn a_negative_maximum_reads_as_empty() {
    let (mut h, opened) = harness();
    set(&mut h, opened.menu, MAX, -100);
    set(&mut h, opened.menu, VALUE, 50);

    let fill = h.fill_of(find(&h, "bar")).expect("a FillValue");
    assert!(fill.fraction().is_finite());
    close(fill.fraction(), 0.0, "a negative maximum reads as empty");
}

/// `FillValue::fraction` is the one place the arithmetic lives, so pin its
/// edges directly as well as through a screen.
#[test]
fn the_fraction_is_defined_for_every_pair_of_numbers() {
    let cases = [
        (50.0, 100.0, 0.5),
        (200.0, 100.0, 1.0),
        (-1.0, 100.0, 0.0),
        (1.0, 0.0, 0.0),
        (1.0, -1.0, 0.0),
        (0.0, 0.0, 0.0),
    ];
    for (value, max, want) in cases {
        let fill = FillValue { value, max };
        close(fill.fraction(), want, &format!("{value} / {max}"));
    }
}

// ---------------------------------------------------------------------------
// Fluids
// ---------------------------------------------------------------------------

/// A `fluid_property` naming an id no pack registered keeps the tank painted
/// with its theme role and says so once, not once a frame.
#[test]
fn an_unknown_fluid_falls_back_to_the_theme_role_and_warns_once() {
    let (mut h, opened) = harness();
    set(&mut h, opened.menu, FLUID, 77);
    let tank = find(&h, "tank");

    assert_eq!(
        h.world().get::<TankFluid>(tank).map(|f| f.0),
        Some(Some(slotted_ui::FluidId(77))),
        "the tank keeps the id it was told, so the id is what the warning names"
    );
    assert_eq!(
        h.world().get::<UnknownFluidWarned>(tank),
        Some(&UnknownFluidWarned(Some(slotted_ui::FluidId(77)))),
        "the tank has complained once"
    );

    // Fifty more frames must not add a second complaint: the marker is what
    // stops the render path repeating itself.
    h.step(50);
    assert_eq!(
        h.world().get::<UnknownFluidWarned>(tank),
        Some(&UnknownFluidWarned(Some(slotted_ui::FluidId(77)))),
        "still exactly the one warning"
    );

    // And the tank still works: pointing it at a real fluid recovers.
    set(&mut h, opened.menu, FLUID, 0);
    assert_eq!(
        h.world().get::<TankFluid>(tank).map(|f| f.0),
        Some(Some(slotted_ui::FluidId(0)))
    );
}

// ---------------------------------------------------------------------------
// Side tabs
// ---------------------------------------------------------------------------

/// A theme hot reload repaints every node from its role. An open tab must come
/// back open: the state is a component, not something the theme owns.
#[test]
fn an_open_side_tab_survives_a_theme_reload() {
    let (mut h, _) = harness();
    let tab = find(&h, "tab_a");
    h.toggle_side_tab(tab);
    h.settle();
    assert_eq!(h.side_tab_open(tab), Some(true));
    let content = h.find(&by::test_id("tab_a_body"));
    assert!(h.is_visible(content), "the body shows while open");

    reload_theme(&mut h);
    h.settle();

    assert_eq!(
        h.side_tab_open(tab),
        Some(true),
        "the tab is still open after the theme reloaded"
    );
    assert!(
        h.is_visible(content),
        "and its body is still on screen, not hidden by the repaint"
    );
    assert_eq!(
        h.world()
            .get::<slotted_theme::Themed>(tab)
            .map(|t| t.0.clone()),
        Some(slotted_theme::roles::TAB_SIDE_OPEN),
        "the open role survived too"
    );
}

/// Two tabs in one rail are two independent widgets: opening one leaves the
/// other closed, and only the open one docks the browser out of its way.
#[test]
fn two_tabs_on_one_rail_stack_and_only_the_open_one_excludes() {
    let (mut h, opened) = harness();
    let a = find(&h, "tab_a");
    let b = find(&h, "tab_b");
    let before = h.exclusion_zones(opened.screen).len();

    h.toggle_side_tab(a);
    h.settle();
    assert_eq!(h.side_tab_open(a), Some(true));
    assert_eq!(h.side_tab_open(b), Some(false), "the other tab stays shut");
    let with_one = h.exclusion_zones(opened.screen).len();
    assert_eq!(with_one, before + 1, "exactly one zone appeared");

    h.toggle_side_tab(b);
    h.settle();
    assert_eq!(h.side_tab_open(a), Some(true));
    assert_eq!(h.side_tab_open(b), Some(true));
    assert_eq!(
        h.exclusion_zones(opened.screen).len(),
        before + 2,
        "each open tab publishes its own"
    );

    h.toggle_side_tab(a);
    h.settle();
    assert_eq!(
        h.exclusion_zones(opened.screen).len(),
        before + 1,
        "closing a tab takes its zone back down"
    );
    assert!(
        h.world()
            .get::<ExclusionZone>(h.find(&by::test_id("tab_a_body")))
            .is_none()
            || !h.is_visible(h.find(&by::test_id("tab_a_body"))),
        "a closed tab's body is not a zone the browser has to avoid"
    );

    // Two tabs in a rail sit one above the other rather than on top of each
    // other, which is the whole reason the tab positions nothing itself.
    let (ra, rb) = (h.rect_of(a), h.rect_of(b));
    assert!(
        ra.max.y <= rb.min.y + 1.0 || rb.max.y <= ra.min.y + 1.0,
        "the two tabs stack: {ra:?} and {rb:?}"
    );
}

/// The tab's state is what the harness reads, so a tab that never opened has
/// a closed body and no zone.
#[test]
fn a_side_tab_that_never_opened_publishes_nothing() {
    let (mut h, opened) = harness();
    assert_eq!(h.exclusion_zones(opened.screen).len(), 0);
    let state = h
        .world()
        .get::<SideTabState>(find(&h, "tab_a"))
        .copied()
        .expect("a SideTabState");
    assert!(!state.open);
    close(
        state.width(),
        state.closed_width,
        "a closed tab is header-wide",
    );
}

// ---------------------------------------------------------------------------
// Icon button
// ---------------------------------------------------------------------------

/// A one-state button is a legal def: a mod that ships one mode today and
/// three next release should not have to special-case the first version.
/// Clicking it cycles back to the state it is on, which is a no-op.
#[test]
fn a_single_state_icon_button_is_a_no_op_on_click() {
    let (mut h, opened) = harness();
    let button = find(&h, "one_state");
    let tag = |h: &UiHarness| {
        h.world()
            .get::<Tags>(button)
            .and_then(|t| t.get("state").map(ToOwned::to_owned))
    };
    assert_eq!(tag(&h), Some("only".to_owned()));
    assert_eq!(h.property_of(button), Some((MODE, 0)));

    for _ in 0..5 {
        h.click(button);
        h.settle();
    }
    assert_eq!(tag(&h), Some("only".to_owned()), "still the one state");
    assert_eq!(
        h.property_of(button),
        Some((MODE, 0)),
        "and the bound property never left zero"
    );

    // Backwards is the same no-op, and neither direction panics on the wrap.
    h.cycle(button, false);
    h.settle();
    assert_eq!(tag(&h), Some("only".to_owned()));
    assert_eq!(h.property_of(button), Some((MODE, 0)));
    let _ = opened;
}

/// A property write naming a state index the button does not have must not
/// index past the end of `states`.
#[test]
fn a_property_past_the_last_state_does_not_index_out_of_bounds() {
    let (mut h, opened) = harness();
    let button = find(&h, "one_state");
    set(&mut h, opened.menu, MODE, 9);
    h.settle();

    let state = h
        .world()
        .get::<slotted_ui::IconButtonState>(button)
        .expect("an IconButtonState");
    assert!(
        state.current < state.states.len(),
        "current stays inside states: {} of {}",
        state.current,
        state.states.len()
    );
}

// ---------------------------------------------------------------------------
// Virtual grid
// ---------------------------------------------------------------------------

/// An empty source is a grid with no cells and no scrollbar travel, not a
/// division by zero.
#[test]
fn an_empty_source_is_an_empty_grid() {
    let (h, _) = harness_with(Cells { len: 0, version: 1 });
    let grid = find(&h, "grid");
    let state = h
        .world()
        .get::<VirtualGridState>(grid)
        .expect("a VirtualGridState");
    assert_eq!(state.total, 0);
    assert_eq!(state.first_row, 0);
    assert_eq!(state.max_first_row(), 0, "there is nowhere to scroll to");
    assert!(
        h.find_all(&by::tag("cell", "0")).is_empty(),
        "no cell was spawned"
    );
}

/// A source that shrinks under a scrolled grid pulls the window back to the
/// end of what is left, rather than showing a page of nothing or reading past
/// the source.
#[test]
fn a_source_that_shrinks_under_a_scrolled_grid_pulls_the_window_back() {
    let (mut h, _) = harness_with(Cells {
        len: 300,
        version: 1,
    });
    let grid = find(&h, "grid");

    // Scroll deep into the source.
    h.world_mut()
        .get_mut::<VirtualGridState>(grid)
        .expect("a VirtualGridState")
        .first_row = 90;
    h.settle();
    assert_eq!(
        h.world().get::<VirtualGridState>(grid).unwrap().first_row,
        90
    );

    // The source refilters down to six cells: two rows of three.
    h.world_mut()
        .resource_mut::<VirtualGridSources>()
        .register(source_id(), Cells { len: 6, version: 2 });
    h.settle();

    let state = h
        .world()
        .get::<VirtualGridState>(grid)
        .cloned()
        .expect("a VirtualGridState");
    assert_eq!(state.total, 6, "the grid saw the new length");
    assert!(
        state.first_row <= state.max_first_row(),
        "the window is inside the source: first_row {} of max {}",
        state.first_row,
        state.max_first_row()
    );
    assert!(
        h.find_all(&by::tag("cell", "90")).is_empty(),
        "no cell from the old window survived"
    );
    assert!(
        !h.find_all(&by::tag("cell", "0")).is_empty(),
        "the grid shows what is left"
    );
}

/// Scrolling a grid past either end stops at the end instead of running the
/// window negative or off the source.
#[test]
fn scrolling_past_either_end_stops_at_the_end() {
    let (mut h, _) = harness_with(Cells {
        len: 12,
        version: 1,
    });
    let grid = find(&h, "grid");

    h.scroll(grid, Vec2::new(0.0, -100.0));
    h.settle();
    let state = h.world().get::<VirtualGridState>(grid).cloned().unwrap();
    assert_eq!(
        state.first_row,
        state.max_first_row(),
        "scrolling down stops at the last page"
    );

    h.scroll(grid, Vec2::new(0.0, 100.0));
    h.settle();
    assert_eq!(
        h.world().get::<VirtualGridState>(grid).unwrap().first_row,
        0,
        "scrolling up stops at the top"
    );
}

/// A grid pointed at a source nobody registered is empty and says so, rather
/// than taking the screen down with it.
#[test]
fn an_unknown_source_is_an_empty_grid_and_not_a_panic() {
    let mut h = UiHarness::builder()
        .plugins(SlottedPlugins::headless())
        .registries(TestRegistries::basic())
        .theme("glass")
        .build();
    h.world_mut().insert_resource(Fluids(vec![water()]));
    // No `VirtualGridSources::register`: the screen names a source that is
    // not there, which is what a mod does when its data pack failed to load.
    let _opened = h.open_screen(screen(), Fixture);
    h.settle();

    let state = h
        .world()
        .get::<VirtualGridState>(find(&h, "grid"))
        .cloned()
        .expect("the grid still spawned");
    assert_eq!(state.total, 0);
    assert!(h.find_all(&by::tag("cell", "0")).is_empty());
}

// ---------------------------------------------------------------------------
// HUD
// ---------------------------------------------------------------------------

/// Placing a layer relative to an id nobody registered is a mod's typo. It
/// comes back as an error naming the id, and the registry is untouched.
#[test]
fn placing_against_a_missing_layer_is_an_error_and_not_a_panic() {
    let mut layers = HudLayers::default();
    layers.register(HudLayerDef::empty(HudLayerId::new("real")));
    let before = layers.order().to_vec();
    let missing = HudLayerId::new("nope");

    let replace = layers.replace(&missing, HudLayerDef::empty(missing.clone()));
    assert!(replace.is_err(), "replace of a missing id is an error");
    assert!(
        replace.unwrap_err().to_string().contains("nope"),
        "and the message names the id"
    );
    assert!(
        layers
            .insert_above(&missing, HudLayerDef::empty(HudLayerId::new("a")))
            .is_err()
    );
    assert!(
        layers
            .insert_below(&missing, HudLayerDef::empty(HudLayerId::new("b")))
            .is_err()
    );
    assert!(layers.remove(&missing).is_none(), "removing it is a None");
    layers.set_visible(&missing, false);

    assert_eq!(
        layers.order(),
        before.as_slice(),
        "nothing moved: a failed placement leaves the order alone"
    );
    assert!(layers.get(&missing).is_none());
}

/// A `HudUpdate` whose path matches no node in the layer is reported and
/// dropped. A mod renaming a `test_id` must not take the frame down.
#[test]
fn a_hud_update_with_a_bad_path_is_dropped_and_not_a_panic() {
    let mut h = UiHarness::builder()
        .plugins(SlottedPlugins::headless())
        .resolution(1280.0, 720.0)
        .build();
    h.settle();

    let crosshair = builtin::CROSSHAIR;
    assert!(
        h.hud_layer("crosshair").is_some(),
        "the built-in layer is spawned"
    );

    for value in [
        HudValue::Text("hello".to_owned()),
        HudValue::Fill {
            value: 1.0,
            max: 2.0,
        },
        HudValue::Visible(false),
    ] {
        h.world_mut().write_message(HudUpdate {
            layer: crosshair.clone(),
            path: "no_such_node".to_owned(),
            value,
        });
        h.step(2);
    }

    // The layer is still there and still the same layer.
    assert!(h.hud_layer("crosshair").is_some());

    // A path on a layer that is not even spawned is the same non-event.
    h.world_mut().write_message(HudUpdate {
        layer: HudLayerId::new("not:a:layer"),
        path: "whatever".to_owned(),
        value: HudValue::Text("x".to_owned()),
    });
    h.step(2);
    assert!(h.hud_layer("crosshair").is_some());
}

// ---------------------------------------------------------------------------
// Recording and replay
// ---------------------------------------------------------------------------

/// A recording carries pointer positions and no locators, so replaying it into
/// a window of another size would click whatever happens to be under the old
/// coordinates. The harness refuses instead, naming both sizes.
///
/// Contract 2.3 asked for a warning here; a warning is not enough, because the
/// replay would still report every input delivered and pass green.
#[test]
fn a_recording_replayed_at_another_resolution_fails_loudly() {
    use slotted_ui::recording::{RecordedButton, RecordedFrame, RecordedInput, Recording};

    let recorded_at = Vec2::new(1280.0, 720.0);
    let recording = Recording {
        version: slotted_ui::RECORDING_VERSION,
        resolution: recorded_at,
        scale_factor: 1.0,
        frame_delta_us: 16_666,
        frames: vec![RecordedFrame {
            frame: 1,
            inputs: vec![
                RecordedInput::PointerMove {
                    pos: Vec2::new(100.0, 100.0),
                },
                RecordedInput::PointerPress(RecordedButton::Primary),
                RecordedInput::PointerRelease(RecordedButton::Primary),
            ],
        }],
    };

    // Same size: it replays.
    let mut same = UiHarness::builder()
        .plugins(SlottedPlugins::headless())
        .resolution(recorded_at.x, recorded_at.y)
        .build();
    let report = same
        .replay_recording(&recording)
        .expect("a recording replays into the window it was made in");
    assert_eq!(report.inputs, 3);

    // A different window: refused, with both sizes in the message.
    let mut other = UiHarness::builder()
        .plugins(SlottedPlugins::headless())
        .resolution(1600.0, 900.0)
        .build();
    let error = other
        .replay_recording(&recording)
        .expect_err("a recording made at another size is refused");
    let message = error.to_string();
    assert!(
        message.contains("1280"),
        "names the recorded size: {message}"
    );
    assert!(message.contains("1600"), "names this window: {message}");

    // And a different scale factor, which moves every logical position too.
    let mut scaled = UiHarness::builder()
        .plugins(SlottedPlugins::headless())
        .resolution(recorded_at.x, recorded_at.y)
        .scale_factor(2.0)
        .build();
    let error = scaled
        .replay_recording(&recording)
        .expect_err("a recording made at another scale factor is refused");
    assert!(
        error.to_string().contains("scale factor"),
        "the message says which of the two it was: {error}"
    );
}
