# Showcase contract

The web playground becomes a showcase of the whole ecosystem: one wasm module,
one page, eight scenes. This document is binding for the two implementation
packages (section 9). Anything not decided here is the implementer's call, and
goes in that package's notes file (`docs/design/showcase-notes-{A,B}.md`).

Owner's words: "can we also have the web playground demo all our other
features? would nice to have a real showcase of the whole ecosystem".

## 1. Shape

- **One Bevy app, one canvas, one page.** A scene is a set of entities and
  resources toggled by a `Scene` value, never an app restart. The 3D backdrop
  (`scene::setup_scene`) is spawned once and shared; the orbit continues across
  switches.
- **Switching** goes through the bus: `Request::SetScene(Scene)` drained in
  `PreUpdate`; the active scene's `leave()` despawns its screens and menus, the
  new one's `enter()` spawns its own. `ActiveScene` is a resource; every scene
  system runs under `.run_if(scene_is(Scene::X))`.
- **Skeleton shipped by the design step** (this document's sibling commit):
  `examples/showcase/` (lib crate `showcase`, the `Scene` enum, `SceneDef`
  table, no Bevy), `web_playground::showcase` (the `ActiveScene` resource, the
  `SceneRegistry` with `Scene::Mods` wired to the existing chest, stub exports),
  `tests/showcase.rs` (one ignored test per scene), and the page's rail. The
  stubbed exports return a `TypeError` whose message begins with `not yet:`;
  the page greys the rail entry when `list_scenes()` reports `ready: false`.
- **The `showcase` crate** is where code lifted from the native examples lives:
  `showcase::chest` (menu def, contents table, labels system, `ChestBinding`),
  `showcase::machine` (`props`, `slots`, `menu_def`, `machine_sim`,
  `FaceConfigWidget`, `MachineDemoPlugin`), `showcase::backdrop` (the cube ring
  and orbit). `examples/chest`, `examples/machine` and `examples/modded` then
  depend on `showcase` and delete their copies; their `main.rs` files, tests
  and screenshots are unchanged in behaviour. `showcase` depends on `slotted`
  with `default-features = false, features = ["ui", "browser", "packs", "dev",
  "viewport", "net"]` and never on `std-fs`, `x11` or `gltf-icons`; anything
  that reads a directory stays in the native example.
- **Content.** The base namespaces `demo` and `machine` under `assets/data/`
  are loaded as mods by giving the bundle synthetic manifests
  (`mods/demo/mod.toml`, `mods/machine/mod.toml`, `version = "0.0.0"`),
  exactly what `UiHarness::mod_layout_with_base` does natively. The three
  modded mods stay the one copy under `examples/modded/mods/`;
  `examples/machine/mods/sorter` is deleted in favour of it (its `tests/sort.lua`
  moves alongside, with the machine fixture registered too). `build.rs` also
  bakes `assets/screens/demo_chest.screen.ron`,
  `examples/machine/screens/furnace.screen.ron`, the `hud_*.screen.ron` files
  the HUD scene adds, and `examples/showcase/recordings/chest.rec.ron`.

## 2. Scenes

Order in the rail is the order below. Captions are the page copy, in the
project's plain voice; the "what to try" list is exactly three items. Every
scene has a `test_id`-free smoke assertion in `tests/showcase.rs` that opens it
headless and checks one fact (column "proves").

| # | Scene | `Scene::` | Reuses | Screen / mods | Proves headless |
|---|---|---|---|---|---|
| 1 | Chest | `Chest` | chest lib | `demo:chest`, no mods | 27 slots, pick+split conserves, `drag_paint` leaves phantoms |
| 2 | Browser | `Browser` | chest lib + `DefaultScreenHandler` | `demo:chest`, browser docked | `search("#c:ingots")` narrows, `open_recipes` shows a page, bookmark toggles |
| 3 | Machine | `Machine` | machine lib | `machine:furnace` + `sorter` | after 3 s virtual time `props::COOK` > 0; the `Sort` button exists |
| 4 | Themes | `Themes` | whichever scene is open | none of its own | `set_theme("neon")` changes the panel role's colour |
| 5 | Mods | `Mods` | today's `scene.rs` | `copper_chest:chest` + 3 mods | today's `plumbing.rs`, `tests_tab.rs`, `restart.rs` (unchanged) |
| 6 | HUD | `Hud` | `slotted::ui::hud`, `hud_editor` | hotbar layer + `showcase:clock` HUD layer from a bundled mod `hud_clock` | `hud_layer("slotted:hotbar")` and `hud_layer("hud_clock:clock")` both spawn; `hud_edit(true)` then a drag moves the anchor |
| 7 | Multiplayer | `Multiplayer` | `slotted-net` Loopback | two `demo:chest` screens, one `ContainerStore` | a click on peer 1 arrives on peer 2's inventory after `advance`; a dropped ack retransmits |
| 8 | Testing | `Testing` | `LiveTestRunner`, `slotted_test::replay` | `copper_chest:chest` | `run_tests("sorter")` logs `ok`; the bundled recording replays to a known inventory |

Captions and "what to try", verbatim for the page:

1. **Chest.** The Minecraft interaction model with a modern skin. Seven click
   modes, a sweep, a phantom preview and a tooltip, all over one list of
   slots. Try: left-click a stack then right-click to split it; hold right and
   drag across empty slots; hover an item and hold Shift.
2. **Browser.** Every item and recipe in the game, docked beside any screen.
   Search has a grammar, and a recipe knows whether it can be transferred.
   Try: type `#ingots` then `-iron`; press R over a card; press A to bookmark
   it.
3. **Machine.** A furnace with a tank, an energy bar and two side tabs, driven
   by menu properties the simulation writes. The Sort button was injected by a
   mod that has never seen this screen. Try: put coal in the fuel slot and
   watch the arrow; open the redstone tab; press Sort.
4. **Themes.** One screen tree, three skins. A theme is a RON file of tokens
   and materials, and swapping it repaints the open screen in place. Try:
   switch to paper; switch to neon; open the Machine scene and switch again.
5. **Mods.** Three Lua mods, editable here, hot-reloaded into the running
   game. The chest keeps its contents across a reload, and a crash restarts the
   runtime. Try: edit `control.lua` and press Run; open Tests and run them;
   type `error("boom")` and watch the restart.
6. **HUD.** Layers anchored to the screen edges, registered from Rust or from a
   mod, and a position editor a player can use. Try: press the edit button
   and drag the hotbar; drag the clock; reload the page and find them where you
   left them.
7. **Multiplayer.** Two clients and a server in this tab, joined by a lossy
   loopback link. The left client predicts; the server corrects; both share
   one chest. Try: click a stack on the left and watch the right; raise the
   loss slider and click again; read the message log.
8. **Testing.** The mods' `tests/*.lua` run against the live game, one action
   per frame, and a recorded session replays through the real input path.
   Try: run the sorter's tests; scrub the recording; press play.

## 3. Scene details

### 3.1 Chest and Browser
Both open `demo:chest` over the chest lib's `inventories()` (the moodboard
contents, worn tools included). Chest hides the browser through
`BrowserConfig` attach policy set to deny for that scene; Browser attaches it
with `DefaultScreenHandler`. The tooltip's live viewport needs the facade's
`viewport` feature, which brings `gpu-icons` (section 8 has the size rule).
The action rail is the screen file's; nothing new.

