# Showcase notes, package A (the Rust side)

Everything the contract left to the implementer, and the four places the
implementation departs from it. `docs/design/showcase-contract.md` is the
contract; section numbers below are its.

## 1. What departs from the contract, and why

**The two compiled-in screens are `include_str!` in `showcase`, not entries in
the playground's `build.rs` table** (section 1). The build script bakes an
`AssetSource` the pack system reads; a screen file is not a pack asset, it is
two constants parsed synchronously. `showcase::screens` holds
`assets/screens/demo_chest.screen.ron` and
`examples/machine/screens/furnace.screen.ron` as `&'static str`, and
`showcase::chest::screen()` / `showcase::machine::screen()` parse them. The file
on disk stays the one copy either way, and the native examples still read it
with the `AssetServer` so it hot-reloads. Parsing synchronously is what makes a
scene switch one frame instead of a poll loop on an asset handle.

The build script does bake the *base namespaces*, which the contract also asked
for: `assets/data/demo` and `assets/data/machine` get synthetic
`mods/<id>/mod.toml` manifests written into `OUT_DIR`, so a browser tab loads
them as mods. That is `UiHarness::mod_layout_with_base` done at build time, and
it is what makes `minecraft:cobblestone` exist in a tab with no game under it.
`bundle::script_mod_ids()` is the editor's list and leaves those two out.

**"The browser attach policy set to deny for that scene" is a handler
registration, not a policy** (section 3.1). `AttachPolicy` has no `Deny`, and it
does not need one: the default `Registered` already means "a screen kind with no
`ScreenHandler` gets no panel". `scenes::chest` removes the `demo:chest` handler
on entering Chest and registers it on entering Browser. Adding a third variant
would have been a library change the contract does not allow and that says
nothing new.

**The built-in hotbar layer's id is `hotbar`, not `slotted:hotbar`** (section 2,
the HUD row). `slotted_ui::hud::builtin::HOTBAR` is `HudLayerId::new_static
("hotbar")` and always has been; the skeleton's assertion guessed a namespace.
`tests/showcase.rs` asserts the id the library actually uses. The mod's layer is
namespaced as the contract expected: `hud_clock:clock`.

**A replay seek walks, one recorded frame per app frame** (section 3.7).
`ReplayCursor::seek` feeds frames into a `&mut World` without running the
schedule, because it cannot: it is called from inside one. Feeding twenty
frames in one call therefore leaves twenty `PointerInput` messages for one
picking pass, the pointer at the last position, and nineteen clicks landing on
whatever was under it. So `scenes::testing::seek` sets a target and
`advance_replay` steps towards it one frame at a time. A backwards seek still
rewinds at once, because that is a despawn and a respawn and nothing has to see
it. `is_seeking` is what a test waits on; the page polls `replay_status`, whose
`playing` is true while a seek is in flight.

## 2. Choices the contract left open

- **`ActiveScene` and `CanvasScene`.** Themes is an entry in the rail that is
  not a scene on the canvas (section 3.3), so there are two "current scenes":
  the one the rail highlights and the one whose entities exist.
  `SceneHandler::overlays_current` is how a handler says it is the first kind;
  exactly one does, which `showcase.rs`'s own test asserts.
- **`leave` is "despawn every screen and every menu", not "undo what I
  spawned".** A scene's `enter` hands entities to `spawn_screen`, to
  `open_menu` and to the browser's own attachment pass; a list of what to undo
  would be a second copy of that knowledge, stale the first time one of them
  spawns something new. `scenes::teardown` is the one implementation, and
  `every_scene_enters_and_leaves_cleanly` asserts the postcondition.
- **The two demo plugins are added for the app's life**, not by the scenes that
  need them: `MachineDemoPlugin` (a widget has to be registered before any
  screen naming it spawns) and `ChestDemoPlugin` (its browser handler is
  registered at `Startup`). Both are inert when their scene is not open --
  `machine_sim` returns without a `MachineMenu`, and the Machine scene's `leave`
  removes it.
- **`ChestDemoPlugin` now registers the `demo:chest` definition too.** A browser
  handler may not name a screen kind the registry has never heard of;
  `slotted-browser` validates that and panics on it in a debug build. It
  registers the compiled-in copy only when nothing else has, so the windowed
  example's `AssetServer` load still wins and the file still hot-reloads.
