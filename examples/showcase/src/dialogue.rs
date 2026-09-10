//! The smith's conversation (docs/design/showcase-refresh-contract.md
//! section 4.3), compiled in.
//!
//! `dialogue/smith.dialogue.ron` is the whole conversation; this module is
//! what a game writes around one: a registration at `Startup` through
//! [`Dialogue::from_ron`] rather than the asset server, so a scene switch
//! never waits on a handle, and one system that answers a choice
//! ([`thank_on_yes`]). The `secret` option is gated on the value store's
//! `demo.found_key`, which the settings screen's Demo tab
//! ([`crate::settings::FOUND_KEY`]) and the playground's switch both write.

use bevy::prelude::*;
use slotted::menu::{
    Dialogue, DialogueChoice, DialogueId, Dialogues, ToastLevel, ToastSpec, start_dialogue, toast,
};

/// The dialogue's id.
pub const SMITH: &str = "showcase:smith";

/// `dialogue/smith.dialogue.ron`, compiled in.
pub const SMITH_RON: &str = include_str!("../dialogue/smith.dialogue.ron");

/// The smith's dialogue id.
pub fn smith_id() -> DialogueId {
    DialogueId::new(SMITH)
}

/// Parses the compiled-in conversation.
///
/// # Panics
///
/// When the RON does not parse or names a node it does not hold, which means
/// the checked-in file is broken; the tests catch it first.
pub fn smith() -> Dialogue {
    Dialogue::from_ron(SMITH_RON).unwrap_or_else(|e| panic!("parsing the smith dialogue: {e}"))
}

/// `Startup`: registers the smith in [`Dialogues`], unless something did.
pub fn register_smith(mut dialogues: ResMut<Dialogues>) {
    if dialogues.get(&smith_id()).is_none() {
        dialogues.register(smith());
    }
}

/// Starts the smith's conversation, ending a running one first.
pub fn talk(commands: &mut Commands) {
    start_dialogue(commands, smith_id());
}

/// `Update`: the game's answer to a `DialogueChoice`. `yes` earns a toast
/// (`demo.showcase.thanks`); everything else the dialogue itself follows
/// through its `next`.
pub fn thank_on_yes(mut choices: MessageReader<DialogueChoice>, mut commands: Commands) {
    for choice in choices.read() {
        if choice.dialogue == smith_id() && choice.option == "yes" {
            toast(
                &mut commands,
                ToastSpec::new("demo.showcase.thanks").level(ToastLevel::Info),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use slotted::menu::{DialogueNode, NodeId};

    #[test]
    fn the_smith_parses_with_the_five_nodes_the_contract_names() {
        let dialogue = smith();
        assert_eq!(dialogue.id, smith_id());
        assert_eq!(dialogue.start, NodeId::new("hello"));
        for id in ["hello", "ask", "yes", "secret", "bye"] {
            assert!(dialogue.node(&NodeId::new(id)).is_some(), "{id} exists");
        }
        let Some(DialogueNode::Choice { options, .. }) = dialogue.node(&NodeId::new("ask")) else {
            panic!("`ask` is the choice");
        };
        let ids: Vec<&str> = options.iter().map(|o| o.id.as_str()).collect();
        assert_eq!(ids, ["yes", "no", "secret"]);
        assert!(
            options[2].enabled_if.is_some(),
            "`secret` is gated on demo.found_key"
        );
        assert!(matches!(
            dialogue.node(&NodeId::new("bye")),
            Some(DialogueNode::End)
        ));
    }
}
