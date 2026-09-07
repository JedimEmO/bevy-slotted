//! Headless tests for the layout model (menus contract 1.4 and 3.5): every
//! `Layout` field maps onto `Node`, `Length` parses each spelling, `place`
//! lands a node in the parent's corner or centre through real layout,
//! `center: true` still centres, and `overflow: scroll` makes a scroll
//! container.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::float_cmp,
    clippy::similar_names,
    clippy::items_after_statements
)]

use std::sync::Arc;

use bevy::prelude::*;
use pretty_assertions::assert_eq;
use slotted_test::prelude::*;
use slotted_theme::Motion;
use slotted_ui::def::{
    Align, Justify, Length, NineAnchor, Overflow, Padding, Place, ScreenDef, ScreenKind, Tags,
    UiNodeDef, nine_anchor_node,
};
use slotted_ui::widgets::{BORDER_WIDTH, layout_node, length_val};
use slotted_ui::{Layout, LayoutDirection, Presentation, Screens, push_screen};

const WIDTH: f32 = 1280.0;
const HEIGHT: f32 = 720.0;
const SM: f32 = 6.0;

fn harness() -> UiHarness {
    UiHarness::builder()
        .plugins(SlottedPlugins::headless())
        .resolution(WIDTH, HEIGHT)
        .theme("glass")
        .motion(Motion::REDUCED)
        .build()
}

fn panel(id: &str, layout: Layout, children: Vec<UiNodeDef>) -> UiNodeDef {
    UiNodeDef::Panel {
        role: slotted_theme::roles::PANEL,
        layout,
        children,
        tags: Tags::new().with(Tags::TEST_ID, id),
    }
}

/// A screen whose root panel fills the window, with `children` inside it.
fn full_screen(children: Vec<UiNodeDef>, root_layout: Layout) -> ScreenDef {
    ScreenDef {
        kind: ScreenKind::new("t:layout"),
        initial_focus: None,
        presentation: Presentation::default(),
        inherits: None,
        remove: vec![],
        listring: vec![],
        root: panel(
            "root",
            Layout {
                width: Some("fill".parse().unwrap()),
                height: Some("fill".parse().unwrap()),
                ..root_layout
            },
            children,
        ),
    }
}

fn open(h: &mut UiHarness, screen: ScreenDef) {
    let def: Arc<ScreenDef> = h.world_mut().resource_mut::<Screens>().register(screen);
    push_screen(&mut h.world_mut().commands(), def, None);
    h.world_mut().flush();
    h.settle();
}

fn sized(w: f32, h: f32) -> Layout {
    Layout {
        width: Some(Length::Px(w)),
        height: Some(Length::Px(h)),
        ..Layout::default()
    }
}

#[test]
fn layout_node_maps_every_field() {
    let layout = Layout {
        direction: LayoutDirection::Row,
        gap: 2.0,
        padding: Padding {
            top: 1.0,
            right: 2.0,
            bottom: 3.0,
            left: 4.0,
        },
        width: Some(Length::Px(100.0)),
        height: Some(Length::Percent(50.0)),
        min_width: Some(Length::Steps(3.0)),
        max_width: Some(Length::Auto),
        min_height: Some(Length::Px(10.0)),
        max_height: Some(Length::Percent(100.0)),
        align: Some(Align::Stretch),
        justify: Justify::SpaceBetween,
        grow: 1.5,
        wrap: true,
        overflow: Overflow::Scroll,
        place: Some(Place {
            anchor: NineAnchor::BottomRight,
            offset: Vec2::new(-8.0, -4.0),
        }),
        center: false,
    };
    let node = layout_node(&layout, SM);
    assert_eq!(node.display, Display::Flex);
    assert_eq!(node.flex_direction, FlexDirection::Row);
    assert_eq!(node.row_gap, Val::Px(12.0));
    assert_eq!(node.column_gap, Val::Px(12.0));
    assert_eq!(
        node.padding,
        UiRect::new(Val::Px(24.0), Val::Px(12.0), Val::Px(6.0), Val::Px(18.0))
    );
    assert_eq!(node.width, Val::Px(100.0));
    assert_eq!(node.height, Val::Percent(50.0));
    assert_eq!(node.min_width, Val::Px(18.0));
    assert_eq!(node.max_width, Val::Auto);
    assert_eq!(node.min_height, Val::Px(10.0));
    assert_eq!(node.max_height, Val::Percent(100.0));
    assert_eq!(node.align_items, AlignItems::Stretch);
    assert_eq!(node.justify_content, JustifyContent::SpaceBetween);
    assert_eq!(node.flex_grow, 1.5);
    assert_eq!(node.flex_wrap, FlexWrap::Wrap);
    assert_eq!(node.overflow, bevy::ui::Overflow::scroll_y());
    assert_eq!(node.position_type, PositionType::Absolute);
    assert_eq!(node.right, Val::Px(8.0));
    assert_eq!(node.bottom, Val::Px(4.0));
    assert_eq!(node.left, Val::Auto);
    assert_eq!(node.top, Val::Auto);

    let defaults = layout_node(&Layout::default(), SM);
    assert_eq!(defaults.flex_direction, FlexDirection::Column);
    assert_eq!(defaults.align_items, AlignItems::FlexStart);
    assert_eq!(defaults.justify_content, JustifyContent::FlexStart);
    assert_eq!(defaults.flex_grow, 0.0);
    assert_eq!(defaults.flex_wrap, FlexWrap::NoWrap);
    assert_eq!(defaults.overflow, bevy::ui::Overflow::visible());
    assert_eq!(defaults.position_type, PositionType::Relative);
    assert_eq!(defaults.width, Val::Auto);

    let centred = layout_node(
        &Layout {
            center: true,
            ..Layout::default()
        },
        SM,
    );
    assert_eq!(centred.align_items, AlignItems::Center);
    let explicit = layout_node(
        &Layout {
            center: true,
            align: Some(Align::End),
            ..Layout::default()
        },
        SM,
    );
    assert_eq!(explicit.align_items, AlignItems::FlexEnd, "align wins");
}

