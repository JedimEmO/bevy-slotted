//! Menus M3 contract, section 2 (package A): focusable overlays and the
//! stack's focus top, `close_stacked`, the rich text reveal, the text role
//! override, `set_text` on a button, and the focus-leak guard under a
//! focusable overlay.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::float_cmp)]

use std::sync::Arc;

use bevy::input_focus::{FocusCause, InputFocus};
use bevy::prelude::*;
use bevy::text::FontWeight;
use pretty_assertions::assert_eq;
use slotted_test::prelude::*;
use slotted_theme::{Motion, Role, ThemeColor, Themed, ThemedFallback, roles};
use slotted_ui::def::{
    ButtonOpts, Layout, Length, LocKey, Presentation, PresentationMode, ScreenDef, ScreenKind,
    Tags, TextOpts, TextRole, UiNodeDef,
};
use slotted_ui::rich::{RichReveal, RichRun, RunKind, RunStyle, parse};
use slotted_ui::widgets::kinds;
use slotted_ui::{
    InputDevice, LocArgs, RichKeySpan, RichPart, RichText, ScreenRoot, ScreenStack, Screens, Scrim,
    UiAction, UiActionEvent, UiBindings, close_stacked, pop_screen, push_screen, spawn_screen,
};

const GLASS: &str = include_str!("../../../assets/themes/glass.theme.ron");
const PANEL_WIDTH: f32 = 160.0;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn harness() -> UiHarness {
    UiHarness::builder()
        .plugins(SlottedPlugins::headless())
        .registries(TestRegistries::basic())
        .resolution(1280.0, 720.0)
        .theme("glass")
        .motion(Motion::REDUCED)
        .build()
}

/// A harness with the glass theme added as an asset directly, so paints
/// can be checked against its palette.
fn themed_harness() -> UiHarness {
    let mut h = harness();
    let theme = slotted_theme::Theme::from_ron(GLASS).unwrap();
    let handle = h
        .world_mut()
        .resource_mut::<Assets<slotted_theme::Theme>>()
        .add(theme);
    h.world_mut()
        .insert_resource(slotted_theme::ActiveTheme(handle));
    h.settle();
    h
}

fn button(id: &str) -> UiNodeDef {
    UiNodeDef::Button {
        widget: Some(kinds::button()),
        opts: ButtonOpts {
            label: Some(LocKey(format!("{id}.label"))),
            ..ButtonOpts::default()
        },
        tags: Tags::new().with(Tags::TEST_ID, id),
    }
}

/// A screen of `kind` with a text and, unless `bare`, a button tagged
/// `<kind>.button`.
fn screen_with(kind: &str, presentation: Presentation, bare: bool) -> ScreenDef {
    let mut children = vec![UiNodeDef::Text {
        opts: TextOpts::default(),
        key: LocKey(format!("{kind}.label")),
        style: TextRole::Body,
        tags: Tags::new().with(Tags::TEST_ID, &format!("{kind}.label")),
    }];
    if !bare {
        children.push(button(&format!("{kind}.button")));
    }
    ScreenDef {
        kind: ScreenKind::new(kind),
        initial_focus: None,
        presentation,
        inherits: None,
        remove: vec![],
        listring: vec![],
        root: UiNodeDef::Panel {
            role: roles::PANEL,
            layout: Layout {
                padding: 2.0.into(),
                ..Layout::default()
            },
            children,
            tags: Tags::new().with(Tags::TEST_ID, kind),
        },
    }
}

