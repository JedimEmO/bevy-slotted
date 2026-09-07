//! Directional focus navigation over `UiActionEvent`s. Bevy ships
//! `AutoDirectionalNavigator` as a `SystemParam` and wires no input to it
//! (ADR 0002); this is what drives it, plus the explicit nav graph.

use std::collections::BTreeSet;

use bevy::input_focus::{FocusCause, InputFocus};
use bevy::math::CompassOctant;
use bevy::prelude::*;
use bevy::text::EditableText;
use bevy::ui::auto_directional_navigation::AutoDirectionalNavigator;

use crate::semantic::SemanticRole;

/// Whether a text-entry node owns the keyboard, as of the last
/// [`track_text_entry_focus`] run.
///
/// Every keyboard system in the crate asks this first. A focused search field
/// has to see its own arrow keys and its own digits, so a UI-level shortcut
/// must not act on a key the field is about to consume: without this, typing
/// "3" into the browser's search box also swaps a hovered slot with hotbar
/// slot 3, and the arrow keys walk the focus out of the field.
///
/// It is a resource rather than a `SystemParam` reading `InputFocus` because
/// `AutoDirectionalNavigator` already takes `ResMut<InputFocus>`, and a
/// system cannot hold both.
#[derive(Resource, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct TextEntryFocused(pub bool);

/// `SlottedUiSet::Input`, before every keyboard system: is the focused entity
/// a text field?
///
/// A node counts as text entry if it is a [`SemanticRole::TextField`] or
/// carries Bevy's [`EditableText`], so a game's own field is covered whether
/// it uses this crate's vocabulary or Bevy's widget.
pub fn track_text_entry_focus(
    focus: Option<Res<InputFocus>>,
    fields: Query<(Option<&SemanticRole>, Option<&EditableText>)>,
    mut focused: ResMut<TextEntryFocused>,
) {
    let active = focus
        .and_then(|f| f.get())
        .and_then(|e| fields.get(e).ok())
        .is_some_and(|(role, editable)| {
            editable.is_some() || role == Some(&SemanticRole::TextField)
        });
    if focused.0 != active {
        focused.0 = active;
    }
}

/// Ids a screen root has already logged as unresolved `nav.*` links, so
/// each is reported once per spawn and not once per key press.
#[derive(Component, Debug, Default, Clone, PartialEq, Eq)]
pub struct NavLinkWarned(pub BTreeSet<String>);

/// The screen root above `entity`, or `None` when it sits under none.
pub fn screen_root_of(
    entity: Entity,
    parents: &Query<&ChildOf>,
    roots: &Query<(), With<crate::semantic::ScreenRoot>>,
) -> Option<Entity> {
    let mut current = entity;
    loop {
        if roots.contains(current) {
            return Some(current);
        }
        current = parents.get(current).ok()?.parent();
    }
}

/// Whether `entity` is `ancestor` or sits somewhere under it.
fn is_under(entity: Entity, ancestor: Entity, parents: &Query<&ChildOf>) -> bool {
    let mut current = entity;
    loop {
        if current == ancestor {
            return true;
        }
        match parents.get(current) {
            Ok(child_of) => current = child_of.parent(),
            Err(_) => return false,
        }
    }
}

/// `entity` itself when it is `Focusable`, else its first `Focusable`
/// descendant in tree order.
pub fn first_focusable(
    entity: Entity,
    focusables: &Query<(), With<crate::focus_ring::Focusable>>,
    children: &Query<&Children>,
) -> Option<Entity> {
    if focusables.contains(entity) {
        return Some(entity);
    }
    let kids = children.get(entity).ok()?;
    kids.iter()
        .find_map(|child| first_focusable(child, focusables, children))
}

