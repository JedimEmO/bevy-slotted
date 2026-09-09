# Menus M3 contract: dialogue

Status: v1.3, 2026-09-09 (v1.0 plus the "v1.1 (B)", "v1.2 (C)" and "v1.3 (review)" notes; the review round is `docs/design/menus-m3-notes-review.md`). Bevy 0.19.1. Companion to `docs/design/menus-proposal.md` (v0.3)
section 5.3, and to the M0, M1 and M2 contracts whose vocabulary this uses without restating. Four
packages: **A** (foundation changes in `slotted-ui` and `slotted-theme`), **B** (the dialogue
model, the registry and asset loader, the runner) and **C** (the `slotted:dialogue` screen: template,
typewriter, choices, history, hint entries, materials) run in parallel; **D** (harness, example,
docs) runs after. The skeleton creates the modules and every public type named in section 1; a body
marked `// M3-IMPL: A|B|C|D` is that package's to fill. Signatures change only by amending this
file. Each package writes `docs/design/menus-m3-notes-{A,B,C,D}.md`.

## 0. Ground rules

- **The runner is data-driven, not a language.** A `Dialogue` is a list of nodes; what a choice
  *does* is the game's or a mod's business, told through messages and answered through commands.
  The crate never evaluates anything richer than "is this value store key truthy".
- **Everything M2 said still holds**: `slotted-menu` reaches the foundation only through public
  `slotted-ui` API (package A adds what is missing), templates are embedded data a game overrides
  by registering the kind first, every string has an English fallback, no `.ftl` is required, and
  the crate stays out of the `server` feature graph.
- **One dialogue at a time.** Starting a dialogue while one runs ends the running one with
  `EndReason::Replaced` and starts the new one. Games that want queues build them on the messages.
- **The screen is an overlay that takes focus.** A dialogue sits over gameplay and under any page
  or modal pushed after it (a pause over a conversation is normal). Overlays never took focus
  before M3; section 2.1 gives an overlay the option, which is the one foundation change the
  runner cannot do without.
- **Time is virtual.** The typewriter counts `Time<Virtual>` like the toasts, so a paused game
  freezes a half-revealed line, and `Motion::reduced` reveals everything at once.
- Headless first: the runner and the screen are driven end to end by `UiHarness` in three themes.

## 1. Public surface (done in the skeleton: types and signatures; bodies per package)

### 1.1 `slotted-ui` and `slotted-theme` additions (A)

```rust
// def.rs
pub struct Presentation { /* existing */ pub focus: Option<bool> }   // RON `focus: true`; default = mode != overlay
impl Presentation { pub fn takes_focus(&self) -> bool; }
// stack.rs
impl ScreenStack { pub fn focus_top(&self) -> Option<&StackEntry>; }   // topmost entry whose presentation takes focus
pub fn close_stacked(commands: &mut Commands, root: Entity);           // closes one entry by root, wherever it sits
// def.rs: Text and RichText nodes
UiNodeDef::Text     { /* existing */ role: Option<Role> }              // RON `role: "dialogue.speaker"`; overrides the TextRole's paint role
UiNodeDef::RichText { /* existing */ role: Option<Role> }
// rich.rs
#[derive(Component)] pub struct RichReveal(pub Option<usize>);        // None = everything; Some(n) = the first n units
impl RichReveal { pub const ALL: Self; pub fn units(runs: &[RichRun]) -> usize; }
// slotted-theme tokens.rs
pub struct Tokens { /* existing */ #[serde(default)] pub dialogue: DialogueTokens }
#[derive(Serialize, Deserialize)] pub struct DialogueTokens { pub chars_per_second: u32 /* 40 */, pub choice_delay: u32 /* ms after the reveal before the choices show, 150 */ }
// slotted-theme role.rs: DIALOGUE, DIALOGUE_SPEAKER, DIALOGUE_TEXT, DIALOGUE_PORTRAIT (roles::ALL 101 -> 105)
```

### 1.2 `slotted-menu::dialogue` (B: model, registry, loader, runner)

