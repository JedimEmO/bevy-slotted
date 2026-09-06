//! Painting roles onto nodes.

use bevy::asset::{AssetEvent, AssetServer, Assets, Handle};
use bevy::prelude::*;
use bevy::ui::prelude::{BorderRect, TextureSlicer};
use bevy::ui::widget::NodeImageMode;

use crate::material::{Corners, Material, ThemeColor};
use crate::role::Role;
use crate::theme::Theme;
use crate::tokens::Tokens;

/// How far along its diagonal a [`Material::Dashed`] border is hatched
/// before the gradient runs out of stops and turns solid. Larger than any
/// slot ring or tooltip; a dashed panel wider than this shows a solid
/// stretch in its far corner.
pub const DASH_SPAN: f32 = 512.0;

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

/// A tiled image paint.
#[derive(Debug, Clone, PartialEq)]
pub struct TiledPaint {
    /// Asset path of the tile.
    pub image: String,
    /// Tile scale.
    pub scale: f32,
    /// Tint.
    pub tint: Color,
}

/// Where a text role's font comes from, resolved from a `tokens.fonts` entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FontPaint {
    /// Load this asset.
    Path(String),
    /// Ask Bevy's font database for this family.
    Family(String),
}

/// The chamfered box a [`Material::CutCorners`] resolves to. Only the `blur`
/// feature (the GPU materials feature) turns this into a `MaterialNode`;
/// the paint always also carries the square fallback in `background`,
/// `border` and `gradient`, which is what a headless app or a game without
/// `CutCornerPlugin` shows.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CutPaint {
    /// Fill.
    pub fill: Color,
    /// Border colour; transparent for none.
    pub border: Color,
    /// Border width in px.
    pub border_width: f32,
    /// Cut length along each edge in px.
    pub cut: f32,
    /// Which corners are cut.
    pub corners: Corners,
    /// Bottom accent bar colour; transparent for none.
    pub bar: Color,
    /// Accent bar height in px.
    pub bar_height: f32,
    /// Glow bleed above the bar in px.
    pub glow: f32,
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
    /// `ImageNode` with `NodeImageMode::Tiled`.
    pub tiled: Option<TiledPaint>,
    /// `BorderGradient`: the hatched stroke of a `Dashed` material.
    pub border_gradient: Option<BorderGradient>,
    /// `Node::border`, all four edges, when a material sets its own stroke
    /// width. `None` leaves the node's border alone.
    pub border_width: Option<f32>,
    /// `TextFont::font`: a font asset to load, or a family name for the OS.
    /// `None` with `text` set means Bevy's default font.
    pub font: Option<FontPaint>,
    /// `TextShadow` colour; `None` with `text` set removes the shadow.
    pub text_shadow: Option<Color>,
    /// The cut-corner material, when the theme asked for one and `blur` is on.
    pub cut: Option<CutPaint>,
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
            Material::Tiled { .. } | Material::Dashed { .. } | Material::CutCorners { .. } => {
                paint.shaped(theme, material);
            }
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
            Material::Text {
                color: c,
                size,
                font,
                shadow: text_shadow,
            } => {
                paint.text = Some((color(c), *size));
                paint.text_shadow = text_shadow.as_ref().map(&color);
                paint.font = font
                    .as_deref()
                    .and_then(|name| tokens.font(name))
                    .and_then(|token| match (&token.path, token.system) {
                        (Some(path), _) => Some(FontPaint::Path(path.clone())),
                        (None, true) => Some(FontPaint::Family(token.family.clone())),
                        (None, false) => None,
                    });
            }
        }
        paint
    }

    /// The Phase 7 shapes: paper's tile and dashes, neon's cut corners.
    fn shaped(&mut self, theme: &Theme, material: &Material) {
        let tokens = &theme.tokens;
        let color = |c: &ThemeColor| theme.color(c);
        let paint = self;
        match material {
            Material::Tiled {
                image,
                scale,
                tint,
                border,
                radius,
                elevation,
            } => {
                paint.tiled = Some(TiledPaint {
                    image: image.clone(),
                    scale: *scale,
                    tint: tint.as_ref().map_or(Color::WHITE, &color),
                });
                paint.border = border.as_ref().map(&color);
                paint.radius = *radius;
                paint.shadow = shadow(theme, tokens, elevation.as_deref());
            }
            Material::Dashed {
                fill,
                stroke,
                width,
                dash,
                gap,
                radius,
                elevation,
            } => {
                paint.background = Some(color(fill));
                paint.border_gradient = Some(dashed_border(color(stroke), *dash, *gap));
                paint.border_width = *width;
                paint.radius = *radius;
                paint.shadow = shadow(theme, tokens, elevation.as_deref());
            }
            Material::CutCorners {
                fill,
                border,
                border_width,
                cut,
                corners,
                bar,
                bar_height,
                glow,
                elevation,
            } => {
                let fill = color(fill);
                let bar = bar.as_ref().map(&color);
                // The square fallback first: a headless app, or one without
                // `CutCornerPlugin`, shows this.
                paint.border = border.as_ref().map(&color);
                paint.shadow = shadow(theme, tokens, elevation.as_deref());
                match bar {
                    Some(bar) => {
                        paint.gradient = Some(bottom_bar(fill, bar, *bar_height, *glow));
                    }
                    None => paint.background = Some(fill),
                }
                if cfg!(feature = "blur") {
                    paint.cut = Some(CutPaint {
                        fill,
                        border: paint.border.unwrap_or(Color::NONE),
                        border_width: *border_width,
                        cut: *cut,
                        corners: *corners,
                        bar: bar.unwrap_or(Color::NONE),
                        bar_height: *bar_height,
                        glow: *glow,
                    });
                }
            }
            _ => {}
        }
    }
}

