//! Text nodes (menus M1 contract 2.1 and 2.2): the plain `text` node with
//! wrapping, alignment, arguments and truncation, and the `rich_text`
//! paragraph built from the markup in [`crate::rich`].

use bevy::asset::AssetServer;
use bevy::prelude::*;
use bevy::text::{ComputedTextBlock, FontStyle, FontWeight, LineHeight};
use slotted_theme::{ActiveTheme, Paint, Theme, ThemeSize, Themed, roles};

use crate::actions::{InputMode, UiBindings};
use crate::def::{IconDef, LocKey, TextAlign, TextOpts, TextRole};
use crate::loc::{LocArgs, Localization};
use crate::rich::{self, RichKeySpan, RichRun, RichRuns, RunKind};
use crate::screen::SpawnCtx;
use crate::semantic::{LocText, SemanticLabel, SemanticRole, WidgetNode};
use crate::widgets::{IconImages, kinds};

/// On a text node: truncate with `…` past this many laid-out lines.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct MaxLines(pub u16);

/// What [`enforce_max_lines`] remembers about a node between frames: the
/// whole string, what is on screen, and the width it was fitted to.
#[derive(Component, Debug, Clone, PartialEq)]
pub struct MaxLinesState {
    /// The resolved string before any truncation.
    pub full: String,
    /// What `Text` holds now: `full`, or a prefix of it plus `…`.
    pub shown: String,
    /// The parent's content width the truncation was fitted to.
    pub width: f32,
    /// `shown` has not been verified against a layout yet.
    pub pending: bool,
}

/// The weight bold runs use.
pub const BOLD: u16 = 700;

const fn justify(align: TextAlign) -> Justify {
    match align {
        TextAlign::Left => Justify::Left,
        TextAlign::Center => Justify::Center,
        TextAlign::Right => Justify::Right,
    }
}

const fn linebreak(wrap: bool) -> LineBreak {
    if wrap {
        LineBreak::WordBoundary
    } else {
        LineBreak::NoWrap
    }
}

/// The `TextLayout` a text node's options ask for.
pub const fn text_layout(opts: &TextOpts) -> TextLayout {
    TextLayout::new(justify(opts.align), linebreak(opts.wrap))
}

/// Spawns a plain text node.
pub fn spawn_text(
    ctx: &mut SpawnCtx<'_>,
    key: &LocKey,
    style: TextRole,
    opts: &TextOpts,
) -> Entity {
    let entity = ctx.spawn_node((
        Node::default(),
        Text::new(key.0.clone()),
        text_layout(opts),
        Themed(style.role()),
        SemanticRole::Text,
        SemanticLabel(key.0.clone()),
        WidgetNode(kinds::text()),
        LocText::with_args(key.clone(), opts.args.clone()),
    ));
    if let Some(max) = opts.max_lines {
        ctx.world.entity_mut(entity).insert(MaxLines(max));
    }
    entity
}

/// On a rich text node: what to render. `render_rich_text` rebuilds the
/// spans whenever this changes, the catalogue is replaced or the theme
/// loads.
#[derive(Component, Debug, Clone)]
pub struct RichText {
    /// Localisation key; the resolved string is the markup.
    pub key: LocKey,
    /// Fluent arguments, also substituted for `{name}` in the markup.
    pub args: LocArgs,
    /// Base style; `[b]`, `[color]` and `[size]` override parts of it.
    pub style: TextRole,
    /// A single-line row whose `{icon:..}` are images beside the text.
    pub inline: bool,
    /// Line layout of the paragraph.
    pub layout: TextLayout,
}

/// Marker on every entity `render_rich_text` spawned under a rich text
/// node, so a rebuild despawns exactly those.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct RichPart;

/// On a rich text node whose markup failed to parse, so the error is logged
/// once per node rather than once per rebuild.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct RichParseWarned;

/// Spawns a rich text paragraph: one `Text` with a `TextSpan` child per run,
/// or with `inline` a flex row of text fragments and icon images.
pub fn spawn_rich_text(
    ctx: &mut SpawnCtx<'_>,
    key: &LocKey,
    style: TextRole,
    opts: &TextOpts,
) -> Entity {
    let rich = RichText {
        key: key.clone(),
        args: opts.args.clone(),
        style,
        inline: opts.inline,
        layout: text_layout(opts),
    };
    let common = (
        Themed(style.role()),
        SemanticRole::Text,
        SemanticLabel(key.0.clone()),
        WidgetNode(kinds::rich_text()),
        rich,
    );
    if opts.inline {
        let gap = ctx.tokens().spacing.xs;
        ctx.spawn_node((
            Node {
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: px(gap),
                ..default()
            },
            common,
        ))
    } else {
        ctx.spawn_node((Node::default(), Text::default(), text_layout(opts), common))
    }
}

/// Where a span's `TextFont` and `TextColor` come from: the theme when it
/// has loaded, Bevy's defaults until then (the theme landing re-renders).
struct SpanPalette<'a> {
    theme: Option<&'a Theme>,
    assets: Option<&'a AssetServer>,
    base: Paint,
    key: Paint,
    icon: Paint,
}

