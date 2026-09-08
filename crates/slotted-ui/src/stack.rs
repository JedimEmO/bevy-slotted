//! The screen stack (menus contract 3): which screens are open, in what
//! order, and how each presents. [`spawn_screen`](crate::spawn_screen) and
//! [`close_screen`](crate::close_screen) remain the low-level path; a screen
//! spawned directly is not in the stack.

use std::sync::Arc;

use bevy::input_focus::{FocusCause, InputFocus};
use bevy::prelude::*;
use bevy::ui::ui_transform::{UiTransform, Val2};
use slotted_ecs::{CloseMenu, OpenMenu};
use slotted_theme::{Motion, MotionPreset, Themed, TweenTarget, roles};

use crate::actions::{UiAction, UiActionClaims, UiActionEvent};
use crate::def::{BackPolicy, Presentation, PresentationMode, ScreenDef, ScreenKind, Transition};
use crate::layers::zbands;
use crate::screen::{ScreenClosed, Screens, SpawnScreen};
use crate::semantic::ScreenRoot;

/// One open screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StackEntry {
    /// The screen root.
    pub root: Entity,
    /// Which screen.
    pub kind: ScreenKind,
    /// How it presents.
    pub presentation: Presentation,
    /// The `OpenMenu` it drives, closed with it on pop.
    pub menu: Option<Entity>,
    /// The last focused node inside it, restored when it regains the top.
    pub focus: Option<Entity>,
}

/// The open screens, bottom to top.
#[derive(Resource, Debug, Default, Clone, PartialEq, Eq)]
pub struct ScreenStack {
    entries: Vec<StackEntry>,
}

impl ScreenStack {
    /// Every entry, bottom to top.
    pub fn entries(&self) -> &[StackEntry] {
        &self.entries
    }

    /// Mutable access for the stack commands and the focus recorder.
    pub fn entries_mut(&mut self) -> &mut Vec<StackEntry> {
        &mut self.entries
    }

    /// The topmost entry that is not an overlay.
    pub fn top(&self) -> Option<&StackEntry> {
        self.entries
            .iter()
            .rev()
            .find(|e| e.presentation.mode != PresentationMode::Overlay)
    }

    /// The topmost entry of any kind.
    pub fn top_any(&self) -> Option<&StackEntry> {
        self.entries.last()
    }

    /// The topmost entry that takes focus (menus M3 contract 2.1): a page,
    /// a modal, or an overlay that said `focus: true`. What focus scoping,
    /// initial focus and the hint bar mean by "the top".
    pub fn focus_top(&self) -> Option<&StackEntry> {
        self.entries
            .iter()
            .rev()
            .find(|e| e.presentation.takes_focus())
    }

    /// Whether a screen of `kind` is open.
    pub fn is_open(&self, kind: &ScreenKind) -> bool {
        self.entries.iter().any(|e| &e.kind == kind)
    }

    /// The open kinds, bottom to top.
    pub fn kinds(&self) -> Vec<ScreenKind> {
        self.entries.iter().map(|e| e.kind.clone()).collect()
    }

    /// The entry owning `root`.
    pub fn entry(&self, root: Entity) -> Option<&StackEntry> {
        self.entries.iter().find(|e| e.root == root)
    }
}

/// The stack changed; `kinds` is the new order, bottom to top.
#[derive(Message, Debug, Clone, PartialEq, Eq)]
pub struct StackChanged {
    /// Bottom to top.
    pub kinds: Vec<ScreenKind>,
}

/// The scrim under a modal entry.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Scrim {
    /// The entry it belongs to.
    pub for_root: Entity,
}

/// Pushes `def` onto the stack and returns its root; the tree exists once
/// commands apply.
pub fn push_screen(commands: &mut Commands, def: Arc<ScreenDef>, menu: Option<Entity>) -> Entity {
    let root = commands.spawn_empty().id();
    commands.queue(PushScreen { root, def, menu });
    root
}

/// Pops the top non-overlay entry, then pushes `def`.
pub fn replace_screen(
    commands: &mut Commands,
    def: Arc<ScreenDef>,
    menu: Option<Entity>,
) -> Entity {
    commands.queue(PopScreen);
    push_screen(commands, def, menu)
}