/// The node with `TestId(id)` under `root`.
fn node_by_id(
    id: &str,
    root: Entity,
    ids: &Query<(Entity, &crate::semantic::TestId)>,
    parents: &Query<&ChildOf>,
) -> Option<Entity> {
    ids.iter()
        .find(|(e, test_id)| test_id.0 == id && is_under(*e, root, parents))
        .map(|(e, _)| e)
}

/// The explicit link from `focused` in `dir`: the first `NavLinks` on the
/// focused node or one of its ancestors up to the screen root that names a
/// neighbour that way.
fn explicit_link(
    focused: Entity,
    dir: CompassOctant,
    root: Entity,
    links: &Query<&crate::def::NavLinks>,
    parents: &Query<&ChildOf>,
) -> Option<String> {
    let mut current = focused;
    loop {
        if let Ok(links) = links.get(current) {
            let link = match dir {
                CompassOctant::North => &links.up,
                CompassOctant::South => &links.down,
                CompassOctant::West => &links.left,
                _ => &links.right,
            };
            if let Some(id) = link {
                return Some(id.clone());
            }
        }
        if current == root {
            return None;
        }
        current = parents.get(current).ok()?.parent();
    }
}

/// `SlottedUiSet::Input`, after `UiActionEmit`: directional actions drive
/// focus (menus contract 2.4).
///
/// An explicit `NavLinks` link on the focused node or one of its ancestors
/// wins; otherwise Bevy's `AutoDirectionalNavigator` picks the target. A link
/// whose id resolves to nothing under the same screen root is logged once per
/// screen spawn and falls through to the navigator. Keyboard-sourced actions
/// are already suppressed while a text field has the keyboard
/// (`emit_ui_actions`), so this needs no guard of its own.
#[allow(clippy::too_many_arguments)]
pub fn directional_nav_actions(
    mut events: MessageReader<crate::actions::UiActionEvent>,
    links: Query<&crate::def::NavLinks>,
    parents: Query<&ChildOf>,
    ids: Query<(Entity, &crate::semantic::TestId)>,
    focusables: Query<(), With<crate::focus_ring::Focusable>>,
    children: Query<&Children>,
    roots: Query<(), With<crate::semantic::ScreenRoot>>,
    mut warned: Query<&mut NavLinkWarned>,
    mut nav: AutoDirectionalNavigator,
    mut commands: Commands,
) {
    for event in events.read() {
        let dir = match event.action {
            crate::actions::UiAction::Up => CompassOctant::North,
            crate::actions::UiAction::Down => CompassOctant::South,
            crate::actions::UiAction::Left => CompassOctant::West,
            crate::actions::UiAction::Right => CompassOctant::East,
            _ => continue,
        };
        let focused = nav.manual_directional_navigation.focus.get();
        let root = focused.and_then(|f| screen_root_of(f, &parents, &roots));
        if let (Some(focused), Some(root)) = (focused, root)
            && let Some(id) = explicit_link(focused, dir, root, &links, &parents)
        {
            let target = node_by_id(&id, root, &ids, &parents)
                .and_then(|node| first_focusable(node, &focusables, &children));
            if let Some(target) = target {
                nav.manual_directional_navigation
                    .focus
                    .set(target, FocusCause::Navigated);
                continue;
            }
            let fresh = if let Ok(mut set) = warned.get_mut(root) {
                set.0.insert(id.clone())
            } else {
                commands
                    .entity(root)
                    .insert(NavLinkWarned(BTreeSet::from([id.clone()])));
                true
            };
            if fresh {
                tracing::warn!(
                    id,
                    ?dir,
                    "nav link names no focusable node under this screen; falling through"
                );
            }
        }
        if let Err(e) = nav.navigate(dir) {
            tracing::trace!(?e, "directional navigation found no target");
        }
    }
}

