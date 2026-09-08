# Dialogue

A conversation: lines with a speaker and a portrait, typed out one glyph at a
time, a choice whose options the game's value store can lock, a history page
of what was said, and messages telling the game what the player picked.
`slotted-menu` runs it from a data file and a screen template; the game
supplies the file, starts the dialogue, and answers the messages it cares
about. Nothing in the runner evaluates anything richer than "is this value
store key truthy": what a choice *does* is the game's business, told through
`DialogueChoice` and answered through commands.

`examples/menus` is the worked example: its Talk button starts
`assets/dialogue/greeting.dialogue.ron`, the settings screen's Demo tab
unlocks the third answer, and `tests/flow.rs` walks it by gamepad.

## The asset

A `.dialogue.ron` is one `Dialogue`: an id, the node it starts on, and a map
of nodes.

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
            (id: "secret", text: "demo-secret", next: "secret", enabled_if: "demo.found_key"),
        ]),
        "yes": say(speaker: "demo-elder", text: "demo-greeting-yes",
                   args: {"name": "Traveller"}, next: "bye"),
        "secret": say(text: "demo-greeting-secret", next: "bye"),
        "bye": end,
    },
)
```

| Node | Fields | |
|---|---|---|
| `say` | `speaker`, `portrait`, `text`, `args`, `next` | A line. `speaker` and `text` are locale keys (`text` is [rich-text markup](rich-text.md), so `[b]`, `[i]` and `{key:..}` work); `args` are the Fluent arguments for `text`; `portrait` is an icon, `(image: "path.png")` or `(item: "ns:item")`. A line with no speaker and no portrait is narration. `next: None` (or the field left out) ends the dialogue after the line. |
| `choice` | `prompt`, `options` | A question. Each option has an `id` (unique within the choice), a `text` key, a `next` node (`None` ends) and an optional `enabled_if`. |
| `end` | | Ends the dialogue. `next: "bye"` to an `end` node and `next: None` mean the same; the node is there for a file that wants one named exit. |

`Dialogue::from_ron` parses (with `implicit_some` on whether or not the file
says so) and validates: the start node exists, every `next` exists, a choice
has at least one option, option ids are unique per choice, every condition
parses. A bad file is a `DialogueError` naming the node.

`enabled_if` is a `Condition`: a value store key, or `!key` for its negation.
A `Bool` is itself, a number is true when non-zero, text when non-empty, a
missing key is false. The store is `ValueStore` ([values.md](values.md)), so
a settings toggle, a `SetValue` from the game, or a mod's `set_value` all
unlock an option; with no store every option is enabled. A disabled option is
drawn as one, skipped by the gamepad walk, and refused by the runner if
activated anyway.

Loading:

```rust
// Through the asset server (hot reloads while the game runs):
fn load(mut dialogues: ResMut<DialogueAssets>, assets: Res<AssetServer>) {
    dialogues.load(&assets, "dialogue/greeting.dialogue.ron");
}
// Or by hand:
dialogues.register(Dialogue::from_ron(include_str!("greeting.dialogue.ron"))?);
```

`DialogueLoader` handles `*.dialogue.ron`; `apply_dialogue_assets` registers
every loaded or edited asset in `Dialogues`. An edit lands the next time the
dialogue starts; a running one keeps the version it started with.

## Starting, answering, driving

```rust
start_dialogue(&mut commands, DialogueId::new("demo:greeting"));
```

pushes `slotted:dialogue` as an overlay that takes focus (below) and enters
the start node. One dialogue runs at a time: starting another ends the running
one with `EndReason::Replaced`. `start_dialogue_with(commands, Arc<Dialogue>)`
runs one that was never registered. While one runs, `ActiveDialogue` is a
resource: the `Dialogue`, the current `node`, the screen `root`, whether the
current line is `revealed`, and the `history`.

The runner writes three messages:

| Message | When |
|---|---|
| `DialogueNodeEntered { dialogue, node }` | Every node, including the start and an `end`. |
| `DialogueChoice { dialogue, node, option, index }` | An option was picked, before the node it leads to is entered. |
| `DialogueEnded { dialogue, node, reason }` | `Finished` (an `end` node or `next: None`), `Cancelled` (`end_dialogue`, `Back` with `back_cancels`, or the screen closed from outside, a `clear_screens` say), `Replaced`. |

and takes five commands, every one a `Command` so a game observing a message
can answer in the same frame:

| | |
|---|---|
| `advance_dialogue` | On a line: reveal it if it is still typing, else go to `next`. On a choice or an end: nothing. |
| `choose_dialogue(id)` | On a choice: pick the option, if enabled. |
| `jump_dialogue(node)` | Go to any node of the running dialogue. The game's answer to a message: on `DialogueNodeEntered` for `ask`, jump to `secret` when the player has the key. |
| `end_dialogue` | End with `Cancelled`. |
| `open_history` | Push the transcript page. |

The example answers `yes` with a toast and nothing else:

```rust
fn thank_on_yes(mut choices: MessageReader<DialogueChoice>, mut commands: Commands) {
    for choice in choices.read() {
        if choice.option == "yes" {
            toast(&mut commands, ToastSpec::new("demo.menus.thanks"));
        }
    }
}
```

The messages carry strings and a `usize` and the commands take strings, so a
Lua bridge is a one-liner each; menus M4 adds it.

### Input

While the dialogue is the focus top (nothing pushed over it), the runner reads
the actions: a fresh `Accept` on a line advances it (skip the typewriter, then
continue); `Secondary` (`X`, West on a pad) opens the history; `Back` ends the
dialogue when `DialogueConfig::back_cancels` is on, and is otherwise left
alone, so Escape over a conversation pauses the game the way it does over
gameplay. On a choice `Accept` belongs to the focused option button, whose
`Activate` goes through `choose_dialogue`. A page or modal pushed above
(the pause, the history page) silences the runner until it pops, and the
typewriter counts virtual time, so a paused game freezes a half-typed line.

`DialogueConfig` is a resource with defaults: `kind` (the screen the runner
pushes, `slotted:dialogue`), `back_cancels` (`false`), `history` (`true`).

## The screen

`slotted:dialogue` is an overlay with `focus: true` ([screens.md](screens.md#presentation)),
no scrim, `slide_up`, `back: ignore`: it sits over gameplay and over whatever
page is open, under anything pushed after it. The panel is placed at the
bottom, `90%` wide up to 900 px, in the `dialogue` role:

| Id | What the screen does with it |
|---|---|
| `portrait` | A `dialogue.portrait` panel, `sizes.portrait` square, the image spawned inside; hidden when the node has none. |
| `speaker` | A `text` in `dialogue.speaker`, the node's speaker key; hidden when none. |
| `text` | A `rich_text` in `dialogue.text`: the line, typed through `RichReveal`; the prompt on a choice. Sits in a box with a three-line `min_height` so a short line does not bounce the panel. |
| `choices` | The column the option buttons spawn in, after `choice_delay`. Each button carries `DialogueOption { id, index }`, the tag `dialogue.option = <id>` and the test id `option.<id>`. |
| `hints` | A hint bar: `Skip` while typing, `Continue` when revealed, the focused option's `Select` on a choice, `History` and `Leave` when the config allows them. |
| `continue` | The `{key:accept} continue` caption, shown once a line is revealed. |

Anchors `header_end`, `text_end`, `choices_end` and `footer` are where a mod
injects. The strings are `slotted.menu.dialogue.*` (`continue`, `skip`,
`next`, `history`, `leave`) with English defaults in the crate.

### Your own template

Register a screen of kind `slotted:dialogue` before `MenuPlugin`'s
`PostStartup` and the crate's template stays out; or point
`DialogueConfig::kind` at any kind. The screen finds its nodes by the test ids
in the table, so a template that keeps an id keeps that behaviour and one that
drops an id (no portrait, say) drops it; a template with none of them still
runs, with nothing to show, so keep at least `text` and `choices`. Give the
presentation `focus: true` or the runner never hears an `Accept`.

### The typewriter

The theme's `dialogue` tokens pace it:

```ron
tokens: (
    dialogue: (chars_per_second: 40, choice_delay: 150),
)
```

A line reveals at `chars_per_second` glyphs of the resolved text per second
of virtual time; a `{key:..}` glyph or an `{icon:..}` is one unit, shown whole.
The layout is laid out in full from the first frame, with the unrevealed rest
transparent, so a paragraph never reflows while it types
([rich-text.md](rich-text.md#reveal)). `0` reveals every line at once, and so
does `Motion::reduced`. A choice's buttons appear `choice_delay` milliseconds
after the prompt (at once under reduced motion), and focus goes to the first
enabled one.

`sizes.portrait` (64) is the portrait's edge. The roles are `dialogue`,
`dialogue.speaker`, `dialogue.text` and `dialogue.portrait`; a theme that
adds the group should define all four ([themes.md](themes.md#roles)).

## History

Every `say` entered is recorded in `ActiveDialogue::history` as a
`HistoryLine { speaker, text }`, resolved through the catalogue at that moment
(a language switch does not rewrite what was already said). `open_history`
(or `Secondary`) pushes a `slotted:page` titled `History` whose body is the
transcript, `[b]speaker[/b]` over each line, narration without a speaker;
`transcript(&history)` builds the same markup for anything else. The page is
a page: it hides the dialogue while it is up and gives the focus back when
Done pops it.

## In a test

`slotted-test`'s `menu` feature adds the readers and drivers
([testing.md](testing.md#dialogue)):

```rust
h.start_dialogue("demo:greeting");                     // registered by hand or loaded
assert_eq!(h.dialogue(), Some((DialogueId::new("demo:greeting"), NodeId::new("hello"))));
h.advance(Duration::from_millis(300));
assert!(!h.dialogue_revealed() && !h.dialogue_text().is_empty());   // typing
h.dialogue_advance();                                  // Accept: the whole line
h.dialogue_advance();                                  // Accept: the choice
h.advance(Duration::from_millis(400));
h.settle();                                            // past `choice_delay`
assert_eq!(h.dialogue_options(), vec![("yes".into(), true), ("no".into(), true), ("secret".into(), false)]);
h.dialogue_choose("yes");
assert!(matches!(h.dialogue_events()[..], [.., DialogueEvent::Chosen(_), DialogueEvent::Entered(_)]));
assert_eq!(h.dialogue_history().len(), 2);
```

Two things to know. `stack_top()` skips overlays, so it is the page under the
dialogue, or `None`; `stack()` lists the dialogue and `dialogue()` says what
runs. And `settle()` runs frames until nothing changes, which lets the
typewriter finish a line; to watch one type, `step` or `advance` instead. A
`text_of` on the `text` node reads its own label, not the spans, which is why
`dialogue_text()` exists: it concatenates the spans painted with any alpha.