impl SpanPalette<'_> {
    fn font_of(&self, paint: &Paint) -> TextFont {
        paint.text_font(self.assets).unwrap_or_default()
    }

    fn color_of(paint: &Paint) -> TextColor {
        paint.text_color().unwrap_or_default()
    }

    /// The font and colour of one run: the base paint with the run's
    /// overrides, or the `text.key` / `text.icon` role for those kinds.
    fn styled(&self, run: &RichRun) -> (TextFont, TextColor) {
        let paint = match run.kind {
            RunKind::Text => &self.base,
            RunKind::Key(_) => &self.key,
            RunKind::Icon(_) => &self.icon,
        };
        let mut font = self.font_of(paint);
        let mut color = Self::color_of(paint);
        if run.style.bold {
            font.weight = FontWeight(BOLD);
        }
        if run.style.italic {
            font.style = FontStyle::Italic;
        }
        if let (Some(c), Some(theme)) = (&run.style.color, self.theme) {
            color = TextColor(theme.color(c));
        }
        if let Some(size) = &run.style.size {
            font.font_size = FontSize::Px(self.size_of(size));
        }
        (font, color)
    }

    fn size_of(&self, size: &ThemeSize) -> f32 {
        match self.theme {
            Some(theme) => theme.size(size),
            None => size.parse_px().unwrap_or(13.0),
        }
    }

    /// The base style's line height in px, for an inline icon's box.
    fn line_px(&self) -> f32 {
        let size = self.base.text.map_or(13.0, |(_, px)| px);
        match self.base.line_height() {
            LineHeight::Px(px) => px,
            LineHeight::RelativeToFont(scale) => size * scale,
        }
    }
}

/// The localised name of an item, for `{icon:..}` in a wrapped paragraph.
fn item_name(
    registries: Option<&slotted_ecs::Registries>,
    loc: &Localization,
    item: &slotted_model::Namespaced,
) -> String {
    registries
        .and_then(|r| {
            let id = r.0.items.id_of(item)?;
            r.0.items.get(id)?.display_name.clone()
        })
        .map_or_else(|| item.to_string(), |name| loc.text_for(&name))
}

/// The runs of a rich text node: the key localised with its arguments, the
/// markup's own `{name}` placeholders substituted, then parsed. A parse
/// error comes back beside a single raw run.
///
/// The argument values go to the catalogue escaped, so a Fluent `{ $name }`
/// can never open a tag either; the `{name}` pass (what a catalogue-less
/// key carries) escapes on its own.
pub fn resolve_runs(
    loc: &Localization,
    rich: &RichText,
) -> (Vec<RichRun>, Option<rich::RichError>) {
    let escaped: LocArgs = rich
        .args
        .iter()
        .map(|(k, v)| {
            let v = match v {
                crate::values::Value::Text(t) => crate::values::Value::Text(rich::escape(t)),
                other => other.clone(),
            };
            (k.clone(), v)
        })
        .collect();
    let markup = rich::substitute(&loc.text_with(&rich.key, &escaped), &rich.args);
    match rich::parse(&markup) {
        Ok(runs) => (runs, None),
        Err(error) => (
            vec![RichRun {
                text: markup,
                style: rich::RunStyle::default(),
                kind: RunKind::Text,
            }],
            Some(error),
        ),
    }
}