```rust
#[derive(Clone, PartialEq, Eq, Hash, Ord, Serialize, Deserialize)] pub struct DialogueId(pub String);   // "demo:greeting"
#[derive(Clone, PartialEq, Eq, Hash, Ord, Serialize, Deserialize)] pub struct NodeId(pub String);

#[derive(Asset, TypePath, Clone, PartialEq, Serialize, Deserialize)]
pub struct Dialogue { pub id: DialogueId, pub start: NodeId, pub nodes: BTreeMap<NodeId, DialogueNode> }
#[derive(Serialize, Deserialize)] #[serde(rename_all = "snake_case")]
pub enum DialogueNode {
    Say { speaker: Option<LocKey>, portrait: Option<IconDef>, text: LocKey, args: LocArgs, next: Option<NodeId> },  // next None = ends after
    Choice { prompt: Option<LocKey>, options: Vec<ChoiceOption> },
    End,
}
pub struct ChoiceOption { pub id: String, pub text: LocKey, pub next: Option<NodeId>, pub enabled_if: Option<Condition> }
#[derive(Serialize, Deserialize)] #[serde(try_from = "String", into = "String")]
pub struct Condition { pub key: String, pub negate: bool }        // "met_elder" or "!met_elder"
impl Condition { pub fn parse(s: &str) -> Result<Self, DialogueError>; pub fn holds(&self, values: &ValueStore) -> bool; }
impl Dialogue {
    pub fn from_ron(text: &str) -> Result<Self, DialogueError>;    // parses and validates
    pub fn validate(&self) -> Result<(), DialogueError>;           // start exists, every next exists, a choice has an option, ids unique per choice
    pub fn node(&self, id: &NodeId) -> Option<&DialogueNode>;
}
#[derive(thiserror::Error)] pub enum DialogueError { Ron(SpannedError), MissingNode { from: NodeId, to: NodeId }, MissingStart(NodeId), EmptyChoice(NodeId), DuplicateOption { node: NodeId, id: String }, BadCondition(String) }

#[derive(Resource, Default)] pub struct Dialogues { /* BTreeMap<DialogueId, Arc<Dialogue>> */ }
impl Dialogues { pub fn register(&mut self, d: Dialogue) -> Arc<Dialogue>; pub fn get(&self, id: &DialogueId) -> Option<Arc<Dialogue>>; pub fn ids(&self) -> Vec<DialogueId>; }
pub struct DialogueLoader;                                          // AssetLoader for `*.dialogue.ron`
#[derive(Resource, Default)] pub struct DialogueAssets(pub Vec<Handle<Dialogue>>);
impl DialogueAssets { pub fn load(&mut self, assets: &AssetServer, path: &str) -> Handle<Dialogue>; }
pub fn apply_dialogue_assets(/* Added|Modified -> Dialogues::register; a running dialogue of that id keeps its Arc */);

#[derive(Resource)] pub struct DialogueConfig { pub kind: ScreenKind /* slotted:dialogue */, pub back_cancels: bool /* false */, pub history: bool /* true */ }

#[derive(Resource, Debug, Clone, PartialEq)]
pub struct ActiveDialogue {
    pub dialogue: Arc<Dialogue>, pub node: NodeId, pub root: Entity,   // the screen root
    pub revealed: bool,                                                  // the current say line is fully shown
    pub history: Vec<HistoryLine>,
    pub entered: u64,   // v1.3 (review): entries so far, counting a re-entry of the current node, so a `jump` back re-presents
}
pub struct HistoryLine { pub speaker: Option<String>, pub text: String }   // resolved markup, recorded when a say node is entered
impl ActiveDialogue { pub fn current(&self) -> &DialogueNode; pub fn options(&self, values: Option<&ValueStore>) -> Vec<(usize, &ChoiceOption, bool /* enabled */)>; }   // v1.1 (B): `Option`, as the skeleton had it; no store = every option enabled

pub fn start_dialogue(commands: &mut Commands, id: DialogueId);          // warns when unknown
pub fn start_dialogue_with(commands: &mut Commands, dialogue: Arc<Dialogue>);
pub fn advance_dialogue(commands: &mut Commands);                        // say: reveal if revealing, else go to next; choice/end: no-op
pub fn choose_dialogue(commands: &mut Commands, option: &str);            // on a choice node: pick the option by id (disabled = no-op with a warning)
pub fn jump_dialogue(commands: &mut Commands, node: NodeId);              // the game's answer to a message: go here
pub fn end_dialogue(commands: &mut Commands);                             // EndReason::Cancelled

#[derive(Message)] pub struct DialogueNodeEntered { pub dialogue: DialogueId, pub node: NodeId }
#[derive(Message)] pub struct DialogueChoice { pub dialogue: DialogueId, pub node: NodeId, pub option: String, pub index: usize }
#[derive(Message)] pub struct DialogueEnded { pub dialogue: DialogueId, pub node: NodeId, pub reason: EndReason }
pub enum EndReason { Finished, Cancelled, Replaced }
```

