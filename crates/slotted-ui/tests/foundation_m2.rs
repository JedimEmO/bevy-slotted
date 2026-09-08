//! The foundation `slotted-menu` builds on (menus M2 contract 2.2, 2.4,
//! 2.7, 2.8): rewriting a cloned template by node id, the glyph sets a
//! `{key:..}` run and a key binding cell render in, a select popup closing
//! on a press outside it, and the harness's `stack_top`.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use bevy::input::gamepad::{GamepadButton, GamepadConnection, GamepadConnectionEvent};
use bevy::picking::pointer::PointerButton;
use bevy::prelude::*;
use slotted_test::prelude::*;
use slotted_theme::roles;
use slotted_ui::def::{BindDef, Layout, SelectOption, Tags, TextOpts, TextRole};
use slotted_ui::rich::{GlyphSet, button_glyph, key_glyph_text, resolved_glyph_set_for_vendor};
use slotted_ui::widgets::key_binding::KeyBindingParts;
use slotted_ui::{
    ButtonOpts, ButtonVariant, InputDevice, InputMode, LocArgs, LocKey, RichKeySpan, ScreenDef,
    Screens, SelectPopup, SelectState, UiAction, UiBindings, UiNodeDef, Value,
};

fn tags(id: &str) -> Tags {
    Tags::new().with(Tags::TEST_ID, id)
}

fn args(pairs: &[(&str, &str)]) -> LocArgs {
    pairs
        .iter()
        .map(|(k, v)| ((*k).to_owned(), Value::Text((*v).to_owned())))
        .collect()
}

// ---------------------------------------------------------------------------
// 2.2: rewriting a template by id
// ---------------------------------------------------------------------------

fn button(id: &str, label: &str, variant: ButtonVariant) -> UiNodeDef {
    UiNodeDef::Button {
        widget: None,
        opts: ButtonOpts {
            label: Some(LocKey(label.to_owned())),
            variant,
            ..ButtonOpts::default()
        },
        tags: tags(id).with("menu", id),
    }
}

/// A confirm-shaped template: title, a message inside a scroll, two accept
/// buttons of which a caller keeps one.
fn confirm_template(kind: &str) -> ScreenDef {
    ScreenDef {
        kind: ScreenKind::new(kind),
        initial_focus: Some("cancel".to_owned()),
        presentation: Presentation::default(),
        inherits: None,
        remove: vec![],
        listring: vec![],
        root: UiNodeDef::Panel {
            role: roles::PANEL,
            layout: Layout {
                padding: 40.0.into(),
                gap: 4.0,
                ..Layout::default()
            },
            children: vec![
                UiNodeDef::Text {
                    key: LocKey("slotted.menu.confirm".to_owned()),
                    style: TextRole::Title,
                    opts: TextOpts::default(),
                    tags: tags("title"),
                },
                UiNodeDef::Scroll {
                    layout: Layout::default(),
                    scrollbar: false,
                    children: vec![UiNodeDef::RichText {
                        key: LocKey("slotted.menu.confirm.message".to_owned()),
                        style: TextRole::Body,
                        opts: TextOpts::default(),
                        tags: tags("message"),
                    }],
                    tags: tags("body"),
                },
                UiNodeDef::Panel {
                    role: roles::PANEL,
                    layout: Layout::default(),
                    children: vec![
                        button("cancel", "slotted.menu.cancel", ButtonVariant::Secondary),
                        button("accept", "slotted.menu.ok", ButtonVariant::Primary),
                        button("accept_danger", "slotted.menu.ok", ButtonVariant::Danger),
                    ],
                    tags: tags("buttons"),
                },
            ],
            tags: tags("confirm"),
        },
    }
}

fn text_key(def: &ScreenDef, id: &str) -> Option<(String, LocArgs)> {
    let mut def = def.clone();
    match def.root.find_mut(id)? {
        UiNodeDef::Text { key, opts, .. } | UiNodeDef::RichText { key, opts, .. } => {
            Some((key.0.clone(), opts.args.clone()))
        }
        _ => None,
    }
}

fn ids(def: &ScreenDef) -> Vec<String> {
    let mut out = Vec::new();
    def.root.walk(&mut |n| {
        if let Some(id) = n.id() {
            out.push(id.to_owned());
        }
    });
    out
}

