//! Scene 4: a conversation with the smith at the furnace.
//!
//! `docs/design/showcase-refresh-contract.md` section 4.3. The furnace is
//! the Machine scene's, opened the same way and left cooking; the
//! conversation is `examples/showcase/dialogue/smith.dialogue.ron`,
//! registered at startup by [`crate::scenes::menus::MenusDemoPlugin`] and
//! started here as `slotted:dialogue` over the furnace. An overlay with
//! focus is what the dialogue screen is, so the furnace keeps its menu and
//! its simulation underneath and gets its focus back when the conversation
//! ends.
//!
//! The keys are the crate's: `Enter` skips the typewriter and then advances,
//! `X` opens the history page, `Esc` on the history goes back. The `secret`
//! answer is gated on the value store's `demo.found_key`, which the page's
//! switch writes through `set_value` and the settings screen's Demo tab
//! through its toggle. The transcript pane on the page is fed by
//! [`log_transcript`], one console line per node entered and per choice.

use bevy::prelude::*;
use slotted::menu::{
    ActiveDialogue, DialogueChoice, DialogueNode, DialogueNodeEntered, Dialogues, end_dialogue,
};
use slotted::prelude::*;
use slotted::ui::{LocArgs, LocKey, Localization, UiActionClaims};

use crate::bus::Bus;
use crate::scenes;
use crate::showcase::{ActiveScene, Scene, SceneHandler};

/// Scene 4.
pub struct DialogueScene;

impl SceneHandler for DialogueScene {
    fn enter(&self, world: &mut World) {
        if !scenes::machine::open(world) {
            return;
        }
        start(world);
    }

    fn leave(&self, world: &mut World) {
        // The runner first, so it ends with a reason of its own rather than
        // finding its screen closed under it by the teardown.
        {
            let mut commands = world.commands();
            end_dialogue(&mut commands);
        }
        world.flush();
        scenes::machine::close(world);
        // The smith's thank-you is a toast, which is not a screen root and so
        // not the teardown's; it must not fade over the next scene.
        scenes::menus::close_toasts(world);
    }
}

/// Starts the smith's conversation now.
fn start(world: &mut World) {
    {
        let mut commands = world.commands();
        showcase::dialogue::talk(&mut commands);
    }
    world.flush();
}

/// The page's "Talk again" button: starts the conversation over, unless one
/// is running, which is a line on the console and nothing else. Restarting
/// a conversation the visitor is in the middle of would throw away the
/// choice they were about to make.
///
/// # Errors
///
/// The Dialogue scene is not the one open.
pub fn talk_again(world: &mut World) -> Result<(), String> {
    if world.resource::<ActiveScene>().0 != Scene::Dialogue {
        return Err("no smith: the Dialogue scene is not open".to_owned());
    }
    if world.contains_resource::<ActiveDialogue>() {
        world.resource::<Bus>().log(
            "info",
            "showcase",
            "the smith is still talking; finish the conversation first",
        );
        return Ok(());
    }
    start(world);
    Ok(())
}

/// `SlottedUiSet::Input`: while the smith is talking, `Back` is claimed.
///
/// The dialogue is an overlay with focus, and `pop_on_back` pops the
/// topmost non-overlay entry, which is the furnace underneath: one `Esc`
/// mid-conversation would leave the smith talking over an empty canvas.
/// `Esc` on the history page still goes back, because the page is the focus
/// top then and this does not fire. Only in this scene, and only while a
/// dialogue runs, so nothing else changes hands.
pub fn shield_furnace_from_back(
    active: Option<Res<ActiveScene>>,
    dialogue: Option<Res<ActiveDialogue>>,
    stack: Res<ScreenStack>,
    mut claims: ResMut<UiActionClaims>,
) {
    if active.is_none_or(|scene| scene.0 != Scene::Dialogue) {
        return;
    }
    let Some(dialogue) = dialogue else {
        return;
    };
    if stack
        .focus_top()
        .is_some_and(|top| top.root == dialogue.root)
    {
        claims.claim(UiAction::Back);
    }
}

/// `Update`: the transcript, as console lines with `who: "dialogue"`. A
/// node entered is what the speaker said (`smith: ...`, or the choice's
/// prompt); a choice is what the visitor answered (`you: ...`). The page
/// mirrors these into its transcript pane.
pub fn log_transcript(
    mut entered: MessageReader<DialogueNodeEntered>,
    mut chosen: MessageReader<DialogueChoice>,
    dialogues: Res<Dialogues>,
    localization: Option<Res<Localization>>,
    bus: Option<Res<Bus>>,
) {
    // A world with no bus has no page to tell; the messages are still read,
    // so they do not pile up until one appears.
    let Some(bus) = bus else {
        entered.clear();
        chosen.clear();
        return;
    };
    let text = |key: &LocKey, args: &LocArgs| -> String {
        localization
            .as_deref()
            .map_or_else(|| key.0.clone(), |loc| loc.text_with(key, args))
    };
    let no_args = LocArgs::new();
    for event in entered.read() {
        let Some(node) = dialogues
            .get(&event.dialogue)
            .and_then(|dialogue| dialogue.node(&event.node).cloned())
        else {
            continue;
        };
        let line = match &node {
            DialogueNode::Say {
                speaker,
                text: key,
                args,
                ..
            } => {
                let who = speaker.as_ref().map_or_else(
                    || "narrator".to_owned(),
                    |speaker| text(speaker, &no_args).to_lowercase(),
                );
                format!("{who}: {}", text(key, args))
            }
            DialogueNode::Choice { prompt, .. } => match prompt {
                Some(prompt) => format!("smith: {}", text(prompt, &no_args)),
                None => continue,
            },
            DialogueNode::End => continue,
        };
        bus.log("info", "dialogue", line);
    }
    for choice in chosen.read() {
        let Some(DialogueNode::Choice { options, .. }) = dialogues
            .get(&choice.dialogue)
            .and_then(|dialogue| dialogue.node(&choice.node).cloned())
        else {
            continue;
        };
        let Some(option) = options.iter().find(|option| option.id == choice.option) else {
            continue;
        };
        bus.log(
            "info",
            "dialogue",
            format!("you: {}", text(&option.text, &no_args)),
        );
    }
}
