# slotted-ui

Screens and widgets for [slotted](https://github.com/JedimEmO/bevy-slotted)
on `bevy_ui`.

A screen is data: a `ScreenDef` holding a tree of `UiNodeDef`s. Rust, RON and
Lua all produce the same tree, and `spawn_screen` is the one place that turns it
into entities. Every spawned node carries a `SemanticRole`, an optional
`SemanticLabel`, `Tags` and a `TestId`. That semantic layer is what `bevy_a11y`
announces and what `slotted-test` locates by.

The crate lays out and picks; it does not draw. It runs unchanged in the
headless harness.

## Main types

| Type | What it is |
|---|---|
| `ScreenDef`, `UiNodeDef`, `spawn_screen`, `Screens` | The screen tree, its 13 node types, the spawner and the registry. |
| `SemanticRole`, `SemanticLabel`, `Tags`, `TestId` | The semantic layer. |
| `AnchorId`, `AnchorNode`, `Injections` | Injection points and the patches other mods splice into them. |
| `Tooltip`, `TooltipPart` | Composed tooltips with a compact and an expanded tier. |
| `HudLayer`, `HudLayers` | HUD layers, their anchors and the dev-mode position editor. |
| `Recording`, `RecordedInput` | Input recording and replay. |
| `SlottedUiPlugin`, `SlottedUiSet`, `SlottedUiConfig` | The plugin and its frame order. |

The node types are `panel`, `slot`, `slot_grid`, `virtual_grid`, `text`,
`button`, `tank`, `bar`, `progress`, `side_tab`, `icon_button`, `viewport`,
`anchor` and `custom`. See
[`docs/guide/screens.md`](https://github.com/JedimEmO/bevy-slotted/blob/main/docs/guide/screens.md)
for the full reference.

## Example

```rust
use slotted_ui::ScreenDef;

let def = ScreenDef::from_ron(r#"(
    kind: "demo:chest",
    root: (
        type: "panel",
        role: "panel",
        layout: (direction: "column", gap: 2.0, padding: 2.0),
        children: [
            (type: "text", key: "demo.chest.title", style: "title"),
            (type: "anchor", id: "title_end"),
            (type: "slot_grid", inventory: 0, cols: 9, rows: 3, first: 0),
        ],
    ),
    listring: [0, 1],
)"#).expect("a valid screen");
assert_eq!(def.root.anchors().len(), 1);
```

## Feature flags

| Feature | Default | Effect |
|---|---|---|
| `dev` | no | Exclusion-zone highlighter, id tooltips, the HUD position editor and the input recorder. No extra dependencies. |
| `viewport` | no | Real cameras behind `viewport` nodes. Pulls in the renderer; off for headless tests and dedicated servers. |

## Licence

MIT OR Apache-2.0, at your option.
