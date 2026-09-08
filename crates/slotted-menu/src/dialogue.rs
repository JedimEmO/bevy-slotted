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
use slotted_ui::{
    CloseStacked, IconDef, LocArgs, LocKey, Localization, PushScreen, ScreenKind, Screens,
    UiAction, Value, ValueStore,
};

use crate::dialogue_screen::DialogueScreen;

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
        /// A portrait beside the speaker: `(image: "portraits/elder.png")`
        /// or `(item: "demo:chest")`.
        #[serde(
            default,
            skip_serializing_if = "Option::is_none",
            deserialize_with = "deserialize_icon"
        )]
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

/// The one-key map an [`IconDef`] writes, `(image: ..)` or `(item: ..)`,
/// read back by hand: RON's typed reader would only know the `image(..)`
/// spelling of the enum, and the screen files use the map.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct IconMap {
    #[serde(default)]
    image: Option<String>,
    #[serde(default)]
    item: Option<slotted_model::Namespaced>,
}

fn deserialize_icon<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<IconDef>, D::Error> {
    let Some(map) = Option::<IconMap>::deserialize(deserializer)? else {
        return Ok(None);
    };
    match (map.image, map.item) {
        (Some(image), None) => Ok(Some(IconDef::Image(image))),
        (None, Some(item)) => Ok(Some(IconDef::Item(item))),
        _ => Err(serde::de::Error::custom(
            "an icon is `(image: \"path\")` or `(item: \"ns:id\")`, one of the two",
        )),
    }
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
        let bad = || DialogueError::BadCondition(text.to_owned());
        let trimmed = text.trim();
        let (negate, key) = match trimmed.strip_prefix('!') {
            Some(rest) => (true, rest.trim_start()),
            None => (false, trimmed),
        };
        if key.is_empty() || key.contains(|c: char| c.is_whitespace() || c == '!') {
            return Err(bad());
        }
        Ok(Self {
            key: key.to_owned(),
            negate,
        })
    }

    /// Whether the condition holds: a `Bool` is itself, a number is
    /// non-zero, text is non-empty, a missing key is false.
    pub fn holds(&self, values: &ValueStore) -> bool {
        let truthy = match values.get(&self.key) {
            None => false,
            Some(Value::Bool(b)) => *b,
            Some(Value::Int(i)) => *i != 0,
            Some(Value::Float(f)) => *f != 0.0,
            Some(Value::Text(t)) => !t.is_empty(),
        };
        truthy != self.negate
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
        let dialogue: Self = ron::Options::default()
            .with_default_extension(ron::extensions::Extensions::IMPLICIT_SOME)
            .from_str(text)?;
        dialogue.validate()?;
        Ok(dialogue)
    }

    /// Every reference resolves, the start exists, every choice has options
    /// with distinct ids.
    pub fn validate(&self) -> Result<(), DialogueError> {
        if !self.nodes.contains_key(&self.start) {
            return Err(DialogueError::MissingStart(self.start.clone()));
        }
        let check_next = |from: &NodeId, next: Option<&NodeId>| -> Result<(), DialogueError> {
            match next {
                Some(to) if !self.nodes.contains_key(to) => Err(DialogueError::MissingNode {
                    from: from.clone(),
                    to: to.clone(),
                }),
                _ => Ok(()),
            }
        };
        for (id, node) in &self.nodes {
            match node {
                DialogueNode::Say { next, .. } => check_next(id, next.as_ref())?,
                DialogueNode::Choice { options, .. } => {
                    if options.is_empty() {
                        return Err(DialogueError::EmptyChoice(id.clone()));
                    }
                    let mut seen = std::collections::BTreeSet::new();
                    for option in options {
                        if !seen.insert(option.id.as_str()) {
                            return Err(DialogueError::DuplicateOption {
                                node: id.clone(),
                                id: option.id.clone(),
                            });
                        }
                        check_next(id, option.next.as_ref())?;
                    }
                }
                DialogueNode::End => {}
            }
        }
        Ok(())
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
    for event in events.read() {
        let id = match event {
            AssetEvent::Added { id } | AssetEvent::Modified { id } => *id,
            _ => continue,
        };
        let Some(dialogue) = assets.get(id) else {
            continue;
        };
        // A touch that changed nothing is not a new registration.
        if dialogues
            .get(&dialogue.id)
            .is_some_and(|old| *old == *dialogue)
        {
            continue;
        }
        tracing::debug!(dialogue = %dialogue.id, "dialogue asset registered");
        dialogues.register(dialogue.clone());
    }
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
        match self.current() {
            DialogueNode::Choice { options, .. } => options
                .iter()
                .enumerate()
                .map(|(index, option)| (index, option, option_enabled(option, values)))
                .collect(),
            DialogueNode::Say { .. } | DialogueNode::End => Vec::new(),
        }
    }
}

