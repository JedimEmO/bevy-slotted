//! Readers and drivers for `slotted-menu` (menus M2 contract 5): toasts, the
//! hint bar's entries, the confirm dialog's buttons and the `MenuChoice`s
//! the templates wrote; and the dialogue runner (menus M3 contract 5): the
//! running node, the visible text, the options, the history and the three
//! dialogue messages. Behind the `menu` feature.

use bevy::prelude::*;
use slotted_menu::confirm::PendingConfirm;
use slotted_menu::dialogue_screen::option_test_id;
use slotted_menu::hint_bar::HintEntries;
use slotted_menu::toast::{Toast, ToastColumn};
use slotted_menu::{
    ActiveDialogue, DialogueChoice, DialogueEnded, DialogueId, DialogueNodeEntered, DialogueOption,
    HintEntry, MenuChoice, NodeId,
};
use slotted_ui::{ScreenStack, UiAction};

use crate::harness::UiHarness;
use crate::locator::by;

/// Every `MenuChoice` since [`UiHarness::menu_choices`] last drained it.
#[derive(Resource, Debug, Default, Clone)]
pub struct MenuChoiceLog(pub Vec<MenuChoice>);

fn record_menu_choices(mut log: ResMut<MenuChoiceLog>, mut choices: MessageReader<MenuChoice>) {
    log.0.extend(choices.read().cloned());
}

/// Added by the builder: keeps the choices until a test reads them. The
/// message is registered here as well, so an app without `MenuPlugin` still
/// builds; `add_message` is idempotent.
#[derive(Debug, Default, Clone, Copy)]
pub struct MenuChoiceRecorder;

impl Plugin for MenuChoiceRecorder {
    fn build(&self, app: &mut App) {
        app.add_message::<MenuChoice>()
            .init_resource::<MenuChoiceLog>()
            .add_systems(Last, record_menu_choices);
    }
}

/// One of the runner's three messages, in the order they were written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DialogueEvent {
    /// `DialogueNodeEntered`.
    Entered(DialogueNodeEntered),
    /// `DialogueChoice`.
    Chosen(DialogueChoice),
    /// `DialogueEnded`.
    Ended(DialogueEnded),
}

/// Every dialogue message since [`UiHarness::dialogue_events`] last drained
/// it.
#[derive(Resource, Debug, Default, Clone)]
pub struct DialogueEventLog(pub Vec<DialogueEvent>);

fn record_dialogue_events(
    mut log: ResMut<DialogueEventLog>,
    mut entered: MessageReader<DialogueNodeEntered>,
    mut chosen: MessageReader<DialogueChoice>,
    mut ended: MessageReader<DialogueEnded>,
) {
    // Three readers cannot interleave by write time, so within one frame the
    // order is fixed: a choice comes before the node it led to, and a node
    // before the end it reached, which is the order one transition writes
    // them in. (A `Replaced` end lands after the new dialogue's first node
    // when both happen in the same frame.)
    log.0
        .extend(chosen.read().cloned().map(DialogueEvent::Chosen));
    log.0
        .extend(entered.read().cloned().map(DialogueEvent::Entered));
    log.0
        .extend(ended.read().cloned().map(DialogueEvent::Ended));
}

/// Added by the builder beside [`MenuChoiceRecorder`]: keeps the dialogue
/// messages until a test reads them. The messages are registered here as
/// well, so an app without `MenuPlugin` still builds.
#[derive(Debug, Default, Clone, Copy)]
pub struct DialogueRecorder;

impl Plugin for DialogueRecorder {
    fn build(&self, app: &mut App) {
        app.add_message::<DialogueNodeEntered>()
            .add_message::<DialogueChoice>()
            .add_message::<DialogueEnded>()
            .init_resource::<DialogueEventLog>()
            .add_systems(Last, record_dialogue_events);
    }
}

impl UiHarness {
    /// The toasts on screen, oldest first (the order they sit in the
    /// column, bottom to top). Empty when none is up.
    pub fn toasts(&mut self) -> Vec<Entity> {
        let world = self.app.world_mut();
        let mut columns = world.query_filtered::<&Children, With<ToastColumn>>();
        let Some(children) = columns.iter(world).next() else {
            return Vec::new();
        };
        let children: Vec<Entity> = children.iter().collect();
        children
            .into_iter()
            .filter(|e| world.get::<Toast>(*e).is_some())
            .collect()
    }

    /// The entries a hint bar shows. Panics when `entity` is not a
    /// `slotted:hint_bar` root.
    pub fn hint_entries(&self, entity: Entity) -> Vec<HintEntry> {
        self.world()
            .get::<HintEntries>(entity)
            .unwrap_or_else(|| panic!("{entity} is not a hint bar"))
            .0
            .clone()
    }

    /// Activates the `accept` button of the confirm dialog on top of the
    /// stack and runs a frame. Panics when the top is not a confirm dialog.
    pub fn confirm_accept(&mut self) {
        self.press_confirm_button("accept");
    }

    /// Activates the `cancel` button of the confirm dialog on top of the
    /// stack and runs a frame. Panics when the top is not a confirm dialog.
    pub fn confirm_cancel(&mut self) {
        self.press_confirm_button("cancel");
    }

