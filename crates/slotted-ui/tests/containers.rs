//! Headless tests for the containers (menus M1 contract 4.2 to 4.6). Scroll:
//! the wheel moves `ScrollPosition`, focus-follow brings a row into view,
//! page actions scroll and claim, rows do not shrink. List: `Up`/`Down` walk
//! and scroll the window, `Accept` selects and activates with the row tag.
//! Tabs: `TabNext` cycles and wraps, hidden pages take no focus, `bind`
//! carries the id. Decor lays out headless.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::float_cmp,
    clippy::similar_names,
    clippy::items_after_statements
)]

use std::sync::Arc;

use bevy::input_focus::InputFocus;
use bevy::prelude::*;
use bevy::ui_widgets::Activate;
use pretty_assertions::assert_eq;
use slotted_model::Namespaced;
use slotted_test::prelude::*;
use slotted_theme::{Motion, Themed, roles};
use slotted_ui::def::{
    BindDef, ButtonOpts, DataSourceId, Length, LocKey, ScreenDef, ScreenKind, TabDef, Tags,
    TextRole, UiNodeDef,
};
use slotted_ui::{
    FocusMask, FocusedAction, InputDevice, InputMode, Layout, LayoutDirection, ListState,
    Presentation, Screens, ScrollPanel, SetValue, TabsState, TextOpts, UiAction, UiActionClaims,
    UiActionEvent, Value, ValueStore, VirtualGridSource, VirtualGridSources, VirtualGridState,
    push_screen,
};

/// Every `SetValue` and every `Activate` (with the target's tags) seen.
#[derive(Resource, Default)]
struct Seen {
    values: Vec<SetValue>,
    activated: Vec<(Entity, Tags)>,
}

fn collect(mut values: MessageReader<SetValue>, mut seen: ResMut<Seen>) {
    seen.values.extend(values.read().cloned());
}

fn on_activate(activate: On<Activate>, tags: Query<&Tags>, mut seen: ResMut<Seen>) {
    let tags = tags.get(activate.entity).cloned().unwrap_or_default();
    seen.activated.push((activate.entity, tags));
}

struct Collector;

impl Plugin for Collector {
    fn build(&self, app: &mut App) {
        app.init_resource::<Seen>()
            .add_systems(Last, collect)
            .add_observer(on_activate);
    }
}

fn harness() -> UiHarness {
    UiHarness::builder()
        .plugins((SlottedPlugins::headless(), Collector))
        .resolution(1280.0, 720.0)
        .theme("glass")
        .motion(Motion::REDUCED)
        .build()
}

fn screen(root: UiNodeDef) -> ScreenDef {
    ScreenDef {
        kind: ScreenKind::new("t:containers"),
        initial_focus: None,
        presentation: Presentation::default(),
        inherits: None,
        remove: vec![],
        listring: vec![],
        root,
    }
}

fn open(h: &mut UiHarness, screen: ScreenDef) {
    let def: Arc<ScreenDef> = h.world_mut().resource_mut::<Screens>().register(screen);
    push_screen(&mut h.world_mut().commands(), def, None);
    h.world_mut().flush();
    h.settle();
}

fn column(id: &str, width: f32, children: Vec<UiNodeDef>) -> UiNodeDef {
    UiNodeDef::Panel {
        role: roles::PANEL,
        layout: Layout {
            direction: LayoutDirection::Column,
            width: Some(Length::Px(width)),
            ..Layout::default()
        },
        children,
        tags: Tags::new().with(Tags::TEST_ID, id),
    }
}

fn button(id: &str) -> UiNodeDef {
    UiNodeDef::Button {
        widget: None,
        opts: ButtonOpts {
            label: Some(LocKey(id.to_owned())),
            ..Default::default()
        },
        tags: Tags::new().with(Tags::TEST_ID, id),
    }
}

fn text(key: &str) -> UiNodeDef {
    UiNodeDef::Text {
        key: LocKey(key.to_owned()),
        style: TextRole::Body,
        opts: TextOpts::default(),
        tags: Tags::new(),
    }
}

fn act(h: &mut UiHarness, entity: Entity, action: UiAction) {
    h.world_mut().trigger(FocusedAction {
        entity,
        action,
        device: InputDevice::Keyboard,
        repeat: false,
    });
}

