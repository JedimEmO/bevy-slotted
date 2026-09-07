//! The type scale (menus M1 contract 1.4, 2.4): every shipped theme resolves
//! every `$` size, a dangling reference is reported rather than painted, and
//! the role table stays complete.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::float_cmp)]

use pretty_assertions::assert_eq;
use slotted_theme::{Material, Paint, Role, Theme, ThemeSize, roles};

const GLASS: &str = include_str!("../../../assets/themes/glass.theme.ron");
const PAPER: &str = include_str!("../../../assets/themes/paper.theme.ron");
const NEON: &str = include_str!("../../../assets/themes/neon.theme.ron");

fn shipped() -> [(&'static str, Theme); 3] {
    [
        ("glass", Theme::from_ron(GLASS).unwrap()),
        ("paper", Theme::from_ron(PAPER).unwrap()),
        ("neon", Theme::from_ron(NEON).unwrap()),
    ]
}

/// The six steps of the scale, in every theme, in descending size.
const SCALE: [&str; 6] = ["display", "title", "heading", "body", "label", "caption"];

#[test]
fn every_theme_defines_the_six_step_scale_in_descending_order() {
    for (name, theme) in shipped() {
        let sizes: Vec<f32> = SCALE
            .iter()
            .map(|step| {
                theme
                    .tokens
                    .typography
                    .get(*step)
                    .unwrap_or_else(|| panic!("{name} lacks typography `{step}`"))
                    .size
            })
            .collect();
        assert!(
            sizes.windows(2).all(|w| w[0] > w[1]),
            "{name}'s scale is not descending: {sizes:?}"
        );
        for step in SCALE {
            let style = &theme.tokens.typography[step];
            if let Some(font) = &style.font {
                assert!(
                    theme.tokens.fonts.contains_key(font),
                    "{name}'s `{step}` names font `{font}` that `fonts` lacks"
                );
            }
        }
    }
}

#[test]
fn every_dollar_size_resolves_in_every_theme() {
    for (name, theme) in shipped() {
        assert_eq!(
            theme.dangling_typography_refs(),
            Vec::<String>::new(),
            "{name}"
        );
        for (role, material) in &theme.roles {
            let Material::Text { size, .. } = material else {
                continue;
            };
            let Some(step) = size.typography_ref() else {
                continue;
            };
            let style = theme.type_style(size).unwrap();
            assert_eq!(
                theme.size(size),
                style.size,
                "{name}: {role} resolves `${step}` to its style's size"
            );
            let paint = Paint::for_role(&theme, role).unwrap();
            assert_eq!(
                paint.text.map(|(_, px)| px),
                Some(style.size),
                "{name}: {role}"
            );
        }
    }
}

/// The roles a `TextRole` maps onto all read the scale, so a game that
/// changes `typography.body` moves every body label at once.
#[test]
fn the_text_role_roles_read_the_scale() {
    for (name, theme) in shipped() {
        for role in [
            roles::TEXT_DISPLAY,
            roles::PANEL_TITLE,
            roles::TEXT_HEADING,
            roles::TEXT,
            roles::TEXT_MUTED,
            roles::TEXT_LABEL,
            roles::TEXT_CAPTION,
            roles::COUNT,
            roles::TEXT_KEY,
            roles::TEXT_ICON,
        ] {
            let Some(Material::Text { size, .. }) = theme.roles.get(&role) else {
                panic!("{name} does not define {role} as text");
            };
            assert!(
                size.typography_ref().is_some(),
                "{name}: {role} carries a raw size {size:?}"
            );
        }
    }
}

#[test]
fn a_dangling_typography_reference_is_reported_and_painted_at_the_fallback_size() {
    let mut theme = Theme::from_ron(GLASS).unwrap();
    theme.roles.insert(
        Role::new("text.experimental"),
        Material::Text {
            color: slotted_theme::ThemeColor::palette("text"),
            size: ThemeSize::typography("nope"),
            font: None,
            weight: None,
            shadow: None,
        },
    );
    assert_eq!(theme.dangling_typography_refs(), vec!["nope".to_owned()]);
    assert_eq!(theme.size(&ThemeSize::typography("nope")), 13.0);
    assert_eq!(theme.type_style(&ThemeSize::typography("nope")), None);
    // A literal keeps working beside the references.
    assert_eq!(theme.size(&ThemeSize::px(17.5)), 17.5);
    assert_eq!(theme.size(&ThemeSize("oops".to_owned())), 13.0);
}

#[test]
fn the_three_themes_still_define_every_role() {
    for (name, theme) in shipped() {
        assert_eq!(theme.missing_roles(), Vec::<Role>::new(), "{name}");
        assert_eq!(theme.dangling_font_refs(), Vec::<String>::new(), "{name}");
        assert_eq!(
            theme.dangling_palette_refs(),
            Vec::<String>::new(),
            "{name}"
        );
    }
}

/// A `$name` size brings the style's font and weight unless the material
/// names its own; a literal size brings nothing.
#[test]
fn a_reference_carries_the_styles_font_and_weight_unless_overridden() {
    let theme = Theme::from_ron(GLASS).unwrap();
    let heading = Paint::for_role(&theme, &roles::TEXT_HEADING).unwrap();
    assert_eq!(heading.text_weight, Some(600));
    assert!(matches!(
        heading.font,
        Some(slotted_theme::FontPaint::Path(ref p)) if p.contains("Manrope")
    ));
    // `text.key` names `display` and 600 itself over `$label` (500, body).
    let key = Paint::for_role(&theme, &roles::TEXT_KEY).unwrap();
    assert_eq!(key.text_weight, Some(600));
    assert!(matches!(
        key.font,
        Some(slotted_theme::FontPaint::Path(ref p)) if p.contains("BarlowCondensed")
    ));
    let literal = Paint::from_material(
        &theme,
        &Material::Text {
            color: slotted_theme::ThemeColor::hex("#FFFFFF"),
            size: ThemeSize::px(20.0),
            font: None,
            weight: None,
            shadow: None,
        },
    );
    assert_eq!(literal.text_weight, None);
    assert_eq!(literal.font, None);
    assert_eq!(literal.text_line_height, None);
}
