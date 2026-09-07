//! Phase 7 adversarial review: what a theme does when it is wrong, when the
//! feature it wants is not compiled in, and when the machine it runs on does
//! not have the font it names.
//!
//! The rule Phase 7 was written to is "a missing role is added to every
//! theme, never special-cased". These tests are the other half of that rule:
//! the check that catches a theme which broke it has to say which role and
//! which theme, and every material a direction added has to degrade to
//! something drawable when the shader behind it is absent.

#![allow(clippy::unwrap_used)]

use std::time::Duration;

use bevy::app::App;
use bevy::asset::{AssetPlugin, Assets};
use bevy::prelude::*;
use bevy::text::{FontSource, TextFont};
use bevy::ui::ui_transform::UiTransform;
use slotted_theme::tokens::FontToken;
use slotted_theme::{
    ActiveTheme, Easing, Material, Motion, MotionPreset, Paint, Role, SlottedThemePlugin, Theme,
    ThemeColor, Themed, Tween, TweenTarget, roles,
};

const GLASS: &str = include_str!("../../../assets/themes/glass.theme.ron");
const PAPER: &str = include_str!("../../../assets/themes/paper.theme.ron");
const NEON: &str = include_str!("../../../assets/themes/neon.theme.ron");

fn glass() -> Theme {
    Theme::from_ron(GLASS).expect("glass parses")
}

fn neon() -> Theme {
    Theme::from_ron(NEON).expect("neon parses")
}

fn paper() -> Theme {
    Theme::from_ron(PAPER).expect("paper parses")
}

// ---------------------------------------------------------------------------
// The parity check, failing
// ---------------------------------------------------------------------------

/// The positive form of the parity check is in `theme.rs`: the three shipped
/// themes define the same roles. It only earns its keep if it fails when a
/// theme drops one, and if the failure names the role rather than saying the
/// two sets differ.
///
/// The two halves of the check are not interchangeable, which is the point of
/// this test. `missing_roles` is about what can be painted, and a role falls
/// back to its parent, so dropping `tank.fill` leaves it paintable, in the
/// tank's own material. Only the key-set comparison sees that one theme now
/// says less than another. A parity suite that ran `missing_roles` alone
/// would sign off on a neon whose fills had all quietly become panels.
#[test]
fn a_theme_missing_a_role_is_caught_and_the_role_is_named() {
    let key_set = |theme: &Theme| {
        let mut keys: Vec<String> = theme.roles.keys().map(|r| r.as_str().to_owned()).collect();
        keys.sort();
        keys
    };
    let reference = key_set(&glass());

    // A parentless role: nothing to fall back to, so both halves catch it.
    let mut orphaned = neon();
    orphaned.name = "neon-without-a-viewport".to_owned();
    orphaned
        .roles
        .remove(&roles::VIEWPORT)
        .expect("neon defines viewport");
    assert_eq!(
        orphaned.missing_roles(),
        vec![roles::VIEWPORT],
        "the missing role is reported, not just a count"
    );
    assert!(
        orphaned.material(&roles::VIEWPORT).is_none(),
        "and asking for it gives nothing to paint with"
    );
    assert_ne!(key_set(&orphaned), reference);

    // A child role: it still paints, in its parent's material, so only the
    // key-set half of the check has anything to say.
    let mut broken = neon();
    broken.name = "neon-with-a-hole".to_owned();
    let dropped = broken
        .roles
        .remove(&roles::TANK_FILL)
        .expect("neon defines tank.fill");
    assert_eq!(
        broken.missing_roles(),
        Vec::<Role>::new(),
        "a child role falls back to its parent, so completeness is silent"
    );
    assert_eq!(
        broken.material(&roles::TANK_FILL),
        broken.material(&roles::TANK),
        "which is exactly the wrong picture: the fill is now the well"
    );

    let broken_keys = key_set(&broken);
    assert_ne!(broken_keys, reference, "the parity check would pass");
    let gone: Vec<&String> = reference
        .iter()
        .filter(|k| !broken_keys.contains(k))
        .collect();
    assert_eq!(
        gone,
        vec![&roles::TANK_FILL.as_str().to_owned()],
        "and it names the one role that went"
    );

    // Putting it back restores parity, so the check is measuring the role
    // and not something else about the file.
    broken.roles.insert(roles::TANK_FILL, dropped);
    assert_eq!(key_set(&broken), reference);
}