fn focused(h: &UiHarness) -> Option<Entity> {
    h.world().resource::<InputFocus>().get()
}

fn scroll_y(h: &UiHarness, panel: Entity) -> f32 {
    let viewport = h.world().get::<ScrollPanel>(panel).unwrap().viewport;
    h.world().get::<ScrollPosition>(viewport).unwrap().y
}

// ---------------------------------------------------------------------------
// Scroll
// ---------------------------------------------------------------------------

/// A 120 px tall scroll panel over ten 40 px buttons.
fn scroll_screen(scrollbar: bool) -> ScreenDef {
    let rows: Vec<UiNodeDef> = (0..10).map(|i| button(&format!("b{i}"))).collect();
    screen(column(
        "root",
        300.0,
        vec![UiNodeDef::Scroll {
            layout: Layout {
                direction: LayoutDirection::Column,
                height: Some(Length::Px(120.0)),
                width: Some(Length::Px(200.0)),
                ..Layout::default()
            },
            scrollbar,
            children: rows,
            tags: Tags::new().with(Tags::TEST_ID, "scroll"),
        }],
    ))
}

#[test]
fn the_wheel_moves_the_scroll_position() {
    let mut h = harness();
    open(&mut h, scroll_screen(true));
    let panel = h.find(&by::test_id("scroll"));
    assert_eq!(scroll_y(&h, panel), 0.0);
    let viewport = h.world().get::<ScrollPanel>(panel).unwrap().viewport;
    assert!(
        h.world()
            .get::<bevy::ui_widgets::ScrollArea>(viewport)
            .is_some(),
        "the viewport is Bevy's scroll area"
    );
    let first = h.find(&by::test_id("b0"));
    // Away from the user is positive `y`; towards is a scroll down.
    h.scroll(first, Vec2::new(0.0, -2.0));
    h.settle();
    assert!(scroll_y(&h, panel) > 0.0, "the wheel scrolled down");
    let after = scroll_y(&h, panel);
    // `b0` has scrolled out of the viewport, so the wheel turns over the
    // panel itself.
    h.scroll(viewport, Vec2::new(0.0, 2.0));
    h.settle();
    assert!(scroll_y(&h, panel) < after, "and back up");

    let bar = h
        .world()
        .get::<Children>(panel)
        .unwrap()
        .iter()
        .find(|c| h.world().get::<bevy::ui_widgets::Scrollbar>(*c).is_some())
        .expect("a scrollbar child");
    assert_eq!(h.world().get::<Themed>(bar).unwrap().0, roles::SCROLL_BAR);
    let thumb = h
        .world()
        .get::<Children>(bar)
        .unwrap()
        .iter()
        .next()
        .unwrap();
    assert_eq!(
        h.world().get::<Themed>(thumb).unwrap().0,
        roles::SCROLL_THUMB
    );
}

#[test]
fn rows_do_not_shrink_and_the_viewport_clips() {
    let mut h = harness();
    open(&mut h, scroll_screen(false));
    let panel = h.find(&by::test_id("scroll"));
    let first = h.rect_of(h.find(&by::test_id("b0")));
    let last = h.rect_of(h.find(&by::test_id("b9")));
    assert_eq!(first.height(), last.height(), "every row keeps its height");
    assert!(first.height() > 30.0, "a row is a control, not a sliver");
    let viewport = h.rect_of(h.world().get::<ScrollPanel>(panel).unwrap().viewport);
    assert!(
        viewport.height() <= 120.0,
        "the viewport is the panel's height"
    );
    assert!(
        last.min.y > viewport.max.y,
        "the tail is below the viewport, not squeezed into it"
    );
    assert!(
        h.world()
            .get::<Children>(panel)
            .unwrap()
            .iter()
            .all(|c| h.world().get::<bevy::ui_widgets::Scrollbar>(c).is_none()),
        "no scrollbar when the screen says so"
    );
}