#[test]
fn a_confirm_style_rewrite_reaches_nested_ids_and_the_root() {
    let mut def = confirm_template("t:confirm");

    // `find_mut` finds the root by its own id and a node two levels down.
    assert!(matches!(
        def.root.find_mut("confirm"),
        Some(UiNodeDef::Panel { .. })
    ));
    assert!(matches!(
        def.root.find_mut("message"),
        Some(UiNodeDef::RichText { .. })
    ));
    assert!(def.root.find_mut("nobody").is_none());

    // The title takes a new key; the message a key with arguments.
    assert!(def.set_text(
        "title",
        LocKey("game.quit.title".to_owned()),
        LocArgs::new()
    ));
    assert!(def.set_text(
        "message",
        LocKey("game.quit.message".to_owned()),
        args(&[("world", "Ravenholm")]),
    ));
    assert_eq!(
        text_key(&def, "title"),
        Some(("game.quit.title".to_owned(), LocArgs::new()))
    );
    assert_eq!(
        text_key(&def, "message"),
        Some((
            "game.quit.message".to_owned(),
            args(&[("world", "Ravenholm")])
        ))
    );
    // A button is not a text node, and an unknown id is nothing.
    assert!(!def.set_text("accept", LocKey("x".to_owned()), LocArgs::new()));
    assert!(!def.set_text("nobody", LocKey("x".to_owned()), LocArgs::new()));

    // A tag lands on a nested node and on the root; an anchor has none.
    assert!(def.set_tag("accept", "variant", "danger"));
    assert!(def.set_tag("confirm", "confirm.id", "quit"));
    assert!(!def.set_tag("nobody", "a", "b"));
    assert_eq!(
        def.root
            .find_mut("accept")
            .and_then(|n| n.tags().and_then(|t| t.get("variant"))),
        Some("danger")
    );
    assert_eq!(
        def.root.tags().and_then(|t| t.get("confirm.id")),
        Some("quit")
    );
    let mut with_anchor = def.clone();
    with_anchor
        .root
        .children_mut()
        .unwrap()
        .push(UiNodeDef::Anchor {
            id: slotted_ui::AnchorId::new("footer"),
        });
    assert!(
        !with_anchor.set_tag("footer", "a", "b"),
        "an anchor has no tags"
    );

    // The unwanted accept button goes; the root cannot.
    assert!(def.remove_node("accept_danger"));
    assert!(!def.remove_node("accept_danger"), "already gone");
    assert!(!def.remove_node("confirm"), "the root stays");
    assert_eq!(
        ids(&def),
        [
            "confirm", "title", "body", "message", "buttons", "cancel", "accept"
        ]
    );
}

#[test]
fn a_rewrite_works_on_a_resolved_def_and_renders_its_arguments() {
    // A game's confirm inherits the template; the resolved tree keeps the
    // ids, so the same rewrite applies after `Screens::resolve`.
    let mut screens = Screens::default();
    screens.register(confirm_template("t:confirm"));
    let child = ScreenDef {
        inherits: Some(ScreenKind::new("t:confirm")),
        remove: vec!["accept_danger".to_owned()],
        ..confirm_template("t:quit")
    };
    let mut resolved = screens.resolve(&child);
    assert_eq!(resolved.kind, ScreenKind::new("t:quit"));
    assert!(resolved.root.find_mut("accept_danger").is_none());
    assert!(resolved.set_text(
        "message",
        LocKey("Delete [b]{world}[/b]?".to_owned()),
        args(&[("world", "Ravenholm")]),
    ));
    assert!(resolved.set_text(
        "title",
        LocKey("Quit to desktop".to_owned()),
        LocArgs::new()
    ));

    let mut h = UiHarness::builder()
        .plugins(SlottedPlugins::headless())
        .registries(TestRegistries::basic())
        .build();
    h.open(resolved);
    h.settle();
    assert_eq!(
        h.text_of(h.find(&by::test_id("title"))).as_deref(),
        Some("Quit to desktop")
    );
    let message = h.find(&by::test_id("message"));
    let spans: Vec<String> = h
        .world()
        .get::<Children>(message)
        .unwrap()
        .iter()
        .filter_map(|c| h.world().get::<TextSpan>(c).map(|s| s.0.clone()))
        .collect();
    assert_eq!(spans, ["Delete ", "Ravenholm", "?"]);
    assert!(h.find_all(&by::test_id("accept_danger")).is_empty());
    assert_eq!(h.find_all(&by::test_id("accept")).len(), 1);
}