/// Pops the top non-overlay entry, closing its screen and its menu.
pub fn pop_screen(commands: &mut Commands) {
    commands.queue(PopScreen);
}

/// Pops until a screen of `kind` is on top. No-op when none is open.
pub fn pop_to(commands: &mut Commands, kind: &ScreenKind) {
    commands.queue(PopTo(kind.clone()));
}

/// Pops everything.
pub fn clear_screens(commands: &mut Commands) {
    commands.queue(ClearScreens);
}

/// Closes one stack entry by its root, wherever it sits (menus M3 contract
/// 2.2): the way an overlay, which `Back` and [`pop_screen`] never reach,
/// leaves the stack. Entries above it stay where they are.
pub fn close_stacked(commands: &mut Commands, root: Entity) {
    // M3-IMPL: A
    let _ = (commands, root);
    todo!("close_stacked");
}

/// The command behind [`push_screen`].
pub struct PushScreen {
    /// Pre-reserved root.
    pub root: Entity,
    /// The screen.
    pub def: Arc<ScreenDef>,
    /// Its menu.
    pub menu: Option<Entity>,
}

impl Command for PushScreen {
    type Out = ();

    fn apply(self, world: &mut World) {
        // Resolved once, here, so the entry exists with the final kind and
        // presentation *before* `ScreenSpawned` fires: the nav observer that
        // picks the initial focus asks the stack whether this screen is on
        // top. `SpawnScreen` resolving the flattened result again is a clone.
        let resolved = world
            .get_resource::<Screens>()
            .map_or_else(|| (*self.def).clone(), |s| s.resolve(&self.def));
        let presentation = resolved.presentation;
        world
            .resource_mut::<ScreenStack>()
            .entries
            .push(StackEntry {
                root: self.root,
                kind: resolved.kind.clone(),
                presentation,
                menu: self.menu,
                focus: None,
            });
        SpawnScreen {
            root: self.root,
            def: Arc::new(resolved),
            menu: self.menu,
        }
        .apply(world);
        world
            .entity_mut(self.root)
            .insert(PushTransition(presentation.transition));
        apply_presentation(world);
        write_stack_changed(world);
    }
}

/// Spawns `def` as a stack entry at position `index` (clamped to the end)
/// rather than on top, keeping whatever is above it where it is. What a hot
/// reload wants: `respawn_screens` closes a stacked root and puts the new
/// tree back in the same place, so an edit to an open screen file does not
/// drop the screen out of the stack.
///
/// The entry's presentation is the resolved `def`'s, so an edit that changes
/// `presentation` takes effect like any other. No arrival transition: the
/// screen was already there. Writes `StackChanged`.
pub fn push_screen_at(
    world: &mut World,
    index: usize,
    def: Arc<ScreenDef>,
    menu: Option<Entity>,
) -> Entity {
    let root = world.spawn_empty().id();
    let resolved = world
        .get_resource::<Screens>()
        .map_or_else(|| (*def).clone(), |s| s.resolve(&def));
    let presentation = resolved.presentation;
    {
        let mut stack = world.resource_mut::<ScreenStack>();
        let index = index.min(stack.entries.len());
        stack.entries.insert(
            index,
            StackEntry {
                root,
                kind: resolved.kind.clone(),
                presentation,
                menu,
                focus: None,
            },
        );
    }
    SpawnScreen {
        root,
        def: Arc::new(resolved),
        menu,
    }
    .apply(world);
    finish_change(world);
    root
}

/// The command behind [`pop_screen`].
#[derive(Debug, Clone, Copy)]
pub struct PopScreen;

impl Command for PopScreen {
    type Out = ();

    fn apply(self, world: &mut World) {
        let top = world
            .resource::<ScreenStack>()
            .entries
            .iter()
            .rposition(|e| e.presentation.mode != PresentationMode::Overlay);
        let Some(index) = top else {
            return;
        };
        pop_entry(world, index);
        finish_change(world);
    }
}

/// The command behind [`pop_to`].
#[derive(Debug, Clone)]
pub struct PopTo(pub ScreenKind);

