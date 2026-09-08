//! Rich text (menus M1 contract 2.2): one inline markup, parsed once into
//! runs that become `TextSpan` children.
//!
//! Tags: `[b]…[/b]`, `[i]…[/i]`, `[color=$accent]…[/color]`,
//! `[color=#RRGGBB]…[/color]`, `[size=heading]…[/size]`, `{key:accept}`,
//! `{icon:demo:chest}`. `{name}` is a Fluent argument and is substituted
//! before parsing. `[[`, `]]`, `{{` and `}}` are literals.

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
///
/// # Errors
///
/// A tag opened and never closed, a closing tag with no open tag, a tag or
/// placeholder the markup does not know, or a `[color=…]` / `[size=…]`
/// whose value is malformed. Every error carries the byte offset.
#[allow(clippy::too_many_lines)]
pub fn parse(markup: &str) -> Result<Vec<RichRun>, RichError> {
    let mut runs = Vec::new();
    let mut text = String::new();
    let mut style = RunStyle::default();
    // `(tag name, offset of its `[`, the style before it)`.
    let mut stack: Vec<(String, usize, RunStyle)> = Vec::new();
    let bytes = markup.as_bytes();
    let mut i = 0;

    let flush = |text: &mut String, style: &RunStyle, runs: &mut Vec<RichRun>| {
        if !text.is_empty() {
            runs.push(RichRun {
                text: std::mem::take(text),
                style: style.clone(),
                kind: RunKind::Text,
            });
        }
    };

    while i < bytes.len() {
        match bytes[i] {
            b'[' if bytes.get(i + 1) == Some(&b'[') => {
                text.push('[');
                i += 2;
            }
            b'{' if bytes.get(i + 1) == Some(&b'{') => {
                text.push('{');
                i += 2;
            }
            b']' if bytes.get(i + 1) == Some(&b']') => {
                text.push(']');
                i += 2;
            }
            b'}' if bytes.get(i + 1) == Some(&b'}') => {
                text.push('}');
                i += 2;
            }
            b'[' => {
                let Some(len) = markup[i + 1..].find(']') else {
                    return Err(RichError::UnknownTag {
                        tag: markup[i..].chars().take(16).collect(),
                        at: i,
                    });
                };
                let tag = &markup[i + 1..i + 1 + len];
                if let Some(name) = tag.strip_prefix('/') {
                    match stack.last() {
                        Some((open, _, _)) if open == name => {
                            let (_, _, before) = stack.pop().expect("checked");
                            flush(&mut text, &style, &mut runs);
                            style = before;
                        }
                        _ => {
                            return Err(RichError::Stray {
                                tag: name.to_owned(),
                                at: i,
                            });
                        }
                    }
                } else {
                    let (name, value) = tag.split_once('=').unwrap_or((tag, ""));
                    let mut next = style.clone();
                    match (name, value) {
                        ("b", "") => next.bold = true,
                        ("i", "") => next.italic = true,
                        ("color", v) if is_color(v) => next.color = Some(ThemeColor(v.to_owned())),
                        ("size", v) if is_size(v) => {
                            next.size = Some(if v.parse::<f32>().is_ok() {
                                ThemeSize(v.to_owned())
                            } else {
                                ThemeSize::typography(v)
                            });
                        }
                        _ => {
                            return Err(RichError::UnknownTag {
                                tag: tag.to_owned(),
                                at: i,
                            });
                        }
                    }
                    flush(&mut text, &style, &mut runs);
                    stack.push((name.to_owned(), i, style));
                    style = next;
                }
                i += len + 2;
            }
            b'{' => {
                let Some(len) = markup[i + 1..].find('}') else {
                    return Err(RichError::UnknownPlaceholder {
                        name: markup[i + 1..].chars().take(16).collect(),
                        at: i,
                    });
                };
                let name = &markup[i + 1..i + 1 + len];
                let kind = if let Some(action) = name.strip_prefix("key:") {
                    action.parse::<UiAction>().ok().map(RunKind::Key)
                } else if let Some(item) = name.strip_prefix("icon:") {
                    Namespaced::parse(item).ok().map(RunKind::Icon)
                } else {
                    None
                };
                let Some(kind) = kind else {
                    return Err(RichError::UnknownPlaceholder {
                        name: name.to_owned(),
                        at: i,
                    });
                };
                flush(&mut text, &style, &mut runs);
                runs.push(RichRun {
                    text: String::new(),
                    style: style.clone(),
                    kind,
                });
                i += len + 2;
            }
            _ => {
                let ch = markup[i..].chars().next().expect("in bounds");
                text.push(ch);
                i += ch.len_utf8();
            }
        }
    }
    if let Some((tag, at, _)) = stack.pop() {
        return Err(RichError::Unclosed { tag, at });
    }
    flush(&mut text, &style, &mut runs);
    // An empty tag pair (`[b][/b]`) split the text around it into two runs
    // of one style; fold those back so a run is always the longest it can be.
    let mut merged: Vec<RichRun> = Vec::with_capacity(runs.len());
    for run in runs {
        match merged.last_mut() {
            Some(last)
                if last.kind == RunKind::Text
                    && run.kind == RunKind::Text
                    && last.style == run.style =>
            {
                last.text.push_str(&run.text);
            }
            _ => merged.push(run),
        }
    }
    Ok(merged)
}

