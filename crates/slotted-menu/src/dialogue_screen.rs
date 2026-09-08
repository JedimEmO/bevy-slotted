//! The `slotted:dialogue` screen (menus M3 contract 1.3 and section 4):
//! presenting the runner's current node, the typewriter, the choices, the
//! history page and the hint entries.
//!
//! The runner (`dialogue.rs`) owns [`ActiveDialogue`] and every transition;
//! this module only draws it. [`present_dialogue_node`] watches the
//! resource: when the node changes it rewrites the template's `speaker`,
//! `text`, `portrait` and `choices` nodes (found by their `test_id`, so a
//! game's own template keeps working as long as it keeps the ids), and on
//! every change it syncs the `continue` caption and the hint bar's tags to
//! the state. [`typewriter`] counts virtual time on the `text` node's
//! [`DialogueLine`] and the choice column's delay. An option button's
//! `Activate` (a click, or `Accept` on the focused one) is
//! [`on_option_activate`], which hands the id to the runner.

use std::sync::Arc;
use std::time::Duration;

use bevy::input_focus::{FocusCause, InputFocus};
use bevy::prelude::*;
use slotted_theme::Motion;
use slotted_ui::widgets::text::resolve_runs;
use slotted_ui::{
    ButtonOpts, IconDef, LocArgs, LocKey, LocText, Localization, PushScreen, RichReveal, RichText,
    ScreenStack, Screens, SemanticLabel, SpawnCtx, Tags, TestId, UiNodeDef, ValueStore,
    active_tokens,
};

use crate::dialogue::{ActiveDialogue, DialogueConfig, DialogueNode, HistoryLine, NodeId};
use crate::hint_bar::{self, HintBar};

/// On the dialogue screen's root.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DialogueScreen;

/// On the `text` node while a line types: how far along it is.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct DialogueLine {
    /// Virtual time since the line started.
    pub elapsed: Duration,
    /// Units shown so far.
    pub units: usize,
    /// Units in the whole line.
    pub total: usize,
}

/// The column the option buttons live in.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DialogueChoices;

/// On the choice column while its buttons wait for `choice_delay`.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct DialogueChoicesPending {
    /// The screen root, so the spawn finds its way back.
    pub root: Entity,
    /// Virtual time since the choice was entered.
    pub elapsed: Duration,
}

/// On each option button.
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub struct DialogueOption {
    /// The option's id.
    pub id: String,
    /// Its index in the choice.
    pub index: usize,
}

/// The tag an option button carries (`dialogue.option = id`), so a locator
/// finds it.
pub const OPTION_TAG: &str = "dialogue.option";

/// The `test_id` of the option button for `id`: `option.<id>`, what the
/// buttons' `nav.up` / `nav.down` links name.
pub fn option_test_id(id: &str) -> String {
    format!("option.{id}")
}

/// What the screen last drew, on the root: the node and how long the
/// history was, so a re-entry of the same node (a `jump` back) presents
/// again while a reveal flip does not.
#[derive(Component, Debug, Clone, PartialEq, Eq)]
struct Presented {
    node: NodeId,
    lines: usize,
}

/// The `test_id`s of the template nodes the screen rewrites.
mod ids {
    pub const SPEAKER: &str = "speaker";
    pub const TEXT: &str = "text";
    pub const PORTRAIT: &str = "portrait";
    pub const CHOICES: &str = "choices";
    pub const CONTINUE: &str = "continue";
}

/// The transcript as one markup string (contract 4.4): `[b]speaker[/b]`,
/// the line, a blank line; a narration line has no speaker.
pub fn transcript(history: &[HistoryLine]) -> String {
    let mut out = String::new();
    for line in history {
        if let Some(speaker) = &line.speaker {
            out.push_str("[b]");
            out.push_str(&slotted_ui::rich::escape(speaker));
            out.push_str("[/b]\n");
        }
        out.push_str(&line.text);
        out.push_str("\n\n");
    }
    out
}

/// Pushes a `slotted:page` with the transcript so far (contract 4.4). The
/// body key *is* the transcript: an unresolved key draws as written.
pub fn open_history(commands: &mut Commands) {
    commands.queue(|world: &mut World| {
        let Some(active) = world.get_resource::<ActiveDialogue>() else {
            tracing::warn!("open_history: no dialogue is running");
            return;
        };
        let body = transcript(&active.history);
        let kind = crate::kinds::page();
        let Some(mut def) = world
            .get_resource::<Screens>()
            .and_then(|screens| crate::templates::cloned(screens, &kind))
        else {
            tracing::warn!("open_history: no `slotted:page` screen is registered");
            return;
        };
        def.set_text(
            "title",
            LocKey("slotted.menu.dialogue.history".to_owned()),
            LocArgs::new(),
        );
        def.set_text("body", LocKey(body), LocArgs::new());
        let root = world.spawn_empty().id();
        PushScreen {
            root,
            def: Arc::new(def),
            menu: None,
        }
        .apply(world);
    });
}

