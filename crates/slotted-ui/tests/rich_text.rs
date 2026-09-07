//! Rich text, the grown `text` node and localisation arguments (menus M1
//! contract 2.1 to 2.4): the markup parser, the spans it renders to, key
//! glyphs that follow the input mode, inline icons, wrapping, truncation
//! and arguments.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::BTreeMap;

use bevy::input::gamepad::GamepadButton;
use bevy::prelude::*;
use bevy::text::{ComputedTextBlock, FontStyle, FontWeight};
use slotted_test::prelude::*;
use slotted_theme::{ThemeColor, ThemeSize, roles};
use slotted_ui::def::{Layout, Length, Tags, TextAlign, TextOpts, TextRole, UiNodeDef};
use slotted_ui::rich::{RichError, RichRun, RunKind, RunStyle, parse, substitute};
use slotted_ui::{
    InputMode, LocArgs, LocKey, Localization, Localizer, MaxLines, Presentation, RichKeySpan,
    RichPart, RichRuns, RichText, ScreenDef, UiAction, UiBindings, Value,
};

const SCREEN: &str = "rich:screen";
const PANEL_WIDTH: f32 = 160.0;

fn text(key: &str) -> RichRun {
    RichRun {
        text: key.to_owned(),
        style: RunStyle::default(),
        kind: RunKind::Text,
    }
}

fn styled(key: &str, style: RunStyle) -> RichRun {
    RichRun {
        text: key.to_owned(),
        style,
        kind: RunKind::Text,
    }
}

fn bold() -> RunStyle {
    RunStyle {
        bold: true,
        ..RunStyle::default()
    }
}

// ---------------------------------------------------------------------------
// The parser
// ---------------------------------------------------------------------------

#[test]
fn every_tag_parses_to_a_run_with_its_style() {
    let runs = parse(
        "a [b]b[/b] [i]i[/i] [color=$accent]c[/color] [color=#FF0000]h[/color] \
         [size=heading]s[/size] [size=18]n[/size] {key:accept} {icon:demo:chest}",
    )
    .unwrap();
    assert_eq!(
        runs,
        vec![
            text("a "),
            styled("b", bold()),
            text(" "),
            styled(
                "i",
                RunStyle {
                    italic: true,
                    ..RunStyle::default()
                }
            ),
            text(" "),
            styled(
                "c",
                RunStyle {
                    color: Some(ThemeColor::palette("accent")),
                    ..RunStyle::default()
                }
            ),
            text(" "),
            styled(
                "h",
                RunStyle {
                    color: Some(ThemeColor::hex("#FF0000")),
                    ..RunStyle::default()
                }
            ),
            text(" "),
            styled(
                "s",
                RunStyle {
                    size: Some(ThemeSize::typography("heading")),
                    ..RunStyle::default()
                }
            ),
            text(" "),
            styled(
                "n",
                RunStyle {
                    size: Some(ThemeSize::px(18.0)),
                    ..RunStyle::default()
                }
            ),
            text(" "),
            RichRun {
                text: String::new(),
                style: RunStyle::default(),
                kind: RunKind::Key(UiAction::Accept),
            },
            text(" "),
            RichRun {
                text: String::new(),
                style: RunStyle::default(),
                kind: RunKind::Icon("demo:chest".parse().unwrap()),
            },
        ]
    );
}

#[test]
fn nested_tags_compose_and_unwind_in_order() {
    let runs = parse("[b]bold [i]both[/i] bold[/b] plain").unwrap();
    let both = RunStyle {
        bold: true,
        italic: true,
        ..RunStyle::default()
    };
    assert_eq!(
        runs,
        vec![
            styled("bold ", bold()),
            styled("both", both),
            styled(" bold", bold()),
            text(" plain"),
        ]
    );
    // A placeholder inside a tag carries the tag's style.
    let runs = parse("[b]press {key:back}[/b]").unwrap();
    assert_eq!(runs[1].kind, RunKind::Key(UiAction::Back));
    assert!(runs[1].style.bold);
}

#[test]
fn doubled_brackets_and_braces_are_literals() {
    assert_eq!(
        parse("a [[b]] {{name}} [[ }}").unwrap(),
        vec![text("a [b] {name} [ }")]
    );
    // Adjacent text in one style is one run, even across an empty tag.
    assert_eq!(parse("one [b][/b]two").unwrap(), vec![text("one two")]);
}

