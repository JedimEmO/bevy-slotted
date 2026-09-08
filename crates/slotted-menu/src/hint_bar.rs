//! The hint bar (menus M2 contract 3.6): the `slotted:hint_bar` widget kind,
//! a row of glyph-and-label entries for what the focused node accepts.
//!
//! A template places one (`(type: "custom", kind: "slotted:hint_bar")`) and
//! so may a game screen. [`update_hint_bars`] rebuilds every bar's
//! [`HintEntries`] and children when focus, the input mode, the glyph set,
//! the bindings, the stack or the theme changes: the focused node's verb by
//! `SemanticRole` (a `hint.accept` tag on the node overrides it), `Back`
//! with the bar's screen's back label when its policy is `Pop`, and the tab
//! actions when the screen holds a `tabs` node. Hidden in pointer mode
//! unless the bar carries `tags: {"hint.always": "true"}`.
//!
//! A screen that owns its actions rather than a focused control (the
//! dialogue, menus M3 contract 4.5) writes the verbs on the bar itself: a
//! `hint.accept` tag on the bar is the `Accept` entry when no focused node
//! contributed one, and `hint.secondary` / `hint.back` add a `Secondary` and
//! a `Back` entry. A change to the bar's tags rebuilds it.

use bevy::input_focus::InputFocus;
use bevy::prelude::*;
use ron::Value;
use slotted_theme::{ActiveTheme, Material, Role, Theme, Themed, roles};
use slotted_ui::{
    GlyphSet, InputMode, LocKey, LocText, PresentationMode, ScreenRoot, ScreenStack, SemanticRole,
    SpawnCtx, TabsState, Tags, UiAction, UiBindings, UiNodeDef, Widget, WidgetKind, WidgetRegistry,
    def::BackPolicy, key_glyph_text, resolved_glyph_set,
};

/// The kind.
pub fn kind() -> WidgetKind {
    WidgetKind::new("slotted:hint_bar")
}

/// The tag on a focusable node that replaces the verb the bar would show
/// for its role; the value is a localisation key.
pub const ACCEPT_TAG: &str = "hint.accept";
/// The tag on a bar that adds a `Secondary` entry; the value is a
/// localisation key (menus M3 contract 4.5).
pub const SECONDARY_TAG: &str = "hint.secondary";
/// The tag on a bar that adds a `Back` entry; the value is a localisation
/// key (menus M3 contract 4.5).
pub const BACK_TAG: &str = "hint.back";
/// The tag on a bar that keeps it visible in pointer mode.
pub const ALWAYS_TAG: &str = "hint.always";
/// The role of the glyph's text inside its `hint.glyph` pill; falls back to
/// `hint.glyph` in a theme that draws the glyph as bare text.
pub const GLYPH_TEXT_ROLE: &str = "hint.glyph.text";

/// Marks a hint bar root.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct HintBar;

/// One entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HintEntry {
    /// The action whose glyph is shown.
    pub action: UiAction,
    /// The verb.
    pub label: LocKey,
}

impl HintEntry {
    fn new(action: UiAction, key: &str) -> Self {
        Self {
            action,
            label: LocKey(key.to_owned()),
        }
    }
}

/// The entries a bar currently shows, for tests.
#[derive(Component, Debug, Clone, Default, PartialEq, Eq)]
pub struct HintEntries(pub Vec<HintEntry>);

/// What a bar last rendered, so a rebuild happens only when something the
/// player would see changed.
#[derive(Component, Debug, Clone, Default, PartialEq, Eq)]
pub struct HintRendered {
    /// The glyph text per entry.
    pub glyphs: Vec<String>,
    /// Whether the glyphs were bracketed (the theme draws them as text).
    pub bracketed: bool,
    /// Whether the bar was shown.
    pub visible: bool,
    /// Whether a render has applied `Visibility` yet. A bar spawned in
    /// pointer mode starts hidden only once this is true.
    pub applied: bool,
}

/// On the text node inside a glyph pill.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct HintGlyph;

/// On a label text node.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct HintLabel;

/// `slotted:hint_bar` through the registry.
#[derive(Debug, Default, Clone, Copy)]
pub struct HintBarWidget;

impl Widget for HintBarWidget {
    fn spawn(&self, ctx: &mut SpawnCtx<'_>, _params: &Value, _children: &[UiNodeDef]) -> Entity {
        ctx.spawn_node((
            Node {
                column_gap: Val::Px(ctx.tokens().spacing.md),
                align_items: AlignItems::Center,
                flex_shrink: 0.0,
                ..default()
            },
            Themed(roles::HINT_BAR),
            SemanticRole::Custom("hint_bar".to_owned()),
            slotted_ui::WidgetNode(kind()),
            HintBar,
            HintEntries::default(),
            HintRendered::default(),
            Pickable::IGNORE,
        ))
    }
}

