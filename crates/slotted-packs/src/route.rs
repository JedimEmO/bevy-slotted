//! UI events to scripts and script commands back. Contract section 2.4.

use std::collections::{HashMap, VecDeque};

use bevy::prelude::*;
use slotted_ecs::{MenuAction, SlotClicked};
use slotted_model::MenuId;
use slotted_script::{ModId, ScriptCommand, ScriptEvent};
use slotted_ui::def::ScreenKind;
use slotted_ui::{ScreenClosed, ScreenSpawned};

use crate::ModError;

/// Events collected this frame, delivered in `SlottedPacksSet::Dispatch`.
#[derive(Resource, Debug, Default)]
pub struct PendingScriptEvents(pub VecDeque<ScriptEvent>);

/// Which screen kind each open menu shows, so a `SlotClicked` can name its
/// screen without walking the hierarchy.
#[derive(Resource, Debug, Default)]
pub struct OpenScreens {
    /// Menu entity to its id and kind.
    pub by_menu: HashMap<Entity, (MenuId, ScreenKind)>,
    /// Screen root to menu entity.
    pub by_root: HashMap<Entity, Option<Entity>>,
}

/// `SlotClicked` -> `ScriptEvent::SlotClick`. Only enqueues.
pub fn on_slot_clicked(click: On<SlotClicked>, mut pending: ResMut<PendingScriptEvents>) {
    // PHASE4-IMPL: B -- resolve SlotRef -> OpenMenu.id, kind via OpenScreens,
    // stack via Inventory + registries name_of.
    let _ = (&click, &mut pending);
}

/// `Activate` on a `WidgetNode` inside a screen -> `WidgetActivate`.
pub fn on_widget_activate(
    activate: On<bevy::ui_widgets::Activate>,
    mut pending: ResMut<PendingScriptEvents>,
) {
    // PHASE4-IMPL: B
    let _ = (&activate, &mut pending);
}

/// `ScreenSpawned` -> `ScreenOpened`, and records the screen.
pub fn on_screen_spawned(
    spawned: On<ScreenSpawned>,
    mut open: ResMut<OpenScreens>,
    mut pending: ResMut<PendingScriptEvents>,
) {
    // PHASE4-IMPL: B
    let _ = (&spawned, &mut open, &mut pending);
}

/// `ScreenClosed` -> `ScreenClosed`, and forgets the screen.
pub fn on_screen_closed(
    closed: On<ScreenClosed>,
    mut open: ResMut<OpenScreens>,
    mut pending: ResMut<PendingScriptEvents>,
) {
    // PHASE4-IMPL: B
    let _ = (&closed, &mut open, &mut pending);
}

/// Browser `OpenRecipes`, `OpenUses`, `SearchChanged` -> events.
pub fn collect_browser_events(
    mut recipes: MessageReader<slotted_browser::OpenRecipes>,
    mut uses: MessageReader<slotted_browser::OpenUses>,
    mut search: MessageReader<slotted_browser::SearchChanged>,
    mut pending: ResMut<PendingScriptEvents>,
) {
    // PHASE4-IMPL: B
    for _ in recipes.read() {}
    for _ in uses.read() {}
    for _ in search.read() {}
    let _ = &mut pending;
}

/// Drains [`PendingScriptEvents`], calls every subscribed control script in
/// load order, and applies the commands. Exclusive so validation can see
/// `OpenMenu`s and trigger `MenuAction`s in one place.
pub fn dispatch_script_events(world: &mut World) {
    // PHASE4-IMPL: B
    let _ = world;
}

/// Validates and applies one control-stage command.
///
/// # Errors
///
/// [`ModError::WrongStage`], [`ModError::UnknownMenu`], [`ModError::BadCommand`].
pub fn apply_control_command(
    world: &mut World,
    mod_id: &ModId,
    command: &ScriptCommand,
) -> Result<Vec<MenuAction>, ModError> {
    // PHASE4-IMPL: B -- Sort/QuickStack/Move/ToggleFavorite/Click -> MenuAction
    // on the entity whose OpenMenu.id matches; Log -> ScriptLogs + ScriptLog;
    // SetHud -> debug log; Deprecated -> one warning per (mod, call).
    let _ = world;
    Err(ModError::WrongStage {
        mod_id: mod_id.clone(),
        command: command.name().to_owned(),
    })
}