fn screen(kind: &str, presentation: Presentation) -> ScreenDef {
    screen_with(kind, presentation, false)
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

fn focusable_overlay() -> Presentation {
    Presentation {
        mode: PresentationMode::Overlay,
        focus: Some(true),
        ..Presentation::default()
    }
}

fn register(h: &mut UiHarness, def: ScreenDef) -> Arc<ScreenDef> {
    h.world_mut().resource_mut::<Screens>().register(def)
}

fn push_def(h: &mut UiHarness, def: ScreenDef) -> Entity {
    let def = register(h, def);
    let root = {
        let world = h.world_mut();
        let mut commands = world.commands();
        push_screen(&mut commands, def, None)
    };
    h.world_mut().flush();
    h.step(1);
    root
}

fn push(h: &mut UiHarness, name: &str, presentation: Presentation) -> Entity {
    push_def(h, screen(name, presentation))
}

fn pop(h: &mut UiHarness) {
    pop_screen(&mut h.world_mut().commands());
    h.world_mut().flush();
    h.step(1);
}

fn kinds_of(h: &UiHarness) -> Vec<String> {
    h.world()
        .resource::<ScreenStack>()
        .kinds()
        .iter()
        .map(|k| k.0.to_string())
        .collect()
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

fn button_of(h: &UiHarness, name: &str) -> Entity {
    h.find(&by::test_id(&format!("{name}.button")))
}

fn scrims(h: &mut UiHarness) -> Vec<Scrim> {
    let mut q = h.world_mut().query::<&Scrim>();
    q.iter(h.world()).copied().collect()
}

fn stack(h: &UiHarness) -> &ScreenStack {
    h.world().resource::<ScreenStack>()
}

// ---------------------------------------------------------------------------
// 2.1 Focusable overlays
// ---------------------------------------------------------------------------

#[test]
fn presentation_focus_defaults_to_mode_and_parses_from_ron() {
    assert!(page().takes_focus());
    assert!(modal().takes_focus());
    assert!(!overlay().takes_focus());
    assert!(focusable_overlay().takes_focus());
    let parsed: Presentation = ron::from_str(r#"(mode: "overlay", focus: Some(true))"#).unwrap();
    assert_eq!(parsed, focusable_overlay());
    // Screen files enable `implicit_some`, so `focus: true` reads too.
    let parsed: Presentation = ron::from_str(
        r#"#![enable(implicit_some)]
        (mode: "page", focus: false)"#,
    )
    .unwrap();
    assert!(!parsed.takes_focus());
}

#[test]
fn a_focusable_overlay_over_nothing_gets_its_initial_focus() {
    let mut h = harness();
    let root = push(&mut h, "m3:dialogue", focusable_overlay());
    let button = button_of(&h, "m3:dialogue");
    assert_eq!(
        h.world().get::<ScreenRoot>(root).unwrap().initial_focus,
        Some(button)
    );
    assert_eq!(focused(&h), Some(button));
    assert_eq!(stack(&h).focus_top().map(|e| e.root), Some(root));
    assert_eq!(stack(&h).top(), None, "top() still skips every overlay");
}

#[test]
fn a_modal_above_a_focusable_overlay_takes_focus_and_gives_it_back() {
    let mut h = harness();
    let overlay_root = push(&mut h, "m3:dialogue", focusable_overlay());
    let overlay_button = button_of(&h, "m3:dialogue");
    assert_eq!(focused(&h), Some(overlay_button));

    let modal_root = push(&mut h, "m3:pause", modal());
    let modal_button = button_of(&h, "m3:pause");
    assert_eq!(focused(&h), Some(modal_button));
    assert_eq!(stack(&h).focus_top().map(|e| e.root), Some(modal_root));

    // A stray move back into the overlay is bounced to the modal.
    focus(&mut h, overlay_button);
    assert_eq!(focused(&h), Some(modal_button));

    pop(&mut h);
    assert_eq!(kinds_of(&h), ["m3:dialogue"]);
    assert_eq!(focused(&h), Some(overlay_button));
    assert_eq!(stack(&h).focus_top().map(|e| e.root), Some(overlay_root));
}

#[test]
fn a_plain_overlay_is_skipped_both_ways() {
    let mut h = harness();
    push(&mut h, "m3:page", page());
    let page_button = button_of(&h, "m3:page");
    assert_eq!(focused(&h), Some(page_button));

    // Pushed: the page keeps focus and the toast's button is never chosen.
    let toast = push(&mut h, "m3:toast", overlay());
    assert_eq!(focused(&h), Some(page_button));
    assert_eq!(
        h.world().get::<ScreenRoot>(toast).unwrap().initial_focus,
        None
    );

    // Popped past: a modal above hands focus back to the page, not the toast.
    push(&mut h, "m3:modal", modal());
    assert_eq!(focused(&h), Some(button_of(&h, "m3:modal")));
    pop(&mut h);
    assert_eq!(kinds_of(&h), ["m3:page", "m3:toast"]);
    assert_eq!(focused(&h), Some(page_button));
    assert_eq!(
        stack(&h).focus_top().map(|e| e.kind.0.to_string()),
        Some("m3:page".to_owned())
    );
}

#[test]
fn back_over_a_focusable_overlay_pops_nothing() {
    let mut h = harness();
    let root = push(&mut h, "m3:dialogue", focusable_overlay());
    press_back(&mut h);
    assert_eq!(kinds_of(&h), ["m3:dialogue"]);
    assert!(h.world().get_entity(root).is_ok());

    // With a page beneath, Back pops the page and leaves the overlay.
    let mut h = harness();
    push(&mut h, "m3:page", page());
    let root = push(&mut h, "m3:dialogue", focusable_overlay());
    assert_eq!(focused(&h), Some(button_of(&h, "m3:dialogue")));
    press_back(&mut h);
    assert_eq!(kinds_of(&h), ["m3:dialogue"]);
    assert!(h.world().get_entity(root).is_ok());
}

// ---------------------------------------------------------------------------
// 2.2 close_stacked
// ---------------------------------------------------------------------------

#[test]
fn close_stacked_drops_an_overlay_under_a_modal_and_leaves_the_modal_alone() {
    let mut h = harness();
    push(&mut h, "m3:page", page());
    let overlay_root = push(&mut h, "m3:dialogue", focusable_overlay());
    let modal_root = push(&mut h, "m3:modal", modal());
    let modal_button = button_of(&h, "m3:modal");
    assert_eq!(focused(&h), Some(modal_button));
    assert_eq!(
        scrims(&mut h),
        vec![Scrim {
            for_root: modal_root
        }]
    );

    close_stacked(&mut h.world_mut().commands(), overlay_root);
    h.world_mut().flush();
    h.step(1);

    assert_eq!(kinds_of(&h), ["m3:page", "m3:modal"]);
    assert!(h.world().get_entity(overlay_root).is_err());
    assert_eq!(focused(&h), Some(modal_button));
    assert_eq!(
        scrims(&mut h),
        vec![Scrim {
            for_root: modal_root
        }]
    );
    assert_eq!(
        h.world().get::<GlobalZIndex>(modal_root).map(|z| z.0),
        Some(slotted_ui::zbands::SCREEN + 2),
        "the modal moved down one slot"
    );
}

#[test]
fn close_stacked_on_a_root_outside_the_stack_closes_it_like_close_screen() {
    let mut h = harness();
    let def = register(&mut h, screen("m3:direct", page()));
    let root = spawn_screen(&mut h.world_mut().commands(), def, None);
    h.world_mut().flush();
    h.step(1);
    assert_eq!(kinds_of(&h), Vec::<String>::new());
    close_stacked(&mut h.world_mut().commands(), root);
    h.world_mut().flush();
    h.step(1);
    assert!(h.world().get_entity(root).is_err());
}

// ---------------------------------------------------------------------------
// 2.7 Focus never leaks under a focusable overlay
// ---------------------------------------------------------------------------

#[test]
fn a_focusable_overlay_without_a_focusable_clears_the_focus_below() {
    let mut h = harness();
    push(&mut h, "m3:page", page());
    let page_button = button_of(&h, "m3:page");
    assert_eq!(focused(&h), Some(page_button));

    push_def(
        &mut h,
        screen_with("m3:narration", focusable_overlay(), true),
    );
    assert_ne!(focused(&h), Some(page_button), "focus left the page");
    assert_eq!(focused(&h), None);
}

#[test]
fn a_focusable_overlay_with_a_button_moves_the_focus_off_the_page() {
    let mut h = harness();
    push(&mut h, "m3:page", page());
    let page_button = button_of(&h, "m3:page");
    assert_eq!(focused(&h), Some(page_button));

    push(&mut h, "m3:dialogue", focusable_overlay());
    assert_eq!(focused(&h), Some(button_of(&h, "m3:dialogue")));

    // And it stays off: a stray move back into the page is bounced.
    focus(&mut h, page_button);
    assert_eq!(focused(&h), Some(button_of(&h, "m3:dialogue")));
}

// ---------------------------------------------------------------------------
// 2.3 Rich text reveal
// ---------------------------------------------------------------------------

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

fn text_screen(children: Vec<UiNodeDef>) -> ScreenDef {
    ScreenDef {
        kind: ScreenKind::new("m3:text"),
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

fn open_text(h: &mut UiHarness, children: Vec<UiNodeDef>) {
    h.open(text_screen(children));
    h.settle();
}

/// `(text, colour, is a key span)` of every span under `root`, in order.
fn spans(h: &UiHarness, root: Entity) -> Vec<(String, Color, bool)> {
    fn walk(w: &World, e: Entity, out: &mut Vec<(String, Color, bool)>) {
        if let Some(span) = w.get::<TextSpan>(e) {
            out.push((
                span.0.clone(),
                w.get::<TextColor>(e).unwrap().0,
                w.get::<RichKeySpan>(e).is_some(),
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

fn shown(h: &UiHarness, root: Entity) -> String {
    spans(h, root)
        .into_iter()
        .filter(|(_, color, _)| color.alpha() > 0.0)
        .map(|(text, ..)| text)
        .collect()
}

fn hidden(h: &UiHarness, root: Entity) -> String {
    spans(h, root)
        .into_iter()
        .filter(|(_, color, _)| color.alpha() == 0.0)
        .map(|(text, ..)| text)
        .collect()
}

fn reveal(h: &mut UiHarness, root: Entity, reveal: RichReveal) {
    h.world_mut().entity_mut(root).insert(reveal);
    h.settle();
}

const PARAGRAPH: &str = "The quick brown fox jumps over the lazy dog and keeps running \
                         until the [b]river[/b] stops it.";

#[test]
fn units_count_chars_of_text_and_one_per_key_or_icon() {
    let runs = parse("ab [b]cd[/b] {key:accept} {icon:demo:chest}").unwrap();
    assert_eq!(RichReveal::units(&runs), 3 + 2 + 1 + 1 + 1 + 1);
    assert_eq!(RichReveal::units(&[]), 0);
    let umlauts = vec![RichRun {
        text: "üß€".to_owned(),
        style: RunStyle::default(),
        kind: RunKind::Text,
    }];
    assert_eq!(RichReveal::units(&umlauts), 3, "chars, not bytes");
}

#[test]
fn reveal_zero_shows_nothing_and_keeps_the_full_height() {
    let mut h = harness();
    open_text(
        &mut h,
        vec![
            rich(PARAGRAPH, TextOpts::default(), "full"),
            rich(PARAGRAPH, TextOpts::default(), "typing"),
        ],
    );
    let full = h.find(&by::test_id("full"));
    let typing = h.find(&by::test_id("typing"));
    let full_height = h.rect_of(full).height();
    assert!(full_height > 30.0, "a wrapped paragraph: {full_height}");

    reveal(&mut h, typing, RichReveal(Some(0)));
    assert_eq!(shown(&h, typing), "");
    assert_eq!(hidden(&h, typing), shown(&h, full));
    assert_eq!(h.rect_of(typing).height(), full_height);
    assert!(
        spans(&h, typing)
            .iter()
            .all(|(_, color, _)| *color == Color::NONE)
    );

    // Half way through: same height still.
    reveal(&mut h, typing, RichReveal(Some(40)));
    assert_eq!(shown(&h, typing).chars().count(), 40);
    assert_eq!(h.rect_of(typing).height(), full_height);
}

#[test]
fn reveal_mid_run_splits_the_run_into_a_shown_and_a_transparent_span() {
    let mut h = harness();
    open_text(
        &mut h,
        vec![rich("ab [b]cd[/b] ef", TextOpts::default(), "p")],
    );
    let p = h.find(&by::test_id("p"));
    reveal(&mut h, p, RichReveal(Some(4)));
    let spans = spans(&h, p);
    let texts: Vec<&str> = spans.iter().map(|(t, ..)| t.as_str()).collect();
    assert_eq!(texts, ["ab ", "c", "d", " ef"]);
    assert!(spans[0].1.alpha() > 0.0);
    assert!(spans[1].1.alpha() > 0.0);
    assert_eq!(spans[2].1, Color::NONE);
    assert_eq!(spans[3].1, Color::NONE);
    // Every span is a part, so the next render replaces them all.
    let parts = h
        .world()
        .get::<Children>(p)
        .unwrap()
        .iter()
        .filter(|c| h.world().get::<RichPart>(*c).is_some())
        .count();
    assert_eq!(parts, 4);

    reveal(&mut h, p, RichReveal(Some(6)));
    assert_eq!(shown(&h, p), "ab cd ");
    assert_eq!(hidden(&h, p), "ef");
}

#[test]
fn a_key_run_flips_whole_and_stays_a_key_span_either_way() {
    let mut h = harness();
    open_text(
        &mut h,
        vec![rich("Press {key:accept} go", TextOpts::default(), "hint")],
    );
    let hint = h.find(&by::test_id("hint"));

    reveal(&mut h, hint, RichReveal(Some(6)));
    let before = spans(&h, hint);
    assert_eq!(before[0], ("Press ".to_owned(), before[0].1, false));
    assert_eq!(before[1].0, "Enter");
    assert_eq!(before[1].1, Color::NONE, "the key is not yet revealed");
    assert!(before[1].2, "and still a key span");
    assert_eq!(before[2], (" go".to_owned(), Color::NONE, false));

    // A rebinding while it is hidden rewrites the text, not the colour.
    h.world_mut()
        .resource_mut::<UiBindings>()
        .keys
        .insert(UiAction::Accept, vec![KeyCode::KeyF]);
    h.settle();
    let rebound = spans(&h, hint);
    assert_eq!(rebound[1].0, "F");
    assert_eq!(rebound[1].1, Color::NONE);

    reveal(&mut h, hint, RichReveal(Some(7)));
    let after = spans(&h, hint);
    assert_eq!(after[1].0, "F");
    assert!(
        after[1].1.alpha() > 0.0,
        "one more unit shows the whole key"
    );
    assert!(after[1].2);
    assert_eq!(after[2].1, Color::NONE);
}

#[test]
fn all_and_a_missing_component_render_identically() {
    let mut h = harness();
    open_text(
        &mut h,
        vec![
            rich(
                "Plain [b]bold[/b] {key:accept}",
                TextOpts::default(),
                "none",
            ),
            rich("Plain [b]bold[/b] {key:accept}", TextOpts::default(), "all"),
        ],
    );
    let none = h.find(&by::test_id("none"));
    let all = h.find(&by::test_id("all"));
    reveal(&mut h, all, RichReveal::ALL);
    assert_eq!(spans(&h, all), spans(&h, none));
    assert_eq!(h.rect_of(all).size(), h.rect_of(none).size());

    // Hidden, then the component removed: back to the full render.
    reveal(&mut h, all, RichReveal(Some(0)));
    assert_eq!(shown(&h, all), "");
    h.world_mut().entity_mut(all).remove::<RichReveal>();
    h.settle();
    assert_eq!(spans(&h, all), spans(&h, none));
}

#[test]
fn an_unrevealed_inline_icon_is_hidden_but_laid_out() {
    let mut h = themed_harness();
    open_text(
        &mut h,
        vec![rich(
            "Take {icon:minecraft:cobblestone} now",
            TextOpts {
                inline: true,
                ..TextOpts::default()
            },
            "inline",
        )],
    );
    let inline = h.find(&by::test_id("inline"));
    let image_of = |h: &UiHarness| {
        h.world()
            .get::<Children>(inline)
            .unwrap()
            .iter()
            .find(|c| h.world().get::<ImageNode>(*c).is_some())
            .unwrap()
    };
    let full_width = h.rect_of(inline).width();
    let icon_width = h.rect_of(image_of(&h)).width();
    assert!(icon_width > 0.0);

    reveal(&mut h, inline, RichReveal(Some(5)));
    assert_eq!(shown(&h, inline), "Take ");
    let image = image_of(&h);
    assert_eq!(
        h.world().get::<Visibility>(image),
        Some(&Visibility::Hidden)
    );
    assert_eq!(h.rect_of(image).width(), icon_width, "still laid out");
    assert_eq!(h.rect_of(inline).width(), full_width);

    reveal(&mut h, inline, RichReveal(Some(6)));
    assert_eq!(
        h.world().get::<Visibility>(image_of(&h)),
        Some(&Visibility::Inherited)
    );
    assert_eq!(hidden(&h, inline), " now");
}

// ---------------------------------------------------------------------------
// 2.4 Text role override
// ---------------------------------------------------------------------------

fn palette(h: &UiHarness, name: &str) -> Color {
    let theme = h.world().resource::<slotted_theme::ActiveTheme>().0.clone();
    let themes = h.world().resource::<Assets<slotted_theme::Theme>>();
    themes
        .get(&theme)
        .unwrap()
        .color(&ThemeColor::palette(name))
}

fn with_role(role: &str) -> TextOpts {
    TextOpts {
        role: Some(Role::new(role)),
        ..TextOpts::default()
    }
}

#[test]
fn role_parses_on_a_text_node() {
    let node: UiNodeDef =
        ron::from_str(r#"(type: "text", key: "k", style: "label", role: "dialogue.speaker")"#)
            .unwrap();
    assert!(
        matches!(node, UiNodeDef::Text { opts, .. } if opts.role == Some(roles::DIALOGUE_SPEAKER))
    );
    let node: UiNodeDef = ron::from_str(r#"(type: "rich_text", key: "k")"#).unwrap();
    assert!(matches!(node, UiNodeDef::RichText { opts, .. } if opts.role.is_none()));
}

#[test]
fn a_text_with_a_role_paints_that_role_and_a_missing_one_falls_back() {
    let mut h = themed_harness();
    open_text(
        &mut h,
        vec![
            plain("Elder", TextOpts::default(), "body"),
            plain("Elder", with_role("dialogue.speaker"), "speaker"),
            plain("Elder", with_role("nope.role"), "missing"),
        ],
    );
    let body = h.find(&by::test_id("body"));
    let speaker = h.find(&by::test_id("speaker"));
    let missing = h.find(&by::test_id("missing"));
    let color = |h: &UiHarness, e: Entity| h.world().get::<TextColor>(e).unwrap().0;
    let weight = |h: &UiHarness, e: Entity| h.world().get::<TextFont>(e).unwrap().weight;

    assert_eq!(color(&h, body), palette(&h, "text"));
    assert_eq!(color(&h, speaker), palette(&h, "accent"));
    assert_eq!(
        weight(&h, speaker),
        FontWeight(700),
        "the role's own weight"
    );
    assert_eq!(
        h.world().get::<Themed>(speaker),
        Some(&Themed(roles::DIALOGUE_SPEAKER))
    );
    assert_eq!(
        h.world().get::<ThemedFallback>(speaker),
        Some(&ThemedFallback(roles::TEXT))
    );
    assert_eq!(
        color(&h, missing),
        palette(&h, "text"),
        "painted the fallback"
    );
    assert_eq!(weight(&h, missing), weight(&h, body));
    assert!(h.world().get::<ThemedFallback>(body).is_none());
}

#[test]
fn a_rich_text_with_a_role_paints_its_spans_in_that_role() {
    let mut h = themed_harness();
    open_text(
        &mut h,
        vec![
            rich(
                "Hello [b]there[/b]",
                with_role("dialogue.speaker"),
                "speaker",
            ),
            rich("Hello [b]there[/b]", with_role("nope.role"), "missing"),
        ],
    );
    let speaker = h.find(&by::test_id("speaker"));
    let missing = h.find(&by::test_id("missing"));
    assert_eq!(
        h.world().get::<RichText>(speaker).unwrap().role,
        Some(roles::DIALOGUE_SPEAKER)
    );
    for (_, color, _) in spans(&h, speaker) {
        assert_eq!(color, palette(&h, "accent"));
    }
    for (_, color, _) in spans(&h, missing) {
        assert_eq!(color, palette(&h, "text"));
    }
}

// ---------------------------------------------------------------------------
// 2.6 set_text reaches a button label
// ---------------------------------------------------------------------------

#[test]
fn set_text_rewrites_a_button_label() {
    let mut def = screen("m3:page", page());
    assert!(def.set_text(
        "m3:page.button",
        LocKey("menu.talk".to_owned()),
        LocArgs::default()
    ));
    assert!(matches!(
        def.root.find_mut("m3:page.button"),
        Some(UiNodeDef::Button { opts, .. }) if opts.label == Some(LocKey("menu.talk".to_owned()))
    ));
    assert!(def.set_text("m3:page.label", LocKey("k".to_owned()), LocArgs::default()));
    assert!(!def.set_text("nowhere", LocKey("k".to_owned()), LocArgs::default()));

    // And the opened screen shows it.
    let mut h = harness();
    let root = push_def(&mut h, def);
    let button = button_of(&h, "m3:page");
    let label = h.text_of(button).or_else(|| {
        h.world()
            .get::<Children>(button)
            .and_then(|c| c.iter().find_map(|c| h.text_of(c)))
    });
    assert_eq!(label.as_deref(), Some("menu.talk"), "{root:?}");
}