### 1.3 `slotted-menu::dialogue_screen` (C)

```rust
#[derive(Component)] pub struct DialogueScreen;                         // on the screen root
#[derive(Component)] pub struct DialogueLine { pub elapsed: Duration, pub units: usize, pub total: usize }  // on the `text` node while revealing
#[derive(Component)] pub struct DialogueChoices;                         // the column the option buttons live in
#[derive(Component)] pub struct DialogueOption { pub id: String, pub index: usize }   // on each option button
pub const OPTION_TAG: &str = "dialogue.option";                          // also a tag on the button: locators find it
pub fn open_history(commands: &mut Commands);                             // pushes a slotted:page of the transcript
// v1.2 (C): additions, nothing above changed
#[derive(Component)] pub struct DialogueChoicesPending { pub root: Entity, pub elapsed: Duration }  // on `choices` while the buttons wait for `choice_delay`
pub fn option_test_id(id: &str) -> String;                                // "option.<id>", the button's test_id and what its nav links name
pub fn transcript(history: &[HistoryLine]) -> String;                     // the history page's body markup
pub fn on_option_activate(/* observer on Activate: an option button calls choose_dialogue */);
```

### 1.4 What the skeleton settles

- Module names: `dialogue.rs` (B) and `dialogue_screen.rs` (C) in `slotted-menu`; the screen
  template is `crates/slotted-menu/screens/dialogue.screen.ron`; `kinds::dialogue()`.
- `MenuPlugin` registers the asset loader, `Dialogues`, `DialogueAssets`, `DialogueConfig` (an
  `init_resource`, unlike `MenuConfig`: a dialogue only runs when started), the three messages, the
  template and the systems of B and C.
- `DialogueId` and `NodeId` are plain newtypes over `String` so M4's Lua bridge passes them as
  strings; `serde(transparent)`.
- `LocKey` serialises from a bare string. `IconDef` *writes* the `(image: ..)`/`(item: ..)` map,
  but RON's typed reader only knows the `image(..)` spelling of an enum (the screen loader gets
  the map through the model path, which a `say(..)`-tagged file cannot take, since `ron::Value`
  drops struct names). **v1.1 (B)**: the `portrait` field reads the map by hand
  (`deserialize_icon`), so a `.dialogue.ron` reads as in section 3.1; the `image(..)` form is not
  accepted.

## 2. Package A: foundation

- 2.1 **Focusable overlays.** `Presentation.focus` (RON `focus: true`), `takes_focus()`, and
  `ScreenStack::focus_top()`. `restore_focus`, `enforce_focus_scope`, `focus_on_spawn` (its overlay
  early return becomes `!takes_focus()`), the tabs fallback in `tabs.rs` and the hint bar's "top"
  in `hint_bar.rs` use `focus_top()`. `top()`, `pop_screen`, `pop_on_back` and `pause_on_menu`
  keep their meaning (the top *non-overlay* entry): Back never pops an overlay, `Menu` over an
  overlay still pauses. Test: a `focus: true` overlay over nothing gets its `initial_focus`; a modal
  pushed above takes it and popping the modal gives it back; a plain overlay is skipped both ways;
  Back over the focusable overlay pops nothing.
- 2.2 **`close_stacked(commands, root)`** (implemented in the skeleton as the `CloseStacked`
  command: `pop_entry` plus `finish_change`; a root the stack does not hold closes like
  `close_screen`): the public way to drop one entry that is not the top. A's test: closing an
  overlay under a modal leaves the modal, its focus and its scrim alone.
