# Showcase refresh contract

The web playground's showcase gets the menus work and loses its sameness. This
document is binding for the two implementation packages (section 9) and
amends `docs/design/showcase-contract.md`; where the two disagree this one
wins. Anything not decided here is the implementer's call, and goes in that
package's notes file (`docs/design/showcase-refresh-notes-{A,B}.md`).

Owner's words: "the showcase doesn't have any of our new features, and some of
the old ones are a bit samey".

## 1. What is wrong today

Five of the eight scenes are the copper chest with a different side panel.
Chest and Browser are the same screen with the item-browser dock toggled;
Mods, Multiplayer and Testing open a chest too. Only Machine, Themes and HUD
look different, and Themes has no canvas of its own.

Nothing from the menus rounds (M0 to M3) is on the page: the playground does
not depend on `slotted-menu`, so the screen stack, focus ring and keyboard
navigation, settings with persistence, rich text, pause, confirm, toast, text
page and the dialogue runner have no scene.

## 2. The new rail

Nine entries. Order is the rail order; `Scene as usize` still indexes the
table. `Browser` leaves the enum; `Menus` and `Dialogue` join it.

| # | Scene | `Scene::` | Change | Screen(s) |
|---|---|---|---|---|
| 1 | Chest | `Chest` | absorbs Browser: the browser is docked | `demo:chest` + browser |
| 2 | Machine | `Machine` | unchanged | `machine:furnace` |
| 3 | Menus | `Menus` | **new** | `showcase:main`, `slotted:pause`, `demo:settings`, confirm, toast, `demo:chest` behind Play |
| 4 | Dialogue | `Dialogue` | **new** | `slotted:dialogue` over `machine:furnace` |
| 5 | Themes | `Themes` | gets a canvas of its own | `demo:settings` |
| 6 | Mods | `Mods` | unchanged | `copper_chest:chest` |
| 7 | HUD | `Hud` | unchanged | HUD layers |
| 8 | Multiplayer | `Multiplayer` | unchanged | two `demo:chest` |
| 9 | Testing | `Testing` | unchanged | `copper_chest:chest`, replay |

Mods, Multiplayer and Testing keep the chest on purpose and this round does
not touch them: the sorter's `tests/sort.lua` opens the copper chest, the
replay recording was made against `demo:chest`, and two identical panes are
the point of Multiplayer. The sameness is answered by folding Browser into
Chest, giving Themes a screen that shows tabs, sliders, selects and rich text
rather than slot borders, and putting two new kinds of screen on the rail.

`Scene::DEFAULT` stays `Chest`. `Alt+1`..`Alt+9` switch scenes. An old link
with `?scene=browser` opens Chest; the page maps the id, the bridge does not
know it.

## 3. Copy

Package B owns this table; package A owns `ready`. Captions are two plain
sentences; exactly three tries. Chest and Themes change, Menus and Dialogue
are new, the rest is verbatim from today.

| Scene | Caption | Tries |
|---|---|---|
| Chest | The Minecraft interaction model with a modern skin, and the item and recipe browser docked beside it. Seven click modes, a sweep, a phantom preview, a tooltip, and a search with a grammar. | `Left-click a stack, then right-click to split it` / `Hold right and drag across empty slots` / `Type #ingots in the browser, then press R over a card` |
| Menus | A title screen, a pause, settings that persist, a confirm and a toast, and the game is a chest behind them. Every screen sits on one stack, and the keyboard walks all of it. | `Press Play, then Esc twice` / `Change a setting, reload the page, open Settings again` / `Walk a menu with the arrow keys and Enter` |
| Dialogue | A conversation with the smith at the furnace, from one RON file. Lines type out, a choice can be gated on a value, and the history page is the transcript. | `Press Enter to skip the typing, then choose` / `Turn on "found the key" on the right and ask again` / `Press X for the history` |
| Themes | One screen tree, three skins. A theme is a RON file of tokens and materials, and swapping it repaints the open settings screen in place: tabs, sliders, selects, toggles and rich text. | `Switch to paper` / `Switch to neon` / `Open the Chest scene and switch again` |

## 4. Scene details

### 4.1 Chest

`ChestScene` docks the browser (`docks_the_item_browser` returns true).
`BrowserScene` and `Scene::Browser` are deleted. The control block that was
Browser's (the search chips and the dry-run readout) becomes Chest's; the
"no page controls on purpose" copy goes. `search(query)` is unchanged.

### 4.2 Menus

The playground grows the `menu` feature of `slotted` and the showcase crate's
`menus` and `settings` modules become shared with the web for real.