#[test]
fn malformed_markup_is_an_error_naming_the_offset() {
    assert_eq!(
        parse("plain [b]bold"),
        Err(RichError::Unclosed {
            tag: "b".to_owned(),
            at: 6
        })
    );
    assert_eq!(
        parse("[b][i]x[/b][/i]"),
        Err(RichError::Stray {
            tag: "b".to_owned(),
            at: 7
        })
    );
    assert_eq!(
        parse("x [/i]"),
        Err(RichError::Stray {
            tag: "i".to_owned(),
            at: 2
        })
    );
    assert_eq!(
        parse("[u]x[/u]"),
        Err(RichError::UnknownTag {
            tag: "u".to_owned(),
            at: 0
        })
    );
    assert!(matches!(
        parse("[color=red]x[/color]"),
        Err(RichError::UnknownTag { at: 0, .. })
    ));
    assert_eq!(
        parse("hi {name}"),
        Err(RichError::UnknownPlaceholder {
            name: "name".to_owned(),
            at: 3
        })
    );
    assert!(matches!(
        parse("{key:jump}"),
        Err(RichError::UnknownPlaceholder { at: 0, .. })
    ));
    assert!(matches!(
        parse("{icon:nocolon}"),
        Err(RichError::UnknownPlaceholder { at: 0, .. })
    ));
}