/// `$name` or `#RRGGBB` / `#RRGGBBAA`.
fn is_color(value: &str) -> bool {
    match value.strip_prefix('#') {
        Some(hex) => {
            (hex.len() == 6 || hex.len() == 8) && hex.chars().all(|c| c.is_ascii_hexdigit())
        }
        None => value.strip_prefix('$').is_some_and(|name| !name.is_empty()),
    }
}

/// A typography name (`heading`) or a literal size (`18`).
fn is_size(value: &str) -> bool {
    !value.is_empty()
        && (value.parse::<f32>().is_ok_and(|px| px > 0.0)
            || value
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-'))
}

/// Escapes `value` so it can sit inside markup without opening or closing
/// anything: every bracket and brace is doubled.
pub fn escape(value: &str) -> String {
    value
        .replace('[', "[[")
        .replace(']', "]]")
        .replace('{', "{{")
        .replace('}', "}}")
}

/// Substitutes `{name}` arguments before parsing, escaping any `[` or `{`
/// in a value so an argument can never open a tag. `{{name}}` stays a
/// literal; a name with no argument stays as written for [`parse`] to
/// report.
pub fn substitute(
    markup: &str,
    args: &std::collections::BTreeMap<String, crate::values::Value>,
) -> String {
    if args.is_empty() {
        return markup.to_owned();
    }
    let mut out = String::with_capacity(markup.len());
    let mut rest = markup;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        if let Some(escaped) = after.strip_prefix('{') {
            out.push_str("{{");
            rest = escaped;
            continue;
        }
        let Some(close) = after.find('}') else {
            out.push('{');
            rest = after;
            continue;
        };
        let name = &after[..close];
        if let Some(value) = args.get(name) {
            out.push_str(&escape(&value_text(value)));
            rest = &after[close + 1..];
        } else {
            out.push('{');
            rest = after;
        }
    }
    out.push_str(rest);
    out
}

/// A value as a Fluent-free string: `true`, `3`, `0.5`, the text.
pub fn value_text(value: &crate::values::Value) -> String {
    use crate::values::Value;
    match value {
        Value::Bool(b) => b.to_string(),
        Value::Int(i) => i.to_string(),
        Value::Float(f) => f.to_string(),
        Value::Text(t) => t.clone(),
    }
}

/// On a rich text node: the parsed runs, kept so a mode or binding change
/// can re-render the `Key` runs without re-parsing.
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub struct RichRuns(pub Vec<RichRun>);

/// On the `TextSpan` a `{key:action}` run rendered to, so a mode or binding
/// change can rewrite its text without re-parsing the node.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct RichKeySpan(pub UiAction);

/// `SlottedUiSet::Render`: re-renders `{key:..}` runs when [`InputMode`],
/// [`UiBindings`] or [`GlyphSet`] changes, or a gamepad connects (menus M2
/// contract 2.4: `Auto` may now resolve to another vendor's set).
///
/// [`InputMode`]: crate::actions::InputMode
/// [`UiBindings`]: crate::actions::UiBindings
pub fn refresh_key_glyphs(
    mode: Res<crate::actions::InputMode>,
    bindings: Res<crate::actions::UiBindings>,
    set: Res<GlyphSet>,
    gamepads: Query<&Gamepad>,
    mut connections: MessageReader<bevy::input::gamepad::GamepadConnectionEvent>,
    mut spans: Query<(&RichKeySpan, &mut TextSpan)>,
) {
    let connected = connections.read().count() > 0;
    if !mode.is_changed() && !bindings.is_changed() && !set.is_changed() && !connected {
        return;
    }
    let set = resolved_glyph_set(*set, *mode, &gamepads);
    for (key, mut span) in &mut spans {
        let want = key_glyph_text(key.0, *mode, &bindings, set);
        if span.0 != want {
            span.0 = want;
        }
    }
}

