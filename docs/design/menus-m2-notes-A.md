# Menus M2 notes: package A (foundation)

What A built over the skeleton, how the open choices were decided, what
changed outside A's own files, and what B, C and D need to know. Contract:
`docs/design/menus-m2-contract.md`, section 2.

## What is built

### Localisation fallback (2.1)

The skeleton already had `Localization { primary, fallbacks }` and
`slotted-packs` already called `set_primary_arc` on install, so the code
did not move. The test that was missing now pins it:
`crates/slotted-packs/tests/lifecycle.rs::a_pushed_fallback_survives_a_pack_install_and_a_reload`
pushes an English-defaults localiser before `run_all`, and checks that the
pack's layer wins for a key both define, that the fallback answers a key
the pack lacks, that `fallbacks.len()` is still one after the install, and
that a `reload_mod` keeps it too.

### Rewriting a template by id (2.2)

`ScreenDef::{set_text, set_tag, remove_node}` and `UiNodeDef::find_mut`
were in the skeleton; A tested them the way `confirm` will use them.
`crates/slotted-ui/tests/foundation_m2.rs`:

- `find_mut` finds the root by its own `test_id` and a `rich_text` two
  levels down inside a `scroll`; `set_text` takes a key with arguments;
  `set_tag` lands on a nested button and on the root and refuses an
  anchor; `remove_node` drops `accept_danger`, reports `false` the second
  time, and never removes the root.
- The same rewrite on a `Screens::resolve`d def of a screen that
  `inherits` the template, opened headless: the title reads the new key,
  the message's spans are `Delete ` / `Ravenholm` / `?` from
  `Delete [b]{world}[/b]?` with `world` as an argument, and the removed
  button is absent from the tree.

One thing to know: `find_mut` walks `children_mut`, which is `Panel`,
`SideTab`, `Scroll`, `Tabs` and `Custom`. A `Tabs` node's pages are its
children, so an id inside a tab page is reachable; the `TabDef`s
themselves are not nodes.

### Snapshot and restore (2.3)

Skeleton code, now tested in `crates/slotted-ui/tests/values.rs`: a
four-variant store serialises through RON as untagged values
(`"audio.master":40.0`, `"video.vsync":true`, `"ui.scale":2`), deserialises
to an equal map, and `restore` into a store carrying a clamping rule, a
refusing guard and an unrelated key bypasses the rule and the guard, keeps
the unrelated key (restore merges, it does not clear) and moves `version`
by exactly one with no `ValueChanged` or `ValueRefused`.

### Glyph sets (2.4)

- `button_glyph` has the four tables. Xbox `A B X Y LB RB LT RT ☰ ⧉`,
  PlayStation `✕ ○ □ △ L1 R1 L2 R2 Options Share`, Switch `B A Y X L R ZL
  ZR + −`, Generic Bevy's names (`South`, `DPadUp`). Sticks and the d-pad
  read `L3` / `D-pad ↑` in the three named sets. `Mode` is `Home`, `PS`
  and `Home`. `Auto` and `Keyboard` read as Xbox in this function alone;
  callers resolve `Auto` first and `key_glyph_text` honours `Keyboard`.
- `resolved_glyph_set(set, mode, &Query<&Gamepad>)` reads the first pad's
  `vendor_id()` and hands it to the new seam
  `resolved_glyph_set_for_vendor(set, Option<u16>)`: `0x054C`
  (`VENDOR_PLAYSTATION`) is PlayStation, `0x057E` (`VENDOR_SWITCH`) is
  Switch, anything else or no pad is Xbox. `mode` is accepted and ignored,
  so a keyboard-mode hint bar still names the pad's buttons for a gamepad
  row. Both constants and the seam are re-exported from `slotted_ui`.
