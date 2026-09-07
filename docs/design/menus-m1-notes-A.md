# Menus M1 notes: package A (typography, rich text, localisation arguments)

What A built, how the open choices in the contract were decided, what changed
outside A's own files, and what B, C and D need to know. Contract:
`docs/design/menus-m1-contract.md`, sections 1.4, 2.1 to 2.4.

## What is built

### `slotted-theme` (1.4, 2.1)

- `Paint` gained `text_line_height`, taken from the type style a `$name`
  size names. `apply_theme` inserts Bevy's `LineHeight` on every text role:
  `RelativeToFont(x)` when the style has one, `LineHeight::default()` when it
  does not, so a role swap never keeps the previous role's metric. That is
  the same rule weight and shadow already followed.
- Three public helpers on `Paint`, for anything that paints text the apply
  system cannot reach (a `TextSpan` carries no `Themed`): `Paint::for_role`
  (material through the dotted fallback, then `from_material`),
  `Paint::text_font(assets)` (the `TextFont` with the theme's font loaded
  through the server, the size and the weight) and `Paint::text_color`.
  `paint_image_and_text` now goes through `text_font` too, so the two paths
  cannot disagree. `FontPaint` and `TypeStyle` are re-exported.
- The three theme files: `text`, `text.muted`, `panel.title` and `count`
  now read the scale (`$body`, `$caption`, `$title`, `$caption`), so every
  role a `TextRole` maps onto is a `$` reference. `body` carries a
  `line_height` (glass 1.4, paper 1.45, neon 1.3), which is what exercises
  the apply path in the shipped themes. Consequences, in px: `text` 13 →
  15/14/15, `text.muted` 11 → 12/11/12, `panel.title` 15/15/17 → 24/22/26,
  `count` 11/11/12 → 12/11/12. The contract's scale says `title` is 24, and
  `TextRole::Title` is `panel.title`; the chest header is the visible change.

### `slotted-ui/src/rich.rs` (2.2)

- `parse`: a single pass over bytes with a style stack. `[b]`, `[i]`,
  `[color=$name|#RRGGBB|#RRGGBBAA]`, `[size=name|px]`, `{key:action}`,
  `{icon:ns:path}`. `[[`, `]]`, `{{`, `}}` are literals; the contract names
  only the opening pairs, but `[[b]]` reading as `[b]` needs the closing
  pair too, and `escape` doubles all four. Errors carry the byte offset of
  the offending bracket: `Unclosed` (innermost open tag at end of input),
  `Stray`, `UnknownTag` (also a malformed colour or size value, and a `[`
  with no `]`), `UnknownPlaceholder` (an unknown action or item, an
  unsubstituted argument, a `{` with no `}`). Text runs of one style are
  merged, including across an empty tag pair.
- `substitute` replaces `{name}` from `args` with the value escaped, leaves
  `{{` alone and leaves a name with no argument for `parse` to report.
  `value_text` is the Fluent-free rendering of a `Value`.
- `key_glyph_text`: keyboard and pointer modes show the first key
  (`Enter`, `Space`, `Esc`, `↑`, `A`, `F5`, `PgUp`, `Num 3`), gamepad mode
  the first button in Xbox-style names (`A`, `B`, `X`, `Y`, `LB`, `RT`,
  `L3`, `D-pad ↑`, `Start`). Pointer mode shows the keyboard binding
  because a mouse has none. An unbound action shows its own name.
  `key_glyph` and `button_glyph` are public for B's `key_binding` cell.
- `refresh_key_glyphs` rewrites the `TextSpan` of every `RichKeySpan`
  (a marker the renderer puts on the span of a `Key` run) when `InputMode`
  or `UiBindings` is changed. No re-parse, no other span touched.

### `slotted-ui/src/widgets/text.rs` (2.1, 2.2)

- `spawn_text`: `TextLayout::new(justify, linebreak)` from `align` and
  `wrap` (`NoWrap` when `wrap: false`), `LocText::with_args`, and a
  `MaxLines` component when `max_lines` is set.
- `spawn_rich_text` spawns the root with a `RichText` component (key, args,
  base style, `inline`, the layout). Wrapped: the root is a `Text` with
  empty content and a `TextSpan` child per run. Inline: the root is a flex
  row (`align_items: center`, `column_gap: spacing.xs`) with no `Text`; the
  renderer puts `Text` fragments (`NoWrap`, `Themed(role)`) and `ImageNode`
  icons under it in run order. Every spawned entity carries `RichPart`, so a
  rebuild despawns exactly those.
