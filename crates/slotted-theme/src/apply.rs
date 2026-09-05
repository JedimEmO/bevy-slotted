//! Painting roles onto nodes.

use bevy::asset::{AssetEvent, AssetServer, Assets, Handle};
use bevy::prelude::*;
use bevy::ui::prelude::{BorderRect, TextureSlicer};
use bevy::ui::widget::NodeImageMode;

use crate::material::{Material, ThemeColor};
use crate::role::Role;
use crate::theme::Theme;
use crate::tokens::Tokens;

/// The theme in use. Swapping the handle repaints every [`Themed`] node.
#[derive(Resource, Debug, Clone, Default)]
pub struct ActiveTheme(pub Handle<Theme>);

/// The role a node plays. Widgets set this and never touch colours.
///
/// State changes are role changes: the slot widget swaps `slot` for
/// `slot.hover` when hovered. `Changed<Themed>` repaints only that node.
#[derive(Component, Debug, Clone, PartialEq, Eq, Hash)]
pub struct Themed(pub Role);

impl Themed {
    /// A themed node from a role.
    pub fn new(role: impl Into<Role>) -> Self {
        Self(role.into())
    }
}

/// The blurred-glass parameters a [`Material::Glass`] resolves to. Only the
/// `blur` feature turns this into a `MaterialNode`; without the feature the
/// paint carries a degraded solid fill instead and this stays `None`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GlassPaint {
    /// Tint over the backdrop.
    pub tint: Color,
    /// Edge highlight colour.
    pub edge: Color,
    /// Blur radius in backdrop texels.
    pub blur_radius: f32,
}

/// A nine-sliced image paint.
#[derive(Debug, Clone, PartialEq)]
pub struct SlicedPaint {
    /// Asset path of the image.
    pub image: String,
    /// Slicer built from the material's border inset and corner scale.
    pub slicer: TextureSlicer,
    /// Tint.
    pub tint: Color,
}

/// The component values a [`Material`] paints onto one node.
///
/// This is the whole of the material mapping and a pure function of the
/// theme, which is what makes it testable without an `App`. [`apply_theme`]
/// does nothing but turn a `Paint` into component inserts and removals.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Paint {
    /// `BackgroundColor`.
    pub background: Option<Color>,
    /// `BorderColor`, applied to all four edges.
    pub border: Option<Color>,
    /// `BorderRadius`, all four corners, in logical px.
    pub radius: Option<f32>,
    /// The single `ShadowStyle` of a `BoxShadow`, from an elevation token.
    pub shadow: Option<ShadowStyle>,
    /// `BackgroundGradient`.
    pub gradient: Option<BackgroundGradient>,
    /// `TextColor` and `TextFont::font_size`.
    pub text: Option<(Color, f32)>,
    /// `ImageNode` with `NodeImageMode::Sliced`.
    pub sliced: Option<SlicedPaint>,
    /// The glass material, when the theme asked for one and `blur` is on.
    pub glass: Option<GlassPaint>,
}