// ---------------------------------------------------------------------------
// 2.4: glyph sets
// ---------------------------------------------------------------------------

#[test]
fn every_set_names_the_face_buttons_and_auto_follows_the_vendor() {
    use GamepadButton as G;
    let row = |set: GlyphSet| -> Vec<String> {
        [
            G::South,
            G::East,
            G::West,
            G::North,
            G::LeftTrigger,
            G::RightTrigger,
            G::LeftTrigger2,
            G::RightTrigger2,
            G::Start,
            G::Select,
        ]
        .into_iter()
        .map(|b| button_glyph(b, set))
        .collect()
    };
    assert_eq!(
        row(GlyphSet::Xbox),
        ["A", "B", "X", "Y", "LB", "RB", "LT", "RT", "Start", "View"]
    );
    assert_eq!(
        row(GlyphSet::PlayStation),
        [
            "Cross", "Circle", "Square", "Triangle", "L1", "R1", "L2", "R2", "Options", "Share"
        ]
    );
    assert_eq!(
        row(GlyphSet::Switch),
        ["B", "A", "Y", "X", "L", "R", "ZL", "ZR", "+", "-"]
    );
    assert_eq!(
        row(GlyphSet::Generic),
        [
            "South",
            "East",
            "West",
            "North",
            "LeftTrigger",
            "RightTrigger",
            "LeftTrigger2",
            "RightTrigger2",
            "Start",
            "Select"
        ]
    );
    // Sticks and the d-pad read the same everywhere but Generic.
    for set in [GlyphSet::Xbox, GlyphSet::PlayStation, GlyphSet::Switch] {
        assert_eq!(button_glyph(G::LeftThumb, set), "L3");
        assert_eq!(button_glyph(G::DPadUp, set), "D-pad Up");
    }
    assert_eq!(button_glyph(G::DPadUp, GlyphSet::Generic), "DPadUp");

    assert_eq!(
        resolved_glyph_set_for_vendor(GlyphSet::Auto, Some(0x054C)),
        GlyphSet::PlayStation
    );
    assert_eq!(
        resolved_glyph_set_for_vendor(GlyphSet::Auto, Some(0x057E)),
        GlyphSet::Switch
    );
    assert_eq!(
        resolved_glyph_set_for_vendor(GlyphSet::Auto, Some(0x045E)),
        GlyphSet::Xbox,
        "Microsoft"
    );
    assert_eq!(
        resolved_glyph_set_for_vendor(GlyphSet::Auto, None),
        GlyphSet::Xbox,
        "no pad"
    );
    assert_eq!(
        resolved_glyph_set_for_vendor(GlyphSet::Switch, Some(0x054C)),
        GlyphSet::Switch,
        "an explicit set ignores the pad"
    );

    // The same `{key:accept}` under the three sets, and the keyboard set in
    // gamepad mode.
    let bindings = UiBindings::default();
    let accept = |set| key_glyph_text(UiAction::Accept, InputMode::Gamepad, &bindings, set);
    assert_eq!(accept(GlyphSet::Xbox), "A");
    assert_eq!(accept(GlyphSet::PlayStation), "Cross");
    assert_eq!(accept(GlyphSet::Switch), "B");
    assert_eq!(accept(GlyphSet::Generic), "South");
    assert_eq!(accept(GlyphSet::Keyboard), "Enter");
    assert_eq!(
        key_glyph_text(
            UiAction::Accept,
            InputMode::Keyboard,
            &bindings,
            GlyphSet::PlayStation
        ),
        "Enter",
        "the set only matters for a pad"
    );
}

fn glyph_screen() -> ScreenDef {
    ScreenDef {
        kind: ScreenKind::new("t:glyphs"),
        initial_focus: None,
        presentation: Presentation::default(),
        inherits: None,
        remove: vec![],
        listring: vec![],
        root: UiNodeDef::Panel {
            role: roles::PANEL,
            layout: Layout {
                padding: 40.0.into(),
                gap: 4.0,
                ..Layout::default()
            },
            children: vec![
                UiNodeDef::RichText {
                    key: LocKey("Press {key:accept} to go".to_owned()),
                    style: TextRole::Body,
                    opts: TextOpts::default(),
                    tags: tags("hint"),
                },
                UiNodeDef::KeyBinding {
                    label: None,
                    action: UiAction::Accept,
                    device: InputDevice::Gamepad,
                    disabled: false,
                    tags: tags("pad_accept"),
                },
            ],
            tags: tags("root"),
        },
    }
}

