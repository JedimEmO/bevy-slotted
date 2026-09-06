# Showcase notes, package B (the page)

What package B decided that `showcase-contract.md` left open, and the two
places it departed from the contract. Package A's notes are the sibling file.

## Departures from the contract

**The stacking breakpoints moved (section 5).** The contract asks for three
columns down to 1080 px, with the rail collapsing to 44 px icons and the right
column to 340 px below 1280. Those numbers do not add up: the canvas column is
`minmax(960px, 1fr)`, so three columns need

| Band | Rail | Canvas | Controls | Gaps and padding | Total |
|---|---|---|---|---|---|
| Full | 200 | 960 | 400 | 56 | 1616 |
| Icons | 44 | 960 | 340 | 56 | 1400 |

Below 1400 px the layout the contract describes cannot be laid out at all and
the page scrolls sideways, which is worse than stacking. So the bands are now
1620 and 1400 rather than 1280 and 1080. Everything else in section 5 stands:
the rail keeps only its numbers in the middle band, and below 1400 it becomes a
horizontal row of chips over a 52 vh canvas with the controls and the console
under it. The contract's intent, that the browser panel is never squeezed, is
better served by stacking early than by a horizontal scrollbar.

**Two exports the contract's table does not name.** Both were agreed with
package A rather than assumed:

- `browser_search(query)`, so the Browser scene's chips can type a query into
  the game's search field. The contract promises a Browser control block and
  the search grammar is the thing worth showing; without an export the block
  is a paragraph of prose.
- `restore_hud_layout("")` means "drop the stored layout and use the
  registered defaults", which is what the HUD scene's Reset button needs.

## What the page decided

**A missing export is a state, not an error.** `sceneCall` in `playground.js`
treats both shapes of "not built yet" the same: an export that throws
`not yet:` and an export that is not in the module at all, which JavaScript
reports as `is not a function`. Either one greys the control, writes
`<export> is coming up` in the panel's head, and remembers the name so a poll
cannot ask again once a frame. The granularity is per export, not per panel:
the Testing scene's Run buttons keep working while `replay_status` is a stub,
because a scene half built should be half usable. A scene the table reports as
`ready: false` greys its whole block instead.

**The head and the controls follow the rail whatever the module says.** Only
`set_scene` is held back for a scene that reports itself unready. A `?scene=`
link therefore lands on the right copy and the right controls even for a scene
whose Rust is not written, which is what lets the smoke test cover all eight
before package A has finished. If `set_scene` disagrees with the table, the
module wins and the rail entry is greyed on the spot.

**The token diff is generated, not hand-copied.** `tools/gen-theme-diff.py`
reads `assets/themes/*.theme.ron` and writes
`examples/web-playground/web/themes.js`, which the page imports. Run it when a
theme's tokens change; the output is checked in, so no build step depends on
Python. It reports the panel role, the radii scale, the blur radius, the slot
size, the motion scale, the hover delay and the two font families, which is
what actually differs between the three skins. Slot size is the default 44 px
in all three because none of them names a `sizes` block; the row stays because
its being the same is itself the point.

**`who: "net"` leaves the console, `who: "test"` is mirrored.** The contract
puts the multiplayer message log in its own pane; the page routes those lines
there and nowhere else. Test lines go to both, because the Testing scene wants
the pass marks and every other scene still wants the text.

**The HUD layout is polled, not pushed.** The drag that moves a layer happens
inside the canvas, so the page never sees it. While the HUD scene is open the
page asks `hud_layout()` once a second and writes the RON to `localStorage`
under `slotted.showcase.hud` when it changed. Every `localStorage` access is
wrapped: a browser with site data switched off still gets a working scene, it
just does not keep the layout past the tab.

**The rail is put back after boot.** Bevy focuses the canvas as it comes up and
the browser answers a focus by scrolling the element into view, which on a
stacked layout throws the rail off the top of the page before the visitor has
seen it. `keepTheRailInView` undoes that until the visitor scrolls somewhere
themselves.

**A CSS ordering trap, recorded because it cost an hour.** The two layout
media queries have the same specificity as the `.rail-list` rules they
override, so they have to come after them in the file, not beside `.split`
where the layout is otherwise described. Separately, an `auto` grid track
holding an item with `min-height: 0` collapses to zero when the rows overflow
their container, which is why the stacked rail's track is `max-content`.

## Verification

`examples/web-playground/tests/smoke.mjs` grew two modes. `--scene <id>` opens
one scene through its `?scene=` link, asserts the head strip, and captures it.
`--showcase` does that for all eight in one browser and switches to the next
one through the rail between shots, so every scene is entered both ways. Each
visit asserts that the scene table has eight entries, that the rendered title,
caption and three "what to try" lines are the ones the module handed the page,
that the rail entry is selected, and that exactly one control block is visible.
At the end it asserts that nothing but an expected `not yet:` reached the
browser's console as an error.

`just shot-showcase` is that mode pointed at
`examples/web-playground/shots/`. `just smoke` is unchanged and still green.

Screenshots were read back and three layout faults were fixed from them: the
theme table overflowed its column, the whole control panel greyed out when one
of its exports was a stub, and the stacked rail collapsed to nothing.