impl Paint {
    /// Resolves `material` against `theme`. Unknown palette references come
    /// back magenta rather than failing, and `Material::Shader` paints
    /// nothing in Phase 2.
    pub fn from_material(theme: &Theme, material: &Material) -> Self {
        let tokens = &theme.tokens;
        let color = |c: &ThemeColor| theme.color(c);
        let mut paint = Self::default();
        match material {
            Material::Solid {
                fill,
                border,
                radius,
                elevation,
            } => {
                paint.background = Some(color(fill));
                paint.border = border.as_ref().map(&color);
                paint.radius = *radius;
                paint.shadow = shadow(theme, tokens, elevation.as_deref());
            }
            Material::Gradient {
                angle,
                stops,
                border,
                radius,
                elevation,
            } => {
                paint.gradient = Some(BackgroundGradient(vec![Gradient::Linear(
                    LinearGradient::new(
                        angle.to_radians(),
                        stops
                            .iter()
                            .map(|(at, c)| ColorStop::percent(color(c), at * 100.0))
                            .collect(),
                    ),
                )]));
                paint.border = border.as_ref().map(&color);
                paint.radius = *radius;
                paint.shadow = shadow(theme, tokens, elevation.as_deref());
            }
            Material::Sliced {
                image,
                border,
                scale,
                tint,
            } => {
                paint.sliced = Some(SlicedPaint {
                    image: image.clone(),
                    slicer: TextureSlicer {
                        border: BorderRect::all(*border),
                        max_corner_scale: *scale,
                        ..default()
                    },
                    tint: tint.as_ref().map_or(Color::WHITE, &color),
                });
            }
            Material::Glass {
                tint,
                blur,
                border,
                radius,
                elevation,
                fallback_alpha_boost,
            } => {
                let tint = color(tint);
                paint.radius = *radius;
                paint.shadow = shadow(theme, tokens, elevation.as_deref());
                paint.border = border.as_ref().map(&color);
                if cfg!(feature = "blur") {
                    paint.glass = Some(GlassPaint {
                        tint,
                        edge: paint.border.unwrap_or(Color::NONE),
                        blur_radius: blur.unwrap_or(tokens.blur.radius),
                    });
                } else {
                    // Degrade to a tinted solid: with no backdrop to blur, the
                    // panel has to carry its own legibility over the world.
                    let alpha = (tint.alpha() + fallback_alpha_boost).clamp(0.0, 1.0);
                    paint.background = Some(tint.with_alpha(alpha));
                }
            }
            Material::Shader { shader, .. } => {
                tracing::debug!(%shader, "Material::Shader is not painted in Phase 2");
            }
            Material::Text { color: c, size } => {
                paint.text = Some((color(c), *size));
            }
        }
        paint
    }
}

fn shadow(theme: &Theme, tokens: &Tokens, name: Option<&str>) -> Option<ShadowStyle> {
    let elevation = tokens.elevation.get(name?)?;
    Some(ShadowStyle {
        color: theme.color(&elevation.color),
        x_offset: Val::ZERO,
        y_offset: Val::Px(elevation.y),
        spread_radius: Val::Px(elevation.spread),
        blur_radius: Val::Px(elevation.blur),
    })
}

/// Repaints nodes whose role changed, and every node when the theme asset
/// loads, is modified on disk, or `ActiveTheme` changes.
///
/// Writes only: `BackgroundColor`, `BorderColor`, `BackgroundGradient`,
/// `BoxShadow`, `BorderRadius`, `ImageNode` (Sliced), `TextColor`,
/// `TextFont::font_size`, and with `blur` the glass `MaterialNode`. Never any
/// other `Node` field, so layout stays the ui crate's.
#[allow(clippy::too_many_arguments)]
pub fn apply_theme(
    active: Res<ActiveTheme>,
    themes: Res<Assets<Theme>>,
    assets: Option<Res<AssetServer>>,
    mut events: MessageReader<AssetEvent<Theme>>,
    mut commands: Commands,
    mut nodes: Query<(
        Entity,
        Ref<Themed>,
        Option<&mut Node>,
        Option<&mut TextFont>,
    )>,
    #[cfg(feature = "blur")] glass_assets: Option<ResMut<Assets<crate::blur::GlassPanelMaterial>>>,
    #[cfg(feature = "blur")] backdrop: Option<Res<crate::blur::BackdropImage>>,
) {
    let repaint_all = active.is_changed()
        || events.read().any(|e| match e {
            AssetEvent::Added { id }
            | AssetEvent::Modified { id }
            | AssetEvent::LoadedWithDependencies { id } => *id == active.0.id(),
            _ => false,
        });
    let Some(theme) = themes.get(&active.0) else {
        return;
    };
    #[cfg(feature = "blur")]
    let mut glass_assets = glass_assets;
    for (entity, themed, node, text_font) in &mut nodes {
        if !repaint_all && !themed.is_changed() {
            continue;
        }
        let Some(material) = theme.material(&themed.0) else {
            tracing::warn!(role = %themed.0, theme = %theme.name, "no material for role");
            continue;
        };
        let paint = Paint::from_material(theme, material);
        let mut e = commands.entity(entity);

        match paint.background {
            Some(c) => {
                e.insert(BackgroundColor(c));
            }
            None => {
                e.remove::<BackgroundColor>();
            }
        }
        match paint.border {
            Some(c) => {
                e.insert(BorderColor::all(c));
            }
            None => {
                e.remove::<BorderColor>();
            }
        }
        // `BorderRadius` is a field of `Node` in Bevy 0.19, not a component
        // of its own; it is the single `Node` field the theme owns.
        if let Some(mut node) = node {
            let radius = paint
                .radius
                .map_or(BorderRadius::ZERO, |r| BorderRadius::all(Val::Px(r)));
            if node.border_radius != radius {
                node.border_radius = radius;
            }
        }
        match paint.shadow {
            Some(s) => {
                e.insert(BoxShadow(vec![s]));
            }
            None => {
                e.remove::<BoxShadow>();
            }
        }
        match paint.gradient {
            Some(g) => {
                e.insert(g);
            }
            None => {
                e.remove::<BackgroundGradient>();
            }
        }
        match (&paint.sliced, &assets) {
            (Some(s), Some(server)) => {
                e.insert(ImageNode {
                    image: server.load(&s.image),
                    image_mode: NodeImageMode::Sliced(s.slicer.clone()),
                    color: s.tint,
                    ..default()
                });
            }
            (Some(s), None) => {
                tracing::warn!(image = %s.image, "no AssetServer; sliced material skipped");
            }
            (None, _) => {
                e.remove::<ImageNode>();
            }
        }
        if let Some((color, size)) = paint.text {
            e.insert(TextColor(color));
            match text_font {
                Some(mut font) => font.font_size = FontSize::Px(size),
                None => {
                    e.insert(TextFont {
                        font_size: FontSize::Px(size),
                        ..default()
                    });
                }
            }
        }

        #[cfg(feature = "blur")]
        if let Some(assets) = glass_assets.as_deref_mut() {
            apply_glass(&mut e, paint.glass, assets, backdrop.as_deref());
        } else if paint.glass.is_some() {
            tracing::warn!("Material::Glass needs UiMaterialPlugin; nothing painted");
        }
    }
}