- **Enter** pushes `showcase:main`, a screen compiled into the showcase crate
  (`examples/showcase/screens/main.screen.ron`) that inherits
  `slotted:main_menu`: Play, Settings, Quit, and an injected About button at
  `buttons_end` in the same way `examples/menus` does it. Title
  `demo.showcase.title`, version from `CARGO_PKG_VERSION`. `MenuConfig` is
  `showcase::menus::menu_config`.
- **Play** pops the title and opens `demo:chest` (the chest scene's `open`,
  browser not docked). `Esc` with the chest open pops it; `Esc` with nothing
  open pauses. The pause is the crate's; its Settings opens `demo:settings`
  from `showcase::settings`.
- **Quit** on the pause asks "leave the game?" and an accepted confirm pops
  back to the title, as in `examples/menus`. Quit on the title asks with the
  danger button and an accepted confirm shows the title again with a toast
  `demo.showcase.no_exit` ("A browser tab has nowhere to go; the title is
  back"). Nothing calls `AppExit` on wasm.
- **About** opens a text page whose body is the showcase's own description
  (`demo.showcase.about.body`, a paragraph of rich text with a bold run and a
  key-styled run so the markup is on screen).
- **Persistence.** `SettingsStorage` is a `PageSettings` store in
  `web_playground`: `save` serialises `SavedSettings` to RON and hands it to
  the page through the bus; `load` returns what the page gave back at boot.
  Exports: `settings_ron() -> String` (empty when nothing was saved) and
  `restore_settings(ron)` (empty string resets, mirroring
  `restore_hud_layout`). The page keeps the RON under
  `localStorage["slotted.settings"]` and calls `restore_settings` before the
  first frame, so the third try works. A test proves `save` reaches the bus.
- **Leave** is `scenes::teardown`, and also clears the stack of every menu
  entry and closes any toast; the settings store survives.
- **Control block**: three buttons Title, Pause, Settings that push the named
  screen through `menu_open(which)` (`title` | `pause` | `settings`; the
  export refuses anything else with a `TypeError`), a "Saved settings" readout
  with the RON size and a Reset button that calls `restore_settings("")`.
- **Proves headless** (`tests/showcase.rs`): after `set_scene(menus)` the
  stack top is `showcase:main`; activating `play` opens `demo:chest`; `Menu`
  pushes `slotted:pause` above it; `settings` opens `demo:settings`; moving
  the UI-scale slider writes a save that reaches the bus and comes back out of
  `settings_ron()`; the About button is reachable by keyboard.

### 4.3 Dialogue

- **Enter** opens `machine:furnace` with the simulation running (the Machine
  scene's `open`, minus its control block), then starts `showcase:smith`.
  The furnace keeps cooking behind the overlay, which is what an overlay
  with focus means.
- **The conversation** is `examples/showcase/dialogue/smith.dialogue.ron`,
  compiled in and registered in `Dialogues` at `Startup` through
  `Dialogue::from_ron`, so no asset handle is waited on. Nodes: `hello` (say,
  speaker `demo-smith`, portrait `portraits/smith.png`), `ask` (choice: `yes`
  → `yes`, `no` → `bye`, `secret` → `secret` with `enabled_if:
  "demo.found_key"`), `yes` (say, with a `{name}` arg), `secret` (say), `bye`
  (end). Strings are Fluent keys in the showcase locale. The portrait is a
  new 96 px image the showcase bundles; package A may reuse the elder's
  file under the new name rather than draw one.
- **Answering** `yes` shows a toast, `demo.showcase.thanks`. When the
  dialogue ends the furnace is still open and focus returns to it.
- **Control block**: a "Talk again" button (`talk_again()`, restarts the
  dialogue; a no-op with a console line if one is running), a "found the key"
  switch that writes `demo.found_key` through `set_value(key, json)` (a
  general export: `key` must be one of the keys `showcase::settings::spec()`
  declares, anything else is a `TypeError`), and a transcript pane fed by
  console lines the scene writes on every `DialogueNodeEntered` and
  `DialogueChoice` (`[dialogue] smith: ...`, `[dialogue] you: ...`).
- **Keys**: `Enter` skips then advances, `X` opens the history, `Esc` on the
  history goes back. Same as `examples/menus`; nothing new is bound.
- **Proves headless**: after `set_scene(dialogue)` `ActiveDialogue` is on
  `hello`; `Advance` twice reaches `ask` with `secret` disabled; setting
  `demo.found_key` true and jumping back to `ask` shows it enabled; choosing
  `yes` writes a `DialogueChoice` and a toast root exists; `DialogueEnded`
  leaves `machine:furnace` on top of the stack.

### 4.4 Themes

`overlays_current` goes. `enter` opens `demo:settings` over the backdrop
through `showcase::settings` (the same screen the Menus scene reaches from
the pause); `leave` is `scenes::teardown`. The three theme buttons and the
token diff table stay in the control block. Swapping the theme with the
settings screen open must keep the focused row and the value each control
shows; the test asserts the slider's value survives a swap.

## 5. Bridge exports

Additions, all in `bridge.rs` with native twins in `web_playground::showcase`.

| Export | Request | Notes |
|---|---|---|
| `menu_open(which)` | `MenuOpen(MenuScreen)` | Menus only; `title`, `pause`, `settings` |
| `settings_ron() -> String` | none | from the `PageSettings` store |
| `restore_settings(ron)` | `RestoreSettings` | empty resets |
| `talk_again()` | `TalkAgain` | Dialogue only |
| `set_value(key, json)` | `SetValue` | a declared settings key; bool, number or string |

`list_scenes()` reports nine entries. `snapshot_state`'s `scene` field keeps
its meaning; a restart lands on the scene the visitor was in, and for Menus
and Dialogue that means `enter` runs again from the top, which is the honest
thing a restart can do.

## 6. Page

- **Rail**: nine entries; `Alt+9` works. `?scene=browser` maps to `chest`.
- **Control blocks**: Chest takes Browser's block. Menus and Dialogue as in
  section 4. Themes' block is unchanged.
- **Settings persistence** as in 4.2, keyed `slotted.settings`, with the same
  guard the HUD layout has against a stale key after a reset.
- **Restart** replays `restore_settings` after `restore_state`, in that
  order, so a crash in the Menus scene keeps the saved values.
- **Shots**: `just shot-showcase` writes nine files; `showcase-browser.png`
  is deleted. The Menus shot is the pause over the chest (the driver presses
  Play then `Esc` twice before capturing); the Dialogue shot is the choice
  node.

## 7. Driver and tests

- `smoke.mjs`: `SCENES` has nine rows; `drive()` gains a branch per new scene
  and Chest's branch takes Browser's search assertion. Menus: Play, `Esc`,
  `Esc`, assert `slotted:pause` is the top through `current_screen()` (a new
  read-only export the driver may add; `snapshot_state` may serve instead),
  open Settings, move the first slider with the keyboard, read
  `settings_ron()` and assert it is non-empty, reload the page, assert
  `settings_ron()` matches. Dialogue: wait for the line, `Enter`, `Enter`,
  assert the transcript pane holds the prompt, click the switch, `talk_again`,
  advance to the choice, assert three enabled options.
- `tests/showcase.rs`: the two "proves" lists above, plus
  `every_scene_enters_and_leaves_cleanly` over nine scenes and
  `list_scenes_matches_the_page` expecting nine.
- `tests/smoke_page.rs` and the `SCENES` table in the driver stay the
  independent copies they are; both change by hand.

## 8. Performance

The optimised module is 40.36 MiB today (`just playground`, `wasm-opt -O1`,
2026-09-10). Budget: at most 3 MiB more with `menu` in. The size line goes in
package A's notes, before and after. The drop list from the original
contract stands; `menu` itself is not droppable, it is the point of the round.

The wasm smoke under SwiftShader has shown a first text draw costing seconds
per frame on the CI runner (`examples/web-playground/web/smoke.html`, the
overlay note). The new scenes draw a lot of text; the driver's per-scene waits
are generous (a scene visit may take up to 30 s before the driver gives up),
and the CI run is the gate, not a local pass.

## 9. Ownership

| Package | Owns | May read |
|---|---|---|
| A (Rust) | `examples/showcase/**` except the copy in `lib.rs`, `examples/web-playground/src/**`, `examples/web-playground/build.rs`, `examples/web-playground/Cargo.toml`, `examples/web-playground/tests/*.rs`, `examples/web-playground/assets/**` it adds, `docs/design/showcase-refresh-notes-A.md` | everything |
| B (page) | `examples/web-playground/web/**`, `examples/web-playground/tests/smoke.mjs`, `examples/web-playground/shots/**`, `docs/guide/showcase.md`, README "Examples" row, `docs/design/showcase-refresh-notes-B.md` | everything |
| Shared, edit by agreement | `examples/showcase/src/lib.rs` (A owns the enum and `ready`, B owns copy), `examples/web-playground/web/smoke.html` | |

Neither package commits. Both keep `just ci`, `just wasm-check`, `just
playground` and `just smoke` green at every hand-off. B develops against the
skeleton's stubbed exports, which return a `TypeError` beginning `not yet:`
until A lands them.