/// The text a `{key:action}` run shows for the action's first binding on
/// the current device: `Enter`, `Esc`, `A`, `D-pad ↑`. The pointer mode
/// shows the keyboard binding, since a mouse has none; an unbound action
/// shows its own name. `set` names the pad's button family; `Auto` reads as
/// Xbox here, so a caller resolves it first through [`resolved_glyph_set`].
/// [`GlyphSet::Keyboard`] shows the keyboard binding in gamepad mode too.
pub fn key_glyph_text(
    action: UiAction,
    mode: crate::actions::InputMode,
    bindings: &crate::actions::UiBindings,
    set: GlyphSet,
) -> String {
    use crate::actions::InputMode;
    let glyph = match (mode, set) {
        (InputMode::Gamepad, GlyphSet::Keyboard)
        | (InputMode::Keyboard | InputMode::Pointer, _) => {
            bindings.first_key(action).map(key_glyph)
        }
        (InputMode::Gamepad, set) => bindings.first_button(action).map(|b| button_glyph(b, set)),
    };
    glyph.unwrap_or_else(|| action.as_str().to_owned())
}

/// Which family of button names a `{key:..}` glyph and the hint bar use
/// (menus M2 contract 2.4). `Auto` follows the first connected pad's vendor.
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum GlyphSet {
    /// `PlayStation` for Sony, `Switch` for Nintendo, `Xbox` otherwise.
    #[default]
    Auto,
    /// Keyboard names even for a pad binding (a text-only UI).
    Keyboard,
    /// `A B X Y LB RB LT RT`.
    Xbox,
    /// `✕ ○ □ △ L1 R1 L2 R2`.
    PlayStation,
    /// `B A Y X L R ZL ZR`.
    Switch,
    /// Bevy's names: `South`, `East`, ...
    Generic,
}

/// The USB vendor id Sony's pads report.
pub const VENDOR_PLAYSTATION: u16 = 0x054C;
/// The USB vendor id Nintendo's pads report.
pub const VENDOR_SWITCH: u16 = 0x057E;

/// Resolves `Auto` from the connected pads (menus M2 contract 2.4): the
/// first `Gamepad` in query order names the vendor. Every other set is
/// returned as it is; `mode` is not consulted (a keyboard-mode hint bar
/// still names the pad's buttons for its gamepad rows).
pub fn resolved_glyph_set(
    set: GlyphSet,
    mode: crate::actions::InputMode,
    gamepads: &Query<&Gamepad>,
) -> GlyphSet {
    let _ = mode;
    match set {
        GlyphSet::Auto => {
            resolved_glyph_set_for_vendor(set, gamepads.iter().next().and_then(Gamepad::vendor_id))
        }
        other => other,
    }
}

/// [`resolved_glyph_set`] with the vendor id in hand: `0x054C` is
/// `PlayStation`, `0x057E` is `Switch`, anything else (or no pad) is Xbox.
pub fn resolved_glyph_set_for_vendor(set: GlyphSet, vendor: Option<u16>) -> GlyphSet {
    match set {
        GlyphSet::Auto => match vendor {
            Some(VENDOR_PLAYSTATION) => GlyphSet::PlayStation,
            Some(VENDOR_SWITCH) => GlyphSet::Switch,
            _ => GlyphSet::Xbox,
        },
        other => other,
    }
}