- `render_rich_text` (`Render`, chained before `refresh_key_glyphs`) runs
  for an added or changed `RichText`, and for all of them when
  `Localization`, `ActiveTheme` or `Assets<Theme>` changes (the theme is an
  asset that lands after the first spawn in a real app). Order: the key is
  resolved with the arguments, whose `Text` values are escaped first so a
  Fluent `{ $name }` cannot open a tag either; then `substitute` for the
  markup's own `{name}`; then `parse`. A parse error renders the whole
  markup as one raw run and warns once per node (`RichParseWarned`). Spans
  resolve their own font and colour: `Paint::for_role` of the base role,
  `text.key` and `text.icon`; `[b]` sets weight 700, `[i]` sets
  `FontStyle::Italic`, `[color]` resolves through the palette, `[size]`
  through the scale (size only, never the style's font or weight). With no
  theme loaded the spans get Bevy's defaults and are rebuilt when it lands.
  `{icon:x}` wrapped is the item's `display_name` through the catalogue
  (the browser's convention), else the namespaced id; inline it is an
  `ImageNode` from `IconImages` in a square of the base style's line
  height. `RichRuns` on the root is kept in step for inspection.
- `enforce_max_lines` (`Render`): Bevy has no max-lines and a layout pass is
  the only way to count lines, so it converges over frames. `MaxLinesState`
  holds the whole string, what is shown and the parent's content width. A
  fresh `Text` (anything that differs from `shown`) is laid out whole; when
  the laid-out count (`ComputedTextBlock::buffer().lines()`) exceeds the
  limit, the shown string is cut in proportion (`len × max / lines`, at
  least one character) and `…` appended. Cutting is monotone, so it
  terminates, and a paragraph settles in two or three frames. The whole
  string comes back only when the parent's content width grows, the one
  event that can make more fit; the parent's width rather than the node's,
  because a truncated node shrinks to its content and would loop. A layout
  that lands on its own (a font arriving, the parent shrinking) is checked
  too; a verified string is otherwise never touched, which is what keeps it
  cheap.

### Localisation arguments (2.3)

The skeleton already carried `Localizer::resolve(key, args)`, `LocText
{ key, args }` and `LocaleTable`'s `FluentArgs` conversion; A exercised
them. The browser's status line now resolves one key,
`browser.status.count`, with `count` as an `Int` argument, falling back to
the English `n item` / `n items` when no catalogue defines it.
`assets/locale/en-US.ftl` carries the Fluent plural selector;
`browser-status-item` / `-items` and `keys::STATUS_ITEM` / `STATUS_ITEMS`
are gone.

## Decisions the contract left open

- **`RunKind::Arg` does not exist.** The skeleton dropped it and A agrees:
  arguments are substituted before parsing, so the parser never sees one.
  An unsubstituted `{name}` is an `UnknownPlaceholder`, which renders the
  markup raw and warns, so a missing argument is visible.
- **Escaping is symmetric.** `]]` and `}}` are literals beside `[[` and
  `{{`; an escaped value round-trips through the parser exactly.
- **`[size]` changes the size only.** A `$name` size on a *material* brings
  the style's font and weight (1.4); a `[size=heading]` inside a paragraph
  keeps the paragraph's font and weight, since a run that switched font
  mid-sentence would read as a different paragraph.
- **The inline root is a row, not a `Text`.** Bevy has no inline image in a
  text block, so `inline: true` gives up on wrapping (the contract calls it
  "a single-line run") and lays fragments and images out as flex items.
  `text_of(root)` on an inline node is the semantic label; a test reads the
  fragments' spans.
- **Line height on every text role.** `None` in the paint inserts the Bevy
  default rather than leaving the component alone, for the reason above.
- **Pointer mode shows the keyboard glyph.** The alternative, hiding the
  glyph, leaves a footer that says "Press  to go".

## Changes to shared files

- `slotted-ui/src/lib.rs`: two added `pub use` lines (`RichKeySpan`,
  `button_glyph`, `key_glyph`; `MaxLinesState`, `RichPart`, `RichText`,
  `render_rich_text`). `plugin.rs` is untouched; `render_rich_text` is
  registered from `rich::build`, chained before `refresh_key_glyphs`.
- `slotted-theme/src/lib.rs`: `FontPaint` and `TypeStyle` re-exported.
- The three theme files: the four role lines and the `body` line named
  above; nothing else.
- `slotted-browser/src/ui/panel.rs` and `status_line.rs`: the count key.
- `slotted-browser/src/ui/dock.rs` (nobody's this milestone, so noted
  here): `choose` docks right unless the left strip is wider by more than
  `SIDE_TIE` (2 px). A centred screen of odd content width leaves strips a
  pixel apart, and the paper theme's title font made the chest panel 603 px
  wide, which flipped the browser to the left and failed the chest
  example's "the screen tree is theme independent" test. The tie-break is
  the fix, not the font: a theme must not choose the dock side. A unit test
  pins it.
- `cargo fmt --all` at the end of the pass reformatted B's and C's
  in-progress files in the shared checkout as well; formatting only.

## Existing tests changed

- `slotted-theme/src/apply.rs::text_maps_to_colour_and_size`: `count` is
  12 px in glass now (`$caption`), was 11.
- `slotted-browser/tests/locale.rs`: the `Fake` localiser substitutes
  `{ $name }` from the arguments, and the status test defines
  `browser.status.count` as `{ $count } Gegenstände`.
- `slotted-packs/tests/browser_chrome_locale.rs`: the temp `.ftl` and the
  base-file check use the count key with a plural selector, asserted with
  `count` 1, 7 and 0, 1, 42.

## Deviations from the skeleton's signatures

- `refresh_key_glyphs(mode, bindings, spans: Query<(&RichKeySpan, &mut
  TextSpan)>)`: the `MessageReader<InputModeChanged>` and the
  `(Entity, &RichRuns, &Children)` query are gone; `Res::is_changed` on the
  mode and the bindings covers both triggers, and a marker on the span
  beats walking children and runs in parallel.
- `enforce_max_lines` takes `Commands`, `Ref<ComputedTextBlock>`, the
  optional `ChildOf` and `MaxLinesState`, and a parent `ComputedNode` query.
- `render_rich_text` is new, not in the skeleton.

## Tests

Nineteen added, all green with `cargo test --workspace` (1121 tests in the
shared checkout with B's and C's work in the tree; 1076 in a clean
worktree holding only A's changes over the skeleton), `cargo clippy
--workspace --all-targets -- -D warnings`, `cargo fmt --check` and
`cargo check --target wasm32-unknown-unknown -p slotted-ui -p
slotted-browser`.

- `crates/slotted-theme/tests/typography.rs` (6): every theme defines the
  six steps in descending size naming fonts that exist; every `$` size in
  every theme resolves through `size`, `type_style` and `Paint::for_role`;
  the ten `TextRole` roles are all `$` references; a dangling reference is
  reported and paints at 13 px; the three themes still define every role
  with no dangling font or palette reference; a reference carries the
  style's font and weight unless the material overrides them.
- `crates/slotted-theme/src/apply.rs` (1): a type style's line height
  reaches the paint and the `TextFont` / `TextColor` helpers agree with the
  material.
- `crates/slotted-ui/tests/rich_text.rs` (13). Parser: every tag, nesting,
  the four escapes, run merging, every error with its offset, an argument
  value containing `[b]{key:back}` that stays literal. Render (glass theme
  added as an asset directly): span count, per-span weight, italic, colour
  and size; `{key:accept}` reads `Enter` in pointer mode, `A` after a pad
  press, `Enter` after a key, `F` after rebinding, in the `text.key` role;
  `{icon}` is the display name in `text.icon` when wrapped and an
  `ImageNode` at line height between two text fragments when inline; a
  catalogue string with tags and `{ $name }` arguments renders and re-renders
  when the catalogue is replaced; bad markup renders raw and a `text` node
  keeps tags literal; a `text` node resolves with arguments, centres, and
  re-resolves when `LocText.args` change; a paragraph in a 160 px panel
  wraps to four or more lines, `max_lines: 2` truncates with `…` to a prefix
  and grows back when the panel triples in width, `wrap: false` stays one
  line wider than the panel; both nodes carry their kinds and appear in the
  screen tree.
- `crates/slotted-browser/src/ui/dock.rs` (1): a one-pixel wider left strip
  still docks right.

A note on headless text: a `.theme("glass")` harness in `slotted-ui`'s own
tests never loads the theme (the crate has no `assets/` directory, so the
handle fails rather than loads), and a theme added directly as an asset
still cannot load its font files. The render tests therefore add the theme
directly and assert paints, and the wrapping test runs with no theme at
all, on Bevy's default font, which is the only one that lays out there.

## Not done, and follow-ups (for D's "Menus M1" heading)

- **`max_lines` is a `text` node feature only.** The contract's `rich_text`
  row has no `max_lines` (1.3), and `TextOpts` carries it for both; on a
  `rich_text` it is ignored. Truncating spans would need the cut to land
  inside a run and keep the tail's styles; not built.
- **Truncation is proportional, not exact.** The cut keeps
  `len × max / lines` characters, which can land a few words short of the
  longest prefix that fits. An exact fit needs Parley's line breaks for a
  candidate string without a layout pass; `TextPipeline` has no such entry
  point in 0.19.
- **Inline icons do not re-resolve when the icon source changes.**
  `reresolve_icons_on_source_change` covers `ItemView`s; a rich text's
  `ImageNode` is rebuilt only when the node, the catalogue or the theme
  changes. Re-rendering on `Icons` change is one more `is_changed` in
  `render_rich_text` once the resource's change semantics are known.
- **A Fluent message reference in a rich string renders raw.** Fluent
  formats an unknown `{name}` reference as `{name}` with an error, which the
  parser reports as an unknown placeholder and renders raw. Correct, and
  logged once, but a translator who meant an argument and forgot the `$`
  sees the whole footer in markup.
- **A shadow is per paragraph, not per span.** Bevy reads `TextShadow` on
  the text root, which `apply_theme` paints from the base role; a `{key}`
  run inside a `count` paragraph gets the count's shadow, and a rich
  paragraph in a role without one has none anywhere. Nothing to do unless
  a direction wants a shadow under glyph keys alone.
- **The chest header grew.** `panel.title` follows the scale now (24/22/26
  px). D's snapshots exclude sizes, so nothing pinned it; if the moodboard
  wants the old 15 px header, that is a `title` step change in the theme
  files, not a code change.
