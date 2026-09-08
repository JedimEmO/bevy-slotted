//! Directional focus navigation over `UiActionEvent`s. Bevy ships
//! `AutoDirectionalNavigator` as a `SystemParam` and wires no input to it
//! (ADR 0002); this is what drives it, plus the explicit nav graph.

use std::collections::BTreeSet;

use bevy::input_focus::directional_navigation::FocusableArea;
use bevy::input_focus::navigator::find_best_candidate;
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
/// it uses this crate's vocabulary or Bevy's widget. The one exception is
/// the M1 `text_field` row (menus M1 contract 4.1): it carries the role but
/// owns the keyboard only while its [`TextFieldState::editing`] is set, so a
/// merely focused row still takes `Accept` to start editing.
///
/// [`TextFieldState::editing`]: crate::widgets::text_field::TextFieldState::editing
pub fn track_text_entry_focus(
    focus: Option<Res<InputFocus>>,
    fields: Query<(
        Option<&SemanticRole>,
        Option<&EditableText>,
        Option<&crate::widgets::text_field::TextFieldState>,
    )>,
    mut focused: ResMut<TextEntryFocused>,
) {
    let active = focus
        .and_then(|f| f.get())
        .and_then(|e| fields.get(e).ok())
        .is_some_and(|(role, editable, field)| {
            editable.is_some()
                || (role == Some(&SemanticRole::TextField)
                    && field.is_none_or(|state| state.editing))
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
    screen_root_where(entity, parents, |e| roots.contains(e))
}

/// [`screen_root_of`] with any root test, for a caller whose root query
/// carries data (`Query<&ScreenRoot>`) rather than a filter.
pub fn screen_root_where(
    entity: Entity,
    parents: &Query<&ChildOf>,
    is_root: impl Fn(Entity) -> bool,
) -> Option<Entity> {
    let mut current = entity;
    loop {
        if is_root(current) {
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
/// wins; then a manual edge in the `DirectionalNavigationMap`; otherwise the
/// best candidate by Bevy's scoring among the `AutoDirectionalNavigation`
/// nodes *under the focused node's screen root*. Bevy's own
/// `AutoDirectionalNavigator` is z-agnostic and would happily pick a button
/// of the screen under a modal (a pause button below a settings row), which
/// `enforce_focus_scope` then bounces back, so focus never moved (menus M2,
/// package D). A link whose id resolves to nothing under the same screen
/// root is logged once per screen spawn and falls through. Keyboard-sourced
/// actions are already suppressed while a text field has the keyboard
/// (`emit_ui_actions`), so this needs no guard of its own.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn directional_nav_actions(
    mut events: MessageReader<crate::actions::UiActionEvent>,
    claims: Res<crate::actions::UiActionClaims>,
    links: Query<&crate::def::NavLinks>,
    parents: Query<&ChildOf>,
    ids: Query<(Entity, &crate::semantic::TestId)>,
    focusables: Query<(), With<crate::focus_ring::Focusable>>,
    children: Query<&Children>,
    roots: Query<(), With<crate::semantic::ScreenRoot>>,
    mut warned: Query<&mut NavLinkWarned>,
    mut nav: AutoDirectionalNavigator,
    navigable: Query<
        (
            Entity,
            &bevy::ui::ComputedUiTargetCamera,
            &ComputedNode,
            &bevy::ui::ui_transform::UiGlobalTransform,
            &InheritedVisibility,
        ),
        With<bevy::ui::auto_directional_navigation::AutoDirectionalNavigation>,
    >,
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
        // A control that consumed the direction (a slider's `Left`) claimed
        // it from its `FocusedAction` observer, which ran before this.
        if claims.is_claimed(event.action) {
            continue;
        }
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
        // A manual edge first, as Bevy's navigator does.
        if let Ok(target) = nav.manual_directional_navigation.navigate(dir) {
            nav.manual_directional_navigation
                .focus
                .set(target, FocusCause::Navigated);
            continue;
        }
        let Some(focused) = focused else { continue };
        let Some((camera, origin)) = focusable_area(focused, &navigable) else {
            continue;
        };
        let nodes: Vec<FocusableArea> = navigable
            .iter()
            .filter(|(entity, target, computed, _, visible)| {
                *entity != focused
                    && !computed.is_empty()
                    && visible.get()
                    && target.get() == Some(camera)
                    && screen_root_of(*entity, &parents, &roots) == root
            })
            .filter_map(|(entity, _, _, _, _)| focusable_area(entity, &navigable).map(|(_, a)| a))
            .collect();
        if let Some(target) = find_best_candidate(&origin, dir, &nodes, &nav.config) {
            nav.manual_directional_navigation
                .focus
                .set(target, FocusCause::Navigated);
        } else {
            tracing::trace!(?dir, "directional navigation found no target");
        }
    }
}

/// The target camera and the navigator's bounds of `entity`, as Bevy's
/// `AutoDirectionalNavigator` computes them.
#[allow(clippy::type_complexity)]
fn focusable_area(
    entity: Entity,
    navigable: &Query<
        (
            Entity,
            &bevy::ui::ComputedUiTargetCamera,
            &ComputedNode,
            &bevy::ui::ui_transform::UiGlobalTransform,
            &InheritedVisibility,
        ),
        With<bevy::ui::auto_directional_navigation::AutoDirectionalNavigation>,
    >,
) -> Option<(Entity, FocusableArea)> {
    let (entity, target, computed, transform, _) = navigable.get(entity).ok()?;
    let camera = target.get()?;
    let (scale, rotation, translation) = transform.to_scale_angle_translation();
    let size = computed.size() * computed.inverse_scale_factor() * scale;
    // Bevy's `get_rotated_bounds`, which is private: the axis-aligned box
    // of the rotated rectangle.
    let (sin, cos) = rotation.sin_cos();
    let rotated = Vec2::new(
        (size.x * cos).abs() + (size.y * sin).abs(),
        (size.x * sin).abs() + (size.y * cos).abs(),
    );
    Some((
        camera,
        FocusableArea {
            entity,
            position: translation * computed.inverse_scale_factor(),
            size: rotated,
        },
    ))
}

/// One action delivered to the focused node (menus M1 contract 1.2). Controls
/// observe it, act, and claim what they consumed.
#[derive(EntityEvent, Debug, Clone, Copy, PartialEq, Eq)]
pub struct FocusedAction {
    /// The focused entity.
    pub entity: Entity,
    /// The action.
    pub action: crate::actions::UiAction,
    /// Who pressed it.
    pub device: crate::actions::InputDevice,
    /// A held-key repeat rather than a fresh press.
    pub repeat: bool,
}

/// On a hidden tab page (menus M1 contract 4.4): its focusable descendants
/// take no focus and get no [`FocusedAction`].
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FocusMask;

/// Whether `entity` or one of its ancestors carries a [`FocusMask`].
pub fn is_masked(
    entity: Entity,
    masked: &Query<(), With<FocusMask>>,
    parents: &Query<&ChildOf>,
) -> bool {
    let mut current = entity;
    loop {
        if masked.contains(current) {
            return true;
        }
        match parents.get(current) {
            Ok(child_of) => current = child_of.parent(),
            Err(_) => return false,
        }
    }
}

/// `SlottedUiSet::Input`, after `UiActionEmit` and before
/// [`directional_nav_actions`]: triggers one [`FocusedAction`] on the
/// `InputFocus` entity per `UiActionEvent` this frame, when that entity is
/// `Focusable`, not `InteractionDisabled` and not under a `FocusMask`.
///
/// An action something already claimed this frame (a key capture that
/// swallowed the press) is not delivered: it was consumed.
pub fn dispatch_focused_actions(
    mut events: MessageReader<crate::actions::UiActionEvent>,
    focus: Option<Res<InputFocus>>,
    claims: Res<crate::actions::UiActionClaims>,
    focusables: Query<Has<bevy::ui::InteractionDisabled>, With<crate::focus_ring::Focusable>>,
    masked: Query<(), With<FocusMask>>,
    parents: Query<&ChildOf>,
    mut commands: Commands,
) {
    let Some(focused) = focus.as_deref().and_then(InputFocus::get) else {
        events.clear();
        return;
    };
    let Ok(disabled) = focusables.get(focused) else {
        events.clear();
        return;
    };
    if disabled || is_masked(focused, &masked, &parents) {
        events.clear();
        return;
    }
    for event in events.read() {
        if claims.is_claimed(event.action) {
            continue;
        }
        commands.trigger(FocusedAction {
            entity: focused,
            action: event.action,
            device: event.device,
            repeat: event.repeat,
        });
    }
}

/// Observer: a fresh `Accept` on a focused slot is a left click
/// ([`slotted_ecs::SlotClicked`] with the modifiers held), from any device,
/// and is claimed. This is the one place a keyboard or a pad picks up and
/// places (menus M1 contract 1.2).
pub fn on_slot_accept(
    action: On<FocusedAction>,
    slots: Query<(), With<slotted_ecs::SlotRef>>,
    keys: Res<ButtonInput<KeyCode>>,
    mut claims: ResMut<crate::actions::UiActionClaims>,
    mut commands: Commands,
) {
    if action.action != crate::actions::UiAction::Accept || action.repeat {
        return;
    }
    let focused = action.entity;
    if !slots.contains(focused) {
        return;
    }
    claims.claim(crate::actions::UiAction::Accept);
    commands.trigger(slotted_ecs::SlotClicked {
        entity: focused,
        button: slotted_model::Button::Left,
        modifiers: crate::widgets::modifiers_from(&keys),
    });
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