#[test]
fn an_argument_value_can_never_open_a_tag() {
    let mut args: LocArgs = BTreeMap::new();
    args.insert("name".to_owned(), Value::Text("[b]{key:back}".to_owned()));
    args.insert("n".to_owned(), Value::Int(3));
    let markup = substitute("Hello {name}, {n} times {{n}} {missing}", &args);
    assert_eq!(markup, "Hello [[b]]{{key:back}}, 3 times {{n}} {missing}");
    assert_eq!(
        parse(&markup),
        Err(RichError::UnknownPlaceholder {
            name: "missing".to_owned(),
            at: markup.find("{missing}").unwrap()
        })
    );
    let runs = parse(&substitute("Hello [b]{name}[/b]!", &args)).unwrap();
    assert_eq!(
        runs,
        vec![text("Hello "), styled("[b]{key:back}", bold()), text("!")]
    );
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

/// A catalogue of fixed strings that substitutes `{ $name }` the way Fluent
/// would, so a `.ftl`-shaped string with tags and arguments can be tested
/// without the loader (which lives in `slotted-packs`).
struct Catalogue(Vec<(&'static str, &'static str)>);

impl Localizer for Catalogue {
    fn resolve(&self, key: &LocKey, args: &LocArgs) -> Option<String> {
        let (_, pattern) = self.0.iter().find(|(k, _)| *k == key.0)?;
        let mut out = (*pattern).to_owned();
        for (name, value) in args {
            let shown = match value {
                Value::Bool(b) => b.to_string(),
                Value::Int(i) => i.to_string(),
                Value::Float(f) => f.to_string(),
                Value::Text(t) => t.clone(),
            };
            out = out.replace(&format!("{{ ${name} }}"), &shown);
        }
        Some(out)
    }
}

const GLASS: &str = include_str!("../../../assets/themes/glass.theme.ron");

/// No theme: Bevy's default font, which is what lays text out in a test
/// that has no asset directory (a theme's font file would never land).
fn bare_harness() -> UiHarness {
    UiHarness::builder()
        .plugins(SlottedPlugins::headless())
        .registries(TestRegistries::basic())
        .build()
}

/// A harness with the glass theme added as an asset directly, so the span
/// paints below can be checked against its palette and scale.
fn harness() -> UiHarness {
    let mut h = bare_harness();
    let theme = slotted_theme::Theme::from_ron(GLASS).unwrap();
    let handle = h
        .world_mut()
        .resource_mut::<Assets<slotted_theme::Theme>>()
        .add(theme);
    h.world_mut()
        .insert_resource(slotted_theme::ActiveTheme(handle));
    h
}

fn rich(key: &str, opts: TextOpts, id: &str) -> UiNodeDef {
    UiNodeDef::RichText {
        key: LocKey(key.to_owned()),
        style: TextRole::Body,
        opts,
        tags: Tags::new().with("test_id", id),
    }
}

fn plain(key: &str, opts: TextOpts, id: &str) -> UiNodeDef {
    UiNodeDef::Text {
        key: LocKey(key.to_owned()),
        style: TextRole::Body,
        opts,
        tags: Tags::new().with("test_id", id),
    }
}

fn screen(children: Vec<UiNodeDef>) -> ScreenDef {
    ScreenDef {
        kind: ScreenKind::new(SCREEN),
        initial_focus: None,
        presentation: Presentation::default(),
        inherits: None,
        remove: vec![],
        root: UiNodeDef::Panel {
            role: roles::PANEL,
            layout: Layout {
                width: Some(Length::Px(PANEL_WIDTH)),
                padding: 0.0.into(),
                gap: 4.0,
                ..Layout::default()
            },
            children,
            tags: Tags::new().with("test_id", "panel"),
        },
        listring: vec![],
    }
}

fn open(h: &mut UiHarness, children: Vec<UiNodeDef>) {
    h.open(screen(children));
    h.settle();
}

/// The `TextSpan` children of `root` (through inline fragments too), in
/// tree order, with their font and colour.
fn spans(h: &UiHarness, root: Entity) -> Vec<(String, TextFont, TextColor, Entity)> {
    fn walk(w: &World, e: Entity, out: &mut Vec<(String, TextFont, TextColor, Entity)>) {
        if let Some(span) = w.get::<TextSpan>(e) {
            out.push((
                span.0.clone(),
                w.get::<TextFont>(e).unwrap().clone(),
                *w.get::<TextColor>(e).unwrap(),
                e,
            ));
        }
        if let Some(children) = w.get::<Children>(e) {
            for c in children.iter() {
                walk(w, c, out);
            }
        }
    }
    let mut out = Vec::new();
    walk(h.world(), root, &mut out);
    out
}

fn lines(h: &UiHarness, entity: Entity) -> usize {
    h.world()
        .get::<ComputedTextBlock>(entity)
        .unwrap()
        .buffer()
        .lines()
        .count()
}

fn palette(h: &UiHarness, name: &str) -> Color {
    let theme = h.world().resource::<slotted_theme::ActiveTheme>().0.clone();
    let themes = h.world().resource::<Assets<slotted_theme::Theme>>();
    themes
        .get(&theme)
        .unwrap()
        .color(&ThemeColor::palette(name))
}

#[test]
fn a_paragraph_renders_one_span_per_run_with_weight_colour_and_size() {
    let mut h = harness();
    open(
        &mut h,
        vec![rich(
            "Plain [b]bold[/b] [i]slant[/i] [color=$accent]blue[/color] [size=heading]big[/size]",
            TextOpts::default(),
            "para",
        )],
    );
    let root = h.find(&by::test_id("para"));
    assert!(
        h.world()
            .get::<RichRuns>(root)
            .is_some_and(|r| r.0.len() == 8)
    );
    let spans = spans(&h, root);
    let texts: Vec<&str> = spans.iter().map(|(t, ..)| t.as_str()).collect();
    assert_eq!(
        texts,
        ["Plain ", "bold", " ", "slant", " ", "blue", " ", "big"]
    );
    // Glass `text` is `$body`: 15 px, weight 400.
    let (_, base, base_color, _) = &spans[0];
    assert_eq!(base.font_size, FontSize::Px(15.0));
    assert_eq!(base.weight, FontWeight(400));
    assert_eq!(base_color.0, palette(&h, "text"));
    assert_eq!(spans[1].1.weight, FontWeight(700));
    assert_eq!(spans[3].1.style, FontStyle::Italic);
    assert_eq!(spans[3].1.weight, FontWeight(400));
    assert_eq!(spans[5].2.0, palette(&h, "accent"));
    assert_eq!(spans[7].1.font_size, FontSize::Px(18.0));
    assert_eq!(
        spans[7].1.weight,
        FontWeight(400),
        "[size] changes size only"
    );
    // Every span is a part of the node, and the root's own text is empty.
    assert!(
        spans
            .iter()
            .all(|(.., e)| h.world().get::<RichPart>(*e).is_some())
    );
    assert_eq!(h.world().get::<Text>(root).unwrap().0, "");
}

#[test]
fn a_key_glyph_follows_the_input_mode_and_the_bindings() {
    let mut h = harness();
    open(
        &mut h,
        vec![rich(
            "Press {key:accept} to go",
            TextOpts::default(),
            "hint",
        )],
    );
    let root = h.find(&by::test_id("hint"));
    let key_span = |h: &UiHarness| {
        spans(h, root)
            .into_iter()
            .find(|(.., e)| h.world().get::<RichKeySpan>(*e).is_some())
            .unwrap()
    };
    // Pointer mode shows the keyboard binding.
    assert_eq!(*h.world().resource::<InputMode>(), InputMode::Pointer);
    let (glyph, font, color, _) = key_span(&h);
    assert_eq!(glyph, "Enter");
    // The `text.key` role: accent colour, 13 px, weight 600 in glass.
    assert_eq!(color.0, palette(&h, "accent"));
    assert_eq!(font.font_size, FontSize::Px(13.0));
    assert_eq!(font.weight, FontWeight(600));

    h.gamepad(GamepadButton::North);
    h.settle();
    assert_eq!(*h.world().resource::<InputMode>(), InputMode::Gamepad);
    assert_eq!(key_span(&h).0, "A");

    h.key(KeyCode::ShiftLeft);
    h.settle();
    assert_eq!(key_span(&h).0, "Enter");

    // A rebinding re-renders without a mode change.
    h.world_mut()
        .resource_mut::<UiBindings>()
        .keys
        .insert(UiAction::Accept, vec![KeyCode::KeyF]);
    h.settle();
    assert_eq!(key_span(&h).0, "F");
    // The other runs are untouched.
    assert_eq!(spans(&h, root)[0].0, "Press ");
}

#[test]
fn an_icon_is_a_name_when_wrapped_and_an_image_beside_the_text_when_inline() {
    let mut h = harness();
    open(
        &mut h,
        vec![
            rich(
                "Take {icon:minecraft:cobblestone} now",
                TextOpts::default(),
                "wrapped",
            ),
            rich(
                "Take {icon:minecraft:cobblestone} now",
                TextOpts {
                    inline: true,
                    ..TextOpts::default()
                },
                "inline",
            ),
        ],
    );
    let wrapped = h.find(&by::test_id("wrapped"));
    let spans_w = spans(&h, wrapped);
    assert_eq!(spans_w[1].0, "Cobblestone", "the registry's display name");
    assert_eq!(spans_w[1].2.0, palette(&h, "warm"), "in the text.icon role");
    assert!(
        h.world()
            .get::<Children>(wrapped)
            .unwrap()
            .iter()
            .all(|c| h.world().get::<ImageNode>(c).is_none()),
        "no image in a wrapped paragraph"
    );

    let inline = h.find(&by::test_id("inline"));
    assert!(
        h.world().get::<Text>(inline).is_none(),
        "the inline root is a row"
    );
    let children: Vec<Entity> = h.world().get::<Children>(inline).unwrap().iter().collect();
    assert_eq!(children.len(), 3, "text, image, text: {children:?}");
    assert!(h.world().get::<Text>(children[0]).is_some());
    assert!(h.world().get::<ImageNode>(children[1]).is_some());
    assert!(h.world().get::<Text>(children[2]).is_some());
    let texts: Vec<String> = spans(&h, inline).into_iter().map(|(t, ..)| t).collect();
    assert_eq!(texts, ["Take ", " now"]);
    // The image sits at the base style's line height (15 px × 1.4 in glass).
    let rect = h.rect_of(children[1]);
    assert!((rect.height() - 21.0).abs() < 0.5, "{rect:?}");
    let row = h.rect_of(inline);
    assert!(row.height() < 40.0, "a single line: {row:?}");
}

#[test]
fn a_catalogue_string_carries_tags_and_arguments() {
    let mut h = harness();
    h.world_mut()
        .insert_resource(Localization::new(Catalogue(vec![(
            "hint.pick",
            "[b]{ $name }[/b] x{ $count } {key:back}",
        )])));
    let mut args: LocArgs = BTreeMap::new();
    args.insert("name".to_owned(), Value::Text("[i]Ore".to_owned()));
    args.insert("count".to_owned(), Value::Int(4));
    open(
        &mut h,
        vec![rich(
            "hint.pick",
            TextOpts {
                args,
                ..TextOpts::default()
            },
            "pick",
        )],
    );
    let root = h.find(&by::test_id("pick"));
    let spans = spans(&h, root);
    let texts: Vec<&str> = spans.iter().map(|(t, ..)| t.as_str()).collect();
    assert_eq!(texts, ["[i]Ore", " x4 ", "Esc"]);
    assert_eq!(spans[0].1.weight, FontWeight(700));

    // Replacing the catalogue re-renders the paragraph.
    h.world_mut()
        .insert_resource(Localization::new(Catalogue(vec![(
            "hint.pick",
            "{ $count } [i]Stück[/i]",
        )])));
    h.settle();
    let texts: Vec<String> = self::spans(&h, root).into_iter().map(|(t, ..)| t).collect();
    assert_eq!(texts, ["4 ", "Stück"]);
}

#[test]
fn bad_markup_renders_raw_and_a_text_node_keeps_tags_literal() {
    let mut h = harness();
    open(
        &mut h,
        vec![
            rich("oops [b]never closed", TextOpts::default(), "bad"),
            plain(
                "a [b]literal[/b] {key:accept}",
                TextOpts::default(),
                "plain",
            ),
        ],
    );
    let bad = h.find(&by::test_id("bad"));
    let spans = spans(&h, bad);
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0].0, "oops [b]never closed");
    let plain = h.find(&by::test_id("plain"));
    assert_eq!(
        h.text_of(plain).as_deref(),
        Some("a [b]literal[/b] {key:accept}")
    );
    assert!(h.world().get::<Children>(plain).is_none());
}