/// Whether `option` is enabled: no condition, no store, or a condition that
/// holds.
fn option_enabled(option: &ChoiceOption, values: Option<&ValueStore>) -> bool {
    match (&option.enabled_if, values) {
        (Some(condition), Some(values)) => condition.holds(values),
        _ => true,
    }
}

/// Ends the running dialogue, if any: the resource goes first, so the
/// screen's close observer has nothing left to cancel, then the message,
/// then the screen.
fn finish(world: &mut World, reason: EndReason) {
    let Some(active) = world.remove_resource::<ActiveDialogue>() else {
        return;
    };
    world.write_message(DialogueEnded {
        dialogue: active.dialogue.id.clone(),
        node: active.node,
        reason,
    });
    CloseStacked(active.root).apply(world);
}

/// Enters `node` of the running dialogue (contract 3.2). The caller has
/// checked that the node exists.
fn enter(world: &mut World, node: NodeId) {
    let Some(active) = world.get_resource::<ActiveDialogue>() else {
        return;
    };
    let dialogue = Arc::clone(&active.dialogue);
    let Some(def) = dialogue.node(&node) else {
        tracing::warn!(dialogue = %dialogue.id, node = %node, "dialogue: no such node");
        return;
    };
    let entered = DialogueNodeEntered {
        dialogue: dialogue.id.clone(),
        node: node.clone(),
    };
    match def {
        DialogueNode::Say {
            speaker,
            text,
            args,
            ..
        } => {
            let line = {
                let loc = world.get_resource::<Localization>();
                HistoryLine {
                    speaker: speaker
                        .as_ref()
                        .map(|key| loc.map_or_else(|| key.0.clone(), |loc| loc.text(key))),
                    text: loc.map_or_else(|| text.0.clone(), |loc| loc.text_with(text, args)),
                }
            };
            let mut active = world.resource_mut::<ActiveDialogue>();
            active.node = node;
            active.revealed = false;
            active.history.push(line);
            world.write_message(entered);
        }
        DialogueNode::Choice { .. } => {
            let mut active = world.resource_mut::<ActiveDialogue>();
            active.node = node;
            active.revealed = true;
            world.write_message(entered);
        }
        DialogueNode::End => {
            world.resource_mut::<ActiveDialogue>().node = node;
            world.write_message(entered);
            finish(world, EndReason::Finished);
        }
    }
}

/// Follows `next` from the current node: enters it, or finishes.
fn follow(world: &mut World, next: Option<NodeId>) {
    match next {
        Some(next) => enter(world, next),
        None => finish(world, EndReason::Finished),
    }
}

/// The body of [`start_dialogue_with`], on the world.
fn start_with(world: &mut World, dialogue: Arc<Dialogue>) {
    if world.contains_resource::<ActiveDialogue>() {
        finish(world, EndReason::Replaced);
    }
    let kind = world
        .get_resource::<DialogueConfig>()
        .map_or_else(crate::kinds::dialogue, |config| config.kind.clone());
    let Some(def) = world
        .get_resource::<Screens>()
        .and_then(|screens| crate::templates::cloned(screens, &kind))
    else {
        tracing::warn!(
            dialogue = %dialogue.id,
            kind = %kind.0,
            "dialogue: the dialogue screen is not registered"
        );
        return;
    };
    let root = world.spawn_empty().id();
    PushScreen {
        root,
        def: Arc::new(def),
        menu: None,
    }
    .apply(world);
    world.entity_mut(root).insert(DialogueScreen);
    let start = dialogue.start.clone();
    world.insert_resource(ActiveDialogue {
        dialogue,
        node: start.clone(),
        root,
        revealed: false,
        history: Vec::new(),
    });
    enter(world, start);
}

/// Starts a registered dialogue, ending a running one first.
pub fn start_dialogue(commands: &mut Commands, id: DialogueId) {
    commands.queue(move |world: &mut World| {
        let found = world
            .get_resource::<Dialogues>()
            .and_then(|dialogues| dialogues.get(&id));
        let Some(dialogue) = found else {
            tracing::warn!(dialogue = %id, "dialogue: nothing registered under this id");
            return;
        };
        start_with(world, dialogue);
    });
}