- 2.3 **Rich text reveal.** `RichReveal` on a `rich_text` node; `render_rich_text` re-renders when
  it changes (a `Ref<RichReveal>` beside `Ref<RichText>`; the runs are not re-parsed, the spans
  are). Units: one per `char` of a `Text` run; a `Key` or `Icon` run is one unit, shown whole or not
  at all. **Layout stays put**: the unrevealed remainder of a run is a second `TextSpan` with the
  same font and a fully transparent colour, an unrevealed icon is `Visibility::Hidden` (still laid
  out), so lines never reflow while a line types. `refresh_key_glyphs` keeps working: a key span
  that is unrevealed carries `RichKeySpan` too and simply stays transparent. `RichReveal::units`
  counts what a full reveal needs. Test: `Some(0)` shows nothing and the node's height equals the
  full one; `Some(n)` mid-run splits it; a key run flips whole; `ALL` and a missing component
  render identically.
- 2.4 **Text role override.** `role: Option<Role>` on `Text` and `RichText` (RON
  `role: "dialogue.speaker"`), painting that role instead of the `TextRole`'s `text.*`; a role the
  theme lacks falls back to the `TextRole` with one warning. The main menu template uses it for
  `menu.title` and `menu.version`, closing the M2 follow-up. Test: a `text` with `role` paints the
  role's colour.
- 2.5 **Tokens and roles.** `DialogueTokens` with defaults, `dialogue`, `dialogue.speaker`,
  `dialogue.text`, `dialogue.portrait` in `roles::ALL` (105) with materials in the three themes:
  glass a translucent sheet, neon a hard-edged panel with the accent speaker, paper a hairline-ruled
  box with the speaker in small caps mono and no scrim. Role completeness test updated.
- 2.6 **`ScreenDef::set_text` reaches a button label** (the `Button` arm writes `ButtonOpts::label`),
  closing the M2 follow-up; `confirm::set_button_label` becomes a call to it.
- 2.7 **Focus never leaks under a focusable overlay.** When the push of a `focus: true` overlay
  leaves a node of the screen below focused, `enforce_focus_scope` clears it the same frame (it
  already does for a modal; the test pins it for the overlay), so an `Accept` the next frame reaches
  the runner and not the button underneath.

## 3. Package B: model, registry, loader, runner

### 3.1 The asset

```ron
#![enable(implicit_some)]
(
    id: "demo:greeting",
    start: "hello",
    nodes: {
        "hello": say(speaker: "demo-elder", portrait: (image: "portraits/elder.png"),
                     text: "demo-greeting-hello", next: "ask"),
        "ask": choice(prompt: "demo-greeting-ask", options: [
            (id: "yes", text: "demo-yes", next: "yes"),
            (id: "no", text: "demo-no", next: "bye"),
            (id: "secret", text: "demo-secret", next: "secret", enabled_if: "found_key"),
        ]),
        "yes": say(speaker: "demo-elder", text: "demo-greeting-yes", args: {"name": "Traveller"}, next: "bye"),
        "secret": say(text: "demo-greeting-secret", next: "bye"),
        "bye": end,
    },
)
```