/// The verb a focused node of `role` accepts, as the bar's entries.
pub fn verb_for(role: &SemanticRole) -> Vec<HintEntry> {
    let accept = |key: &str| vec![HintEntry::new(UiAction::Accept, key)];
    match role {
        SemanticRole::Button | SemanticRole::Tab | SemanticRole::ListItem => {
            accept("slotted.menu.select")
        }
        SemanticRole::Toggle | SemanticRole::Chip => accept("slotted.menu.toggle"),
        SemanticRole::Slider => vec![
            HintEntry::new(UiAction::Left, "slotted.menu.adjust"),
            HintEntry::new(UiAction::Right, "slotted.menu.adjust"),
        ],
        SemanticRole::Select | SemanticRole::RadioGroup => accept("slotted.menu.change"),
        SemanticRole::TextField => accept("slotted.menu.edit"),
        SemanticRole::KeyBinding => accept("slotted.menu.rebind"),
        SemanticRole::Slot => accept("slotted.menu.pick_up"),
        _ => Vec::new(),
    }
}

/// Whether `entity` is `ancestor` or sits under it.
fn is_under(entity: Entity, ancestor: Entity, parents: &Query<&ChildOf>) -> bool {
    entity == ancestor || parents.iter_ancestors(entity).any(|a| a == ancestor)
}

/// Whether a `tabs` node sits under `entity`. The tabs are few, so this
/// walks up from each of them rather than down through the whole screen.
fn has_tabs(
    entity: Entity,
    tabs: &Query<Entity, With<TabsState>>,
    parents: &Query<&ChildOf>,
) -> bool {
    tabs.iter().any(|tab| is_under(tab, entity, parents))
}

/// The entries for a bar under `root` (or none) while `focused` has focus,
/// `bar_tags` being the bar's own tags.
#[allow(clippy::too_many_arguments)]
fn entries_for(
    root: Option<Entity>,
    bar_tags: Option<&Tags>,
    focused: Option<Entity>,
    roots: &Query<&ScreenRoot>,
    nodes: &Query<(&SemanticRole, Option<&Tags>)>,
    parents: &Query<&ChildOf>,
    tabs: &Query<Entity, With<TabsState>>,
) -> Vec<HintEntry> {
    let mut out = Vec::new();
    if let Some(focused) = focused
        && root.is_none_or(|r| is_under(focused, r, parents))
        && let Ok((role, tags)) = nodes.get(focused)
    {
        let mut verb = verb_for(role);
        if let Some(label) = tags.and_then(|t| t.get(ACCEPT_TAG)) {
            if verb.is_empty() {
                verb.push(HintEntry::new(UiAction::Accept, label));
            } else {
                for entry in &mut verb {
                    entry.label = LocKey(label.to_owned());
                }
            }
        }
        out.extend(verb);
    }
    if let Some(tags) = bar_tags {
        if out.is_empty()
            && let Some(label) = tags.get(ACCEPT_TAG)
        {
            out.push(HintEntry::new(UiAction::Accept, label));
        }
        if let Some(label) = tags.get(SECONDARY_TAG) {
            out.push(HintEntry::new(UiAction::Secondary, label));
        }
        if let Some(label) = tags.get(BACK_TAG) {
            out.push(HintEntry::new(UiAction::Back, label));
        }
    }
    if let Some(root) = root
        && let Ok(screen) = roots.get(root)
    {
        // An overlay is never popped by `Back` whatever its policy says
        // (menus M3: a `focus: true` overlay names its own `hint.back`).
        if screen.presentation.back == BackPolicy::Pop {
            let key = match screen.presentation.mode {
                PresentationMode::Modal => Some("slotted.menu.close"),
                PresentationMode::Page => Some("slotted.menu.back"),
                PresentationMode::Overlay => None,
            };
            if let Some(key) = key {
                out.push(HintEntry::new(UiAction::Back, key));
            }
        }
        if has_tabs(root, tabs, parents) {
            out.push(HintEntry::new(UiAction::TabPrev, "slotted.menu.prev_tab"));
            out.push(HintEntry::new(UiAction::TabNext, "slotted.menu.next_tab"));
        }
    }
    out
}

