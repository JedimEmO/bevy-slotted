//! Rich text (menus M1 contract 2.2): one inline markup, parsed once into
//! runs that become `TextSpan` children.
//!
//! Tags: `[b]…[/b]`, `[i]…[/i]`, `[color=$accent]…[/color]`,
//! `[color=#RRGGBB]…[/color]`, `[size=heading]…[/size]`, `{key:accept}`,
//! `{icon:demo:chest}`. `{name}` is a Fluent argument and is substituted
//! before parsing. `[[` and `{{` are literals.

use bevy::prelude::*;
use slotted_model::Namespaced;
use slotted_theme::{ThemeColor, ThemeSize};

use crate::actions::UiAction;

/// The style of one run.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RunStyle {
    /// `[b]`.
    pub bold: bool,
    /// `[i]`.
    pub italic: bool,
    /// `[color=…]`.
    pub color: Option<ThemeColor>,
    /// `[size=…]`.
    pub size: Option<ThemeSize>,
}

/// What a run is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunKind {
    /// Plain text in the run's style.
    Text,
    /// `{key:action}`: the glyph text of the action's current binding.
    Key(UiAction),
    /// `{icon:ns:item}`: an inline item icon, or its name when wrapped.
    Icon(Namespaced),
}

/// One run of the parsed markup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RichRun {
    /// The text; empty for `Key` and `Icon`.
    pub text: String,
    /// The style.
    pub style: RunStyle,
    /// What it is.
    pub kind: RunKind,
}

/// Where the markup went wrong.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RichError {
    /// A tag was opened and never closed.
    #[error("unclosed `[{tag}]` opened at offset {at}")]
    Unclosed {
        /// The tag name.
        tag: String,
        /// Byte offset of the opening bracket.
        at: usize,
    },
    /// A closing tag with no matching open tag.
    #[error("stray `[/{tag}]` at offset {at}")]
    Stray {
        /// The tag name.
        tag: String,
        /// Byte offset.
        at: usize,
    },
    /// A tag the markup does not know.
    #[error("unknown tag `{tag}` at offset {at}")]
    UnknownTag {
        /// The tag as written.
        tag: String,
        /// Byte offset.
        at: usize,
    },
    /// A `{…}` that is neither a key, an icon nor a substituted argument.
    #[error("unknown placeholder `{{{name}}}` at offset {at}")]
    UnknownPlaceholder {
        /// The placeholder as written.
        name: String,
        /// Byte offset.
        at: usize,
    },
}

/// Parses markup into runs. Adjacent text with the same style is one run.
pub fn parse(markup: &str) -> Result<Vec<RichRun>, RichError> {
    // M1-IMPL: A
    let _ = markup;
    Ok(vec![RichRun {
        text: markup.to_owned(),
        style: RunStyle::default(),
        kind: RunKind::Text,
    }])
}

/// Substitutes `{name}` arguments before parsing, escaping any `[` or `{`
/// in a value so an argument can never open a tag.
pub fn substitute(
    markup: &str,
    args: &std::collections::BTreeMap<String, crate::values::Value>,
) -> String {
    // M1-IMPL: A
    let _ = args;
    markup.to_owned()
}

/// On a rich text node: the parsed runs, kept so a mode or binding change
/// can re-render the `Key` runs without re-parsing.
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub struct RichRuns(pub Vec<RichRun>);

/// `SlottedUiSet::Render`: re-renders `{key:..}` runs on `InputModeChanged`
/// or when `UiBindings` changes.
pub fn refresh_key_glyphs(
    _mode: Res<crate::actions::InputMode>,
    _bindings: Res<crate::actions::UiBindings>,
    _changed: MessageReader<crate::actions::InputModeChanged>,
    _nodes: Query<(Entity, &RichRuns, &Children)>,
    _spans: Query<&mut TextSpan>,
) {
    // M1-IMPL: A
}

/// The text a `{key:action}` run shows for the action's first binding on
/// the current device: `Enter`, `Esc`, `A`, `D-pad ↑`.
pub fn key_glyph_text(
    action: UiAction,
    mode: crate::actions::InputMode,
    bindings: &crate::actions::UiBindings,
) -> String {
    // M1-IMPL: A
    let _ = (mode, bindings);
    action.as_str().to_owned()
}

/// Registers the rich text systems.
pub fn build(app: &mut App) {
    app.add_systems(
        Update,
        refresh_key_glyphs.in_set(crate::plugin::SlottedUiSet::Render),
    );
}