/// A keyboard key's display text.
pub fn key_glyph(key: KeyCode) -> String {
    use KeyCode as K;
    let fixed = match key {
        K::Enter | K::NumpadEnter => "Enter",
        K::Space => "Space",
        K::Escape => "Esc",
        K::Tab => "Tab",
        K::Backspace => "Backspace",
        K::Delete => "Del",
        K::Insert => "Ins",
        K::Home => "Home",
        K::End => "End",
        K::PageUp => "PgUp",
        K::PageDown => "PgDn",
        K::ArrowUp => "↑",
        K::ArrowDown => "↓",
        K::ArrowLeft => "←",
        K::ArrowRight => "→",
        K::ShiftLeft | K::ShiftRight => "Shift",
        K::ControlLeft | K::ControlRight => "Ctrl",
        K::AltLeft | K::AltRight => "Alt",
        K::SuperLeft | K::SuperRight => "Super",
        K::CapsLock => "Caps",
        K::Minus => "-",
        K::Equal => "=",
        K::Comma => ",",
        K::Period => ".",
        K::Slash => "/",
        K::Backslash => "\\",
        K::Semicolon => ";",
        K::Quote => "'",
        K::Backquote => "`",
        K::BracketLeft => "[",
        K::BracketRight => "]",
        _ => "",
    };
    if !fixed.is_empty() {
        return fixed.to_owned();
    }
    // `KeyA` → `A`, `Digit1` → `1`, `F5` → `F5`, `Numpad3` → `Num 3`.
    let name = format!("{key:?}");
    if let Some(rest) = name.strip_prefix("Key") {
        return rest.to_owned();
    }
    if let Some(rest) = name.strip_prefix("Digit") {
        return rest.to_owned();
    }
    if let Some(rest) = name.strip_prefix("Numpad") {
        return format!("Num {rest}");
    }
    name
}

/// A gamepad button's display text in `set` (`Auto` and `Keyboard` read as
/// Xbox here; the caller resolves `Auto` first through
/// [`resolved_glyph_set`], and `Keyboard` is [`key_glyph_text`]'s to
/// honour). `Generic` uses Bevy's own names, so it never claims a face
/// button's colour or letter.
pub fn button_glyph(button: bevy::input::gamepad::GamepadButton, set: GlyphSet) -> String {
    use bevy::input::gamepad::GamepadButton as G;
    let shared = match button {
        G::LeftThumb => Some("L3"),
        G::RightThumb => Some("R3"),
        G::DPadUp => Some("D-pad ↑"),
        G::DPadDown => Some("D-pad ↓"),
        G::DPadLeft => Some("D-pad ←"),
        G::DPadRight => Some("D-pad →"),
        _ => None,
    };
    let named = match set {
        GlyphSet::Generic => None,
        GlyphSet::PlayStation => match button {
            G::South => Some("✕"),
            G::East => Some("○"),
            G::West => Some("□"),
            G::North => Some("△"),
            G::LeftTrigger => Some("L1"),
            G::RightTrigger => Some("R1"),
            G::LeftTrigger2 => Some("L2"),
            G::RightTrigger2 => Some("R2"),
            G::Start => Some("Options"),
            G::Select => Some("Share"),
            G::Mode => Some("PS"),
            _ => shared,
        },
        GlyphSet::Switch => match button {
            G::South => Some("B"),
            G::East => Some("A"),
            G::West => Some("Y"),
            G::North => Some("X"),
            G::LeftTrigger => Some("L"),
            G::RightTrigger => Some("R"),
            G::LeftTrigger2 => Some("ZL"),
            G::RightTrigger2 => Some("ZR"),
            G::Start => Some("+"),
            G::Select => Some("−"),
            G::Mode => Some("Home"),
            _ => shared,
        },
        GlyphSet::Auto | GlyphSet::Keyboard | GlyphSet::Xbox => match button {
            G::South => Some("A"),
            G::East => Some("B"),
            G::West => Some("X"),
            G::North => Some("Y"),
            G::LeftTrigger => Some("LB"),
            G::RightTrigger => Some("RB"),
            G::LeftTrigger2 => Some("LT"),
            G::RightTrigger2 => Some("RT"),
            G::Start => Some("☰"),
            G::Select => Some("⧉"),
            G::Mode => Some("Home"),
            _ => shared,
        },
    };
    named.map_or_else(|| format!("{button:?}"), str::to_owned)
}

/// Registers the rich text systems.
pub fn build(app: &mut App) {
    app.init_resource::<GlyphSet>();
    app.add_systems(
        Update,
        (crate::widgets::text::render_rich_text, refresh_key_glyphs)
            .chain()
            .in_set(crate::plugin::SlottedUiSet::Render),
    );
}
