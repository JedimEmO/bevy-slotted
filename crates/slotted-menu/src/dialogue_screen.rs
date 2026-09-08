//! The `slotted:dialogue` screen (menus M3 contract 1.3 and section 4):
//! presenting the runner's current node, the typewriter, the choices, the
//! history page and the hint entries.

use std::time::Duration;

use bevy::prelude::*;

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

/// Pushes a `slotted:page` with the transcript so far (contract 4.4).
pub fn open_history(commands: &mut Commands) {
    // M3-IMPL: C
    let _ = commands;
    todo!("open_history")
}

/// On `ActiveDialogue` change: rewrites `speaker`, `text` and the portrait,
/// spawns or clears the option buttons, starts the reveal (contract 4.2).
pub fn present_dialogue_node(
    active: Option<Res<crate::dialogue::ActiveDialogue>>,
    mut commands: Commands,
) {
    // M3-IMPL: C
    let _ = (&active, &mut commands);
}

/// Virtual time: advances every typing line and shows the choices after
/// `choice_delay` (contract 4.3).
pub fn typewriter(time: Res<Time<Virtual>>, mut commands: Commands) {
    // M3-IMPL: C
    let _ = (&time, &mut commands);
}

/// Registers the systems.
pub fn build(app: &mut App) {
    app.add_systems(
        Update,
        (present_dialogue_node, typewriter)
            .chain()
            .in_set(slotted_ui::SlottedUiSet::Render)
            .before(slotted_ui::rich::refresh_key_glyphs),
    );
}