#[test]
fn a_text_node_resolves_with_arguments_and_again_when_they_change() {
    let mut h = harness();
    h.world_mut()
        .insert_resource(Localization::new(Catalogue(vec![(
            "status.count",
            "{ $count } items",
        )])));
    let mut args: LocArgs = BTreeMap::new();
    args.insert("count".to_owned(), Value::Int(3));
    open(
        &mut h,
        vec![plain(
            "status.count",
            TextOpts {
                args,
                align: TextAlign::Center,
                ..TextOpts::default()
            },
            "count",
        )],
    );
    let node = h.find(&by::test_id("count"));
    assert_eq!(h.text_of(node).as_deref(), Some("3 items"));
    assert_eq!(
        h.world().get::<TextLayout>(node).unwrap().justify,
        Justify::Center
    );
    h.world_mut()
        .get_mut::<slotted_ui::LocText>(node)
        .unwrap()
        .args
        .insert("count".to_owned(), Value::Int(12));
    h.settle();
    assert_eq!(h.text_of(node).as_deref(), Some("12 items"));
}

const PARAGRAPH: &str = "The quick brown fox jumps over the lazy dog while the panel \
                         stays one hundred and sixty pixels wide";

#[test]
fn a_paragraph_wraps_at_the_panel_width_and_max_lines_truncates() {
    let mut h = bare_harness();
    open(
        &mut h,
        vec![
            plain(PARAGRAPH, TextOpts::default(), "wrap"),
            plain(
                PARAGRAPH,
                TextOpts {
                    max_lines: Some(2),
                    ..TextOpts::default()
                },
                "two",
            ),
            plain(
                PARAGRAPH,
                TextOpts {
                    wrap: false,
                    ..TextOpts::default()
                },
                "nowrap",
            ),
        ],
    );
    let wrap = h.find(&by::test_id("wrap"));
    let two = h.find(&by::test_id("two"));
    let nowrap = h.find(&by::test_id("nowrap"));

    assert!(lines(&h, wrap) >= 4, "{} lines", lines(&h, wrap));
    assert!(h.rect_of(wrap).width() <= PANEL_WIDTH + 0.5);

    assert_eq!(lines(&h, two), 2);
    let shown = h.text_of(two).unwrap();
    assert!(shown.ends_with('…'), "{shown:?}");
    assert!(PARAGRAPH.starts_with(shown.trim_end_matches('…').trim_end()));
    assert!(shown.len() < PARAGRAPH.len());
    assert_eq!(h.world().get::<MaxLines>(two), Some(&MaxLines(2)));

    assert_eq!(lines(&h, nowrap), 1);
    assert_eq!(h.text_of(nowrap).as_deref(), Some(PARAGRAPH));
    assert!(h.rect_of(nowrap).width() > PANEL_WIDTH);
    assert_eq!(
        h.world().get::<TextLayout>(nowrap).unwrap().linebreak,
        LineBreak::NoWrap
    );

    // A wider panel brings the whole string back and re-fits it.
    let panel = h.find(&by::test_id("panel"));
    h.world_mut().get_mut::<Node>(panel).unwrap().width = px(PANEL_WIDTH * 3.0);
    h.settle();
    assert_eq!(lines(&h, two), 2);
    let wider = h.text_of(two).unwrap();
    assert!(wider.len() > shown.len(), "{wider:?} vs {shown:?}");
}

#[test]
fn rich_and_text_nodes_are_typed_in_the_screen_tree() {
    let mut h = harness();
    open(
        &mut h,
        vec![
            rich("[b]hi[/b]", TextOpts::default(), "r"),
            plain("hello", TextOpts::default(), "t"),
        ],
    );
    let r = h.find(&by::test_id("r"));
    assert!(h.world().get::<RichText>(r).is_some());
    assert_eq!(
        h.world()
            .get::<slotted_ui::WidgetNode>(r)
            .map(|w| w.0.clone()),
        Some(slotted_ui::widgets::kinds::rich_text())
    );
    let tree = h.screen_tree();
    let rendered = format!("{tree}");
    assert!(rendered.contains("[b]hi[/b]"), "{rendered}");
    assert!(rendered.contains("hello"), "{rendered}");
}