/// A role nothing has ever heard of is not a missing role: a theme may carry
/// extra entries for its own widgets. What parity forbids is one theme
/// carrying an entry another lacks, which the shipped set is checked for.
#[test]
fn an_unknown_extra_role_is_not_reported_as_missing() {
    let mut theme = glass();
    theme.roles.insert(
        Role::new("game:reactor.core"),
        Material::Solid {
            fill: ThemeColor::hex("#ff0000"),
            border: None,
            radius: None,
            elevation: None,
        },
    );
    assert_eq!(theme.missing_roles(), Vec::<Role>::new());
    assert!(theme.material(&Role::new("game:reactor.core")).is_some());
}

// ---------------------------------------------------------------------------
// Materials without the shader behind them
// ---------------------------------------------------------------------------

/// `CutCorners` is neon's shape, and it is a `UiMaterial` behind the `blur`
/// feature. Without that feature there is no shader, and a headless app has
/// no renderer at all: the paint has to carry a square fallback that is
/// still the right colour, still has the rarity bar, and still has a border.
/// A theme that painted nothing here would leave neon's slots invisible.
#[test]
fn cut_corners_degrades_to_a_drawable_square() {
    let theme = neon();
    let material = theme
        .material(&roles::SLOT)
        .expect("neon's slot is cut-cornered");
    assert!(
        matches!(material, Material::CutCorners { .. }),
        "the direction under test is the cut-corner one: {material:?}"
    );
    let paint = Paint::from_material(&theme, material);

    assert!(
        paint.background.is_some() || paint.gradient.is_some(),
        "a cut-corner slot with no shader still paints its fill"
    );
    assert_eq!(
        paint.cut.is_some(),
        cfg!(feature = "blur"),
        "the shader paint exists exactly when the feature that draws it does"
    );

    // The rarity bar: neon's rare slot is a bottom bar, and without the
    // shader it is a hard-stop gradient rising from the bottom edge, not a
    // silently dropped decoration.
    let rare = theme
        .material(&Role::new("slot.rarity.rare"))
        .expect("neon rings its rarities");
    let rare = Paint::from_material(&theme, rare);
    assert!(
        rare.gradient.is_some(),
        "the rarity bar survives as a gradient when the shader is absent"
    );
}

/// `Dashed` is paper's rarity stamp, and `bevy_ui` has no dashed border. It
/// resolves to a `BorderGradient` of hard stops plus a border width, both of
/// which the plain renderer draws: nothing about it needs a feature.
#[test]
fn dashed_needs_no_feature_to_draw() {
    let theme = paper();
    let material = theme
        .material(&Role::new("slot.rarity.rare"))
        .expect("paper stamps its rarities");
    assert!(
        matches!(material, Material::Dashed { .. }),
        "the direction under test is the dashed one: {material:?}"
    );
    let paint = Paint::from_material(&theme, material);

    let gradient = paint
        .border_gradient
        .expect("a dashed ring is a border gradient");
    assert!(
        paint.border_width.is_some_and(|w| w > 0.0),
        "a border gradient with no border width paints nothing"
    );
    let stops = match &gradient.0[0] {
        bevy::ui::Gradient::Linear(linear) => &linear.stops,
        other => panic!("expected a linear gradient, got {other:?}"),
    };
    assert!(
        stops.len() >= 8,
        "a hatch is many hard stops, not a solid ring: {} stops",
        stops.len()
    );
    assert!(
        stops.iter().any(|s| s.color.alpha() == 0.0),
        "the gaps between the dashes are transparent"
    );
    assert!(paint.cut.is_none(), "nothing here wants the cut shader");
}

