//! The dialogue model, registry, asset loader and runner (menus M3 contract
//! 1.2 and section 3).
//!
//! A [`Dialogue`] is a map of nodes: a line somebody says, a choice, or the
//! end. The runner walks it one node at a time, tells the game what happened
//! through three messages and takes its answers as commands. What a choice
//! *does* is never the crate's business.

use std::collections::BTreeMap;
use std::sync::Arc;

use bevy::asset::{AssetLoader, LoadContext, io::Reader};
use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use slotted_ui::{IconDef, LocArgs, LocKey, ScreenKind, ValueStore};

/// A dialogue's id, namespaced like everything else: `demo:greeting`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DialogueId(pub String);

impl DialogueId {
    /// An id.
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
}

/// A node's id within its dialogue.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct NodeId(pub String);

impl NodeId {
    /// An id.
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
}

/// One conversation.
#[derive(Asset, TypePath, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Dialogue {
    /// Its id.
    pub id: DialogueId,
    /// Where it starts.
    pub start: NodeId,
    /// Every node.
    pub nodes: BTreeMap<NodeId, DialogueNode>,
}

/// One node.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DialogueNode {
    /// A line.
    Say {
        /// Who says it; none is narration.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        speaker: Option<LocKey>,
        /// A portrait beside the speaker.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        portrait: Option<IconDef>,
        /// The line, rich markup through the catalogue.
        text: LocKey,
        /// Its arguments.
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        args: LocArgs,
        /// What follows; none ends the dialogue after this line.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        next: Option<NodeId>,
    },
    /// A choice.
    Choice {
        /// The question, shown where a line would be.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        prompt: Option<LocKey>,
        /// The options, in order.
        options: Vec<ChoiceOption>,
    },
    /// The end.
    End,
}

/// One option of a choice.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChoiceOption {
    /// Its id, unique within the choice; what `DialogueChoice` reports.
    pub id: String,
    /// The label.
    pub text: LocKey,
    /// What follows; none ends the dialogue.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next: Option<NodeId>,
    /// Shown disabled unless this holds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled_if: Option<Condition>,
}

/// A value store predicate: `key` is truthy, or with `negate`, is not.
/// Written `"found_key"` or `"!found_key"`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Condition {
    /// The value store key.
    pub key: String,
    /// `!key`.
    pub negate: bool,
}

impl Condition {
    /// Parses `key` or `!key`; empty is an error.
    pub fn parse(text: &str) -> Result<Self, DialogueError> {
        // M3-IMPL: B
        let _ = text;
        todo!("Condition::parse")
    }

    /// Whether the condition holds: a `Bool` is itself, a number is
    /// non-zero, text is non-empty, a missing key is false.
    pub fn holds(&self, values: &ValueStore) -> bool {
        // M3-IMPL: B
        let _ = values;
        todo!("Condition::holds")
    }
}

impl TryFrom<String> for Condition {
    type Error = DialogueError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(&value)
    }
}

impl From<Condition> for String {
    fn from(c: Condition) -> Self {
        if c.negate {
            format!("!{}", c.key)
        } else {
            c.key
        }
    }
}

/// Why a dialogue did not load.
#[derive(Debug, thiserror::Error)]
pub enum DialogueError {
    /// RON did not parse.
    #[error("could not parse dialogue: {0}")]
    Ron(#[from] ron::error::SpannedError),
    /// A `next` names no node.
    #[error("node `{from}` continues to `{to}`, which does not exist")]
    MissingNode {
        /// The node with the dangling reference.
        from: NodeId,
        /// The missing target.
        to: NodeId,
    },
    /// `start` names no node.
    #[error("start node `{0}` does not exist")]
    MissingStart(NodeId),
    /// A choice without options.
    #[error("choice `{0}` has no options")]
    EmptyChoice(NodeId),
    /// Two options of one choice share an id.
    #[error("choice `{node}` has two options called `{id}`")]
    DuplicateOption {
        /// The choice.
        node: NodeId,
        /// The repeated id.
        id: String,
    },
    /// A condition that is not `key` or `!key`.
    #[error("bad condition `{0}`")]
    BadCondition(String),
}

impl std::fmt::Display for NodeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::fmt::Display for DialogueId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl Dialogue {
    /// Parses and validates.
    pub fn from_ron(text: &str) -> Result<Self, DialogueError> {
        // M3-IMPL: B
        let _ = text;
        todo!("Dialogue::from_ron")
    }

    /// Every reference resolves, the start exists, every choice has options
    /// with distinct ids.
    pub fn validate(&self) -> Result<(), DialogueError> {
        // M3-IMPL: B
        todo!("Dialogue::validate")
    }

