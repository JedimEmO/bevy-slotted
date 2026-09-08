# Menus M3 notes: package A (foundation)

What A built over the skeleton, the choices it made, what changed outside
A's own files, and what B, C and D need to know. Contract:
`docs/design/menus-m3-contract.md`, section 2. No signature in the contract
changed; there is no v1.1 amendment.

Tests live in `crates/slotted-ui/tests/foundation_m3.rs` (nineteen of them;
names below). `cargo fmt -p slotted-ui -p slotted-theme` was run; nothing
outside A's ownership was reformatted.

## 2.1 Focusable overlays

`Presentation.focus`, `takes_focus()` and `ScreenStack::focus_top()` were in
the skeleton. What moved to `focus_top()`:

- `stack.rs`: `restore_focus` (so popping a modal over a `focus: true`
  overlay lands back on the overlay) and `enforce_focus_scope` (so focus
  under such an overlay is bounced into it).
- `nav.rs`: `focus_on_spawn`'s early return is `!takes_focus()`, and "on
  top" is `focus_top()`. A plain overlay still returns early, so
  `focus_ring.rs::an_overlay_never_takes_focus` passes unchanged.
- `tabs.rs`: the no-focus fallback ("the tabs of the top of the stack") is
  the focus top.
- `hint_bar.rs`: **nothing to change**. The bar never asked the stack for a
  top; its `root` is the screen the bar sits under, and its verb comes from
  the focused node. The one `ScreenStack` use there is `stack.is_changed()`
  as a rebuild trigger, which is right as it is. Noted here so the contract's
  "the hint bar's top" line is accounted for. (The M2 follow-up "overlay
  screens' bars say Back" is the label in `entries_for`; the dialogue
  template has `back: none`, so it shows no Back entry at all, and the label
  itself was outside A's edit permission.)

Unchanged, as the contract asks: `top()`, `top_any()`, `PopScreen`,
`pop_on_back` and `pause_on_menu`. `Back` over a focusable overlay pops the
page under it (or nothing); `Menu` still pauses.

Tests: `presentation_focus_defaults_to_mode_and_parses_from_ron`,
`a_focusable_overlay_over_nothing_gets_its_initial_focus`,
`a_modal_above_a_focusable_overlay_takes_focus_and_gives_it_back`,
`a_plain_overlay_is_skipped_both_ways`,
`back_over_a_focusable_overlay_pops_nothing`.

One thing for B and C: `ron::from_str` on a bare `Presentation` needs
`focus: Some(true)`; screen files carry `#![enable(implicit_some)]`, so
`focus: true` is right there.

## 2.2 `close_stacked`

Skeleton code, now tested:
`close_stacked_drops_an_overlay_under_a_modal_and_leaves_the_modal_alone`
(the modal keeps its focus and its one scrim, and slides down one z slot)
and `close_stacked_on_a_root_outside_the_stack_closes_it_like_close_screen`.

## 2.3 Rich text reveal

`render_rich_text` takes `Option<Ref<RichReveal>>` beside `Ref<RichText>`
and a `RemovedComponents<RichReveal>`, and re-renders a node when the
component changed or was removed (so `ALL` and no component draw the same,
and a typewriter that removes the component when it is done leaves the
full render behind). The runs are not re-parsed on a reveal change: the
existing `RichRuns` compare equal and stay.

Units are what the contract says, one per `char` of a `Text` run and one
per `Key` or `Icon` run; `RichReveal::units` sums them.

The reveal is painted, not laid out. Every run is spawned whole; a `Text`
run the cursor falls inside becomes two spans, the shown part in its colour
and the remainder in `Color::NONE` with the same `TextFont`. A `Key` or a
wrapped `Icon` run is one span, shown or transparent. An inline icon image
is `Visibility::Hidden` (still laid out). Bevy lays a paragraph out from
the concatenated span text, so a mid-word split produces the same line
breaks as one span; the tests pin the height and the width.

**`refresh_key_glyphs` and an unrevealed key**: the rule is that the reveal
lives in a span's colour and nowhere else, and `refresh_key_glyphs` writes a
span's text and nothing else. An unrevealed key span carries `RichKeySpan`
and `Color::NONE`; a binding change rewrites its text (`Enter` to `F`) and
it stays transparent; the next reveal render paints it. Nothing needs to
know about the other. This is documented on `render_rich_text`.

Tests: `units_count_chars_of_text_and_one_per_key_or_icon`,
`reveal_zero_shows_nothing_and_keeps_the_full_height`,
`reveal_mid_run_splits_the_run_into_a_shown_and_a_transparent_span`,
`a_key_run_flips_whole_and_stays_a_key_span_either_way`,
`all_and_a_missing_component_render_identically`,
`an_unrevealed_inline_icon_is_hidden_but_laid_out`.

For C and D: a `RichPart` span with alpha zero is the unrevealed part, so
`dialogue_text()` is "concatenate the `TextSpan`s under the node whose
`TextColor` alpha is above zero". The test helper `shown()` in
`foundation_m3.rs` does exactly that.

## 2.4 Text role override

`TextOpts.role` (skeleton) is honoured by both spawners:

- `spawn_text` puts `Themed(role)` on the node and, when a role was given,
  `ThemedFallback(style.role())` beside it. `ThemedFallback` is **new in
  `slotted-theme`** (`apply.rs`, re-exported): `apply_theme` paints the
  `Themed` role, or the fallback's material with one warning per paint when
  the theme lacks the role. A paint happens on spawn and on a theme change,
  so the warning is per node per theme, not per frame. The component is
  generic; any widget can use it.
- `spawn_rich_text` records `RichText.role` (a new field on the public
  struct; the only constructor is the spawner) and `render_rich_text`
  resolves the base paint from it, falling back to `style.role()` with one
  warning per node (`RichRoleWarned`, the `RichParseWarned` pattern).

One caveat that follows from `Theme::material`'s dotted fallback: a theme
that defines `dialogue` but not `dialogue.speaker` paints the speaker with
`dialogue`'s panel material (no text paint), because the dotted lookup
succeeds before the fallback is consulted. The three shipped themes define
all four, and a game theme that adds the group should add all four too. D
may want a line in `themes.md`.

`main_menu.screen.ron` uses `role: "menu.title"` and `role: "menu.version"`
on its two text nodes, closing that M2 follow-up.

Tests: `role_parses_on_a_text_node`,
`a_text_with_a_role_paints_that_role_and_a_missing_one_falls_back`,
`a_rich_text_with_a_role_paints_its_spans_in_that_role`.

## 2.5 Tokens and roles

`DialogueTokens`, `Sizes.portrait` and the four roles were in the skeleton;
`roles::ALL` is 105 and `every_shipped_theme_defines_the_ring_and_the_scrim`
holds. The materials were tuned from the skeleton's first pass:

- glass: `dialogue` tint `#141A24CC` (was `E6`; the screen draws no scrim,
  so a more translucent sheet lets the world through), edge `#FFFFFF2E`,
  portrait tint `#0B0F16D9` with the accent edge, speaker in the accent.
- neon: `dialogue` fill `#12141AF2` (the tooltip's near-black, so the cyan
  cut edge reads as the panel's line rather than a charcoal block), speaker
  lime display, portrait black with a magenta cut edge.
- paper: `dialogue` is `Solid(fill: "$sheet", border: "$rule", radius: 0.0)`
  with no elevation (the hairline rule, no ink shadow), speaker bold mono
  ink (the fonts have no small caps; weight stands in), portrait on the
  lighter `#FFFCF4` with an ink frame.

D looks at the shots; every value is a theme-file edit.

## 2.6 `set_text` reaches a button label

`ScreenDef::set_text` has a `Button` arm writing `ButtonOpts::label`. A
button label takes no arguments, so `args` is dropped for that arm (the doc
says so). `confirm::set_button_label` is one line: `def.set_text(id,
label.clone(), LocArgs::default())`. Test: `set_text_rewrites_a_button_label`
(the def, and the opened screen's label text).

## 2.7 Focus never leaks under a focusable overlay

`enforce_focus_scope` on `focus_top()` covers it; two tests pin it:
`a_focusable_overlay_without_a_focusable_clears_the_focus_below` (an overlay
with nothing focusable pushed over a page with a focused button: the same
frame, focus is `None`) and
`a_focusable_overlay_with_a_button_moves_the_focus_off_the_page` (focus
moves to the overlay's button and a stray move back is bounced).

## Outside A's files

- `crates/slotted-ui/src/stack.rs` line 146: a rustdoc link to the private
  `pop_entry` in the skeleton's `CloseStacked` doc failed `cargo doc` with
  `-D warnings`; it is plain code text now.
- `crates/slotted-menu/src/confirm.rs`: only `set_button_label`.
- `crates/slotted-menu/src/hint_bar.rs`: untouched (see 2.1).

## Gates

- `cargo clippy -p slotted-ui -p slotted-theme --all-targets -- -D warnings`
  and `RUSTDOCFLAGS="-D warnings" cargo doc -p slotted-ui -p slotted-theme
  --no-deps --features slotted-ui/dev` (the `dev` feature is what the
  workspace-wide doc build unifies in; without it the pre-existing
  `hud_editor` link in `hud.rs` is unresolved) are clean.
- `cargo clippy --workspace --all-targets -- -D warnings` is clean and
  `cargo test -p slotted-ui -p slotted-theme -p slotted-menu` is green
  (with B's `tests/dialogue.rs` at 24 passing at the time of the run).
- `crates/slotted-ui/tests/foundation_m2.rs::a_confirm_style_rewrite_reaches_nested_ids_and_the_root`
  pinned "a button is not a text node" for `set_text`; 2.6 reverses that,
  so its assertion now expects the label to be written.

## Deferrals

- The "one warning" for a missing text role is per paint (spawn, theme
  change), not per node lifetime, for plain `text`; `rich_text` warns once
  per node. Making plain text warn once per node needs a marker component
  in `slotted-theme`; not worth it until someone sees the log spam.
- The dotted-fallback caveat under 2.4.