/// A hard-stop diagonal gradient alternating `stroke` and transparent, for
/// a `BorderGradient`. Positions are in px along the 45 degree gradient
/// line, so the dashes are the same size on every node up to [`DASH_SPAN`].
pub fn dashed_border(stroke: Color, dash: f32, gap: f32) -> BorderGradient {
    let dash = dash.max(0.5);
    let gap = gap.max(0.5);
    let mut stops = Vec::new();
    let mut at = 0.0;
    while at < DASH_SPAN {
        stops.push(ColorStop::px(stroke, at));
        stops.push(ColorStop::px(stroke, at + dash));
        stops.push(ColorStop::px(Color::NONE, at + dash));
        stops.push(ColorStop::px(Color::NONE, at + dash + gap));
        at += dash + gap;
    }
    BorderGradient(vec![Gradient::Linear(LinearGradient::new(
        45.0_f32.to_radians(),
        stops,
    ))])
}

/// The fallback for a cut-corner accent bar: a bottom-to-top gradient that
/// is `bar` for `height` px, fades through `glow` px, and is `fill` above.
pub fn bottom_bar(fill: Color, bar: Color, height: f32, glow: f32) -> BackgroundGradient {
    let height = height.max(0.0);
    let mut stops = vec![ColorStop::px(bar, 0.0), ColorStop::px(bar, height)];
    if glow > 0.0 {
        // The glow is the bar colour at a third of its alpha, blended over
        // the fill by the renderer.
        let haze = bar.with_alpha(bar.alpha() * 0.35);
        let haze = fill.mix(&haze, haze.alpha());
        stops.push(ColorStop::px(haze, height));
        stops.push(ColorStop::px(fill, height + glow));
    } else {
        stops.push(ColorStop::px(fill, height));
    }
    stops.push(ColorStop::percent(fill, 100.0));
    BackgroundGradient(vec![Gradient::Linear(LinearGradient::new(0.0, stops))])
}

fn shadow(theme: &Theme, tokens: &Tokens, name: Option<&str>) -> Option<ShadowStyle> {
    let elevation = tokens.elevation.get(name?)?;
    Some(ShadowStyle {
        color: theme.color(&elevation.color),
        x_offset: Val::Px(elevation.x),
        y_offset: Val::Px(elevation.y),
        spread_radius: Val::Px(elevation.spread),
        blur_radius: Val::Px(elevation.blur),
    })
}