- **The scene commands are a message, not work done in `drain_requests`.**
  `SceneCommand` covers the eight new requests, and `apply_scene_commands` is
  exclusive: a theme swap needs the `AssetServer`, a replay seek respawns a
  menu, the net sliders reach into a `MenuServer` in a resource. Same shape
  `RestoreState` already had.
- **`hud_tick` is on, at 250 ms.** The playground had it off. The clock mod
  needs it; four a second is enough to watch and cheap enough that the other
  seven scenes never notice.
- **`shared_namespaces` gains `minecraft` and `slotted`.** The base namespaces
  are loaded as mods and register into those; for a real mod that is worth a
  warning, and here it is the whole job.
- **The multiplayer message log is bus lines with `who = "net"`**, as the
  contract said. Client-side lines come from `RoutingAuthority::submit`,
  server-side ones from `MenuServer::pump`'s `Outcome`, numbered so the page can
  order them.
- **`Loopback` latency is milliseconds on the page and ticks in the link.** A
  tick is a frame, so `latency_ms * 60 / 1000`, rounded up. A round trip costs
  twice the slider, which is what a person expects of a ping.
- **`snapshot_state` grew its `scene` field** (section 4): `Snapshot.scene` is
  the scene id as text, `#[serde(default)]` so a value the page kept from
  before this field still restores its stacks. `apply_restore` writes a
  `SwitchScene` for it before it writes the stacks, so a visitor who was in the
  Machine scene when a mod raised comes back to the Machine scene rather than
  to the page's default. The Multiplayer scene is the one place the contract
  wanted the *server's* container snapshotted rather than a client's; that is
  not done, and `publish_snapshot` takes the first open menu, which is client
  A's. A restart out of Multiplayer therefore restores one client's copy into
  a freshly built pair. It is on the follow-ups list rather than in this
  branch.
- **The recording's geometry check is reported, not enforced, in the browser.**
  A recording carries pointer positions and no locators, so a window of another
  size lands the clicks elsewhere. In a test that has to refuse, and
  `UiHarness::replay_recording` does. In a tab the canvas is whatever size the
  visitor's window makes it, and refusing would mean the scrubber never worked;
  so the page gets a `warn` line and the replay runs. The guarantee lives in
  `the_bundled_recording_replays_to_the_state_it_recorded`, which replays at the
  recorded 1600x900.

## 3. The two `sorter` mods became one

`examples/machine/mods/` is deleted (section 1). The surviving copy is
`examples/modded/mods/sorter`, and it injects into the wildcard `slotted:any`,
so the same mod puts its button on the copper chest and on the furnace.

Which inventory to sort could not stay the same: on a chest, inventory 0 is the
chest; on the machine, inventory 0 is input, fuel and output, and sorting those
would be nonsense. `control.lua` decides from `ev.screen`, which is the only
thing it knows about a tree it has never seen. The log line is now
`sorting inventory <n> of menu <m>`.

The tests moved with it. `tests/sort.lua` is the pair the playground's Tests tab
runs (the live runner takes the first file in the directory) and now carries a
chest fixture so the same file also passes under a `UiHarness`.
`tests/sort_machine.lua` is the machine fixture the contract asked to keep.

Two things outside package A's list had to follow, both forced by the deletion:
`machine::mods_dir()` now points at `examples/modded/mods`, and the `test-mods`
recipe's default argument in the `justfile` with it. Package B owns the
justfile; this is the one line, and it is noted here rather than silently left
pointing at a directory that no longer exists.

## 4. The two library changes

**`slotted_ui::hud_editor::HudLayoutStorage`** (section 3.5). `HudLayoutStore`
was `{ path: PathBuf }`; it is now a port, `HudLayoutStore(Arc<dyn
HudLayoutStorage>)`, with `load` and `save`. Two adapters ship: `FileHudLayout`
is today's code, reached by `HudLayoutStore::file(path)` (what
`examples/chest` uses), and `MemoryHudLayout` holds one in a `Mutex` with
`to_ron`/`from_ron`. The playground inserts the second, a process-wide instance
in `hud_store::global`, and the page is the actual storage: it reads the RON
with `hud_layout()` and hands it back with `restore_hud_layout(ron)` from
`localStorage`. Same push-pull the snapshot uses, and for the same reason -- the
exports are free functions with no handle on the `App`.