#[test]
fn focus_follow_brings_a_row_into_view() {
    let mut h = harness();
    open(&mut h, scroll_screen(true));
    let panel = h.find(&by::test_id("scroll"));
    let viewport_entity = h.world().get::<ScrollPanel>(panel).unwrap().viewport;
    let b8 = h.find(&by::test_id("b8"));
    h.set_focus(Some(b8));
    h.settle();
    assert!(scroll_y(&h, panel) > 0.0, "focus scrolled the panel");
    let viewport = h.rect_of(viewport_entity);
    let row = h.rect_of(b8);
    assert!(
        row.min.y >= viewport.min.y - 0.5 && row.max.y <= viewport.max.y + 0.5,
        "the focused row is inside the viewport: row {row:?}, viewport {viewport:?}"
    );
    let b0 = h.find(&by::test_id("b0"));
    h.set_focus(Some(b0));
    h.settle();
    assert_eq!(scroll_y(&h, panel), 0.0, "and back to the top");
}

#[test]
fn page_actions_scroll_by_the_visible_height_and_claim() {
    let mut h = harness();
    open(&mut h, scroll_screen(true));
    let panel = h.find(&by::test_id("scroll"));
    let b0 = h.find(&by::test_id("b0"));
    h.set_focus(Some(b0));
    h.settle();
    let visible = h
        .rect_of(h.world().get::<ScrollPanel>(panel).unwrap().viewport)
        .height();

    act(&mut h, b0, UiAction::PageNext);
    assert!(
        h.world()
            .resource::<UiActionClaims>()
            .is_claimed(UiAction::PageNext),
        "the page action is claimed"
    );
    assert_eq!(scroll_y(&h, panel), visible, "one visible height down");
    act(&mut h, b0, UiAction::PageNext);
    assert_eq!(scroll_y(&h, panel), 2.0 * visible);
    act(&mut h, b0, UiAction::PagePrev);
    assert!(
        h.world()
            .resource::<UiActionClaims>()
            .is_claimed(UiAction::PagePrev)
    );
    assert_eq!(scroll_y(&h, panel), visible, "and one back up");
    for _ in 0..20 {
        act(&mut h, b0, UiAction::PageNext);
    }
    let max = scroll_y(&h, panel);
    act(&mut h, b0, UiAction::PageNext);
    assert_eq!(scroll_y(&h, panel), max, "clamped at the end");
}

// ---------------------------------------------------------------------------
// List
// ---------------------------------------------------------------------------

fn source_id() -> DataSourceId {
    DataSourceId(Namespaced::parse("t:rows").expect("id"))
}

struct Rows(usize);

impl VirtualGridSource for Rows {
    fn len(&self) -> usize {
        self.0
    }
    fn version(&self) -> u64 {
        1
    }
    fn cell(&self, index: usize) -> UiNodeDef {
        text(&format!("row {index}"))
    }
}

fn list_screen(bind: Option<&str>) -> ScreenDef {
    screen(column(
        "root",
        300.0,
        vec![
            button("above"),
            UiNodeDef::List {
                source: source_id(),
                rows: 4,
                bind: BindDef {
                    bind: bind.map(str::to_owned),
                    property: None,
                    disabled: false,
                },
                tags: Tags::new().with(Tags::TEST_ID, "list"),
            },
        ],
    ))
}

fn open_list(h: &mut UiHarness, total: usize, bind: Option<&str>) -> Entity {
    h.world_mut()
        .resource_mut::<VirtualGridSources>()
        .register(source_id(), Rows(total));
    open(h, list_screen(bind));
    h.find(&by::test_id("list"))
}

fn row(h: &UiHarness, index: usize) -> Option<Entity> {
    h.try_find(&by::tag("row", &index.to_string()))
}