### 3.2 Machine
`MachineDemoPlugin` from the `showcase` crate, `Redstone` toggled by a page
button (bridge `machine_redstone(on)`), `MachineSim { paused: false }`. The
side tabs are the screen file's. `sorter` injects into `slotted:any`, so the
same mod provides the button here and in Mods.

### 3.3 Themes
Not a scene of its own on the canvas: selecting it keeps the current screen and
shows the theme controls. `set_theme(name)` replaces `ActiveTheme` with the
bundled handle (`themes/<name>.theme.ron`, fetched beside the page as today).
The page's "token diff" panel is static copy per theme, generated at build time
by package B from the three RON files (fonts, radii, blur, slot size,
durations), not computed in wasm.

### 3.4 Mods
Today's playground unchanged: editor, Tests tab, hot reload, restart with
snapshot restore. `scene.rs` becomes `showcase/mods.rs`. The
`OPENING_MOD`/`OPENING_FILE` behaviour stays.

### 3.5 HUD
Opens no screen. Registers the builtin layers (`register_builtin_layers`) with
the hotbar over the demo hotbar inventory, and loads the bundled mod
`hud_clock` (new, `examples/modded/mods/hud_clock/`, `data.lua` calling
`slotted.register_hud_layer`, `control.lua` calling `slotted.cmd.hud_update`
on `tick`). Edit mode: `hud_edit(on)` writes `HudEditMode`. Persistence: a
new `HudLayoutStore` port in `slotted-ui` (`trait HudLayoutStorage { load,
save }`, the file adapter is today's code); the playground's adapter is
`hud_layout()` / `restore_hud_layout(ron)` exports, and the page keeps the RON
in `localStorage` under `slotted.showcase.hud`. This is the one library
change this contract allows, and it is package A's.

### 3.6 Multiplayer
In-process: one `MenuServer` over a `ContainerStore` with one shared chest
container, one `Loopback`, two `ClientEnd`s, two `RemoteAuthority`s. Two
`demo:chest` screens side by side at half width, each spawned with its own
`Authority` resource scope: `slotted-ecs` has one global `Authority`, so the
scene tags each menu with `PeerTag(PeerId)` and installs a
`RoutingAuthority` that dispatches `submit` by menu id. The server is pumped
in `PostUpdate` after `advance(1)` per frame. `net_config(latency_ms,
drop_percent)` maps to `Conditions`. The message log is `ClientMessage` and
`ServerMessage` names with sequence numbers, pushed on the bus with
`who = "net"` and rendered by the page as its own pane, not the console.

### 3.7 Testing
Left: the mods list with Run per mod (same path as today's Tests tab). Right:
the recording. `slotted-test` gains `ReplayCursor` (frame index into a
`Recording`, `seek(frame)` resets the menu to the recording's opening
inventory and re-feeds frames `0..=frame`), which is the second and last
library change, package A. Exports: `replay_load()` (bundled), `replay_seek(frame)`,
`replay_play(bool)`, `replay_status() -> {frame, frames, playing}`. The
recording is made with `cargo run -p chest -- --record` at 1600x900 and checked
in; `tests/showcase.rs` replays it headless to a fixed inventory snapshot.

## 4. Bridge exports

All in `bridge.rs`, all free functions over `Bus::global()`, all with a native
twin in `web_playground::showcase` so a test can call the same thing.

| Export | Request | Notes |
|---|---|---|
| `list_scenes() -> json` | none | `[{id,title,caption,tries:[..],ready}]` from `showcase::SCENES` |
| `set_scene(id)` | `SetScene` | unknown id is a `TypeError`; stub scenes are `not yet:` until real |
| `current_scene() -> id` | none | from the snapshot slot beside the inventories |
| `set_theme(name)` | `SetTheme` | `glass`, `paper`, `neon` |
| `machine_redstone(on)` | `Redstone` | Machine only |
| `hud_edit(on)`, `hud_layout()`, `restore_hud_layout(ron)` | `HudEdit`, none, `RestoreHud` | HUD only |
| `net_config(latency_ms, drop_percent)` | `NetConfig` | Multiplayer only |
| `replay_load()`, `replay_seek(frame)`, `replay_play(on)`, `replay_status()` | `Replay*` | Testing only |
| existing: `reload_mod`, `run_tests`, `get_mod_file`, `list_mods`, `snapshot_state`, `restore_state`, `drain_console`, `console_history`, `subscribe_console`, `set_canvas_console` | unchanged | |

`snapshot_state` grows a `scene` field so a restart lands on the scene the
visitor was in. The Multiplayer scene snapshots the server's container, not the
clients'.

## 5. Page

Layout, desktop (>= 1280 px): three columns, `grid-template-columns: 200px
minmax(960px, 1fr) minmax(360px, 420px)`; header as today. The canvas column
never drops below 960 px because the browser panel needs its strip. Below
1280 px the rail collapses to icons (40 px) and the right column to 340 px;
below 1080 px everything stacks: rail as a horizontal scroller of chips, canvas
at 56 vh, controls, console.

- **Rail** (`<nav class="rail">`): one `<button role="tab">` per scene with
  number, title, and the caption on hover (a `title` attribute plus a
  `.rail-caption` block shown for the active one). Disabled entries carry
  `aria-disabled` and the eyebrow `soon`. Keyboard: `1`..`8` with `Alt` switch
  scenes; plain digits stay the game's hotbar keys.
- **Stage**: the canvas, the boot overlay, and a `.scene-head` strip above it
  with the caption and the three "what to try" items as `<kbd>`-styled chips.
- **Controls column** (`<aside class="side">`): a `.panel` whose body is the
  active scene's control block, then the console panel (unchanged). Blocks:
  Chest none; Browser a "dry run" readout (last `TransferFailed`/success line);
  Machine a redstone toggle; Themes three buttons and the token diff table;
  Mods the editor and Tests tab as today; HUD an edit toggle, a "reset layout"
  button and the stored RON size; Multiplayer latency (0..400 ms) and loss
  (0..50 %) sliders plus the message log pane; Testing the mods list with Run
  buttons and the scrubber (`<input type="range">`, play/pause, frame counter).
- **Tokens**: today's `playground.css` root block stays the source; add
  `--rail-w:200px`, `--side-w:400px`, `--accent-2:#ffb454`. Fonts are the three
  already loaded. Panel radius 12, gap 14, eyebrow 10.5 px mono.
- **State in the URL**: `?scene=<id>&theme=<name>`; the hash keeps the edited
  mod buffers as today.
- **Screenshots**: `smoke.mjs --scene <id> --shot <png>` captures one scene;
  `just shot-showcase` loops all eight into `examples/web-playground/shots/`.

## 6. Native tests

`examples/web-playground/tests/showcase.rs`, one `#[test]` per scene, built on
`tests_tab.rs`'s `playground_world()` lifted into a `tests/common/mod.rs`.
Each test: `set_scene` through the bus, `settle()`, assert the "proves" column.
Two more: `every_scene_enters_and_leaves_cleanly` (cycle all eight, assert no
`ScreenRoot` or `OpenMenu` from a previous scene survives) and
`list_scenes_matches_the_page` (the JSON has eight entries in rail order).
The skeleton ships them as `#[ignore = "showcase: enabled at integration"]`;
package A removes the attribute as each scene lands. `smoke.mjs` extends its
phase two with one visit per scene: `set_scene`, wait for `current_scene`,
assert no `error` line reached the console, screenshot.

## 7. Performance

One app; scenes are entities toggled. Both multiplayer clients and the server
run in the one world, no workers. The backdrop is shared; the Multiplayer
scene's two screens are two `ScreenRoot`s under one camera. Budget: the
optimised module grows by at most 2 MiB over today's (measure with the size
report `just playground` prints, before and after, in the notes). The ordered
drop list if the budget is missed: `gltf` never; `viewport` (the hover 3D
tooltip becomes the icon tier) first; the recording second. No other feature
is negotiable.

## 8. Voice and visuals

Captions are plain sentences, no exclamation marks, no "amazing". The rail and
controls keep the moodboard's dark glass: `--panel` cards, one accent, mono
eyebrows. No new fonts, no icons beyond the eight scene numbers. The canvas
theme is the game's, never the page's.

## 9. Ownership

| Package | Owns | May read |
|---|---|---|
| A (Rust) | `examples/showcase/**`, `examples/web-playground/src/**`, `examples/web-playground/build.rs`, `Cargo.toml` of the four examples, `examples/web-playground/tests/*.rs`, `examples/modded/mods/hud_clock/**`, the two library changes (`slotted-ui` `HudLayoutStorage`, `slotted-test` `ReplayCursor`), `examples/showcase/recordings/**`, `docs/design/showcase-notes-A.md` | everything |
| B (page) | `examples/web-playground/web/**`, `examples/web-playground/tests/smoke.mjs`, `examples/web-playground/shots/**`, `justfile` (`shot-showcase`), README "Examples" row and a `docs/guide/showcase.md` (linked from `docs/guide/README.md`), `docs/design/showcase-notes-B.md` | everything |
| Shared, edit by agreement | `examples/showcase/src/lib.rs` (the `SceneDef` table: A owns `ready`, B owns copy), `examples/web-playground/web/smoke.html` (raw export names) | |

Neither package commits. Both keep `just ci`, `just wasm-check`, `just
playground` and `just smoke` green at every hand-off. B may develop against
the stubbed exports: a `not yet:` error is rendered as a disabled control, so
the page is complete before the Rust is.
