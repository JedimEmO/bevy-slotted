# Rich text

A `rich_text` node is a paragraph whose string carries markup: bold and italic
runs, a colour or a size from the theme, the glyph of whatever key an action is
bound to, and an item's icon. The string comes out of the locale file, so a
translator writes the tags and the screen file names only the key.

```ron
(type: "rich_text", key: "settings.footer", style: "caption"),
```

```fluent
settings-footer = {key:back} back · {key:tab_next} next tab
```

A plain `text` node shows the same string with the brackets in it. The tags
mean something only on a `rich_text`.

## The markup

| Tag | Renders |
|---|---|
| `[b]…[/b]` | Bold: the run's `TextFont.weight` is 700. |
| `[i]…[/i]` | Italic. |
| `[color=$accent]…[/color]` | A palette colour. `#RRGGBB` and `#RRGGBBAA` literals work too. |
| `[size=heading]…[/size]` | The size of a typography step, or a number in pixels. Size only: the paragraph keeps its font and weight, since a run that switched font mid-sentence would read as a different paragraph. |
| `{key:accept}` | The glyph text of the action's current binding, in the `text.key` role. |
| `{icon:demo:chest}` | The item's localised name in the `text.icon` role, or its icon as an image when the node is `inline`. |
| `{count}` | A Fluent argument, from the node's `args`. |
| `[[`, `]]`, `{{`, `}}` | A literal bracket. |

Tags nest, and a run's style is the union of the tags around it. Text runs
that end up with one style are merged.

Errors are reported with the byte offset of the offending bracket: a tag
opened and never closed, a `[/b]` with no `[b]`, a tag or placeholder the
parser does not know, a `[` with no `]`. A paragraph with an error renders its
whole markup raw and logs one warning per node, so a broken string is visible
on screen rather than silently blank. `slotted_ui::rich::parse` is the parser
if you want to check a string yourself; `escape` doubles the four brackets.

## What happens in what order

1. The key resolves through `Localization` with the node's `args`. Every
   `Text` argument is escaped first, so a player name containing `[b]` cannot
   open a tag from inside a Fluent `{ $name }`.
2. `{name}` placeholders left in the markup are substituted from `args`, also
   escaped.
3. The result is parsed. A `{name}` with no argument behind it is an unknown
   placeholder, which is the error above; a Fluent message reference written
   without the `$` looks the same, so the whole footer renders raw and the
   log says why.

Because localisation runs first, a `.ftl` file may contain any of the tags,
and a translation may bold a different word than the source did. The square
brackets pass straight through Fluent. The braces do not: `{` opens a Fluent
placeable, so `{key:back}` written bare is a Fluent syntax error that drops
the whole file. Write a rich placeholder as a Fluent string literal, which
resolves to the braces the parser wants:

```ftl
demo-settings-footer = Press {"{key:back}"} to close, {"{key:tab_next}"} for the next tab
```

## Key glyphs

`{key:action}` shows the first binding of the action for the current
`InputMode`, through `slotted_ui::key_glyph_text`:

| Mode | `{key:accept}` | `{key:back}` | `{key:up}` |
|---|---|---|---|
| `Keyboard` | `Enter` | `Esc` | `Up` |
| `Gamepad` | `A` | `B` | `D-pad Up` |
| `Pointer` | `Enter` | `Esc` | `Up` |

Pointer mode shows the keyboard binding, because a mouse has none and a footer
that says "Press  to go back" helps nobody. Gamepad buttons are named for the
resolved `GlyphSet` ([input.md](input.md#glyph-sets)): Xbox-style by default
(`A`, `B`, `X`, `Y`, `LB`, `RT`, `L3`, `Start`), `Cross`, `Circle`, `L1` and
`Options` on a PlayStation pad, and the Nintendo names on a Switch one; an
unbound action shows its own name. The span re-renders when the mode flips and when `UiBindings`
changes, so a `key_binding` row that rebinds `Accept` updates every footer on
the screen the same frame. `key_glyph(KeyCode)` and `button_glyph(button)` are
public for anything else that wants the same spelling.

## Icons

Wrapped (the default), `{icon:ns:item}` is the item's display name in the
`text.icon` role: text can wrap, and a name reads aloud. With `inline: true`
the node is a single-line row instead of a `Text`: text fragments and
`ImageNode` icons sit side by side as flex items, each icon a square of the
base style's line height. Inline gives up wrapping, which is the price of a
real picture in a sentence; use it for a hint bar, not a paragraph.

## Reveal

`RichReveal` on a `rich_text` node shows the first `n` units of the paragraph:
`RichReveal(Some(n))`, or `RichReveal::ALL` (the same as no component). A unit
is one `char` of a text run, and a `{key:..}` or `{icon:..}` run is one unit,
shown whole or not at all; `RichReveal::units(&runs)` counts what a full
reveal needs. The reveal is painted, not laid out: every run is spawned in
full, and the unrevealed rest of a run is a second span in the same font with
a transparent colour (an inline icon is `Visibility::Hidden`, still laid out),
so the paragraph keeps its line breaks and its height while it types. A
change to the component re-renders the spans without re-parsing the markup.
The dialogue's typewriter ([dialogue.md](dialogue.md#the-typewriter)) writes
it every frame; anything else that wants a line to appear gradually can too.

## Styling

Every span resolves its own font, size and colour: the base role for the node's
`style` (`text.caption` for `caption`), `text.key` for glyphs, `text.icon` for
item names, then the tags on top. A theme therefore styles glyphs once, in
`text.key`, rather than per screen. A text shadow is per paragraph, from the
base role, since Bevy reads `TextShadow` on the root.

`max_lines` is a `text` feature and is ignored on a `rich_text`: truncating
spans would need the cut to land inside a run and keep the tail's styles.

## In a test

`text_of(root)` on a wrapped node is the semantic label; the rendered runs are
the `TextSpan` children, and `RichRuns` on the root holds the parsed runs for
inspection. A revealed prefix is the spans whose `TextColor` has any alpha,
which is what `UiHarness::dialogue_text()` concatenates. The settings demo's tests read the footer's spans to assert that
`{key:back}` says `Esc` on a keyboard and `B` on a pad.