**`slotted_test::ReplayCursor`** (section 3.7). A scrubbable position in a
`Recording`: `step`, `tick`, `seek(world, frame, rewind)`, `set_playing`,
`status`, `check_geometry`. It drives a plain `&mut World` rather than a
`UiHarness`, because the playground has no harness. `rewind` is a callback: the
cursor knows about inputs and nothing about which menu is open, and putting the
world back where the recording started is the caller's business.

## 5. Size

Measured with `just playground` (`wasm-opt -O1`), on 2026-09-06.

| Build | bindgen | after wasm-opt |
|---|---|---|
| Baseline, the skeleton (contract section 7) | -- | 37.92 MiB |
| Eight scenes, shipped | 42.95 MiB | 38.31 MiB |
| The same with `viewport` on, measured and reverted | 43.07 MiB | 38.42 MiB |

`+0.39 MiB` against a `2 MiB` budget. Nothing on the contract's drop list was
dropped.

**`viewport` is off, and not because of the budget.** The row above is what it
would have cost: `0.11 MiB`, which the budget has room for many times over. It
is off because it would buy nothing. The live hover tooltip
(`slotted_ui::tooltip::live_subject`) fires only when the `Icons` source answers
`IconRef::Live`, and only `LiveIcons` ever does. Inserting that means the
facade's `live-icons`, which means every slot in every scene draws from a baked
3D atlas instead of the PNG icons the page ships -- a change to how the whole
showcase looks and to what it costs per frame on WebGL2, which is a good deal
more than the contract's "if the size budget allows". The tooltip stays on the
icon tier, which is the first entry on the contract's own drop list.

## 6. Shared files touched

- `examples/showcase/src/lib.rs`: `ready: true` on all eight (A owns `ready`),
  and `Scene::DEFAULT` is now `Scene::Chest` rather than `Scene::Mods` -- the
  page should open on the scene that shows the interaction model the project is
  about, and every scene is real now. Captions and `tries` are untouched.
- `examples/web-playground/web/smoke.html`: not touched.

## 7. Two exports package B asked for

Both landed, and both are in `tests/showcase.rs`.

**`browser_search(query)`.** It writes the same `SearchChanged` message a
category chip writes, which is the browser's own way in: `apply_search`
re-evaluates and `diff_search_field` writes the text back into the field, so
the field shows what was searched and a visitor can carry on typing from it.
Nothing touches the text editor directly. Outside the Browser scene it does
nothing and logs an `info` line rather than failing, which is what B asked for:
a chip pressed a frame after a scene switch is ordinary. It also declines to
queue the query for later, so nothing surprises whoever opens the Browser next.

**`restore_hud_layout("")` is Reset.** An empty string forgets the stored
layout and puts every layer back on its own definition's anchor, and the next
`hud_layout()` reads back the empty string so the page can clear its key. The
rule lives in `scenes::hud::restore_layout_ron` beside the parse it is an
alternative to, rather than in the caller: "nothing stored" and "put them back"
are the same instruction, and the page reaches both through the one export.

One thing moved to make that testable. `scenes::hud`'s three functions read the
`HudLayoutStore` out of the world rather than `hud_store::global` directly. In
the running app they are the same object; in a test each harness gets its own
`MemoryHudLayout`, so two tests in one binary no longer fight over one
process-wide store. `publish_hud_layout` copies the world's store into the
global each frame, which is what the `hud_layout()` export -- a free function
with no world -- reads.

## 8. For package B

- `browser_search(query)` and `restore_hud_layout("")` are in, per section 7.
- Every export that threw `not yet:` is real. `NOT_YET_PREFIX` still exists so a
  page written against the stubs keeps working, but nothing throws it.
- Six exports that used to return `Result` no longer can and are now plain
  calls: `machine_redstone`, `hud_edit`, `net_config`, `replay_load`,
  `replay_seek`, `replay_play`. `browser_search` is a plain call too. `set_theme`, `hud_layout` and
  `restore_hud_layout` still return `Result`; `replay_status` returns the JSON
  directly.
- `replay_status()` reads `{"frame":n,"frames":n,"playing":bool}`. `playing` is
  true while a seek is walking as well as during playback, so a page can poll
  the same field for both.
- The bundled recording has eight frames. A scrub from one end to the other
  takes eight app frames.
- The multiplayer message log is on the ordinary console queue with
  `who == "net"`; filter on that for the message pane.
- `list_mods()` lists four mods now: `appleskin_like`, `copper_chest`,
  `hud_clock`, `sorter`. `hud_clock` is new and is what the HUD scene's clock
  layer comes from.