impl Command for PopTo {
    type Out = ();

    fn apply(self, world: &mut World) {
        if !world.resource::<ScreenStack>().is_open(&self.0) {
            return;
        }
        let mut popped = false;
        loop {
            let entries = &world.resource::<ScreenStack>().entries;
            match entries.last() {
                Some(top) if top.kind != self.0 => {
                    pop_entry(world, entries.len() - 1);
                    popped = true;
                }
                _ => break,
            }
        }
        if popped {
            finish_change(world);
        }
    }
}

/// The command behind [`clear_screens`].
#[derive(Debug, Clone, Copy)]
pub struct ClearScreens;

impl Command for ClearScreens {
    type Out = ();

    fn apply(self, world: &mut World) {
        let mut popped = false;
        while let Some(index) = world.resource::<ScreenStack>().entries.len().checked_sub(1) {
            pop_entry(world, index);
            popped = true;
        }
        if popped {
            finish_change(world);
        }
    }
}

/// A screen the stack just pushed, waiting for its arrival motion. Removed by
/// [`start_push_transitions`] on the first frame the theme has painted it.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct PushTransition(pub Transition);

/// Removes entry `index`, closes its screen and its menu, and drops its
/// scrim. Focus, z, visibility and `StackChanged` are [`finish_change`]'s.
fn pop_entry(world: &mut World, index: usize) {
    let entry = world.resource_mut::<ScreenStack>().entries.remove(index);
    despawn_scrims(world, entry.root);
    // The entry is already gone, so `on_screen_closed` finds nothing to do.
    if world.get_entity(entry.root).is_ok() {
        world.trigger(ScreenClosed { entity: entry.root });
        world.despawn(entry.root);
    }
    if let Some(menu) = entry.menu
        && let Some(id) = world.get::<OpenMenu>(menu).map(|open| open.id)
    {
        // The same close the game did by hand before the stack existed:
        // the carried stack lands in `Dropped`, so nothing is lost.
        CloseMenu { menu, id }.apply(world);
    }
}

/// After one or more pops: z, visibility, focus and the message.
fn finish_change(world: &mut World) {
    apply_presentation(world);
    restore_focus(world);
    write_stack_changed(world);
}

fn despawn_scrims(world: &mut World, root: Entity) {
    let scrims: Vec<Entity> = world
        .query::<(Entity, &Scrim)>()
        .iter(world)
        .filter(|(_, scrim)| scrim.for_root == root)
        .map(|(entity, _)| entity)
        .collect();
    for scrim in scrims {
        world.despawn(scrim);
    }
}

fn write_stack_changed(world: &mut World) {
    let kinds = world.resource::<ScreenStack>().kinds();
    if let Some(mut messages) = world.get_resource_mut::<Messages<StackChanged>>() {
        messages.write(StackChanged { kinds });
    }
}

/// Menus contract 3.2: entry `i` at `zbands::SCREEN + 2 * i`, its scrim one
/// below, everything under the topmost `page` hidden and the rest shown.
/// Idempotent, so it runs after every change and after a direct
/// `close_screen` on a stacked root.
pub fn apply_presentation(world: &mut World) {
    let entries = world.resource::<ScreenStack>().entries.clone();
    let top_page = entries
        .iter()
        .rposition(|e| e.presentation.mode == PresentationMode::Page);
    let scrims: Vec<(Entity, Entity)> = world
        .query::<(Entity, &Scrim)>()
        .iter(world)
        .map(|(entity, scrim)| (entity, scrim.for_root))
        .collect();
    for (i, entry) in entries.iter().enumerate() {
        let Ok(mut root) = world.get_entity_mut(entry.root) else {
            continue;
        };
        #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
        let z = zbands::SCREEN + 2 * i as i32;
        let visibility = if top_page.is_none_or(|p| i >= p) {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        set_if_changed(&mut root, GlobalZIndex(z));
        set_if_changed(&mut root, visibility);
        let existing = scrims
            .iter()
            .find(|(_, for_root)| *for_root == entry.root)
            .map(|(scrim, _)| *scrim);
        match (entry.presentation.scrim(), existing) {
            (true, Some(scrim)) => {
                let mut scrim = world.entity_mut(scrim);
                set_if_changed(&mut scrim, GlobalZIndex(z - 1));
                set_if_changed(&mut scrim, visibility);
            }
            (true, None) => {
                world.spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        width: percent(100),
                        height: percent(100),
                        ..default()
                    },
                    GlobalZIndex(z - 1),
                    visibility,
                    Pickable::default(),
                    Themed(roles::SCRIM),
                    Scrim {
                        for_root: entry.root,
                    },
                ));
            }
            (false, Some(scrim)) => {
                world.despawn(scrim);
            }
            (false, None) => {}
        }
    }
}