#[test]
fn up_and_down_walk_the_rows_and_scroll_the_window() {
    let mut h = harness();
    let list = open_list(&mut h, 10, None);
    assert_eq!(
        h.world().get::<ListState>(list).unwrap(),
        &ListState {
            selected: None,
            len: 10
        }
    );
    assert_eq!(
        h.find_all(&by::role(slotted_ui::SemanticRole::ListItem))
            .len(),
        4
    );
    let compact = slotted_ui::screen::active_tokens(h.world())
        .sizes
        .control_height_compact;
    assert_eq!(h.rect_of(row(&h, 0).unwrap()).height(), compact);

    let r0 = row(&h, 0).unwrap();
    h.set_focus(Some(r0));
    act(&mut h, r0, UiAction::Down);
    assert!(
        h.world()
            .resource::<UiActionClaims>()
            .is_claimed(UiAction::Down),
        "the walk claims the action"
    );
    h.step(1);
    assert_eq!(focused(&h), row(&h, 1), "Down moved to the next row");

    for i in 1..4 {
        let r = row(&h, i).unwrap();
        act(&mut h, r, UiAction::Down);
        h.step(1);
    }
    h.settle();
    let state = h.world().get::<VirtualGridState>(list).unwrap();
    assert_eq!(state.first_row, 1, "the window scrolled by one row");
    assert_eq!(
        focused(&h),
        row(&h, 4),
        "focus followed onto the row that scrolled in"
    );
    assert!(row(&h, 0).is_none(), "row 0 scrolled out");

    let r4 = row(&h, 4).unwrap();
    act(&mut h, r4, UiAction::Up);
    h.step(1);
    assert_eq!(focused(&h), row(&h, 3));

    // The top edge leaves the action alone, so focus can leave the list.
    h.world_mut().resource_mut::<UiActionClaims>().clear();
    let r1 = row(&h, 1).unwrap();
    h.set_focus(Some(r1));
    act(&mut h, r1, UiAction::Up);
    h.step(1);
    h.settle();
    assert_eq!(
        h.world().get::<VirtualGridState>(list).unwrap().first_row,
        0,
        "Up at the top of the window scrolled it back"
    );
    assert_eq!(focused(&h), row(&h, 0));
    let r0 = row(&h, 0).unwrap();
    act(&mut h, r0, UiAction::Up);
    assert!(
        !h.world()
            .resource::<UiActionClaims>()
            .is_claimed(UiAction::Up),
        "Up on the first row is not claimed"
    );
}

#[test]
fn accept_selects_and_activates_with_the_row_tag() {
    let mut h = harness();
    let list = open_list(&mut h, 6, Some("list.pick"));
    let r2 = row(&h, 2).unwrap();
    h.set_focus(Some(r2));
    act(&mut h, r2, UiAction::Accept);
    assert!(
        h.world()
            .resource::<UiActionClaims>()
            .is_claimed(UiAction::Accept),
        "Accept is claimed"
    );
    h.step(1);
    assert_eq!(h.world().get::<ListState>(list).unwrap().selected, Some(2));
    let seen = h.world().resource::<Seen>();
    assert_eq!(
        seen.values,
        vec![SetValue {
            key: "list.pick".into(),
            value: Value::Int(2),
            source: Some(list),
        }]
    );
    assert_eq!(seen.activated.len(), 1);
    let (entity, tags) = &seen.activated[0];
    assert_eq!(*entity, r2);
    assert_eq!(tags.get("row"), Some("2"));
    assert_eq!(
        h.world().get::<Themed>(r2).unwrap().0,
        roles::LIST_ROW_SELECTED
    );

    // A click selects too.
    let r3 = row(&h, 3).unwrap();
    h.click(r3);
    h.settle();
    assert_eq!(h.world().get::<ListState>(list).unwrap().selected, Some(3));
    assert_eq!(h.world().resource::<Seen>().activated.len(), 2);

    // The store paints the selection.
    h.world_mut()
        .resource_mut::<ValueStore>()
        .insert("list.pick", 1_i64);
    h.settle();
    assert_eq!(h.world().get::<ListState>(list).unwrap().selected, Some(1));
}

// ---------------------------------------------------------------------------
// Tabs
// ---------------------------------------------------------------------------

fn tabs_screen(bind: Option<&str>) -> ScreenDef {
    let tab = |id: &str| TabDef {
        id: id.to_owned(),
        label: LocKey(format!("tab.{id}")),
        icon: None,
    };
    screen(column(
        "root",
        400.0,
        vec![UiNodeDef::Tabs {
            tabs: vec![tab("display"), tab("audio"), tab("controls")],
            bind: BindDef {
                bind: bind.map(str::to_owned),
                property: None,
                disabled: false,
            },
            children: vec![
                column(
                    "page.display",
                    380.0,
                    vec![button("display.a"), button("display.b")],
                ),
                column("page.audio", 380.0, vec![button("audio.a")]),
                column("page.controls", 380.0, vec![button("controls.a")]),
            ],
            tags: Tags::new().with(Tags::TEST_ID, "tabs"),
        }],
    ))
}