/// `SlottedUiSet::Input`, after `UiActionEmit`: `Accept` acts on the focused
/// node, and is claimed when it did.
///
/// A focused slot takes a left click ([`slotted_ecs::SlotClicked`] with the
/// modifiers held), from either device: Bevy's `Button` turns Enter and
/// Space into `Activate` too, but nothing on a slot listens to that, so this
/// is the one place a keyboard or a pad picks up and places. A focused
/// `Button` is activated only for a gamepad-sourced press; the keyboard
/// already reaches it through Bevy's own `Activate`, and forwarding both
/// would activate twice. Only a fresh press counts.
pub fn accept_focused(
    mut events: MessageReader<crate::actions::UiActionEvent>,
    focus: Option<Res<InputFocus>>,
    slots: Query<(), With<slotted_ecs::SlotRef>>,
    buttons: Query<Has<bevy::ui::InteractionDisabled>, With<bevy::ui_widgets::Button>>,
    keys: Res<ButtonInput<KeyCode>>,
    mut claims: ResMut<crate::actions::UiActionClaims>,
    mut commands: Commands,
) {
    let press = events
        .read()
        .find(|e| e.action == crate::actions::UiAction::Accept && !e.repeat)
        .copied();
    let Some(press) = press else {
        return;
    };
    let Some(focused) = focus.as_deref().and_then(InputFocus::get) else {
        return;
    };
    if slots.contains(focused) {
        claims.claim(crate::actions::UiAction::Accept);
        commands.trigger(slotted_ecs::SlotClicked {
            entity: focused,
            button: slotted_model::Button::Left,
            modifiers: crate::widgets::modifiers_from(&keys),
        });
        return;
    }
    if press.device != crate::actions::InputDevice::Gamepad {
        return;
    }
    let Ok(disabled) = buttons.get(focused) else {
        return;
    };
    if disabled {
        return;
    }
    claims.claim(crate::actions::UiAction::Accept);
    commands.trigger(bevy::ui_widgets::Activate { entity: focused });
}

/// Observer on `ScreenSpawned`: gives the new screen its initial focus
/// (menus contract 2.4).
///
/// Only a `page` or a `modal` takes focus, and only when it is the top of the
/// stack or not in the stack at all. The choice, `initial_focus`'s first
/// focusable descendant or else the first `Focusable` in tree order, is
/// recorded in `ScreenRoot::initial_focus` even when `InputFocus` is absent,
/// so the stack can restore it later.
#[allow(clippy::too_many_arguments)]
pub fn focus_on_spawn(
    spawned: On<crate::screen::ScreenSpawned>,
    mut hints: Query<(
        &mut crate::semantic::ScreenRoot,
        &crate::semantic::ScreenFocusHint,
    )>,
    ids: Query<(Entity, &crate::semantic::TestId)>,
    focusables: Query<(), With<crate::focus_ring::Focusable>>,
    children: Query<&Children>,
    parents: Query<&ChildOf>,
    stack: Res<crate::stack::ScreenStack>,
    focus: Option<ResMut<InputFocus>>,
) {
    let root = spawned.entity;
    let Ok((mut screen, hint)) = hints.get_mut(root) else {
        return;
    };
    if screen.presentation.mode == crate::def::PresentationMode::Overlay {
        return;
    }
    let in_stack = stack.entry(root).is_some();
    let on_top = stack.top().is_some_and(|top| top.root == root);
    if in_stack && !on_top {
        return;
    }
    let named = hint
        .0
        .as_deref()
        .and_then(|id| node_by_id(id, root, &ids, &parents))
        .and_then(|node| first_focusable(node, &focusables, &children));
    if hint.0.is_some() && named.is_none() {
        tracing::warn!(
            id = hint.0.as_deref().unwrap_or_default(),
            "initial_focus names no focusable node; using the first focusable"
        );
    }
    let chosen = named.or_else(|| first_focusable(root, &focusables, &children));
    if screen.initial_focus != chosen {
        screen.initial_focus = chosen;
    }
    if let (Some(mut focus), Some(target)) = (focus, chosen) {
        focus.set(target, FocusCause::Navigated);
    }
}