fn set_if_changed<C: Component + PartialEq>(entity: &mut EntityWorldMut<'_>, value: C) {
    if entity.get::<C>() != Some(&value) {
        entity.insert(value);
    }
}

/// Puts `InputFocus` on the top entry's recorded focus, else its root's
/// initial focus. Clears it when the focused node died with a popped
/// screen and nothing is left to take over.
fn restore_focus(world: &mut World) {
    let top = world.resource::<ScreenStack>().top().cloned();
    let alive = |world: &World, e: Option<Entity>| e.filter(|e| world.get_entity(*e).is_ok());
    let target = top.and_then(|top| {
        alive(world, top.focus).or_else(|| {
            alive(
                world,
                world
                    .get::<ScreenRoot>(top.root)
                    .and_then(|root| root.initial_focus),
            )
        })
    });
    let current = world.get_resource::<InputFocus>().and_then(InputFocus::get);
    let current_alive = alive(world, current);
    let Some(mut focus) = world.get_resource_mut::<InputFocus>() else {
        return;
    };
    match target {
        Some(target) if current != Some(target) => focus.set(target, FocusCause::Navigated),
        None if current.is_some() && current_alive.is_none() => focus.clear(),
        _ => {}
    }
}

/// Observer on `ScreenClosed`: drops the entry for a root closed through
/// [`close_screen`](crate::close_screen) directly.
pub fn on_screen_closed(
    closed: On<ScreenClosed>,
    mut stack: ResMut<ScreenStack>,
    mut changed: MessageWriter<StackChanged>,
    scrims: Query<(Entity, &Scrim)>,
    mut commands: Commands,
) {
    let root = closed.entity;
    let Some(index) = stack.entries.iter().position(|e| e.root == root) else {
        return;
    };
    stack.entries.remove(index);
    for (scrim, _) in scrims.iter().filter(|(_, s)| s.for_root == root) {
        commands.entity(scrim).despawn();
    }
    changed.write(StackChanged {
        kinds: stack.kinds(),
    });
    commands.queue(|world: &mut World| {
        apply_presentation(world);
        restore_focus(world);
    });
}

/// `SlottedUiSet::Navigate`: pops the top entry on an unclaimed `Back`
/// (menus contract 3.3).
pub fn pop_on_back(
    mut events: MessageReader<UiActionEvent>,
    claims: Res<UiActionClaims>,
    stack: Res<ScreenStack>,
    mut commands: Commands,
) {
    let back = events.read().any(|e| e.action == UiAction::Back);
    if !back || claims.is_claimed(UiAction::Back) {
        return;
    }
    let Some(top) = stack.top() else {
        return;
    };
    if top.presentation.back == BackPolicy::Pop {
        pop_screen(&mut commands);
    }
}

/// The screen root above `entity`, or `entity` itself when it is one.
fn screen_root_of(
    entity: Entity,
    parents: &Query<&ChildOf>,
    roots: &Query<&ScreenRoot>,
) -> Option<Entity> {
    let mut current = entity;
    loop {
        if roots.contains(current) {
            return Some(current);
        }
        current = parents.get(current).ok()?.parent();
    }
}