#[cfg(feature = "blur")]
fn apply_glass(
    e: &mut bevy::ecs::system::EntityCommands<'_>,
    glass: Option<GlassPaint>,
    assets: &mut Assets<crate::blur::GlassPanelMaterial>,
    backdrop: Option<&crate::blur::BackdropImage>,
) {
    use bevy::ui_render::prelude::MaterialNode;

    match (glass, backdrop) {
        (Some(g), Some(image)) => {
            let handle = assets.add(crate::blur::GlassPanelMaterial {
                tint: g.tint.into(),
                edge_color: g.edge.into(),
                blur_radius: g.blur_radius,
                blur_enabled: 1.0,
                pad: Vec2::ZERO,
                backdrop: image.0.clone(),
            });
            e.insert(MaterialNode(handle));
        }
        (Some(_), None) => {
            tracing::warn!("Material::Glass needs BackdropPlugin's image; nothing painted");
        }
        (None, _) => {
            e.remove::<MaterialNode<crate::blur::GlassPanelMaterial>>();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::role::roles;
    use pretty_assertions::assert_eq;

    const GLASS: &str = include_str!("../../../assets/themes/glass.theme.ron");

    fn theme() -> Theme {
        Theme::from_ron(GLASS).expect("glass theme parses")
    }

    fn hex(s: &str) -> Color {
        Color::Srgba(bevy::color::Srgba::hex(s).expect("hex literal"))
    }

    #[test]
    fn solid_maps_to_background_border_and_radius() {
        let theme = theme();
        let paint = Paint::from_material(
            &theme,
            theme.material(&roles::SLOT).expect("slot is defined"),
        );
        assert_eq!(paint.background, Some(hex("141A24B8")));
        assert_eq!(paint.border, Some(hex("2A34468C")));
        assert_eq!(paint.radius, Some(8.0));
        assert_eq!(paint.shadow, None);
        assert_eq!(paint.gradient, None);
        assert_eq!(paint.text, None);
    }

    #[test]
    fn elevation_becomes_a_box_shadow() {
        let theme = theme();
        let paint = Paint::from_material(
            &theme,
            theme.material(&roles::TOOLTIP).expect("tooltip is defined"),
        );
        let shadow = paint.shadow.expect("tooltip has elevation mid");
        assert_eq!(shadow.y_offset, Val::Px(8.0));
        assert_eq!(shadow.blur_radius, Val::Px(24.0));
        assert_eq!(shadow.spread_radius, Val::Px(1.0));
        assert_eq!(shadow.color, hex("0000008C"));
    }

    #[test]
    fn gradient_maps_stops_and_angle() {
        let theme = theme();
        let paint = Paint::from_material(
            &theme,
            theme
                .material(&roles::BUTTON_PRIMARY)
                .expect("button.primary is defined"),
        );
        let gradient = paint.gradient.expect("gradient material");
        let Gradient::Linear(linear) = &gradient.0[0] else {
            panic!("expected a linear gradient");
        };
        assert!((linear.angle - std::f32::consts::PI).abs() < 1e-5);
        assert_eq!(linear.stops.len(), 2);
        assert_eq!(linear.stops[0].point, Val::Percent(0.0));
        assert_eq!(linear.stops[1].point, Val::Percent(100.0));
        assert_eq!(linear.stops[0].color, hex("7FD1FF"));
    }

    #[test]
    fn text_maps_to_colour_and_size() {
        let theme = theme();
        let paint = Paint::from_material(
            &theme,
            theme.material(&roles::COUNT).expect("count is defined"),
        );
        assert_eq!(paint.text, Some((hex("E6EDF3"), 11.0)));
        assert_eq!(paint.background, None);
    }

    #[test]
    fn palette_reference_resolves_through_the_theme() {
        let theme = theme();
        let paint = Paint::from_material(
            &theme,
            theme
                .material(&roles::SLOT_FOCUS)
                .expect("slot.focus is defined"),
        );
        assert_eq!(paint.border, Some(hex("7FD1FF")));
    }

    #[test]
    fn sliced_carries_the_border_inset_and_scale() {
        let theme = theme();
        let paint = Paint::from_material(
            &theme,
            &Material::Sliced {
                image: "ui/frame.png".to_owned(),
                border: 6.0,
                scale: 2.0,
                tint: Some(ThemeColor::palette("accent")),
            },
        );
        let sliced = paint.sliced.expect("sliced paint");
        assert_eq!(sliced.image, "ui/frame.png");
        assert_eq!(sliced.slicer.border, BorderRect::all(6.0));
        assert!((sliced.slicer.max_corner_scale - 2.0).abs() < f32::EPSILON);
        assert_eq!(sliced.tint, hex("7FD1FF"));
    }

    #[test]
    #[cfg(not(feature = "blur"))]
    fn glass_degrades_to_a_boosted_solid_without_the_feature() {
        let theme = theme();
        let paint = Paint::from_material(
            &theme,
            theme.material(&roles::PANEL).expect("panel is defined"),
        );
        assert_eq!(paint.glass, None);
        let fill = paint.background.expect("degraded fill");
        assert!(
            (fill.alpha() - (hex("141A247A").alpha() + 0.35)).abs() < 1e-5,
            "alpha was {}",
            fill.alpha()
        );
        assert_eq!(paint.radius, Some(16.0));
        assert!(paint.shadow.is_some());
    }

    #[test]
    #[cfg(feature = "blur")]
    fn glass_maps_to_a_glass_paint_with_the_feature() {
        let theme = theme();
        let paint = Paint::from_material(
            &theme,
            theme.material(&roles::PANEL).expect("panel is defined"),
        );
        let glass = paint.glass.expect("glass paint");
        assert_eq!(glass.tint, hex("141A247A"));
        assert!((glass.blur_radius - 4.0).abs() < f32::EPSILON);
        assert_eq!(paint.background, None);
    }

    #[test]
    fn shader_paints_nothing_in_phase_2() {
        let theme = theme();
        let paint = Paint::from_material(
            &theme,
            &Material::Shader {
                shader: "shaders/whatever.wgsl".to_owned(),
                params: std::collections::BTreeMap::new(),
            },
        );
        assert_eq!(paint, Paint::default());
    }

    #[test]
    fn an_unknown_palette_reference_paints_magenta() {
        let theme = theme();
        let paint = Paint::from_material(
            &theme,
            &Material::Solid {
                fill: ThemeColor::palette("nope"),
                border: None,
                radius: None,
                elevation: None,
            },
        );
        assert_eq!(paint.background, Some(Color::srgb(1.0, 0.0, 1.0)));
    }

    #[test]
    fn a_state_role_falls_back_to_its_parent_material() {
        let theme = theme();
        let hover = Paint::from_material(
            &theme,
            theme
                .material(&Role::new("slot.hover.pressed"))
                .expect("falls back to slot.hover"),
        );
        let parent = Paint::from_material(
            &theme,
            theme.material(&roles::SLOT_HOVER).expect("slot.hover"),
        );
        assert_eq!(hover, parent);
    }
}