fn key_span(h: &UiHarness) -> String {
    let root = h.find(&by::test_id("hint"));
    h.world()
        .get::<Children>(root)
        .unwrap()
        .iter()
        .find(|c| h.world().get::<RichKeySpan>(*c).is_some())
        .map(|c| h.world().get::<TextSpan>(c).unwrap().0.clone())
        .expect("a key span")
}

fn cell_text(h: &UiHarness) -> String {
    let row = h.find(&by::test_id("pad_accept"));
    let parts = h.world().get::<KeyBindingParts>(row).unwrap();
    h.world().get::<Text>(parts.text).unwrap().0.clone()
}

fn connect(h: &mut UiHarness, vendor: Option<u16>) {
    let pad = h.gamepad_entity();
    h.world_mut().write_message(GamepadConnectionEvent::new(
        pad,
        GamepadConnection::Connected {
            name: "test pad".to_owned(),
            vendor_id: vendor,
            product_id: None,
        },
    ));
    h.settle();
}

#[test]
fn a_key_run_and_a_binding_cell_follow_the_glyph_set_and_the_connected_pad() {
    let mut h = UiHarness::builder()
        .plugins(SlottedPlugins::headless())
        .registries(TestRegistries::basic())
        .build();
    h.open(glyph_screen());
    h.gamepad(GamepadButton::North);
    h.settle();
    assert_eq!(h.input_mode(), InputMode::Gamepad);
    // `Auto` with a vendor-less pad is Xbox.
    assert_eq!(key_span(&h), "A");
    assert_eq!(cell_text(&h), "A");

    // An explicit set re-renders without a mode or binding change.
    h.world_mut().insert_resource(GlyphSet::PlayStation);
    h.settle();
    assert_eq!(key_span(&h), "Cross");
    assert_eq!(cell_text(&h), "Cross");
    h.world_mut().insert_resource(GlyphSet::Switch);
    h.settle();
    assert_eq!(key_span(&h), "B");
    assert_eq!(cell_text(&h), "B");
    h.world_mut().insert_resource(GlyphSet::Keyboard);
    h.settle();
    assert_eq!(
        key_span(&h),
        "Enter",
        "the keyboard set even in gamepad mode"
    );
    assert_eq!(
        cell_text(&h),
        "South",
        "a gamepad row still names the pad's button, in Bevy's words"
    );

    // Back to `Auto`: a Sony pad connecting flips it to PlayStation, a
    // Nintendo one to Switch, through Bevy's own connection event.
    h.world_mut().insert_resource(GlyphSet::Auto);
    h.settle();
    assert_eq!(key_span(&h), "A");
    connect(&mut h, Some(0x054C));
    assert_eq!(key_span(&h), "Cross");
    assert_eq!(cell_text(&h), "Cross");
    connect(&mut h, Some(0x057E));
    assert_eq!(key_span(&h), "B");
    assert_eq!(cell_text(&h), "B");

    // A node spawned while the Nintendo pad is connected starts in Switch.
    // (Popped by pad, so the mode stays gamepad.)
    h.gamepad(GamepadButton::East);
    h.settle();
    assert!(h.stack().is_empty());
    h.open(glyph_screen());
    h.settle();
    assert_eq!(h.input_mode(), InputMode::Gamepad);
    assert_eq!(key_span(&h), "B");
    assert_eq!(cell_text(&h), "B");

    // The other runs are untouched throughout.
    let root = h.find(&by::test_id("hint"));
    let first = h
        .world()
        .get::<Children>(root)
        .unwrap()
        .iter()
        .next()
        .unwrap();
    assert_eq!(h.world().get::<TextSpan>(first).unwrap().0, "Press ");
}

// ---------------------------------------------------------------------------
// 2.7: a press outside the select popup closes it
// ---------------------------------------------------------------------------

