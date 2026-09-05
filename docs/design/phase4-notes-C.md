# Phase 4, package C: the modded example and the scripted harness

Companion to `docs/design/phase4-contract.md` section 3. Everything here is a place where the
contract, the package brief and the code that already existed did not agree, or where the contract
left a choice open. Packages A and B have their own notes.

## 1. The mods are three, not two

The contract names `copper_chest` and `appleskin_like`. The brief adds `sorter`, a third mod whose
only job is to put a button on a screen it does not own. That is the case worth showing: `sorter`
never sees the copper chest's tree, it names the anchor `title_end`, and the host splices the node
in when the screen spawns. Injection plus `widget_activate` plus `slotted.cmd.sort` is the whole
mod, in eleven lines of Lua across two files.

`sorter/mod.toml` declares a required dependency on `copper_chest` so the load order is stable,
even though injections are collected regardless of order.

## 2. `copper_chest` registers two items and a 3x3 recipe type

The contract has one item (`copper_chest:copper_chest`, tag `c:storage`) and a 2x2 recipe type; the
brief asks for a `copper_chest:copper_ingot` item, a `c:chests` tag and a recipe of eight ingots.
Eight ingots do not fit a 2x2 grid, so:

- `copper_chest:copper_chest` keeps its contract id (the contract's own test text names it),
  gains `rarity = "uncommon"` and is in both `c:storage` and `c:chests`.
- `copper_chest:copper_ingot` is new, and joins `c:ingots`, a tag `assets/data/demo/tags/ingots.ron`
  already owns. That is the one line in the example that proves tags merge across a mod boundary.
- `copper_chest:assembly` is 3x3 and the recipe is a ring of `#c:ingots`.

## 3. `appleskin_like` gained a `control.lua`

The contract calls it data-only. The brief wants the "restores N" line to come from a script. Both
now exist and they show the two halves of the tooltip API: `data.lua` adds a static part filtered by
the tag `c:foods`, and `control.lua` answers `tooltip_build` with a per-item line, because a control
script sees the stack but not the tag index. The static line is `appleskin_like.food`; the dynamic
one is `appleskin_like.food.apple`.

## 4. The console toggles on `F1` and `F8`

The contract says `F8`, the brief says `F1`. Both are bound; neither is a key `slotted-ui` uses.
`--no-console` starts it hidden, which is how `shots/modded.png` is captured without the overlay
covering the chest.

`ScriptLogs` is a resource, but a `ModFailed` is a message and lives two frames, so the example
keeps its own `ConsoleErrors` list. Errors are drawn first and in red, then the last log lines, up
to `CONSOLE_LINES`.

## 5. `load_mods` still takes a `PackLayout`

The brief's sketch is `h.load_mods()` with the directory coming from the builder. The contract's
signature is `load_mods(&mut self, layout: PackLayout)` and section 4 says signatures change only by
amending the contract, so it is unchanged. What the builder's new `mods_dir(path)` gives is
`UiHarness::mod_layout()`, which returns a layout over the staged copy:

```rust
let mut h = UiHarness::builder().plugins(SlottedPlugins::headless()).mods_dir(dir).build();
let layout = h.mod_layout();
h.load_mods(layout);
```

`mods_dir` copies the directory to a temporary one **at build time**, not on the first
`edit_mod_file`. A lazy copy would be invisible to a `PackLayout` already inserted by an earlier
`load_mods`, so the reload tests would have read the repository's files. The copy is removed when
the harness drops.

## 6. `mod_errors` reads two sources

The contract lists a `ModErrors` resource; the skeleton's comment said the harness keeps the
`ModFailed` messages. Package B has since added `slotted_packs::lifecycle::ModErrors`. The harness
returns the union: `ModErrors` if the resource exists, plus every `ModFailed` message it has seen
since it was built (a system it adds to the `Last` schedule the first time mods are loaded), minus
duplicates. A test can therefore assert on a failure that happened many frames ago.

## 7. Open questions for the contract

- **A control script that does not compile.** Section 2.6 step 1 says a script error keeps the
  previous registries and stops, but a `control.lua` syntax error is only found in step 3, after the
  freeze. Whether the previous control script stays loaded is not stated.
  `a_broken_script_is_reported_and_the_old_one_stays` therefore asserts only that the failure
  reaches `mod_errors()`, that inventory state and conservation survive, and that no handler from
  the broken file runs. It passes against B's implementation; tighten it once the rule is written
  down.
- **No icon node, and `UiNodeDef::Button` has no label.** The brief asks for an icon button at
  `title_end`. `UiNodeDef` has no icon variant outside `SideTab`, and `spawn_node` calls
  `spawn_button(.., None)` for a `Button` node, which spawns no text child at all, so the injected
  button rendered as an empty pill. The injection is a `Custom { kind: "slotted:button", params:
  { label: "Sort" } }` instead. That loses the mod's own `WidgetKind` from the `widget_activate`
  event, so `control.lua` filters on the node's `sorter_action` tag rather than on `widget`. Either
  `UiNodeDef::Button` should carry a label, or `slotted:button` should take a `LocKey`.
- **Base data is not in the load order.** `DataStage::new(order)` is built from the mod ids, so
  `assets/data/demo/` is not loaded by the example and `minecraft:*` items do not exist in it. The
  seeded inventories are made entirely of mod items, which is a better demonstration anyway, but it
  means the modded example cannot reuse `chest::inventories`.

## 8. Fluent message ids cannot contain a dot, and packs now normalises

Contract 2.7's convention key `copper_chest.item.copper_chest` is not a legal Fluent identifier
(`[a-zA-Z][a-zA-Z0-9_-]*`), and one bad id rejects the whole file, so every `locale/en-US.ftl` here
failed to parse with "Expected a token starting with =". The mods used a dash form for a while.

Package B has since made `locale.rs` rewrite the identifier of each definition when a layer is
parsed and try both forms on lookup, so the mods are back on the contract's dotted keys and the
example is what the prose describes. A modder may write either form in either place.

## 9. Item display names are not localised anywhere yet

**Resolved in integration** by the `Localizer` port in `slotted-ui`, which `slotted-packs` fills
from the same `LocaleTable` the `Locales` resource holds and `IngredientCtx` carries into the
browser. A card, the search index and a recipe page title all read it.
`a_card_shows_the_name_from_the_mods_ftl` pins it. The browser's own chrome is still English; see
`docs/FOLLOWUPS.md`. The rest of this section is the diagnosis as it was found.

`ItemIngredientType::display_name` in `slotted-browser` returns `ItemDef::display_name` verbatim as
the card's text, and the same string feeds the search index. Nothing resolves it through `Locales`,
so a browser card for a mod item reads `copper_chest-item-chest` rather than "Copper Chest". Phase 3
never noticed because `assets/data/demo` uses literal display names.

The mods here keep localisation keys, which is what `ItemDef::display_name` is documented to accept,
and `shots/modded.png` shows the raw keys in the browser as a result. It is a Phase 3/4 seam rather
than anything the example can fix: resolving it needs `Locales` inside the ingredient type. Reported
to package B.

## 10. Recipe categories, and where binding them belongs

`slotted-browser` binds a default category to each unclaimed recipe type in `validate_categories`,
a `Startup` system. In the windowed example that is after packs' `PreStartup` load and works. It is
wrong for a harness, which loads mods after `build`, and for a hot reload, which can introduce a
recipe type at any time: with no category, `RecipeStore` drops every recipe of that type and the
recipe key over a mod item's card opens nothing.

The harness carried a stopgap that re-ran `bind_defaults`. Package B has moved the call into the
control stage beside the icon re-bake and the browser rebuild of contract 2.3 step 4, so the stopgap
is gone and `slotted-test` knows nothing about categories again.

## 11. Tests

All eight tests in `examples/modded/tests/mods.rs` pass against packages A and B and the `#[ignore]`
markers are gone. Two locator details worth keeping: the seeded inventories put the same item in
three inventories, so a slot locator has to name `region = "chest"` and an index, and the browser's
recipe hotkey is a plain letter, so a test must `blur_search()` after typing.

`shots/modded.png` and `shots/modded-console.png` are captured from `just shot-modded`.
