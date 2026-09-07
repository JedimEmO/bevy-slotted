//! The screen stack (menus contract 3): which screens are open, in what
//! order, and how each presents. [`spawn_screen`](crate::spawn_screen) and
//! [`close_screen`](crate::close_screen) remain the low-level path; a screen
//! spawned directly is not in the stack.

use std::sync::Arc;

use bevy::prelude::*;

use crate::def::{Presentation, ScreenDef, ScreenKind};

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
            .find(|e| e.presentation.mode != crate::def::PresentationMode::Overlay)
    }

    /// The topmost entry of any kind.
    pub fn top_any(&self) -> Option<&StackEntry> {
        self.entries.last()
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

    fn apply(self, _world: &mut World) {
        // M0-IMPL: B
    }
}

/// The command behind [`pop_screen`].
#[derive(Debug, Clone, Copy)]
pub struct PopScreen;

impl Command for PopScreen {
    type Out = ();

    fn apply(self, _world: &mut World) {
        // M0-IMPL: B
    }
}

/// The command behind [`pop_to`].
#[derive(Debug, Clone)]
pub struct PopTo(pub ScreenKind);

impl Command for PopTo {
    type Out = ();

    fn apply(self, _world: &mut World) {
        // M0-IMPL: B
    }
}

/// The command behind [`clear_screens`].
#[derive(Debug, Clone, Copy)]
pub struct ClearScreens;

impl Command for ClearScreens {
    type Out = ();

    fn apply(self, _world: &mut World) {
        // M0-IMPL: B
    }
}

/// Observer on `ScreenClosed`: drops the entry for a root closed through
/// [`close_screen`](crate::close_screen) directly.
pub fn on_screen_closed(
    _closed: On<crate::screen::ScreenClosed>,
    _stack: ResMut<ScreenStack>,
    _changed: MessageWriter<StackChanged>,
    _commands: Commands,
) {
    // M0-IMPL: B
}

/// `SlottedUiSet::Navigate`: pops the top entry on an unclaimed `Back`
/// (menus contract 3.3).
pub fn pop_on_back(
    _events: MessageReader<crate::actions::UiActionEvent>,
    _claims: Res<crate::actions::UiActionClaims>,
    _stack: Res<ScreenStack>,
    _commands: Commands,
) {
    // M0-IMPL: B
}

/// `SlottedUiSet::Navigate`: keeps `InputFocus` inside the top entry
/// (menus contract 3.2).
pub fn enforce_focus_scope(
    _focus: Option<ResMut<bevy::input_focus::InputFocus>>,
    _stack: Res<ScreenStack>,
    _parents: Query<&ChildOf>,
    _roots: Query<&crate::semantic::ScreenRoot>,
) {
    // M0-IMPL: B
}

/// `SlottedUiSet::Navigate`, after [`enforce_focus_scope`]: records the
/// focused node into its entry.
pub fn record_stack_focus(
    _focus: Option<Res<bevy::input_focus::InputFocus>>,
    _stack: ResMut<ScreenStack>,
    _parents: Query<&ChildOf>,
) {
    // M0-IMPL: B
}

/// Registers the stack's resources, messages and systems.
pub fn build(app: &mut App) {
    app.init_resource::<ScreenStack>()
        .add_message::<StackChanged>()
        .add_observer(on_screen_closed)
        .add_systems(
            Update,
            (pop_on_back, enforce_focus_scope, record_stack_focus)
                .chain()
                .in_set(crate::plugin::SlottedUiSet::Navigate),
        );
}
