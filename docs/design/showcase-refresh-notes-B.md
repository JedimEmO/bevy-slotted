# Showcase refresh, package B notes

The page side of `docs/design/showcase-refresh-contract.md`: the two new
control blocks, settings persistence, the driver's Menus and Dialogue laps,
the shots, the guide and the README. Built against the skeleton's stubs
first, then rerun against package A's module once
`docs/design/showcase-refresh-notes-A.md` appeared. Everything below is
verified against the real module unless it says otherwise.

## What was built

**Control blocks** (`web/index.html`, `web/playground.js`). Menus: Title,
Pause and Settings buttons through `menu_open`, a "saved settings" readout
with the RON size, a Reset button through `restore_settings('')`. Dialogue:
a Talk again button, a "found the key" switch that writes `demo.found_key`
through `set_value` as a JSON `true`/`false`, a transcript pane fed by every
console line whose `who` is `dialogue`, cleared on entering the scene, with
its own Clear button. Both blocks hand the focus back to the canvas after a
press, because the game hears the keyboard through the canvas and a focused
page button would take the next Enter for itself.

**Settings persistence.** `localStorage["slotted.settings"]`. The page copies
`settings_ron()` into it on the console poll's cadence, at most once a
second, on every scene rather than only in Menus, because the settings
screen is reachable from Themes as well. The guard is the HUD layout's: only a
non-empty read is written, so a store that is empty for a frame after a reset
or a restart cannot wipe the key, and only Reset clears it. `restore_settings`
is called once at boot, after `module.default()` returns and before Bevy's
first animation frame, and again after every restart, after `restore_state`.

**Quiet stub handling.** The settings store is asked on every scene, so a
`not yet:` from it would have written its note on whatever block was open.
`sceneCall` grew a `quiet` option that records the stub without touching the
page, and `remarkNotYet` re-says what is missing when the block that owns the
control comes on screen. `renderControls` calls it for a ready scene only; a
scene the table says is not ready keeps the whole-block `coming up`.

**Keyboard.** The page's `keydown` handler only claims `Ctrl+Enter` and
`Alt+1..9`; Enter, Esc, X and the arrows reach the canvas untouched. Checked
by the driver, which walks the menus and the dialogue with CDP key events.

**Driver** (`tests/smoke.mjs`). A `key()` helper next to `mouse()`, a
`stubbed()` probe that goes past the page's wrappers to the raw export, a
`skip()` verdict, `stubOrFail()` that skips on `not yet:` or a missing export
and fails on any other throw, `topScreen()` over package A's `current_screen`,
and a per-scene `pose()` step between `check` and the screenshot. The lap's
`drive()` starts from the pose. Menus: Enter on the title (Play), Esc, Esc,
assert `slotted:pause` on top; Settings from the block; five arrow presses on
the display tab; `settings_ron` non-empty; `localStorage` and the readout
follow; a `Page.navigate` reload and `settings_ron` equal to before; Reset
clears the key and the readout. Dialogue: the smith's first line in the
transcript; Enter until the prompt is the second line; Enter three times to
answer and finish, `machine:furnace` back on top; the switch on and
`set_value` not greyed; Talk again grows the transcript; Enter to the choice,
ArrowDown twice, Enter, and the `you:` line is "I found the key", which only
happens when the gated option is enabled. Waits that depend on the module
drawing text go up to 30 s (`SCENE_WAIT_MS`).

**Docs.** `docs/guide/showcase.md` has nine scenes, the Menus and Dialogue
sections with their shots embedded, Chest absorbing the Browser section,
Themes over the settings screen, `Alt+1..9`, the id list with the
`?scene=browser` mapping, and the two `localStorage` keys. The README's
"Examples" row and the two showcase paragraphs say nine.

## Decisions left to me

- **The Menus shot is the pause over the backdrop, not over the chest.**
  Section 6 says "the pause over the chest" and in the same sentence says the
  driver presses Play then Esc twice; with package A's semantics the first Esc
  pops the chest and the second pauses, so the two halves of that sentence
  disagree and the key sequence won. The picture is the contract's sequence
  and the scene's first "what to try". If the owner wants the chest under the
  pause, the pose is one line: `menu_open('pause')` after Play, no Esc.
- **The pose falls back to `menu_open('pause')`** when the keyboard route
  does not reach the pause, so a shot is still the pause; the drive then
  fails the "Play then Esc twice" verdict and says the pose fell back.
- **The "found the key" switch is not reset on entering the scene.** The value
  store is the game's and survives a scene switch, so the switch showing what
  the page last wrote is the truth. After a page reload the store may hold
  `true` from the saved settings while the switch starts off; the switch shows
  what the page wrote, not what the store holds, and there is no read export
  for one value. A settings key readout would close that; not this round.
- **The stub probe for Dialogue is `set_value('demo.found_key','false')`**,
  not `talk_again`, because `talk_again` while the conversation runs answers
  with a console line and the screenshot had that line in its console.
- **Transcript speaker colours** reuse the message log's classes: `you` is
  the client colour, the smith the server colour. No new tokens.
- **The Esc note.** Package A says Esc during the dialogue is swallowed and
  Esc on the history goes back; the block's copy says so in one line.

## SKIP versus verified

Nothing is SKIP in the final lap. Verified against package A's module,
`/usr/bin/google-chrome` headless, `--size 1760,900`:

- `just smoke` equivalent (`smoke.mjs` with no mode): RESULT PASS, exit 0,
  including the three `index.html` restarts.
- The lap (`--showcase --shots examples/web-playground/shots`): 55 PASS,
  0 FAIL, 0 SKIP, exit 0. Both new scenes reached by link and by rail; the two
  laps of all nine scenes leave the snapshot the same size; no unexpected
  console errors.
- All nine shots are from the real module; `showcase-browser.png` is deleted.

The SKIP path (a `not yet:` export on a scene the table says is ready) is
written and syntax-checked but was never exercised: the skeleton's stubs were
filtered out of the lap by `ready: false` before the branches ran, and
package A flipped both flags and landed all five exports together.

## Things noticed, not mine to fix

- (Fixed at integration: `Presentation.wildcards`, false on the menu
  templates, keeps `slotted:any` off them.) The pause in the Menus scene
  carried a **Sort** button injected by the
  `sorter` mod (see `showcase-menus.png`). The mod targets `slotted:any` and
  the pause is a screen, so it is doing what it says; whether a pause should
  take mod buttons is a question for the menus crate, not the page.
- `ready()` in the driver can answer on the page being navigated away from,
  because the old page's `state.editor` still exists until the new document
  replaces it. Pre-existing; the Menus reload waits on the value it wants for
  up to 30 s, which covers it.

## Files changed

- `examples/web-playground/web/index.html`
- `examples/web-playground/web/playground.js`
- `examples/web-playground/web/playground.css`
- `examples/web-playground/tests/smoke.mjs`
- `examples/web-playground/shots/showcase-menus.png` (new)
- `examples/web-playground/shots/showcase-dialogue.png` (new)
- `examples/web-playground/shots/showcase-browser.png` (deleted)
- `examples/web-playground/shots/showcase-{chest,machine,themes,mods,hud,multiplayer,testing}.png` (recaptured)
- `docs/guide/showcase.md`
- `README.md`
- `docs/design/showcase-refresh-notes-B.md`

Not touched: `examples/showcase/src/lib.rs` (the skeleton's copy is the
contract's and matches `FALLBACK_SCENES` entry for entry; checked by script),
`web/smoke.html`, `justfile`, any `.rs`, `Cargo.toml`, `docs/FOLLOWUPS.md`.