/// `SlottedUiSet::Render`, before `refresh_key_glyphs`: builds the spans of
/// every new or changed [`RichText`], and of all of them when the catalogue
/// or the theme changes.
#[allow(clippy::too_many_arguments)]
pub fn render_rich_text(
    mut commands: Commands,
    loc: Res<Localization>,
    active: Option<Res<ActiveTheme>>,
    themes: Option<Res<Assets<Theme>>>,
    assets: Option<Res<AssetServer>>,
    icons: IconImages,
    registries: Option<Res<slotted_ecs::Registries>>,
    mode: Res<InputMode>,
    bindings: Res<UiBindings>,
    mut nodes: Query<(
        Entity,
        Ref<RichText>,
        Option<&Children>,
        Has<RichParseWarned>,
        Option<&mut RichRuns>,
    )>,
    parts: Query<(), With<RichPart>>,
) {
    let theme = active
        .as_deref()
        .zip(themes.as_deref())
        .and_then(|(active, themes)| themes.get(&active.0));
    let all = loc.is_changed()
        || active.as_ref().is_some_and(Res::is_changed)
        || themes.as_ref().is_some_and(Res::is_changed);
    for (entity, rich, children, warned, runs) in &mut nodes {
        if !all && !rich.is_changed() {
            continue;
        }
        let (parsed, error) = resolve_runs(&loc, &rich);
        if let Some(error) = error
            && !warned
        {
            tracing::warn!(key = %rich.key.0, %error, "rich text markup did not parse; rendered raw");
            commands.entity(entity).insert(RichParseWarned);
        }
        match runs {
            Some(mut existing) if existing.0 != parsed => existing.0.clone_from(&parsed),
            Some(_) => {}
            None => {
                commands.entity(entity).insert(RichRuns(parsed.clone()));
            }
        }

        for child in children.into_iter().flatten().copied() {
            if parts.contains(child) {
                commands.entity(child).despawn();
            }
        }

        let role = rich.style.role();
        let palette = SpanPalette {
            theme,
            assets: assets.as_deref(),
            base: theme
                .and_then(|t| Paint::for_role(t, &role))
                .unwrap_or_default(),
            key: theme
                .and_then(|t| Paint::for_role(t, &roles::TEXT_KEY))
                .unwrap_or_default(),
            icon: theme
                .and_then(|t| Paint::for_role(t, &roles::TEXT_ICON))
                .unwrap_or_default(),
        };
        let span_of = |run: &RichRun| -> (TextSpan, TextFont, TextColor, RichPart) {
            let text = match &run.kind {
                RunKind::Text => run.text.clone(),
                RunKind::Key(action) => rich::key_glyph_text(*action, *mode, &bindings),
                RunKind::Icon(item) => item_name(registries.as_deref(), &loc, item),
            };
            let (font, color) = palette.styled(run);
            (TextSpan(text), font, color, RichPart)
        };

        if !rich.inline {
            for run in &parsed {
                let mut span = commands.spawn((span_of(run), ChildOf(entity)));
                if let RunKind::Key(action) = run.kind {
                    span.insert(RichKeySpan(action));
                }
            }
            continue;
        }

        // Inline: text fragments and icon images in a row.
        let line = palette.line_px();
        let mut fragment: Option<Entity> = None;
        for run in &parsed {
            if let RunKind::Icon(item) = &run.kind {
                fragment = None;
                commands.spawn((
                    icons.image(&IconDef::Item(item.clone())),
                    Node {
                        width: px(line),
                        height: px(line),
                        flex_shrink: 0.0,
                        ..default()
                    },
                    Pickable::IGNORE,
                    RichPart,
                    ChildOf(entity),
                ));
                continue;
            }
            let parent = *fragment.get_or_insert_with(|| {
                commands
                    .spawn((
                        Node::default(),
                        Text::default(),
                        TextLayout::new(rich.layout.justify, LineBreak::NoWrap),
                        Themed(role.clone()),
                        Pickable::IGNORE,
                        RichPart,
                        ChildOf(entity),
                    ))
                    .id()
            });
            let mut span = commands.spawn((span_of(run), ChildOf(parent)));
            if let RunKind::Key(action) = run.kind {
                span.insert(RichKeySpan(action));
            }
        }
    }
}

/// `SlottedUiSet::Render`: enforces [`MaxLines`] by truncating the resolved
/// string until the laid-out line count fits.
///
/// Bevy has no max-lines, and a layout pass is the only way to know how
/// many lines a string takes, so this converges over frames: a fresh
/// string is laid out whole, and each time the laid-out count exceeds the
/// limit the string is cut in proportion and `…` appended, until it fits.
/// Cutting is monotone, so it terminates; a typical paragraph settles in two
/// or three frames. The whole string comes back only when the parent's
/// content width grows, which is the one event that can make more fit.
pub fn enforce_max_lines(
    mut commands: Commands,
    mut texts: Query<(
        Entity,
        &MaxLines,
        &mut Text,
        Ref<ComputedTextBlock>,
        Option<&ChildOf>,
        Option<&mut MaxLinesState>,
    )>,
    parents: Query<&ComputedNode>,
) {
    for (entity, max, mut text, block, parent, state) in &mut texts {
        let width = parent
            .and_then(|p| parents.get(p.parent()).ok())
            .map_or(0.0, |node| node.content_box().width());
        let Some(mut state) = state else {
            commands.entity(entity).insert(MaxLinesState {
                full: text.0.clone(),
                shown: text.0.clone(),
                width,
                pending: true,
            });
            continue;
        };
        if text.0 != state.shown {
            // Someone else wrote the text: a fresh resolve. Lay it out whole.
            state.full.clone_from(&text.0);
            state.shown.clone_from(&text.0);
            state.width = width;
            state.pending = true;
            continue;
        }
        if width > state.width + 0.5 {
            state.width = width;
            if state.shown != state.full {
                let full = state.full.clone();
                text.0.clone_from(&full);
                state.shown = full;
            }
            state.pending = true;
            continue;
        }
        state.width = state.width.min(width);
        // A layout that landed on its own (the theme's font arrived, the
        // parent shrank) is checked too; a verified string is otherwise left
        // alone, which is what keeps this cheap.
        if !state.pending && !block.is_changed() {
            continue;
        }
        let lines = block.buffer().lines().count();
        let max_lines = usize::from(max.0.max(1));
        if lines <= max_lines {
            state.pending = false;
            continue;
        }
        let kept: Vec<char> = state
            .shown
            .strip_suffix('…')
            .unwrap_or(&state.shown)
            .chars()
            .collect();
        // Proportional cut, always by at least one character.
        let keep = (kept.len() * max_lines / lines).min(kept.len().saturating_sub(1));
        let mut next: String = kept[..keep].iter().collect();
        let trimmed = next.trim_end().len();
        next.truncate(trimmed);
        next.push('…');
        if next == state.shown {
            continue;
        }
        text.0.clone_from(&next);
        state.shown = next;
    }
}