#[test]
fn length_parses_every_spelling_and_rejects_units() {
    assert_eq!(length_val(None, SM), Val::Auto);
    assert_eq!(length_val(Some(Length::Steps(2.0)), SM), Val::Px(12.0));
    #[derive(serde::Deserialize)]
    struct Wrap {
        width: Option<Length>,
    }
    let parse = |src: &str| {
        ron::from_str::<Wrap>(&format!("#![enable(implicit_some)] (width: {src})")).map(|w| w.width)
    };
    assert_eq!(parse("12").unwrap(), Some(Length::Px(12.0)));
    assert_eq!(parse("12.5").unwrap(), Some(Length::Px(12.5)));
    assert_eq!(parse("\"50%\"").unwrap(), Some(Length::Percent(50.0)));
    assert_eq!(parse("\"fill\"").unwrap(), Some(Length::Percent(100.0)));
    assert_eq!(parse("\"auto\"").unwrap(), Some(Length::Auto));
    assert_eq!(parse("\"3s\"").unwrap(), Some(Length::Steps(3.0)));
    assert_eq!(parse("None").unwrap(), None);
    let err = parse("\"12px\"").unwrap_err().to_string();
    assert!(err.contains("12px") && err.contains("50%"), "{err}");
}

#[test]
fn nine_anchor_node_covers_the_nine_points() {
    let window = Vec2::new(WIDTH, HEIGHT);
    let known = nine_anchor_node(NineAnchor::Center, Vec2::new(4.0, -6.0), Some(window));
    assert_eq!(known.left, Val::Px(WIDTH * 0.5 + 4.0));
    assert_eq!(known.top, Val::Px(HEIGHT * 0.5 - 6.0));
    let unknown = nine_anchor_node(NineAnchor::Center, Vec2::new(4.0, -6.0), None);
    assert_eq!(unknown.left, Val::Percent(50.0));
    assert_eq!(unknown.top, Val::Percent(50.0));
    assert_eq!(unknown.margin.left, Val::Px(4.0));
    assert_eq!(unknown.margin.top, Val::Px(-6.0));
    let top_left = nine_anchor_node(NineAnchor::TopLeft, Vec2::new(3.0, 5.0), None);
    assert_eq!((top_left.left, top_left.top), (Val::Px(3.0), Val::Px(5.0)));
    let bottom = nine_anchor_node(NineAnchor::Bottom, Vec2::new(0.0, -5.0), None);
    assert_eq!(
        (bottom.left, bottom.bottom),
        (Val::Percent(50.0), Val::Px(5.0))
    );
    let right = nine_anchor_node(NineAnchor::Right, Vec2::new(-7.0, 0.0), None);
    assert_eq!((right.right, right.top), (Val::Px(7.0), Val::Percent(50.0)));
}