- `key_glyph_text(.., set)`: in gamepad mode with `GlyphSet::Keyboard` it
  shows the keyboard binding (the contract's "text-only UI"); in keyboard
  and pointer mode the set is irrelevant.
- `render_rich_text` resolves the set once per run and passes it to every
  `{key}` span it spawns. `refresh_key_glyphs` re-renders every
  `RichKeySpan` when `InputMode`, `UiBindings` or `GlyphSet` is changed or
  a `GamepadConnectionEvent` arrived this frame (Bevy's
  `gamepad_connection_system` in `PreUpdate` has re-inserted the `Gamepad`
  with its vendor by then).
- `key_binding.rs`: the cell's spawn text resolves the set from the world
  (`glyph_set`), and `paint_key_bindings` repaints on the same two new
  triggers. A gamepad row under `GlyphSet::Keyboard` uses `Generic`
  rather than the keyboard binding: a row that exists to rebind the pad
  must name a pad button, and the keyboard key would be the other
  device's binding.
- Test: the same `{key:accept}` and the same gamepad `key_binding` row read
  `A` / `✕` / `B` under the three sets, `Enter` / `South` under
  `Keyboard`, flip to `✕` then `B` under `Auto` when a
  `GamepadConnectionEvent` with vendor `0x054C` then `0x057E` arrives on
  the harness pad, and a screen spawned while the Nintendo pad is
  connected starts in Switch. Bevy's mock does carry the vendor, through
  the connection event; the seam is tested on its own as well.

### `zbands::TOAST` (2.5)

Skeleton. Untouched.

### Select popup closes on a press outside it (2.7)

`widgets/select.rs::on_press_outside_select_popup`, a global
`Pointer<Press>` observer registered in `widgets/controls.rs::build`. It
acts only when `press.entity == press.original_event_target()`, because the
event propagates up `ChildOf` and a global observer runs once per hop. For
every open `SelectPopup` it checks whether the original target is the
popup, the popup's descendant, the select row or the row's descendant; if
none, it triggers `CloseSelectPopup` for that select. The row is excluded so
a press on the pill with the popup open does not close it on the press and
reopen it on the click that follows; the M1 behaviour there (a click on the
open row does nothing) is unchanged. Closing on the press rather than the
click means a click on another control both closes the popup and reaches
that control, which the test checks (focus lands on the other button).

Test: a press on the screen root's padding closes the popup before the
release and the next `Back` pops the screen; a press on the popup's own
padding leaves it open; a click on a sibling button closes it and focuses
the button. FOLLOWUPS M1 item 8 is closed.

### `UiHarness::stack_top()` (2.8)

In `crates/slotted-test/src/actions.rs` beside `stack()`, not in
`harness.rs`, so the stack readers sit together. It returns
`ScreenStack::top()`'s kind, which skips overlays, so a HUD layer never
reads as the top. Tested with two stacked screens popped one at a time.

## Decisions the contract left open

- **The seam exists even though the mock carries a vendor.**
  `resolved_glyph_set_for_vendor` is what `key_binding.rs` needs at spawn
  time anyway (it has a `&mut World`, not a `Query`), so it is not a
  test-only door.
- **`Auto` reads the first pad in query order.** With two pads of
  different vendors the set follows whichever Bevy spawned first. A
  "last pad pressed" rule needs the emitter to record the pad entity per
  action; noted as a follow-up.
- **`Keyboard` in a gamepad `key_binding` row is `Generic`**, see above.
- **The outside press excludes the select row.** The alternative, closing
  on the row press and suppressing the following click, needs state that
  outlives the press; a no-op click on the open row is what M1 shipped.

## Changes to shared files

- `crates/slotted-ui/src/lib.rs`: one added `pub use rich::{
  VENDOR_PLAYSTATION, VENDOR_SWITCH, resolved_glyph_set_for_vendor }`.
- `crates/slotted-ui/src/widgets/controls.rs`: one `add_observer` line.
- `crates/slotted-ui/src/widgets/text.rs`: `render_rich_text` gained
  `Res<GlyphSet>` and `Query<&Gamepad>` and resolves the set once.
- `crates/slotted-ui/src/widgets/key_binding.rs`: as above.
- `cargo fmt --all` reformatted B's and C's in-progress files as well;
  formatting only.

## Deviations from the skeleton's signatures

- `refresh_key_glyphs` takes `Res<GlyphSet>`, `Query<&Gamepad>` and
  `MessageReader<GamepadConnectionEvent>` in addition.
- `paint_key_bindings` takes the same three (with `too_many_arguments`
  allowed).
- `resolved_glyph_set_for_vendor`, `VENDOR_PLAYSTATION` and
  `VENDOR_SWITCH` are new public items in `rich.rs`.

## Tests

Eight added. In a worktree holding only A's changes over the skeleton:
`cargo test --workspace` 1151 passed, `cargo clippy --workspace
--all-targets -- -D warnings` clean, `cargo fmt --check` clean, `cargo check
--target wasm32-unknown-unknown -p slotted-ui` clean.

- `crates/slotted-ui/tests/foundation_m2.rs` (6): the confirm-style
  rewrite; the rewrite on a resolved def rendered headless; the four
  tables, the vendor seam and `key_glyph_text` under every set; the key
  run and binding cell following the set and the connected pad; the
  outside press; `stack_top`.
- `crates/slotted-ui/tests/values.rs` (1): the RON round trip.
- `crates/slotted-packs/tests/lifecycle.rs` (1): the fallback surviving an
  install and a reload.

In the shared checkout with B's and C's work in progress, `cargo test
--workspace --no-fail-fast` is 1180 passed, 4 failed, none in A's files:
`slotted-menu/tests/templates.rs` (two), `slotted-test/tests/harness.rs::
keyboard_focus_and_arrow_navigation_work` (Tab no longer wraps to the
first slot: something focusable was added to the graph) and
`slotted-test/tests/lua_tests.rs::a_test_reads_and_writes_the_value_store_and_types_into_a_field`
(likely the deleted `assets/screens/demo_settings.screen.ron`). All four
pass in the clean worktree, so they belong to B or C. Clippy in the shared
checkout fails on `slotted-menu` lints only.

`RUSTDOCFLAGS="-D warnings" cargo doc -p slotted-ui --no-deps` fails on a
pre-existing `crate::hud_editor` link in `hud.rs:439` when the crate is
documented alone (the module is feature-gated); `--workspace` unifies the
features and passes. Not A's, noted for D.

## Not done, and follow-ups (for D's "Menus M2" heading)

- **`Auto` follows the first pad, not the last one used.** Two pads of
  different vendors show the first's glyphs. Recording the pad entity on
  `UiActionEvent` and resolving from it is the fix; the emitter is the
  place.
- **A `GamepadConnectionEvent` re-renders every key span and cell** even
  when the resolved set did not change (an Xbox pad reconnecting). Cheap,
  and the alternative is a resource caching the last resolved set; not
  built.
- **The select row press is a no-op with the popup open**, unchanged from
  M1. A toggle wants the press to close and the click to be swallowed.
- **`docs/guide/rich-text.md` and `input.md` do not yet mention
  `GlyphSet`**; D's docs pass.