/// `SlottedUiSet::Render`: rebuilds every bar's entries and children when
/// focus, the input mode, the glyph set, the bindings, the stack or the
/// theme changed, a bar just spawned, or a bar's tags changed.
#[allow(clippy::too_many_arguments)]
pub fn update_hint_bars(
    focus: Option<Res<InputFocus>>,
    mode: Res<InputMode>,
    glyphs: Res<GlyphSet>,
    bindings: Res<UiBindings>,
    stack: Res<ScreenStack>,
    active: Option<Res<ActiveTheme>>,
    themes: Option<Res<Assets<Theme>>>,
    tokens: slotted_ui::tooltip::ThemeTokens,
    gamepads: Query<&Gamepad>,
    mut bars: Query<(
        Entity,
        Ref<HintBar>,
        &mut HintEntries,
        &mut HintRendered,
        Option<Ref<Tags>>,
    )>,
    roots: Query<&ScreenRoot>,
    nodes: Query<(&SemanticRole, Option<&Tags>)>,
    parents: Query<&ChildOf>,
    tabs: Query<Entity, With<TabsState>>,
    mut commands: Commands,
) {
    let theme_changed = active.as_ref().is_some_and(DetectChanges::is_changed)
        || themes.as_ref().is_some_and(DetectChanges::is_changed);
    let any_added = bars
        .iter()
        .any(|(_, bar, _, _, tags)| bar.is_added() || tags.is_some_and(|t| t.is_changed()));
    if !(focus.as_ref().is_some_and(DetectChanges::is_changed)
        || mode.is_changed()
        || glyphs.is_changed()
        || bindings.is_changed()
        || stack.is_changed()
        || theme_changed
        || any_added)
    {
        return;
    }
    let focused = focus.as_deref().and_then(InputFocus::get);
    let set = resolved_glyph_set(*glyphs, *mode, &gamepads);
    let bracketed = active
        .as_ref()
        .zip(themes.as_ref())
        .and_then(|(a, t)| t.get(&a.0))
        .and_then(|theme| theme.material(&roles::HINT_GLYPH))
        .is_some_and(|m| matches!(m, Material::Text { .. }));
    let tokens = tokens.get();
    for (bar, _, mut entries, mut rendered, bar_tags) in &mut bars {
        let root = slotted_ui::screen_root_where(bar, &parents, |e| roots.contains(e));
        let bar_tags = bar_tags.as_deref();
        let wanted = entries_for(root, bar_tags, focused, &roots, &nodes, &parents, &tabs);
        let always = bar_tags.and_then(|t| t.get(ALWAYS_TAG)) == Some("true");
        let visible = *mode != InputMode::Pointer || always;
        let glyph_texts: Vec<String> = wanted
            .iter()
            .map(|e| key_glyph_text(e.action, *mode, &bindings, set))
            .collect();
        let next = HintRendered {
            glyphs: glyph_texts,
            bracketed,
            visible,
            applied: true,
        };
        if entries.0 == wanted && *rendered == next {
            continue;
        }
        if !rendered.applied || rendered.visible != visible {
            commands.entity(bar).insert(if visible {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            });
        }
        if entries.0 != wanted || rendered.glyphs != next.glyphs || rendered.bracketed != bracketed
        {
            commands.entity(bar).despawn_related::<Children>();
            spawn_groups(
                &mut commands,
                bar,
                &wanted,
                &next.glyphs,
                bracketed,
                &tokens,
            );
        }
        if entries.0 != wanted {
            entries.0 = wanted;
        }
        *rendered = next;
    }
}

/// Spawns the bar's children: consecutive entries with the same label share
/// one group (`[Left][Right] Adjust`), each group a row of glyph pills and a label.
fn spawn_groups(
    commands: &mut Commands,
    bar: Entity,
    entries: &[HintEntry],
    glyphs: &[String],
    bracketed: bool,
    tokens: &slotted_theme::Tokens,
) {
    let mut i = 0;
    while i < entries.len() {
        let label = &entries[i].label;
        let mut j = i;
        while j < entries.len() && entries[j].label == *label {
            j += 1;
        }
        let group = commands
            .spawn((
                Node {
                    align_items: AlignItems::Center,
                    column_gap: Val::Px(tokens.spacing.xs),
                    flex_shrink: 0.0,
                    ..default()
                },
                Pickable::IGNORE,
                ChildOf(bar),
            ))
            .id();
        for glyph in &glyphs[i..j] {
            let text = if bracketed {
                format!("[{glyph}]")
            } else {
                glyph.clone()
            };
            let pill = commands
                .spawn((
                    Node {
                        padding: UiRect::axes(
                            Val::Px(tokens.spacing.sm),
                            Val::Px(tokens.spacing.xs / 2.0),
                        ),
                        border_radius: BorderRadius::all(Val::Px(999.0)),
                        align_items: AlignItems::Center,
                        justify_content: JustifyContent::Center,
                        flex_shrink: 0.0,
                        ..default()
                    },
                    Themed(roles::HINT_GLYPH),
                    Pickable::IGNORE,
                    ChildOf(group),
                ))
                .id();
            commands.spawn((
                Node::default(),
                Text::new(text),
                Themed(Role::new(GLYPH_TEXT_ROLE)),
                HintGlyph,
                Pickable::IGNORE,
                ChildOf(pill),
            ));
        }
        commands.spawn((
            Node::default(),
            Text::new(label.0.clone()),
            LocText::new(label.clone()),
            Themed(roles::HINT_LABEL),
            HintLabel,
            Pickable::IGNORE,
            ChildOf(group),
        ));
        i = j;
    }
}

/// Registers the kind and the system.
pub fn build(app: &mut App) {
    if let Some(mut registry) = app.world_mut().get_resource_mut::<WidgetRegistry>() {
        registry.register(kind(), HintBarWidget);
    } else {
        tracing::warn!("MenuPlugin was added before SlottedUiPlugin; no hint bar kind registered");
    }
    app.add_systems(
        Update,
        update_hint_bars.in_set(slotted_ui::SlottedUiSet::Render),
    );
}