`Dialogue::from_ron` parses through `ron::Options` with `implicit_some` enabled and validates.
`DialogueLoader` handles `dialogue.ron` (longest extension wins over `slotted-packs`' `ron`);
`apply_dialogue_assets` registers on `Added` and `Modified` and keeps a running dialogue on its
old `Arc` (a hot edit takes effect the next time it starts). Registering by hand:
`Dialogues::register(Dialogue::from_ron(include_str!(..))?)`.

### 3.2 The runner

`start_dialogue_with` (a `Command`): if an `ActiveDialogue` exists, end it (`Replaced`) first.
Clone the registered `DialogueConfig.kind` def (warn and return when none), push it through
`PushScreen` with a fresh root, insert `ActiveDialogue { node: start, revealed: false, history:
[] }`, `DialogueScreen` on the root, then *enter* the start node. Entering a node:

- `Say`: resolve `speaker` and `text` (with `args`) through `Localization` into a `HistoryLine`
  pushed to `history`; `revealed = false`; write `DialogueNodeEntered`.
- `Choice`: write `DialogueNodeEntered`; `revealed = true` (the prompt is short; it does not type).
- `End`: write `DialogueNodeEntered` then finish: `DialogueEnded { reason: Finished }`, remove
  `ActiveDialogue`, `close_stacked(root)`.

Transitions: `advance_dialogue` on a `Say` with `revealed == false` sets `revealed = true` (C's
typewriter jumps to the end); with `revealed == true` enters `next` or finishes when `next` is
`None`. `choose_dialogue(id)` on a `Choice` whose option is enabled (`Condition::holds` against
`ValueStore`, absent store = every condition holds) writes `DialogueChoice` then enters the
option's `next`, or finishes when `None`. `jump_dialogue` enters any node of the running dialogue
(warn on unknown). `end_dialogue` finishes with `Cancelled`. Every transition is a `Command`, so a
game observing a message can answer in the same frame and the order is deterministic.

The runner also owns the input: `dialogue_actions` in `SlottedUiSet::Navigate`, before
`pop_on_back` and before `pause_on_menu` (**v1.1 (B)**: Escape is `Back` and `Menu`; the pause
claims `Back` when it pushes, so the dialogue's `back_cancels` claim has to land first), reads `UiActionEvent`s while `stack.focus_top()` is the dialogue root (a modal above
silences it). A fresh unclaimed `Accept` on a `Say` calls `advance_dialogue` and claims. `Back`
with `back_cancels` ends the dialogue and claims; otherwise Back is left alone (nothing pops an
overlay). `Secondary` with `history` opens the history page (C's `open_history`) and claims. On a
`Choice`, `Accept` belongs to the focused option button (the normal `Activate` path) and the runner
ignores it.

Closing the screen from outside (`clear_screens`, a hot-reload respawn) is observed on
`ScreenClosed` for a root carrying `DialogueScreen` with an `ActiveDialogue` still pointing at it:
`DialogueEnded { Cancelled }` and the resource removed.

### 3.3 Tests (B)

`crates/slotted-menu/tests/dialogue.rs` (B owns the file; C tests in `dialogue_screen.rs`): `from_ron` parses the 3.1 sample and
rejects a dangling `next`, a missing start, an empty choice, a duplicate option id and a bad
condition, each with the right error; `Condition` parses both forms and reads `Bool`, numbers and
text truthiness; a registered dialogue starts, the screen kind is on the stack as an overlay,
`DialogueNodeEntered` fires per node; `advance` reveals then advances; `choose` writes
`DialogueChoice` and follows `next`; a disabled option is a no-op; `jump` from a message answer
redirects; `end` cancels; starting over a running dialogue ends it `Replaced`; `End` node and
`next: None` both finish with `Finished` and close the screen; `clear_screens` cancels; a modal
pushed above silences Accept and popping it restores; the asset loader round trip through
`UiHarness` with an asset dir (`apply_dialogue_assets` registers).

## 4. Package C: the screen

### 4.1 Template `slotted:dialogue`

Presentation `overlay`, `focus: true`, no scrim, transition `slide_up`, `back: none`. Root: a
`panel` in role `dialogue`, `place: (anchor: "bottom", offset: (0.0, -24.0))`, `width: "90%"`,
`max_width: 900`, padding `3`, column, `tags: {"test_id": "dialogue"}`:

- a row (id `header`): a `panel` (id `portrait`, role `dialogue.portrait`, `sizes.portrait` square
  (a new `Sizes` field, default 64, A adds it beside `DialogueTokens`),
  hidden when the node has no portrait; the image is spawned inside through `IconImages`), a
  `text` (id `speaker`, style `label`, role `dialogue.speaker`, key `slotted.menu.dialogue.speaker`
  rewritten per node, hidden when none);
- `rich_text` (id `text`, style `body`, role `dialogue.text`, `wrap: true`, `min_height` of three
  lines so a short line does not bounce the panel);
- a column (id `choices`, `DialogueChoices`, gap `1`, hidden until the choices show);
- footer row: hint bar (id `hints`), spacer, a `text` caption (id `continue`, key
  `slotted.menu.dialogue.continue` = "{key:accept} continue", shown only when a `Say` is revealed).
- anchors `header_end`, `text_end`, `choices_end`, `footer`.

### 4.2 Presenting a node

`present_dialogue_node` (Update, after the runner's commands applied, i.e. in
`SlottedUiSet::Layout`-adjacent, on `ActiveDialogue` change): on a `Say`, set the `speaker` and
`text` nodes (`LocText`/`RichText` with the node's key and args, so the catalogue does the work
and a language switch repaints), spawn or clear the portrait, despawn any option buttons, put
`RichReveal(Some(0))` and `DialogueLine { elapsed: 0, units: 0, total }` on `text` (`total` from
`RichReveal::units` of the resolved runs; under `Motion::reduced` or when `chars_per_second == 0`
put `RichReveal::ALL` and mark the runner `revealed`). On a `Choice`: `text` shows the prompt (or
is empty), `RichReveal::ALL`, and one `button` per option in `choices`, spawned as `UiNodeDef::
Button` through a `SpawnCtx` the way the toast spawns its snippet, labelled with the option's
key, `disabled` when its condition fails, carrying `DialogueOption { id, index }`, the tag
`dialogue.option = id` and `nav` tags for a vertical walk; focus goes to the first enabled option.
On `End` nothing (the screen is closing).

### 4.3 The typewriter

`typewriter` (Update, virtual time): for each `DialogueLine`, `elapsed += delta`, `units =
min(total, elapsed * cps)`, writes `RichReveal(Some(units))` when it changed; at `total` remove
`DialogueLine`, set the runner's `revealed = true`, show `continue`. When the runner's `revealed`
flips true from outside (Accept skipped the reveal) the system jumps to `total` the same frame. A
`Choice`'s buttons appear after `choice_delay` (one `DialogueLine`-like timer on `choices`); under
reduced motion at once. The `text` node's `RichReveal` is removed when the screen closes, nothing
else to clean.

### 4.4 History

`open_history`: builds the transcript from `ActiveDialogue.history` as one markup string,
`[b]{speaker}[/b]\n{text}\n\n` per line (a narration line has no speaker), and pushes it as a
`PageSpec` whose `body` key is the transcript itself (an unresolved key draws as written, and
`looks_like_key` keeps it out of the missing-key warning; a `PageSpec::literal` helper is A's
one-liner if a cleaner route appears). Title `slotted.menu.dialogue.history`. The page is a modal
over the overlay, so the runner goes quiet until Done pops it.

### 4.5 Hint entries

The hint bar on the dialogue lists: while revealing, `Accept` "Skip"; revealed on a `Say`,
`Accept` "Continue"; on a `Choice`, the focused button's verb as usual; `Secondary` "History" when
`history`; `Back` "Leave" when `back_cancels`. The bar reads these through a `hint.accept` tag
C rewrites per state, plus two new verbs `hint_bar.rs` learns: a `hint.secondary` and a
`hint.back` tag on the bar (small edit in B's M2 file; keys `slotted.menu.dialogue.skip`,
`continue`, `history`, `leave`). **v1.2 (C)**: all three tags live on the *bar* (the dialogue has
no focused node on a line), `hint.accept` on the bar applies only when the focused node
contributed no verb, and a change to a bar's tags rebuilds it; the "Continue" label is the
existing `slotted.menu.dialogue.next` key (`.continue` is the caption with the glyph).

### 4.6 Tests (C)

`crates/slotted-menu/tests/dialogue_screen.rs` (C's own file; copy the helpers from
`tests/dialogue.rs`, which B owns) in three themes: the template registers
and opens as a focusable overlay; a `Say` shows speaker, portrait and a `text` whose reveal is
`Some(0)` then grows with virtual time at `chars_per_second`; Accept mid-reveal shows everything
and the next Accept advances; reduced motion shows everything at once; a `Choice` spawns the
buttons after `choice_delay`, focus on the first enabled, the disabled one skipped by the gamepad
walk, `Activate` on an option yields the `DialogueChoice` and the next line; `Secondary` opens the
history page listing every line said so far in order and Done returns to the dialogue with its
focus; the hint entries per state; the `text` height does not change during a reveal; a pause
pushed over the dialogue takes focus and the dialogue resumes after it pops; snapshots of a say and
a choice in three themes.

## 5. Package D: harness, example, docs

- Harness (`crates/slotted-test/src/menu.rs`, `menu` feature): `dialogue() -> Option<(DialogueId,
  NodeId)>`, `dialogue_text() -> String` (the visible units of the `text` node), `dialogue_revealed()
  -> bool`, `dialogue_options() -> Vec<(String, bool)>` (id, enabled, in order),
  `dialogue_advance()` (Accept and a frame), `dialogue_choose(id)` (activate the button),
  `dialogue_history() -> Vec<(Option<String>, String)>`, `dialogue_events()` draining
  `DialogueNodeEntered`/`DialogueChoice`/`DialogueEnded` as one enum from a recorder plugin the
  builder adds (the `MenuChoiceRecorder` pattern), `start_dialogue(id)`.
- `examples/menus`: a `Talk` button injected into the main menu (like About) starts
  `demo:greeting` from `assets/dialogue/greeting.dialogue.ron` (loaded through `DialogueAssets`;
  strings in `assets/locale/en-US.ftl`; a portrait PNG under `assets/portraits/`), whose `secret`
  option is enabled once the settings' `demo.found_key` toggle is on (a `Custom` settings row or a
  plain `Toggle` in the demo tab); the game answers `DialogueChoice` `yes` with a toast. `--shot`
  flags `--dialogue`, `--dialogue-choice`, `--dialogue-history` in three themes; reference shots
  under `examples/menus/shots/`; `just shot-menus` extended. `examples/menus/tests/flow.rs` gains
  the dialogue walk by gamepad.
- Docs: `docs/guide/dialogue.md` (the asset format, starting and answering, conditions, the
  screen and its anchors, overriding the template, the typewriter tokens, history, testing);
  `menus.md` links it; `screens.md` (`focus` on an overlay, `role` on text); `rich-text.md`
  (`RichReveal`, and the `GlyphSet` mention the M2 follow-up asked for); `themes.md` (105 roles,
  the dialogue group, `dialogue` tokens); `testing.md` (the dialogue helpers); `PLAN.md` M3 done;
  `CHANGELOG.md` "Menus M3"; `FOLLOWUPS.md` "Menus M3" with every deferral, and the M2 entries M3
  closes (text role override, overlay Back wording, `rich-text.md` GlyphSet) deleted and named in
  the M2 header.
- M4 hook: the three messages carry only strings and a `usize`, and `start_dialogue`,
  `advance_dialogue`, `choose_dialogue`, `jump_dialogue`, `end_dialogue` take only strings, so the
  Lua commands and events in M4 are one-line bridges. `slotted-packs` does not depend on
  `slotted-menu`, so where the forwarding lives (an optional dependency, or the facade) is M4's
  decision; D notes it in `FOLLOWUPS.md` together with "a dialogue shipped inside a mod pack waits
  for a `RegistryKind::Dialogues`".
- Harness caveats D documents: `stack_top()` skips overlays by design (use `stack()` or
  `dialogue()`), and `text_of` reads a node's own `Text`, so `dialogue_text()` concatenates the
  visible `RichPart` spans.

## 6. Done when

- `just ci` green on native and wasm; `slotted-menu` checks on `wasm32-unknown-unknown`.
- A game registers a `.dialogue.ron`, calls `start_dialogue`, and gets a typed conversation with
  portraits, choices gated by its value store, a history page, and messages telling it what was
  chosen, with no screen file of its own; a game that registers `slotted:dialogue` first gets its
  own look.
- `examples/menus` walks the conversation by gamepad headless and the reference shots exist for
  three themes.
- FOLLOWUPS M2 items "the `menu.title` and `menu.version` roles are unused", "overlay screens'
  bars say Back", "`set_text` does not reach a button label" and "`rich-text.md` does not mention
  `GlyphSet`" are closed.