    /// A node by id.
    pub fn node(&self, id: &NodeId) -> Option<&DialogueNode> {
        self.nodes.get(id)
    }
}

/// The registered dialogues.
#[derive(Resource, Debug, Default, Clone)]
pub struct Dialogues {
    by_id: BTreeMap<DialogueId, Arc<Dialogue>>,
}

impl Dialogues {
    /// Registers, replacing one with the same id.
    pub fn register(&mut self, dialogue: Dialogue) -> Arc<Dialogue> {
        let arc = Arc::new(dialogue);
        self.by_id.insert(arc.id.clone(), Arc::clone(&arc));
        arc
    }

    /// A dialogue by id.
    pub fn get(&self, id: &DialogueId) -> Option<Arc<Dialogue>> {
        self.by_id.get(id).cloned()
    }

    /// Every id.
    pub fn ids(&self) -> Vec<DialogueId> {
        self.by_id.keys().cloned().collect()
    }
}

/// Loads `*.dialogue.ron`.
#[derive(Debug, Default, Clone, Copy, TypePath)]
pub struct DialogueLoader;

impl AssetLoader for DialogueLoader {
    type Asset = Dialogue;
    type Settings = ();
    type Error = DialogueAssetError;

    async fn load(
        &self,
        reader: &mut dyn Reader,
        _settings: &(),
        _ctx: &mut LoadContext<'_>,
    ) -> Result<Dialogue, DialogueAssetError> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        Ok(Dialogue::from_ron(&String::from_utf8_lossy(&bytes))?)
    }

    fn extensions(&self) -> &[&str] {
        &["dialogue.ron"]
    }
}

/// Why a dialogue asset failed.
#[derive(Debug, thiserror::Error)]
pub enum DialogueAssetError {
    /// Read failure.
    #[error("could not read dialogue: {0}")]
    Io(#[from] std::io::Error),
    /// Parse or validation failure.
    #[error(transparent)]
    Dialogue(#[from] DialogueError),
}

/// The dialogue handles the game keeps alive, so the files stay watched.
#[derive(Resource, Debug, Default, Clone)]
pub struct DialogueAssets(pub Vec<Handle<Dialogue>>);

impl DialogueAssets {
    /// Starts loading `path` and keeps the handle.
    pub fn load(&mut self, assets: &AssetServer, path: &str) -> Handle<Dialogue> {
        let handle = assets.load(path.to_owned());
        self.0.push(handle.clone());
        handle
    }
}

/// `SlottedUiSet::Render`: every added or modified dialogue asset registers
/// in [`Dialogues`]. A running dialogue keeps its old `Arc`.
pub fn apply_dialogue_assets(
    mut events: MessageReader<AssetEvent<Dialogue>>,
    assets: Res<Assets<Dialogue>>,
    mut dialogues: ResMut<Dialogues>,
) {
    // M3-IMPL: B
    let _ = (&mut events, &assets, &mut dialogues);
}

/// How the runner behaves.
#[derive(Resource, Debug, Clone, PartialEq, Eq)]
pub struct DialogueConfig {
    /// The screen the runner pushes.
    pub kind: ScreenKind,
    /// Whether `Back` ends a running dialogue.
    pub back_cancels: bool,
    /// Whether `Secondary` opens the history page.
    pub history: bool,
}

impl Default for DialogueConfig {
    fn default() -> Self {
        Self {
            kind: crate::kinds::dialogue(),
            back_cancels: false,
            history: true,
        }
    }
}

/// One line of the transcript, resolved when its node was entered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryLine {
    /// Who said it; none is narration.
    pub speaker: Option<String>,
    /// The line's markup.
    pub text: String,
}

/// The running dialogue. Absent when none runs.
#[derive(Resource, Debug, Clone, PartialEq)]
pub struct ActiveDialogue {
    /// The dialogue.
    pub dialogue: Arc<Dialogue>,
    /// The current node.
    pub node: NodeId,
    /// The screen root.
    pub root: Entity,
    /// Whether the current line is fully shown.
    pub revealed: bool,
    /// Everything said so far, in order.
    pub history: Vec<HistoryLine>,
}

impl ActiveDialogue {
    /// The current node.
    pub fn current(&self) -> &DialogueNode {
        self.dialogue
            .node(&self.node)
            .expect("the runner only enters nodes that exist")
    }