    fn press_confirm_button(&mut self, id: &str) {
        let root = self
            .world()
            .get_resource::<ScreenStack>()
            .and_then(|s| s.top().map(|e| e.root))
            .unwrap_or_else(|| panic!("no screen is open, so nothing to {id}"));
        let pending = self
            .world()
            .get::<PendingConfirm>(root)
            .unwrap_or_else(|| panic!("the top screen is not a confirm dialog waiting for {id}"))
            .0
            .clone();
        let button = self
            .try_find(&by::test_id(id).within(root))
            .unwrap_or_else(|| panic!("the confirm dialog {pending:?} has no {id} button"));
        self.activate(button);
        self.step(1);
    }

    /// Every `MenuChoice` written since the last call (or since the app
    /// started), in order.
    pub fn menu_choices(&mut self) -> Vec<MenuChoice> {
        std::mem::take(&mut self.world_mut().get_resource_or_init::<MenuChoiceLog>().0)
    }

    // ---- dialogue (menus M3 contract 5) -------------------------------------

    /// The running dialogue and the node it is on, `None` when none runs.
    pub fn dialogue(&self) -> Option<(DialogueId, NodeId)> {
        self.world()
            .get_resource::<ActiveDialogue>()
            .map(|a| (a.dialogue.id.clone(), a.node.clone()))
    }

    fn active_dialogue(&self) -> &ActiveDialogue {
        self.world()
            .get_resource::<ActiveDialogue>()
            .expect("no dialogue is running")
    }

    /// Whether the current line is fully shown. On a choice node always
    /// true. Panics when no dialogue runs.
    pub fn dialogue_revealed(&self) -> bool {
        self.active_dialogue().revealed
    }

    /// The text the `text` node shows right now: the spans painted with any
    /// alpha under it, in order. The unrevealed rest of a typing line is a
    /// transparent span and is left out, so this grows with the typewriter
    /// and is the whole line once [`Self::dialogue_revealed`]. Empty when
    /// the template has no `text` node. Panics when no dialogue runs.
    pub fn dialogue_text(&self) -> String {
        fn walk(world: &World, entity: Entity, out: &mut String) {
            if let (Some(span), Some(color)) = (
                world.get::<TextSpan>(entity),
                world.get::<TextColor>(entity),
            ) && color.0.alpha() > 0.0
            {
                out.push_str(&span.0);
            }
            if let Some(children) = world.get::<Children>(entity) {
                for child in children.iter() {
                    walk(world, child, out);
                }
            }
        }
        let root = self.active_dialogue().root;
        let Some(text) = self.try_find(&by::test_id("text").within(root)) else {
            return String::new();
        };
        let mut out = String::new();
        walk(self.world(), text, &mut out);
        out
    }

    /// The option buttons of the current choice, in column order: each
    /// option's id and whether it is enabled. Empty on a line, and on a
    /// choice whose buttons have not appeared yet (`choice_delay`). Panics
    /// when no dialogue runs.
    pub fn dialogue_options(&self) -> Vec<(String, bool)> {
        let root = self.active_dialogue().root;
        let Some(column) = self.try_find(&by::test_id("choices").within(root)) else {
            return Vec::new();
        };
        let world = self.world();
        world
            .get::<Children>(column)
            .map(|children| {
                children
                    .iter()
                    .filter_map(|e| {
                        let option = world.get::<DialogueOption>(e)?;
                        let disabled = world.get::<slotted_ui::ButtonState>(e)?.disabled;
                        Some((option.id.clone(), !disabled))
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Presses `Accept` and runs a frame: a typing line shows whole, a
    /// revealed line goes to its `next`. On a choice `Accept` belongs to the
    /// focused option button.
    pub fn dialogue_advance(&mut self) {
        self.action(UiAction::Accept);
        self.step(1);
    }

    /// Activates the option button with test id `option.<id>` and runs a
    /// frame. Panics when no dialogue runs or the current choice has no such
    /// button (a disabled one is activated and refused by the runner with a
    /// warning, like a click on it).
    pub fn dialogue_choose(&mut self, id: &str) {
        let root = self.active_dialogue().root;
        let button = self
            .try_find(&by::test_id(&option_test_id(id)).within(root))
            .unwrap_or_else(|| {
                panic!(
                    "the dialogue has no option `{id}` on screen (the buttons show after `choice_delay`)"
                )
            });
        self.activate(button);
        self.step(1);
    }

    /// The transcript so far: every line said, as (speaker, text), oldest
    /// first; a narration line has no speaker. Panics when no dialogue runs.
    pub fn dialogue_history(&self) -> Vec<(Option<String>, String)> {
        self.active_dialogue()
            .history
            .iter()
            .map(|line| (line.speaker.clone(), line.text.clone()))
            .collect()
    }

    /// Every dialogue message written since the last call (or since the app
    /// started), in order.
    pub fn dialogue_events(&mut self) -> Vec<DialogueEvent> {
        std::mem::take(
            &mut self
                .world_mut()
                .get_resource_or_init::<DialogueEventLog>()
                .0,
        )
    }

    /// Starts the registered dialogue `id` and runs a frame, so the screen
    /// is up and the first node presented. Warns and does nothing when the
    /// id is not registered, like `start_dialogue`.
    pub fn start_dialogue(&mut self, id: &str) {
        let id = DialogueId::new(id);
        let world = self.world_mut();
        let mut commands = world.commands();
        slotted_menu::start_dialogue(&mut commands, id);
        world.flush();
        self.step(1);
    }
}