#[test]
fn place_bottom_right_puts_the_rect_in_the_corner() {
    let mut h = harness();
    open(
        &mut h,
        full_screen(
            vec![panel(
                "corner",
                Layout {
                    place: Some(Place {
                        anchor: NineAnchor::BottomRight,
                        offset: Vec2::new(-10.0, -20.0),
                    }),
                    ..sized(100.0, 50.0)
                },
                vec![],
            )],
            Layout::default(),
        ),
    );
    let root = h.rect_of(h.find(&by::test_id("root")));
    assert_eq!(
        root.size(),
        Vec2::new(WIDTH, HEIGHT),
        "the root fills the window"
    );
    let corner = h.rect_of(h.find(&by::test_id("corner")));
    assert_eq!(corner.size(), Vec2::new(100.0, 50.0));
    // Insets are measured inside the parent's border.
    assert_eq!(
        corner.max,
        Vec2::new(WIDTH - BORDER_WIDTH - 10.0, HEIGHT - BORDER_WIDTH - 20.0)
    );
}

#[test]
fn place_center_centres_without_knowing_the_parent_size() {
    let mut h = harness();
    open(
        &mut h,
        full_screen(
            vec![
                panel(
                    "middle",
                    Layout {
                        place: Some(Place {
                            anchor: NineAnchor::Center,
                            offset: Vec2::new(10.0, 0.0),
                        }),
                        ..sized(100.0, 50.0)
                    },
                    vec![],
                ),
                panel(
                    "top",
                    Layout {
                        place: Some(Place {
                            anchor: NineAnchor::Top,
                            offset: Vec2::new(0.0, 8.0),
                        }),
                        ..sized(60.0, 30.0)
                    },
                    vec![],
                ),
            ],
            Layout::default(),
        ),
    );
    let middle = h.rect_of(h.find(&by::test_id("middle")));
    assert_eq!(middle.center(), Vec2::new(WIDTH * 0.5 + 10.0, HEIGHT * 0.5));
    let top = h.rect_of(h.find(&by::test_id("top")));
    assert_eq!(top.center().x, WIDTH * 0.5);
    assert_eq!(top.min.y, BORDER_WIDTH + 8.0);
}

#[test]
fn center_true_still_centres_and_fill_fills() {
    let mut h = harness();
    open(
        &mut h,
        full_screen(
            vec![
                panel("child", sized(100.0, 50.0), vec![]),
                panel(
                    "wide",
                    Layout {
                        width: Some("fill".parse().unwrap()),
                        height: Some("2s".parse().unwrap()),
                        ..Layout::default()
                    },
                    vec![],
                ),
            ],
            Layout {
                center: true,
                ..Layout::default()
            },
        ),
    );
    let child = h.rect_of(h.find(&by::test_id("child")));
    assert_eq!(child.center().x, WIDTH * 0.5);
    let wide = h.rect_of(h.find(&by::test_id("wide")));
    assert_eq!(wide.width(), WIDTH - 2.0 * BORDER_WIDTH);
    assert_eq!(wide.height(), 2.0 * SM);
}

#[test]
fn overflow_scroll_makes_a_scroll_container() {
    let mut h = harness();
    open(
        &mut h,
        full_screen(
            vec![panel(
                "list",
                Layout {
                    overflow: Overflow::Scroll,
                    ..sized(200.0, 100.0)
                },
                // `min_height` rather than `height`: a flex child shrinks to
                // fit unless something forbids it, scroll container or not.
                (0..10)
                    .map(|i| {
                        panel(
                            &format!("row{i}"),
                            Layout {
                                width: Some(Length::Px(180.0)),
                                min_height: Some(Length::Px(40.0)),
                                ..Layout::default()
                            },
                            vec![],
                        )
                    })
                    .collect(),
            )],
            Layout::default(),
        ),
    );
    let list = h.find(&by::test_id("list"));
    assert!(
        h.world().get::<ScrollPosition>(list).is_some(),
        "a scroll panel carries ScrollPosition"
    );
    assert_eq!(
        h.world().get::<Node>(list).map(|n| n.overflow),
        Some(bevy::ui::Overflow::scroll_y())
    );
    assert_eq!(h.rect_of(list).height(), 100.0, "the panel keeps its size");
    let last = h.rect_of(h.find(&by::test_id("row9")));
    assert!(
        last.min.y > h.rect_of(list).max.y,
        "the tail is clipped below"
    );
}