    /// The current choice's options with their index and whether each is
    /// enabled; empty on a line or the end.
    pub fn options(&self, values: Option<&ValueStore>) -> Vec<(usize, &ChoiceOption, bool)> {
        // M3-IMPL: B
        let _ = values;
        todo!("ActiveDialogue::options")
    }
}

/// Starts a registered dialogue, ending a running one first.
pub fn start_dialogue(commands: &mut Commands, id: DialogueId) {
    // M3-IMPL: B
    let _ = (commands, id);
    todo!("start_dialogue")
}

/// Starts `dialogue`, ending a running one first.
pub fn start_dialogue_with(commands: &mut Commands, dialogue: Arc<Dialogue>) {
    // M3-IMPL: B
    let _ = (commands, dialogue);
    todo!("start_dialogue_with")
}

/// On a line: reveals it if it is still typing, else moves on. Nothing on
/// a choice or the end.
pub fn advance_dialogue(commands: &mut Commands) {
    // M3-IMPL: B
    let _ = commands;
    todo!("advance_dialogue")
}

/// On a choice: picks the option by id and follows it; a disabled or
/// unknown option is a no-op with a warning.
pub fn choose_dialogue(commands: &mut Commands, option: &str) {
    // M3-IMPL: B
    let _ = (commands, option);
    todo!("choose_dialogue")
}

/// Enters `node` of the running dialogue; the game's answer to a message.
pub fn jump_dialogue(commands: &mut Commands, node: NodeId) {
    // M3-IMPL: B
    let _ = (commands, node);
    todo!("jump_dialogue")
}

/// Ends the running dialogue with [`EndReason::Cancelled`].
pub fn end_dialogue(commands: &mut Commands) {
    // M3-IMPL: B
    let _ = commands;
    todo!("end_dialogue")
}

/// A node was entered.
#[derive(Message, Debug, Clone, PartialEq, Eq)]
pub struct DialogueNodeEntered {
    /// The dialogue.
    pub dialogue: DialogueId,
    /// The node.
    pub node: NodeId,
}

/// An option was chosen.
#[derive(Message, Debug, Clone, PartialEq, Eq)]
pub struct DialogueChoice {
    /// The dialogue.
    pub dialogue: DialogueId,
    /// The choice node.
    pub node: NodeId,
    /// The option's id.
    pub option: String,
    /// The option's index.
    pub index: usize,
}

/// A dialogue ended.
#[derive(Message, Debug, Clone, PartialEq, Eq)]
pub struct DialogueEnded {
    /// The dialogue.
    pub dialogue: DialogueId,
    /// The node it was on.
    pub node: NodeId,
    /// Why.
    pub reason: EndReason,
}

/// Why a dialogue ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EndReason {
    /// It reached an end.
    Finished,
    /// `end_dialogue`, `Back`, or its screen closed from outside.
    Cancelled,
    /// Another dialogue started over it.
    Replaced,
}

/// `SlottedUiSet::Navigate`, before `pop_on_back`: `Accept`, `Back` and
/// `Secondary` while the dialogue is the focus top (contract 3.2).
pub fn dialogue_actions(
    mut events: MessageReader<slotted_ui::UiActionEvent>,
    mut claims: ResMut<slotted_ui::UiActionClaims>,
    stack: Res<slotted_ui::ScreenStack>,
    active: Option<Res<ActiveDialogue>>,
    config: Res<DialogueConfig>,
    mut commands: Commands,
) {
    // M3-IMPL: B
    let _ = (
        &mut events,
        &mut claims,
        &stack,
        &active,
        &config,
        &mut commands,
    );
}

/// Observer on `ScreenClosed`: a dialogue screen closed from outside cancels
/// the run.
pub fn on_dialogue_closed(
    closed: On<slotted_ui::ScreenClosed>,
    active: Option<Res<ActiveDialogue>>,
    mut ended: MessageWriter<DialogueEnded>,
    mut commands: Commands,
) {
    // M3-IMPL: B
    let _ = (&closed, &active, &mut ended, &mut commands);
}

/// Registers the loader, the registry, the config, the messages and the
/// systems.
pub fn build(app: &mut App) {
    app.init_resource::<Dialogues>()
        .init_resource::<DialogueAssets>()
        .init_resource::<DialogueConfig>()
        .add_message::<DialogueNodeEntered>()
        .add_message::<DialogueChoice>()
        .add_message::<DialogueEnded>()
        .add_observer(on_dialogue_closed)
        .add_systems(
            Update,
            dialogue_actions
                .before(slotted_ui::pop_on_back)
                .in_set(slotted_ui::SlottedUiSet::Navigate),
        );
    if app.world().contains_resource::<AssetServer>() {
        app.init_asset::<Dialogue>()
            .register_asset_loader(DialogueLoader)
            .add_systems(
                Update,
                apply_dialogue_assets.in_set(slotted_ui::SlottedUiSet::Render),
            );
    }
}