fn tab_action(h: &mut UiHarness, action: UiAction) {
    h.world_mut().write_message(UiActionEvent {
        action,
        device: InputDevice::Keyboard,
        repeat: false,
    });
    h.step(1);
}

fn active(h: &UiHarness, tabs: Entity) -> usize {
    h.world().get::<TabsState>(tabs).unwrap().active
}

#[test]
fn tab_next_cycles_and_wraps_and_bind_carries_the_id() {
    let mut h = harness();
    open(&mut h, tabs_screen(Some("settings.tab")));
    let tabs = h.find(&by::test_id("tabs"));
    let display = h.find(&by::test_id("page.display"));
    let audio = h.find(&by::test_id("page.audio"));
    let controls = h.find(&by::test_id("page.controls"));
    assert_eq!(active(&h, tabs), 0);
    assert!(h.is_visible(display));
    assert!(!h.is_visible(audio));
    assert!(h.world().get::<FocusMask>(audio).is_some());
    assert!(h.world().get::<FocusMask>(display).is_none());

    // From anywhere: focus is on a button in the first page.
    let a = h.find(&by::test_id("display.a"));
    h.set_focus(Some(a));

    tab_action(&mut h, UiAction::TabNext);
    assert_eq!(active(&h, tabs), 1);
    assert!(
        h.world()
            .resource::<UiActionClaims>()
            .is_claimed(UiAction::TabNext),
        "claimed"
    );
    h.settle();
    assert!(h.is_visible(audio));
    assert!(!h.is_visible(display));
    assert!(h.world().get::<FocusMask>(display).is_some());
    assert!(h.world().get::<FocusMask>(audio).is_none());
    assert_eq!(
        focused(&h),
        Some(h.find(&by::test_id("audio.a"))),
        "focus moved from the old page into the new one"
    );

    tab_action(&mut h, UiAction::TabNext);
    assert_eq!(active(&h, tabs), 2);
    tab_action(&mut h, UiAction::TabNext);
    assert_eq!(active(&h, tabs), 0, "wraps");
    tab_action(&mut h, UiAction::TabPrev);
    assert_eq!(active(&h, tabs), 2, "and backwards");
    h.settle();
    assert!(h.is_visible(controls));

    let ids: Vec<Value> = h
        .world()
        .resource::<Seen>()
        .values
        .iter()
        .filter(|v| v.key == "settings.tab")
        .map(|v| v.value.clone())
        .collect();
    assert_eq!(
        ids,
        vec![
            Value::Text("audio".into()),
            Value::Text("controls".into()),
            Value::Text("display".into()),
            Value::Text("controls".into()),
        ]
    );

    // The bar paints the active tab.
    let bar_buttons = h.find_all(&by::role(slotted_ui::SemanticRole::Tab));
    assert_eq!(bar_buttons.len(), 3);
    let roles_now: Vec<_> = bar_buttons
        .iter()
        .map(|b| h.world().get::<Themed>(*b).unwrap().0.clone())
        .collect();
    assert_eq!(roles_now, vec![roles::TAB, roles::TAB, roles::TAB_ACTIVE]);

    // The store paints the tabs.
    h.world_mut()
        .resource_mut::<ValueStore>()
        .insert("settings.tab", "audio");
    h.settle();
    assert_eq!(active(&h, tabs), 1);
    assert!(h.is_visible(audio));
}