/// `SlottedUiSet::Navigate`: keeps `InputFocus` inside the top entry
/// (menus contract 3.2).
pub fn enforce_focus_scope(
    focus: Option<ResMut<InputFocus>>,
    stack: Res<ScreenStack>,
    parents: Query<&ChildOf>,
    roots: Query<&ScreenRoot>,
) {
    let Some(mut focus) = focus else {
        return;
    };
    let Some(focused) = focus.get() else {
        return;
    };
    let Some(root) = screen_root_of(focused, &parents, &roots) else {
        return;
    };
    let Some(top) = stack.top() else {
        return;
    };
    if root == top.root || stack.entry(root).is_none() {
        return;
    }
    let target = top
        .focus
        .filter(|e| parents.contains(*e))
        .or_else(|| roots.get(top.root).ok().and_then(|r| r.initial_focus));
    match target {
        Some(target) => focus.set(target, FocusCause::Navigated),
        None => focus.clear(),
    }
}

/// `SlottedUiSet::Navigate`, after [`enforce_focus_scope`]: records the
/// focused node into its entry.
pub fn record_stack_focus(
    focus: Option<Res<InputFocus>>,
    mut stack: ResMut<ScreenStack>,
    parents: Query<&ChildOf>,
    roots: Query<&ScreenRoot>,
) {
    let Some(focused) = focus.as_deref().and_then(InputFocus::get) else {
        return;
    };
    let Some(root) = screen_root_of(focused, &parents, &roots) else {
        return;
    };
    let Some(entry) = stack.entries.iter().position(|e| e.root == root) else {
        return;
    };
    if stack.entries[entry].focus != Some(focused) {
        stack.entries_mut()[entry].focus = Some(focused);
    }
}

/// After `SlottedThemeSet::Apply`: starts the arrival motion of every screen
/// pushed this frame (menus contract 3.2).
///
/// Bevy UI has no opacity group, so the fade is the root panel's own
/// `BackgroundColor` alpha, from zero back to what the theme painted; the
/// slide is a `Translate` on the screen root, so the whole tree moves. Both
/// start values are written here as well, so the first drawn frame is
/// already at the start of the motion. Under reduced motion only the fade
/// runs, and it completes on its first tick.
pub fn start_push_transitions(
    motion: Res<Motion>,
    tokens: crate::tooltip::ThemeTokens,
    pending: Query<(Entity, &PushTransition, Option<&Children>)>,
    mut backgrounds: Query<&mut BackgroundColor>,
    mut commands: Commands,
) {
    if pending.is_empty() {
        return;
    }
    let tokens = tokens.get();
    for (root, transition, children) in &pending {
        commands.entity(root).remove::<PushTransition>();
        let transition = if motion.reduced {
            Transition::Fade
        } else {
            transition.0
        };
        if transition == Transition::None {
            continue;
        }
        let panel = children.and_then(|c| c.iter().find(|c| backgrounds.contains(*c)));
        if let Some(panel) = panel
            && let Ok(mut background) = backgrounds.get_mut(panel)
        {
            let to = background.0.alpha();
            background.0.set_alpha(0.0);
            commands.entity(panel).insert(motion.preset_tween(
                MotionPreset::Fade,
                TweenTarget::Alpha { from: 0.0, to },
                &tokens,
            ));
        }
        let from = match transition {
            Transition::SlideUp => Vec2::new(0.0, tokens.spacing.xl),
            Transition::SlideLeft => Vec2::new(tokens.spacing.xl, 0.0),
            Transition::Fade | Transition::None => continue,
        };
        commands.entity(root).insert((
            UiTransform {
                translation: Val2::px(from.x, from.y),
                ..UiTransform::IDENTITY
            },
            motion.preset_tween(
                MotionPreset::Slide,
                TweenTarget::Translate {
                    from,
                    to: Vec2::ZERO,
                },
                &tokens,
            ),
        ));
    }
}

/// Registers the stack's resources, messages and systems.
pub fn build(app: &mut App) {
    app.init_resource::<ScreenStack>()
        .add_message::<StackChanged>()
        .add_observer(on_screen_closed)
        .add_systems(
            Update,
            (
                (pop_on_back, enforce_focus_scope, record_stack_focus)
                    .chain()
                    .in_set(crate::plugin::SlottedUiSet::Navigate),
                start_push_transitions.after(slotted_theme::SlottedThemeSet::Apply),
            ),
        );
}