/// The node with `TestId(id)` under `root`, depth first.
fn node_under(world: &World, root: Entity, id: &str) -> Option<Entity> {
    let mut stack = vec![root];
    while let Some(entity) = stack.pop() {
        if world.get::<TestId>(entity).is_some_and(|t| t.0 == id) {
            return Some(entity);
        }
        if let Some(children) = world.get::<Children>(entity) {
            stack.extend(children.iter());
        }
    }
    None
}

/// Every hint bar under `root`.
fn hint_bars_under(world: &World, root: Entity) -> Vec<Entity> {
    let mut out = Vec::new();
    let mut stack = vec![root];
    while let Some(entity) = stack.pop() {
        if world.get::<HintBar>(entity).is_some() {
            out.push(entity);
        }
        if let Some(children) = world.get::<Children>(entity) {
            stack.extend(children.iter());
        }
    }
    out
}

fn set_visible(world: &mut World, entity: Entity, visible: bool) {
    let want = if visible {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };
    match world.get_mut::<Visibility>(entity) {
        Some(mut current) if *current != want => *current = want,
        Some(_) => {}
        None => {
            world.entity_mut(entity).insert(want);
        }
    }
}

/// The `speaker` node: its key, or hidden when the line is narration.
fn set_speaker(world: &mut World, root: Entity, speaker: Option<&LocKey>) {
    let Some(node) = node_under(world, root, ids::SPEAKER) else {
        return;
    };
    if let Some(key) = speaker {
        if let Some(mut loc) = world.get_mut::<LocText>(node)
            && loc.key != *key
        {
            loc.key = key.clone();
            loc.args = LocArgs::new();
        }
        if let Some(mut label) = world.get_mut::<SemanticLabel>(node)
            && label.0 != key.0
        {
            label.0.clone_from(&key.0);
        }
    }
    set_visible(world, node, speaker.is_some());
}

/// The `portrait` panel: the image inside it at `sizes.portrait`, or
/// hidden when the line has none.
fn set_portrait(world: &mut World, root: Entity, portrait: Option<&IconDef>, size: f32) {
    let Some(node) = node_under(world, root, ids::PORTRAIT) else {
        return;
    };
    world.entity_mut(node).despawn_related::<Children>();
    if let Some(mut layout) = world.get_mut::<Node>(node) {
        layout.width = Val::Px(size);
        layout.height = Val::Px(size);
        layout.flex_shrink = 0.0;
    }
    if let Some(icon) = portrait {
        let image = slotted_ui::widgets::icon_image(world, icon);
        world.spawn((
            Node {
                width: percent(100),
                height: percent(100),
                ..default()
            },
            image,
            Pickable::IGNORE,
            ChildOf(node),
        ));
    }
    set_visible(world, node, portrait.is_some());
}

/// The `text` node: its key and arguments; the units of the resolved runs.
fn set_text(
    world: &mut World,
    root: Entity,
    key: &LocKey,
    args: &LocArgs,
) -> Option<(Entity, usize)> {
    let node = node_under(world, root, ids::TEXT)?;
    let mut rich = world.get_mut::<RichText>(node)?;
    if rich.key != *key || rich.args != *args {
        rich.key = key.clone();
        rich.args = args.clone();
    }
    let rich = rich.clone();
    if let Some(mut label) = world.get_mut::<SemanticLabel>(node)
        && label.0 != key.0
    {
        label.0.clone_from(&key.0);
    }
    let units = world
        .get_resource::<Localization>()
        .map_or(0, |loc| RichReveal::units(&resolve_runs(loc, &rich).0));
    Some((node, units))
}

/// The `choices` column, emptied and hidden, with [`DialogueChoices`] on it.
fn clear_choices(world: &mut World, root: Entity) -> Option<Entity> {
    let column = node_under(world, root, ids::CHOICES)?;
    world.entity_mut(column).despawn_related::<Children>();
    world
        .entity_mut(column)
        .insert(DialogueChoices)
        .remove::<DialogueChoicesPending>();
    set_visible(world, column, false);
    Some(column)
}