/// Repaints nodes whose role changed, and every node when the theme asset
/// loads, is modified on disk, or `ActiveTheme` changes.
///
/// Writes only: `BackgroundColor`, `BorderColor`, `BackgroundGradient`,
/// `BorderGradient`, `BoxShadow`, `BorderRadius`, `ImageNode` (Sliced and
/// Tiled), `TextColor`, `TextFont`, `TextShadow`, and with `blur` the glass and cut-corner
/// `MaterialNode`s. `Node::border` is written only for a material that names
/// its own stroke width (`Dashed`), and no other `Node` field, so layout
/// stays the ui crate's.
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
    #[cfg(feature = "blur")] cut_assets: Option<ResMut<Assets<crate::cut::CutCornerMaterial>>>,
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
    #[cfg(feature = "blur")]
    let mut cut_assets = cut_assets;
    for (entity, themed, node, text_font) in &mut nodes {
        if !repaint_all && !themed.is_changed() {
            continue;
        }
        let Some(material) = theme.material(&themed.0) else {
            tracing::warn!(role = %themed.0, theme = %theme.name, "no material for role");
            continue;
        };
        #[allow(unused_mut)]
        let mut paint = Paint::from_material(theme, material);
        let mut e = commands.entity(entity);

        // With a cut-corner material available the shader draws the fill,
        // border and bar itself; the square fallback would show through the
        // chamfers, so it is dropped here rather than in `from_material`,
        // which cannot know whether the plugin is present.
        #[cfg(feature = "blur")]
        if paint.cut.is_some() && cut_assets.is_some() {
            paint.background = None;
            paint.border = None;
            paint.gradient = None;
        }

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
        if let Some(node) = node {
            paint_node_geometry(&paint, node);
        }
        match paint.shadow {
            Some(s) => {
                e.insert(BoxShadow(vec![s]));
            }
            None => {
                e.remove::<BoxShadow>();
            }
        }
        match &paint.gradient {
            Some(g) => {
                e.insert(g.clone());
            }
            None => {
                e.remove::<BackgroundGradient>();
            }
        }
        match &paint.border_gradient {
            Some(g) => {
                e.insert(g.clone());
            }
            None => {
                e.remove::<BorderGradient>();
            }
        }
        paint_image_and_text(&paint, &mut e, assets.as_deref(), text_font);

        #[cfg(feature = "blur")]
        if let Some(assets) = glass_assets.as_deref_mut() {
            apply_glass(&mut e, paint.glass, assets, backdrop.as_deref());
        } else if paint.glass.is_some() {
            tracing::warn!("Material::Glass needs UiMaterialPlugin; nothing painted");
        }
        #[cfg(feature = "blur")]
        if let Some(assets) = cut_assets.as_deref_mut() {
            apply_cut(&mut e, paint.cut, assets);
        } else if paint.cut.is_some() {
            tracing::debug!(
                "Material::CutCorners without CutCornerPlugin; square fallback painted"
            );
        }
    }
}

#[cfg(feature = "blur")]
fn apply_cut(
    e: &mut bevy::ecs::system::EntityCommands<'_>,
    cut: Option<CutPaint>,
    assets: &mut Assets<crate::cut::CutCornerMaterial>,
) {
    use bevy::ui_render::prelude::MaterialNode;

    match cut {
        Some(c) => {
            let handle = assets.add(crate::cut::CutCornerMaterial::from(c));
            e.insert(MaterialNode(handle));
        }
        None => {
            e.remove::<MaterialNode<crate::cut::CutCornerMaterial>>();
        }
    }
}

/// The two `Node` fields the theme owns: the radius always, the border width
/// only when the material names one.
fn paint_node_geometry(paint: &Paint, mut node: Mut<'_, Node>) {
    let radius = paint
        .radius
        .map_or(BorderRadius::ZERO, |r| BorderRadius::all(Val::Px(r)));
    if node.border_radius != radius {
        node.border_radius = radius;
    }
    if let Some(width) = paint.border_width {
        let border = UiRect::all(Val::Px(width));
        if node.border != border {
            node.border = border;
        }
    }
}