/// Starts `dialogue`, ending a running one first.
pub fn start_dialogue_with(commands: &mut Commands, dialogue: Arc<Dialogue>) {
    commands.queue(move |world: &mut World| start_with(world, dialogue));
}

/// On a line: reveals it if it is still typing, else moves on. Nothing on
/// a choice or the end.
pub fn advance_dialogue(commands: &mut Commands) {
    commands.queue(|world: &mut World| {
        let Some(active) = world.get_resource::<ActiveDialogue>() else {
            return;
        };
        let DialogueNode::Say { next, .. } = active.current() else {
            return;
        };
        if active.revealed {
            let next = next.clone();
            follow(world, next);
        } else {
            world.resource_mut::<ActiveDialogue>().revealed = true;
        }
    });
}

/// On a choice: picks the option by id and follows it; a disabled or
/// unknown option is a no-op with a warning.
pub fn choose_dialogue(commands: &mut Commands, option: &str) {
    let option = option.to_owned();
    commands.queue(move |world: &mut World| {
        let Some(active) = world.get_resource::<ActiveDialogue>() else {
            tracing::warn!(option, "dialogue: nothing is running to choose from");
            return;
        };
        let dialogue = active.dialogue.id.clone();
        let node = active.node.clone();
        let found = active
            .options(world.get_resource::<ValueStore>())
            .into_iter()
            .find(|(_, o, _)| o.id == option)
            .map(|(index, o, enabled)| (index, o.next.clone(), enabled));
        match found {
            Some((index, next, true)) => {
                world.write_message(DialogueChoice {
                    dialogue,
                    node,
                    option,
                    index,
                });
                follow(world, next);
            }
            Some((_, _, false)) => {
                tracing::warn!(%dialogue, %node, option, "dialogue: that option is disabled");
            }
            None => {
                tracing::warn!(%dialogue, %node, option, "dialogue: no such option here");
            }
        }
    });
}

/// Enters `node` of the running dialogue; the game's answer to a message.
pub fn jump_dialogue(commands: &mut Commands, node: NodeId) {
    commands.queue(move |world: &mut World| {
        let Some(active) = world.get_resource::<ActiveDialogue>() else {
            tracing::warn!(%node, "dialogue: nothing is running to jump in");
            return;
        };
        if active.dialogue.node(&node).is_none() {
            tracing::warn!(dialogue = %active.dialogue.id, %node, "dialogue: no such node to jump to");
            return;
        }
        enter(world, node);
    });
}

/// Ends the running dialogue with [`EndReason::Cancelled`].
pub fn end_dialogue(commands: &mut Commands) {
    commands.queue(|world: &mut World| finish(world, EndReason::Cancelled));
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
    let Some(active) = active else {
        return;
    };
    // A page or modal pushed above holds the focus; the runner goes quiet
    // until it pops.
    if stack.focus_top().is_none_or(|top| top.root != active.root) {
        return;
    }
    let on_line = matches!(active.current(), DialogueNode::Say { .. });
    for event in events.read() {
        if event.repeat || claims.is_claimed(event.action) {
            continue;
        }
        match event.action {
            // On a choice, Accept belongs to the focused option button.
            UiAction::Accept if on_line => {
                advance_dialogue(&mut commands);
                claims.claim(UiAction::Accept);
            }
            UiAction::Back if config.back_cancels => {
                end_dialogue(&mut commands);
                claims.claim(UiAction::Back);
            }
            UiAction::Secondary if config.history => {
                crate::dialogue_screen::open_history(&mut commands);
                claims.claim(UiAction::Secondary);
            }
            _ => {}
        }
    }
}

/// Observer on `ScreenClosed`: a dialogue screen closed from outside cancels
/// the run.
pub fn on_dialogue_closed(
    closed: On<slotted_ui::ScreenClosed>,
    active: Option<Res<ActiveDialogue>>,
    mut ended: MessageWriter<DialogueEnded>,
    mut commands: Commands,
) {
    let Some(active) = active else {
        return;
    };
    if active.root != closed.entity {
        return;
    }
    ended.write(DialogueEnded {
        dialogue: active.dialogue.id.clone(),
        node: active.node.clone(),
        reason: EndReason::Cancelled,
    });
    commands.remove_resource::<ActiveDialogue>();
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
            // Before the pause too: a `Back` the dialogue cancels on is
            // claimed before `pause_on_menu` reads Escape as `Menu`.
            dialogue_actions
                .before(slotted_ui::pop_on_back)
                .before(crate::plugin::pause_on_menu)
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