/// Spawns one button per option of the current choice into the column and
/// focuses the first enabled one (contract 4.2). Nothing when the runner
/// has moved on since the delay started.
fn spawn_choices(world: &mut World, root: Entity) {
    let Some(active) = world.get_resource::<ActiveDialogue>() else {
        return;
    };
    if active.root != root || !matches!(active.current(), DialogueNode::Choice { .. }) {
        return;
    }
    let options: Vec<(usize, String, LocKey, bool)> = active
        .options(world.get_resource::<ValueStore>())
        .into_iter()
        .map(|(index, option, enabled)| (index, option.id.clone(), option.text.clone(), enabled))
        .collect();
    let Some(column) = node_under(world, root, ids::CHOICES) else {
        return;
    };
    world.entity_mut(column).despawn_related::<Children>();
    let enabled: Vec<&str> = options
        .iter()
        .filter(|(_, _, _, on)| *on)
        .map(|(_, id, _, _)| id.as_str())
        .collect();
    let mut first = None;
    for (index, id, text, on) in &options {
        let mut tags = Tags::new()
            .with(OPTION_TAG, id)
            .with(Tags::TEST_ID, &option_test_id(id));
        // A vertical walk over the enabled buttons only, wrapping at the
        // ends, so the geometric fallback never lands on a disabled one.
        if *on && let Some(at) = enabled.iter().position(|e| e == id) {
            let up = enabled[(at + enabled.len() - 1) % enabled.len()];
            let down = enabled[(at + 1) % enabled.len()];
            tags = tags
                .with(Tags::NAV_UP, &option_test_id(up))
                .with(Tags::NAV_DOWN, &option_test_id(down));
        }
        let def = UiNodeDef::Button {
            widget: None,
            opts: ButtonOpts {
                label: Some(text.clone()),
                icon: None,
                variant: default(),
                disabled: !on,
                compact: false,
            },
            tags,
        };
        let mut ctx = SpawnCtx {
            world,
            screen: root,
            kind: crate::kinds::dialogue(),
            menu: None,
            parent: column,
        };
        let button = ctx.spawn_child(&def);
        world.entity_mut(button).insert(DialogueOption {
            id: id.clone(),
            index: *index,
        });
        if *on && first.is_none() {
            first = Some(button);
        }
    }
    set_visible(world, column, true);
    // Focus goes to the first enabled option while the dialogue holds the
    // focus top; under a modal the modal's pop restores what it finds.
    let on_top = world
        .get_resource::<ScreenStack>()
        .and_then(ScreenStack::focus_top)
        .is_some_and(|top| top.root == root);
    if let (Some(first), true) = (first, on_top)
        && let Some(mut focus) = world.get_resource_mut::<InputFocus>()
    {
        focus.set(first, FocusCause::Navigated);
    }
}

/// The `continue` caption and the hint bar tags for the state the runner is
/// in (contract 4.5).
fn sync_state(world: &mut World, root: Entity) {
    let Some(active) = world.get_resource::<ActiveDialogue>() else {
        return;
    };
    let on_line = matches!(active.current(), DialogueNode::Say { .. });
    let revealed = active.revealed;
    let config = world
        .get_resource::<DialogueConfig>()
        .cloned()
        .unwrap_or_default();
    if let Some(caption) = node_under(world, root, ids::CONTINUE) {
        set_visible(world, caption, on_line && revealed);
    }
    let accept = match (on_line, revealed) {
        (true, false) => Some("slotted.menu.dialogue.skip"),
        (true, true) => Some("slotted.menu.dialogue.next"),
        (false, _) => None,
    };
    let secondary = config.history.then_some("slotted.menu.dialogue.history");
    let back = config.back_cancels.then_some("slotted.menu.dialogue.leave");
    for bar in hint_bars_under(world, root) {
        let mut tags = world.get::<Tags>(bar).cloned().unwrap_or_default();
        let mut changed = false;
        for (tag, want) in [
            (hint_bar::ACCEPT_TAG, accept),
            (hint_bar::SECONDARY_TAG, secondary),
            (hint_bar::BACK_TAG, back),
        ] {
            match want {
                Some(key) if tags.get(tag) != Some(key) => {
                    tags.0.insert(tag.to_owned(), key.to_owned());
                    changed = true;
                }
                None if tags.get(tag).is_some() => {
                    tags.0.remove(tag);
                    changed = true;
                }
                _ => {}
            }
        }
        if changed {
            world.entity_mut(bar).insert(tags);
        }
    }
}