#[test]
fn hidden_pages_take_no_focus_and_a_click_or_accept_on_a_tab_switches() {
    let mut h = harness();
    open(&mut h, tabs_screen(None));
    let tabs = h.find(&by::test_id("tabs"));
    let hidden_button = h.find(&by::test_id("audio.a"));
    h.set_input_mode(InputMode::Keyboard);

    h.set_focus(Some(hidden_button));
    h.step(1);
    assert_ne!(
        focused(&h),
        Some(hidden_button),
        "focus does not rest under a FocusMask"
    );
    assert_eq!(
        focused(&h),
        Some(h.find(&by::test_id("display.a"))),
        "it lands on the active page's first focusable"
    );
    assert_ne!(
        h.focus_ring().target,
        Some(hidden_button),
        "the ring never sat on the hidden button"
    );

    let buttons = h.find_all(&by::role(slotted_ui::SemanticRole::Tab));
    let audio_tab = h.find(&by::tag("tab", "audio"));
    assert!(buttons.contains(&audio_tab));
    h.click(audio_tab);
    h.settle();
    assert_eq!(active(&h, tabs), 1, "a click switches");
    assert!(h.is_visible(hidden_button));

    let controls_tab = h.find(&by::tag("tab", "controls"));
    h.set_focus(Some(controls_tab));
    act(&mut h, controls_tab, UiAction::Accept);
    h.settle();
    assert_eq!(active(&h, tabs), 2, "Accept on a tab switches");
    assert_eq!(
        h.world().get::<TabsState>(tabs).unwrap().active_id(),
        Some("controls")
    );
    assert_eq!(
        h.world().get::<Themed>(controls_tab).unwrap().0,
        roles::TAB_ACTIVE
    );

    // A tab bar's own focus follows the switch.
    tab_action(&mut h, UiAction::TabNext);
    h.settle();
    assert_eq!(active(&h, tabs), 0);
    assert_eq!(
        focused(&h),
        Some(h.find(&by::tag("tab", "display"))),
        "focus that sat on a tab button moved to the new active button"
    );
}

// ---------------------------------------------------------------------------
// Decor
// ---------------------------------------------------------------------------

#[test]
fn decor_lays_out_headless() {
    let mut h = harness();
    open(
        &mut h,
        screen(column(
            "root",
            200.0,
            vec![UiNodeDef::Panel {
                role: roles::PANEL,
                layout: Layout {
                    direction: LayoutDirection::Column,
                    height: Some(Length::Px(300.0)),
                    width: Some(Length::Px(200.0)),
                    ..Layout::default()
                },
                children: vec![
                    button("top"),
                    UiNodeDef::Separator {
                        direction: LayoutDirection::Row,
                        tags: Tags::new().with(Tags::TEST_ID, "rule"),
                    },
                    UiNodeDef::Spacer {
                        size: Length::Percent(100.0),
                        tags: Tags::new().with(Tags::TEST_ID, "fill"),
                    },
                    UiNodeDef::Image {
                        path: "icons/missing.png".into(),
                        width: Length::Px(32.0),
                        height: Length::Px(24.0),
                        tags: Tags::new().with(Tags::TEST_ID, "image"),
                    },
                    UiNodeDef::Spacer {
                        size: Length::Px(10.0),
                        tags: Tags::new().with(Tags::TEST_ID, "gap"),
                    },
                    button("bottom"),
                ],
                tags: Tags::new().with(Tags::TEST_ID, "box"),
            }],
        )),
    );
    let rule = h.rect_of(h.find(&by::test_id("rule")));
    assert_eq!(rule.height(), 1.0, "a hairline");
    let inner = h.rect_of(h.find(&by::test_id("box")));
    assert!(rule.width() >= inner.width() - 4.0, "across the panel");
    assert_eq!(
        h.world()
            .get::<Themed>(h.find(&by::test_id("rule")))
            .unwrap()
            .0,
        roles::SEPARATOR
    );

    let image = h.find(&by::test_id("image"));
    let image_rect = h.rect_of(image);
    assert_eq!(image_rect.size(), Vec2::new(32.0, 24.0));
    assert!(h.world().get::<ImageNode>(image).is_some());
    assert!(h.world().get::<slotted_ui::Decorative>(image).is_some());

    let gap = h.rect_of(h.find(&by::test_id("gap")));
    assert_eq!(gap.height(), 10.0, "a fixed spacer is its length");

    let bottom = h.rect_of(h.find(&by::test_id("bottom")));
    assert!(
        (bottom.max.y - (inner.max.y - 1.0)).abs() < 1.5,
        "a fill spacer pushes the last row to the bottom: {bottom:?} in {inner:?}"
    );
    let fill = h.rect_of(h.find(&by::test_id("fill")));
    assert!(fill.height() > 100.0, "the spacer took the slack");
    for id in ["rule", "fill", "image", "gap"] {
        let e = h.find(&by::test_id(id));
        assert_eq!(
            h.world().get::<slotted_ui::SemanticRole>(e),
            Some(&slotted_ui::SemanticRole::Decor)
        );
        assert!(
            h.world().get::<slotted_ui::Focusable>(e).is_none(),
            "decor takes no focus"
        );
    }
}