/// Every material in every shipped theme resolves to a paint that draws
/// something, whatever the feature set. This is the property the two tests
/// above check one material at a time: no role in any direction can come out
/// of `Paint::from_material` with nothing to put on the screen.
#[test]
fn no_shipped_role_resolves_to_an_empty_paint() {
    for (name, theme) in [("glass", glass()), ("paper", paper()), ("neon", neon())] {
        for (role, material) in &theme.roles {
            let paint = Paint::from_material(&theme, material);
            let paints_something = paint.background.is_some()
                || paint.gradient.is_some()
                || paint.sliced.is_some()
                || paint.tiled.is_some()
                || paint.border.is_some()
                || paint.border_gradient.is_some()
                || paint.text.is_some()
                || paint.glass.is_some()
                || paint.cut.is_some();
            assert!(
                paints_something,
                "{name}: {role} resolves to an empty paint from {material:?}"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Overshoot
// ---------------------------------------------------------------------------

/// `Overshoot` is the one curve allowed to leave `0..=1`, which is what makes
/// it the one curve that could strand a slot at 1.06 scale. It may not: the
/// curve ends exactly at 1, the tween writes that value on the frame it
/// finishes, and the component is gone afterwards, so the slot is back at
/// identity with nothing left running for `settle()` to wait on.
#[test]
fn overshoot_leaves_a_slot_at_identity_and_nothing_running() {
    const MS: u64 = 140;

    // No `TimePlugin`: this test drives `Time<Virtual>` by hand, and a
    // plugin that calls `update()` every frame would overwrite the delta
    // with the wall clock, which is the one thing `slotted-ui` never reads.
    let mut app = App::new();
    app.insert_resource(Motion::default())
        .init_resource::<slotted_theme::ActiveMotions>()
        .insert_resource(Time::<Virtual>::default())
        .add_systems(Update, slotted_theme::advance_tweens);

    let slot = app
        .world_mut()
        .spawn((
            UiTransform::IDENTITY,
            Tween::new(
                TweenTarget::Scale { from: 0.8, to: 1.0 },
                Duration::from_millis(MS),
            )
            .with_easing(Easing::Overshoot),
        ))
        .id();

    // Mid-flight it really does pass the end value, or the test below is
    // asserting nothing.
    let mut overshot = false;
    for _ in 0..14 {
        app.world_mut()
            .resource_mut::<Time<Virtual>>()
            .advance_by(Duration::from_millis(10));
        app.update();
        if let Some(transform) = app.world().get::<UiTransform>(slot) {
            overshot |= transform.scale.x > 1.0001;
        }
    }
    assert!(overshot, "the back-out curve never passed its end value");

    // Past the end: exactly identity, and the tween is gone.
    app.world_mut()
        .resource_mut::<Time<Virtual>>()
        .advance_by(Duration::from_millis(MS));
    app.update();
    let transform = app.world().get::<UiTransform>(slot).expect("a transform");
    assert_eq!(
        transform.scale,
        Vec2::ONE,
        "the slot settled at identity, not at the top of the overshoot"
    );
    assert!(
        app.world().get::<Tween>(slot).is_none(),
        "a finished tween is removed, so settle() has nothing to wait for"
    );
    assert_eq!(app.world().resource::<slotted_theme::ActiveMotions>().0, 0);
}

/// The same property as arithmetic, over every curve and every preset a
/// theme can choose: 0 maps to 0 and 1 maps to 1 exactly. Only the middle is
/// allowed to be interesting.
#[test]
// Exact comparison on purpose: "close to 1" is not the property. A curve that
// ends at 0.9999 leaves a slot visibly off identity.
#[allow(clippy::float_cmp)]
fn every_easing_starts_and_ends_exactly_where_it_says() {
    for easing in [
        Easing::Standard,
        Easing::Linear,
        Easing::EaseInOut,
        Easing::Stamp,
        Easing::Snap,
        Easing::Overshoot,
    ] {
        assert_eq!(easing.apply(0.0), 0.0, "{easing:?} at 0");
        assert_eq!(easing.apply(1.0), 1.0, "{easing:?} at 1");
        // Past the end is clamped, not extrapolated: a frame that overshoots
        // the duration must not overshoot the value too.
        assert_eq!(easing.apply(1.5), 1.0, "{easing:?} past the end");
        assert_eq!(easing.apply(-0.5), 0.0, "{easing:?} before the start");
        assert_eq!(
            easing.overshoots(),
            easing == Easing::Overshoot,
            "{easing:?} reports whether it springs"
        );
    }
}

/// And the shipped themes only spring where they say they do: paper's rule is
/// no springs anywhere, so a preset that quietly gained `Overshoot` there
/// would be a direction change nobody asked for.
#[test]
fn only_neon_springs() {
    for preset in [
        MotionPreset::Hover,
        MotionPreset::Press,
        MotionPreset::DropSquash,
        MotionPreset::FlyToSlot,
        MotionPreset::Stagger,
        MotionPreset::Fade,
    ] {
        assert!(
            !paper().tokens.easing(preset).overshoots(),
            "paper {preset:?}"
        );
        assert!(
            !glass().tokens.easing(preset).overshoots(),
            "glass {preset:?}"
        );
    }
    assert!(
        neon().tokens.easing(MotionPreset::DropSquash).overshoots(),
        "neon's drop is the one spring in the shipped set"
    );
}

// ---------------------------------------------------------------------------
// Fonts that are not installed
// ---------------------------------------------------------------------------

/// A theme names a font family the machine does not have. Bevy resolves a
/// family against the OS font database at render time; a name it cannot
/// resolve has to fall through to the default face, and never take the app
/// down on the way. None of IBM Plex, Manrope or Rajdhani is installed here,
/// so the shipped themes are already this case.
#[test]
fn an_uninstalled_font_family_paints_text_without_panicking() {
    let mut theme = glass();
    theme.tokens.fonts.insert(
        "mono".to_owned(),
        FontToken {
            family: "No Such Family 8b3f".to_owned(),
            path: None,
            system: true,
        },
    );
    theme.roles.insert(
        roles::COUNT,
        Material::Text {
            color: ThemeColor::hex("#ffffff"),
            size: (11.0).into(),
            weight: None,
            font: Some("mono".to_owned()),
            shadow: None,
        },
    );

    let mut app = themed_app(theme);
    let node = app
        .world_mut()
        .spawn((Node::default(), Themed(roles::COUNT)))
        .id();
    app.update();

    let font = app.world().get::<TextFont>(node).expect("a TextFont");
    assert_eq!(
        font.font,
        FontSource::Family("No Such Family 8b3f".into()),
        "the family reaches Bevy, which is the one place that can resolve it"
    );
    // A second pass over the same node, the way a hot reload repaints: still
    // no panic, still the same font.
    app.update();
    assert!(app.world().get::<TextFont>(node).is_some());
}

/// A text role naming a font token the theme does not define. The role is
/// still painted, in Bevy's default face, and the dangling reference is
/// something the theme check reports rather than something the renderer
/// trips over.
#[test]
fn a_text_role_naming_an_unknown_font_token_falls_back_to_the_default_face() {
    let mut theme = glass();
    theme.roles.insert(
        roles::COUNT,
        Material::Text {
            color: ThemeColor::hex("#ffffff"),
            size: (11.0).into(),
            weight: None,
            font: Some("a-token-nobody-defined".to_owned()),
            shadow: None,
        },
    );
    assert_eq!(
        theme.dangling_font_refs(),
        vec!["a-token-nobody-defined".to_owned()],
        "the check names the token, which is what a theme author needs"
    );

    let paint = Paint::from_material(&theme, theme.material(&roles::COUNT).unwrap());
    assert_eq!(paint.font, None, "no token, no font: Bevy's default face");
    assert!(paint.text.is_some(), "the role is still painted");

    let mut app = themed_app(theme);
    let node = app
        .world_mut()
        .spawn((Node::default(), Themed(roles::COUNT)))
        .id();
    app.update();
    let font = app.world().get::<TextFont>(node).expect("a TextFont");
    assert_eq!(font.font, FontSource::default());
}

/// An app with the theme systems and `theme` already loaded and active, so a
/// test can spawn a `Themed` node and see it painted on the next update.
fn themed_app(theme: Theme) -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_plugins(AssetPlugin::default())
        .add_plugins(SlottedThemePlugin);
    let handle = app.world_mut().resource_mut::<Assets<Theme>>().add(theme);
    app.insert_resource(ActiveTheme(handle));
    app
}