/// The body of [`present_dialogue_node`], on the world.
fn present(world: &mut World) {
    let Some(active) = world.get_resource::<ActiveDialogue>().cloned() else {
        return;
    };
    let root = active.root;
    if world.get_entity(root).is_err() {
        return;
    }
    let fresh = world
        .get::<Presented>(root)
        .is_none_or(|p| p.node != active.node || p.lines != active.history.len());
    if fresh {
        let tokens = active_tokens(world);
        let reduced = world.get_resource::<Motion>().is_some_and(|m| m.reduced);
        match active.current() {
            DialogueNode::Say {
                speaker,
                portrait,
                text,
                args,
                ..
            } => {
                set_speaker(world, root, speaker.as_ref());
                set_portrait(world, root, portrait.as_ref(), tokens.sizes.portrait);
                clear_choices(world, root);
                let instant = reduced || tokens.dialogue.chars_per_second == 0;
                if let Some((node, total)) = set_text(world, root, text, args) {
                    if instant {
                        world
                            .entity_mut(node)
                            .insert(RichReveal::ALL)
                            .remove::<DialogueLine>();
                    } else {
                        world.entity_mut(node).insert((
                            RichReveal(Some(0)),
                            DialogueLine {
                                elapsed: Duration::ZERO,
                                units: 0,
                                total,
                            },
                        ));
                    }
                }
                if instant && !active.revealed {
                    world.resource_mut::<ActiveDialogue>().revealed = true;
                }
            }
            DialogueNode::Choice { prompt, .. } => {
                set_speaker(world, root, None);
                set_portrait(world, root, None, tokens.sizes.portrait);
                let prompt = prompt.clone().unwrap_or_else(|| LocKey(String::new()));
                if let Some((node, _)) = set_text(world, root, &prompt, &LocArgs::new()) {
                    world
                        .entity_mut(node)
                        .insert(RichReveal::ALL)
                        .remove::<DialogueLine>();
                }
                let column = clear_choices(world, root);
                if reduced || tokens.dialogue.choice_delay == 0 {
                    spawn_choices(world, root);
                } else if let Some(column) = column {
                    world.entity_mut(column).insert(DialogueChoicesPending {
                        root,
                        elapsed: Duration::ZERO,
                    });
                }
            }
            DialogueNode::End => {}
        }
        world.entity_mut(root).insert(Presented {
            node: active.node.clone(),
            lines: active.history.len(),
        });
    }
    sync_state(world, root);
}

/// On `ActiveDialogue` change: rewrites `speaker`, `text` and the portrait,
/// spawns or clears the option buttons, starts the reveal (contract 4.2),
/// and syncs the caption and the hint tags to the state.
pub fn present_dialogue_node(active: Option<Res<ActiveDialogue>>, mut commands: Commands) {
    if active.is_some_and(|a| a.is_changed()) {
        commands.queue(present);
    }
}

/// Virtual time: advances every typing line and shows the choices after
/// `choice_delay` (contract 4.3). A line the runner marked `revealed` from
/// outside (Accept skipped the reveal) jumps to its end the same frame.
pub fn typewriter(
    time: Res<Time<Virtual>>,
    tokens: slotted_ui::tooltip::ThemeTokens,
    active: Option<ResMut<ActiveDialogue>>,
    mut lines: Query<(Entity, Mut<DialogueLine>)>,
    mut pending: Query<(Entity, Mut<DialogueChoicesPending>)>,
    mut commands: Commands,
) {
    if lines.is_empty() && pending.is_empty() {
        return;
    }
    let Some(mut active) = active else {
        return;
    };
    let delta = time.delta();
    let tokens = tokens.get();
    let cps = tokens.dialogue.chars_per_second;
    for (entity, mut line) in &mut lines {
        let jump = active.revealed;
        // A line that started this frame shows nothing until the next.
        if line.is_added() && !jump {
            continue;
        }
        line.elapsed += delta;
        let units = if jump || cps == 0 {
            line.total
        } else {
            let typed = (line.elapsed.as_secs_f64() * f64::from(cps)).floor();
            // Saturating: a paused game cannot overflow this.
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let typed = typed as usize;
            typed.min(line.total)
        };
        if units != line.units {
            line.units = units;
            commands.entity(entity).insert(RichReveal(Some(units)));
        }
        if units >= line.total {
            commands
                .entity(entity)
                .remove::<DialogueLine>()
                .insert(RichReveal::ALL);
            if !active.revealed {
                active.revealed = true;
            }
        }
    }
    let delay = Duration::from_millis(u64::from(tokens.dialogue.choice_delay));
    for (entity, mut wait) in &mut pending {
        wait.elapsed += delta;
        if wait.elapsed >= delay {
            let root = wait.root;
            commands.entity(entity).remove::<DialogueChoicesPending>();
            commands.queue(move |world: &mut World| spawn_choices(world, root));
        }
    }
}

/// Observer on `Activate`: an option button picks its option through
/// `choose_dialogue`, which checks the option is still there and enabled.
pub fn on_option_activate(
    activate: On<bevy::ui_widgets::Activate>,
    options: Query<&DialogueOption>,
    mut commands: Commands,
) {
    if let Ok(option) = options.get(activate.entity) {
        crate::dialogue::choose_dialogue(&mut commands, &option.id);
    }
}

/// Registers the systems: present, then type, then the rich text render
/// sees the reveal the same frame.
pub fn build(app: &mut App) {
    app.add_observer(on_option_activate).add_systems(
        Update,
        (present_dialogue_node, typewriter)
            .chain()
            .in_set(slotted_ui::SlottedUiSet::Render)
            .before(slotted_ui::render_rich_text),
    );
}