fn select_screen() -> ScreenDef {
    ScreenDef {
        kind: ScreenKind::new("t:select"),
        initial_focus: None,
        presentation: Presentation::default(),
        inherits: None,
        remove: vec![],
        listring: vec![],
        root: UiNodeDef::Panel {
            role: roles::PANEL,
            layout: Layout {
                padding: 60.0.into(),
                gap: 4.0,
                ..Layout::default()
            },
            children: vec![
                UiNodeDef::Select {
                    label: Some(LocKey("video.quality".to_owned())),
                    options: ["low", "medium", "high"]
                        .into_iter()
                        .map(|id| SelectOption {
                            id: id.to_owned(),
                            label: LocKey(format!("opt.{id}")),
                        })
                        .collect(),
                    bind: BindDef {
                        bind: Some("video.quality".to_owned()),
                        property: None,
                        disabled: false,
                    },
                    tags: tags("quality"),
                },
                button("other", "btn.other", ButtonVariant::Secondary),
            ],
            tags: tags("root"),
        },
    }
}

fn popup(h: &mut UiHarness) -> Option<Entity> {
    let mut q = h.world_mut().query_filtered::<Entity, With<SelectPopup>>();
    q.iter(h.world()).next()
}

#[test]
fn a_press_outside_the_popup_closes_it_and_one_inside_does_not() {
    let mut h = UiHarness::builder()
        .plugins(SlottedPlugins::headless())
        .registries(TestRegistries::basic())
        .theme("glass")
        .build();
    h.open(select_screen());
    h.settle();
    let quality = h.find(&by::test_id("quality"));
    let root = h.find(&by::test_id("root"));
    let open = |h: &mut UiHarness, row: Entity| {
        h.click(row);
        h.settle();
        popup(h).expect("the click opened the popup")
    };

    // A press on the screen root's padding, well away from the popup.
    let popup_entity = open(&mut h, quality);
    let root_rect = h.rect_of(root);
    let popup_rect = h.rect_of(popup_entity);
    let outside = Vec2::new(root_rect.max.x - 10.0, root_rect.max.y - 10.0);
    assert!(!popup_rect.contains(outside));
    h.pointer_move_to(outside);
    h.pointer_press(PointerButton::Primary);
    h.settle();
    assert!(
        popup(&mut h).is_none(),
        "closed on the press, before any release"
    );
    assert!(!h.world().get::<SelectState>(quality).unwrap().open);
    h.pointer_release(PointerButton::Primary);
    h.settle();
    assert_eq!(h.stack(), vec![ScreenKind::new("t:select")]);
    assert_eq!(
        h.world().get::<SelectState>(quality).unwrap().index,
        0,
        "nothing was picked"
    );
    // The next Back pops the screen, not a stale popup.
    h.action(UiAction::Back);
    h.settle();
    assert!(h.stack().is_empty());

    // A press inside the popup, on its padding rather than an option, leaves
    // it open.
    h.open(select_screen());
    h.settle();
    let quality = h.find(&by::test_id("quality"));
    let popup_entity = open(&mut h, quality);
    let popup_rect = h.rect_of(popup_entity);
    let inside = popup_rect.min + Vec2::splat(1.0);
    h.pointer_move_to(inside);
    h.pointer_press(PointerButton::Primary);
    h.pointer_release(PointerButton::Primary);
    h.settle();
    assert_eq!(popup(&mut h), Some(popup_entity), "still open");
    assert!(h.world().get::<SelectState>(quality).unwrap().open);

    // A click on another control closes it and reaches that control too.
    let other = h.find(&by::test_id("other"));
    h.click(other);
    h.settle();
    assert!(popup(&mut h).is_none());
    assert_eq!(h.focused(), Some(other));
}

// ---------------------------------------------------------------------------
// 2.8: `stack_top`
// ---------------------------------------------------------------------------

#[test]
fn stack_top_is_the_open_screen_and_none_when_the_stack_is_empty() {
    let mut h = UiHarness::builder()
        .plugins(SlottedPlugins::headless())
        .registries(TestRegistries::basic())
        .build();
    assert_eq!(h.stack_top(), None);
    h.open(glyph_screen());
    h.settle();
    assert_eq!(h.stack_top(), Some(ScreenKind::new("t:glyphs")));
    h.open(select_screen());
    h.settle();
    assert_eq!(h.stack_top(), Some(ScreenKind::new("t:select")));
    assert_eq!(h.stack().len(), 2);
    h.action(UiAction::Back);
    h.settle();
    assert_eq!(h.stack_top(), Some(ScreenKind::new("t:glyphs")));
    h.action(UiAction::Back);
    h.settle();
    assert_eq!(h.stack_top(), None);
}