/// The `ImageNode` and text half of a paint: sliced or tiled image, and
/// `TextColor` plus `TextFont` with the theme's font.
fn paint_image_and_text(
    paint: &Paint,
    e: &mut bevy::ecs::system::EntityCommands<'_>,
    assets: Option<&AssetServer>,
    text_font: Option<Mut<'_, TextFont>>,
) {
    let image = match (&paint.sliced, &paint.tiled) {
        (Some(s), _) => Some((
            s.image.clone(),
            NodeImageMode::Sliced(s.slicer.clone()),
            s.tint,
        )),
        (None, Some(t)) => Some((
            t.image.clone(),
            NodeImageMode::Tiled {
                tile_x: true,
                tile_y: true,
                stretch_value: t.scale,
            },
            t.tint,
        )),
        (None, None) => None,
    };
    match (image, assets) {
        (Some((path, image_mode, color)), Some(server)) => {
            e.insert(ImageNode {
                image: server.load(&path),
                image_mode,
                color,
                ..default()
            });
        }
        (Some((path, ..)), None) => {
            tracing::warn!(image = %path, "no AssetServer; image material skipped");
        }
        (None, _) => {
            e.remove::<ImageNode>();
        }
    }
    if let Some((color, size)) = paint.text {
        e.insert(TextColor(color));
        match paint.text_shadow {
            Some(color) => {
                e.insert(TextShadow {
                    offset: Vec2::splat(1.0),
                    color,
                });
            }
            None => {
                e.remove::<TextShadow>();
            }
        }
        let font = match (&paint.font, assets) {
            (Some(FontPaint::Path(path)), Some(server)) => {
                FontSource::Handle(server.load::<Font>(path))
            }
            (Some(FontPaint::Path(path)), None) => {
                tracing::warn!(font = %path, "no AssetServer; theme font skipped");
                FontSource::default()
            }
            (Some(FontPaint::Family(family)), _) => FontSource::Family(family.as_str().into()),
            (None, _) => FontSource::default(),
        };
        match text_font {
            Some(mut text_font) => {
                text_font.font_size = FontSize::Px(size);
                if text_font.font != font {
                    text_font.font = font;
                }
            }
            None => {
                e.insert(TextFont {
                    font,
                    font_size: FontSize::Px(size),
                    ..default()
                });
            }
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
    const PAPER: &str = include_str!("../../../assets/themes/paper.theme.ron");
    const NEON: &str = include_str!("../../../assets/themes/neon.theme.ron");

    fn theme() -> Theme {
        Theme::from_ron(GLASS).expect("glass theme parses")
    }

    fn paper() -> Theme {
        Theme::from_ron(PAPER).expect("paper theme parses")
    }

    fn neon() -> Theme {
        Theme::from_ron(NEON).expect("neon theme parses")
    }

    fn linear(g: &[Gradient]) -> &LinearGradient {
        let Gradient::Linear(linear) = &g[0] else {
            panic!("expected a linear gradient");
        };
        linear
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
    fn tiled_maps_to_a_tiled_image_with_border_and_shadow() {
        let theme = paper();
        let paint = Paint::from_material(
            &theme,
            theme.material(&roles::PANEL).expect("panel is defined"),
        );
        let tiled = paint.tiled.expect("tiled paint");
        assert_eq!(tiled.image, "themes/paper-dot.png");
        assert!((tiled.scale - 1.0).abs() < f32::EPSILON);
        assert_eq!(tiled.tint, Color::WHITE);
        assert_eq!(paint.border, Some(hex("1E1B18")));
        assert_eq!(paint.radius, Some(2.0));
        assert!(paint.shadow.is_some());
        assert_eq!(paint.background, None, "the tile is its own ground");
        assert_eq!(paint.glass, None);
    }

    #[test]
    fn elevation_carries_a_horizontal_offset() {
        let theme = paper();
        let paint = Paint::from_material(
            &theme,
            theme.material(&roles::SLOT).expect("slot is defined"),
        );
        let shadow = paint.shadow.expect("ink shadow");
        assert_eq!(shadow.x_offset, Val::Px(2.0));
        assert_eq!(shadow.y_offset, Val::Px(2.0));
        assert_eq!(shadow.blur_radius, Val::Px(0.0));
    }

    #[test]
    fn dashed_maps_to_a_hatched_border_gradient_and_a_stroke_width() {
        let theme = paper();
        let paint = Paint::from_material(
            &theme,
            theme
                .material(&Role::new("slot.rarity.rare"))
                .expect("rare ring is defined"),
        );
        assert_eq!(paint.border_width, Some(2.0));
        assert_eq!(paint.border, None, "the dashes replace the flat border");
        assert_eq!(paint.background, Some(hex("00000000")));
        let border = paint.border_gradient.expect("border gradient");
        let linear = linear(&border.0);
        assert!((linear.angle - 45.0_f32.to_radians()).abs() < 1e-6);
        // 3 px on, 3 px off: four stops per 6 px period across the span.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let periods = (DASH_SPAN / 6.0).ceil() as usize;
        assert_eq!(linear.stops.len(), periods * 4);
        assert_eq!(linear.stops[0].color, hex("E0672B"));
        assert_eq!(linear.stops[0].point, Val::Px(0.0));
        assert_eq!(linear.stops[1].point, Val::Px(3.0));
        assert_eq!(linear.stops[2].color, Color::NONE);
        assert_eq!(linear.stops[2].point, Val::Px(3.0));
        assert_eq!(linear.stops[3].point, Val::Px(6.0));
        assert_eq!(linear.stops[4].point, Val::Px(6.0));
    }

    #[test]
    fn cut_corners_always_carry_the_square_fallback() {
        let theme = neon();
        let paint = Paint::from_material(
            &theme,
            theme.material(&roles::SLOT).expect("slot is defined"),
        );
        assert_eq!(paint.background, Some(hex("1A1C22")));
        assert_eq!(paint.border, Some(hex("353B49")));
        assert_eq!(paint.radius, None, "cut corners are square when degraded");
        assert_eq!(paint.gradient, None);
        assert_eq!(paint.cut.is_some(), cfg!(feature = "blur"));
    }

    #[test]
    fn a_rarity_bar_degrades_to_a_bottom_up_gradient() {
        let theme = neon();
        let paint = Paint::from_material(
            &theme,
            theme
                .material(&Role::new("slot.rarity.rare"))
                .expect("rare ring is defined"),
        );
        assert_eq!(paint.background, None, "the fill rides in the gradient");
        let gradient = paint.gradient.expect("bar gradient");
        let linear = linear(&gradient.0);
        assert!(linear.angle.abs() < 1e-6, "bottom to top");
        assert_eq!(linear.stops[0].color, hex("C7F464"));
        assert_eq!(linear.stops[0].point, Val::Px(0.0));
        assert_eq!(linear.stops[1].point, Val::Px(3.0));
        // The glow fades out over its length and the fill takes over.
        assert_eq!(linear.stops[3].point, Val::Px(13.0));
        assert_eq!(linear.stops[3].color, hex("00000000"));
        assert_eq!(
            linear.stops.last().map(|s| s.point),
            Some(Val::Percent(100.0))
        );
    }

    #[test]
    #[cfg(feature = "blur")]
    fn cut_corners_map_to_a_cut_paint_with_the_feature() {
        let theme = neon();
        let paint = Paint::from_material(
            &theme,
            theme
                .material(&Role::new("slot.rarity.legendary"))
                .expect("legendary ring is defined"),
        );
        let cut = paint.cut.expect("cut paint");
        assert_eq!(cut.fill, hex("00000000"));
        assert_eq!(cut.border, Color::NONE);
        assert!((cut.cut - 6.0).abs() < f32::EPSILON);
        assert_eq!(cut.corners, Corners::DIAGONAL);
        assert_eq!(cut.bar, hex("FF5FA2"));
        assert!((cut.bar_height - 3.0).abs() < f32::EPSILON);
        assert!((cut.glow - 14.0).abs() < f32::EPSILON);
        let material = crate::cut::CutCornerMaterial::from(cut);
        assert_eq!(material.corners, Vec4::new(0.0, 1.0, 0.0, 1.0));
        assert_eq!(material.params, Vec4::new(1.0, 6.0, 3.0, 14.0));
    }

    #[test]
    fn a_text_role_names_its_font_token() {
        let theme = paper();
        let paint = Paint::from_material(
            &theme,
            theme.material(&roles::COUNT).expect("count is defined"),
        );
        assert_eq!(paint.text, Some((hex("1E1B18"), 11.0)));
        // paper ships IBM Plex Mono under `assets/fonts/`, so the count's
        // `mono` token resolves to that file and a path beats every fallback.
        assert_eq!(
            paint.font,
            Some(FontPaint::Path(
                "fonts/ibm-plex-mono/IBMPlexMono-Regular.ttf".to_owned()
            ))
        );

        // Drop the file and the family is documentation again: no path and no
        // system flag means Bevy's built-in face.
        let mut named_only = theme.clone();
        named_only
            .tokens
            .fonts
            .get_mut("mono")
            .expect("mono token")
            .path = None;
        let paint = Paint::from_material(
            &named_only,
            named_only.material(&roles::COUNT).expect("count"),
        );
        assert_eq!(paint.font, None);

        // Same token with `system: true` asks the OS for the family instead.
        let mut system = named_only;
        system
            .tokens
            .fonts
            .get_mut("mono")
            .expect("mono token")
            .system = true;
        let paint = Paint::from_material(&system, system.material(&roles::COUNT).expect("count"));
        assert_eq!(
            paint.font,
            Some(FontPaint::Family("IBM Plex Mono".to_owned()))
        );
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
