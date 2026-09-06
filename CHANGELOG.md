# Changelog

Notable changes to the slotted workspace. Every crate shares one version.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and
the project uses [semantic versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Security

- **A peer could drive any open menu on the server.** `MenuServer` checked that
  a menu existed and then acted, so any connected client could click, resync or
  otherwise move items in any other player's open container, and would be sent
  its whole contents on a state-id mismatch. A menu id now names a *session*
  belonging to exactly one peer, every message goes through one `authorize`
  gate, and a refusal carries no state at all -- not a snapshot, not a slot.
  The new `Access` port adds a game's own distance or lock check on top and can
  take access away mid-session; the ownership and privacy rules underneath it
  are invariants no policy can switch off.
- **One player could reach into another's inventory.** A `ServerMenu` owned one
  `MenuState`, one `Actor` and one `Inventories` for every viewer, so two
  players at one chest shared a cursor, a drag and a player inventory.
  Inventories now live in a `ContainerStore`, each `Shared` or `Private(peer)`,
  and a session binds ids rather than owning items. Binding an inventory
  private to another player is refused whatever the access policy says.

### Fixed

- **A client closing a screen destroyed whatever was on its cursor.**
  `ClientMessage::CloseMenu` is documented to return the carried stack to the
  player's own inventory; the server dropped the session and the stack with it,
  so any client could sink items by pressing escape mid-click. The stack now
  goes back into a binding private to that peer.
- **A lost `CloseMenu` left a session open on the server for ever**, bound to
  that player's private inventories, with nobody left who would ever close it.
  The request is now repeated on the same timer a click is, until the server
  answers.
- **A lost correction was replaced by an ack on the retry**, leaving a client
  permanently divergent from one dropped packet. The server records which kind
  of answer it gave per `(session, sequence)` in a bounded window and replays
  the kind: an ack repeats as an ack, a correction as the container as it is
  now.
- **A snapshot retired the oldest submission in flight rather than the one it
  answered**, which is the wrong one as soon as two clicks are outstanding.
  `ServerMessage::SetContent` now carries `answers: Option<u32>`.
- **A resync moved a menu's properties and left every widget drawing the old
  value.** `apply_resync` replaced the whole `MenuState` and touched no
  `MenuProperty` component, so a snapshot carrying 42 moved the model to 42 and
  left the tank on screen at 0. `slotted_ecs::systems::write_property` is now
  the one path a property value changes by, and all three callers go through
  it; the resync case diffs, so a resync that moves nothing fires nothing.
- **A screen only respawned when its own definition changed by name.** An edit
  to a base screen, to a widget template, or an injection added or removed
  reached nothing. `ScreenDependencies` answers the dependency question
  instead, and `slotted_ui::respawn_screens` is now the only respawn
  implementation in the workspace -- there were two, and they had drifted.
- **Nothing a mod registered could ever be removed.** A screen, template,
  injection or tooltip part a mod stopped shipping outlived the mod for the
  life of the process. Every entry carries an `Owner`, each registry is
  reconciled rather than appended to, a game definition a mod took over comes
  back when the mod stops registering it, and an open screen whose kind nothing
  registers any more is closed and reported to the mod log.

### Changed

- **`SlottedPlugins::server()` and the `server` feature.** The facade's `bevy`
  dependency enabled `bevy_ui`, `bevy_text`, `bevy_picking`, `bevy_window`,
  `bevy_scene` and `default_font` unconditionally, so the lightweight
  dedicated-server build its feature comments advertised did not exist. Every
  Bevy UI feature moved into the facade's own `ui` feature, `packs` no longer
  implies `ui`, and `server` is `packs + script-luaur + net` and nothing else:
  160 crates against a default build's 300, with no `bevy_ui`, `bevy_text`,
  `bevy_picking`, `bevy_winit`, `bevy_window` or `bevy_render` in the graph on
  either target. `slotted-packs` gained a `ui` feature of its own to make that
  possible. `just server-check` asserts the graph on native and on
  `wasm32-unknown-unknown`, builds both, and runs
  `crates/slotted/tests/server.rs`, which drives a client click to an ack
  through a `MenuServer` in a `server()` app with no UI plugins present.
- `ClientMessage::CloseMenu`, `ServerMessage::Refused` and
  `ServerMessage::Closed` are new on the wire. `Outcome::UnknownMenu` became
  `Outcome::Denied(Refusal)`, and `Outcome::Closed` joined it.
- `slotted_model::InventoryId` is new: `InventoryRef` says which of the
  inventories a menu addresses, `InventoryId` says which inventory in the
  world, and a server needs both.
- The `pages` CI job now needs `native`, `wasm`, `browser`, `server` and
  `docs`, so it cannot deploy over a red branch; the `browser` job installs a
  Chrome rather than skipping itself when it finds none.


### Changed

- **There is one script runtime now, `slotted-script-luaur`, on every target**
  ([ADR 0004](docs/adr/0004-web-runtime-luaur.md)). It wraps `luaur`, a
  pure-Rust line-for-line port of Luau, and it replaces both adapters the
  workspace used to carry: piccolo on the web and mlua natively. It passes all
  32 conformance cases natively and its core cases in headless Chrome, and a
  `SlotClick` round trip costs 4.6 us against the mlua adapter's 7.2 us.
- The facade's `script-luaur` feature is **on by default** and `LuaurHostPlugin`
  is in the default plugin group. There is no per-target feature to set: a wasm
  build takes the same features a native one does. `slotted::script_luaur`
  re-exports the adapter.
- **Nothing in the workspace compiles C or C++ any more.** mlua's vendored Luau
  was the only such build. `cargo tree -e build --workspace --all-features`
  lists no build dependencies at all.
- A mod now behaves identically in a browser and on a desktop, because it is the
  same VM: `string.find` takes patterns and `table.sort` takes a comparator on
  both. The guide's "the two runtimes differ" section is gone.
- **On `wasm32` an uncaught Lua error now aborts the module**, and `pcall` in a
  script does not contain it. This is the price ADR 0004 pays for a faithful,
  fast Luau on both targets. `slotted_script_luaur::install_error_reporter`
  hands the host the error text before the trap, and the host restarts;
  `examples/web-playground` is the reference implementation.
- The web playground survives that: `snapshot_state` and `restore_state`
  exports, and a page that catches a `WebAssembly.RuntimeError`, says "the mod
  crashed the Lua runtime; restarting" with the last Lua error, re-instantiates
  the module and puts the chest back stack for stack.
- The optimised playground module grew from 23.05 MiB to 26.46 MiB. luaur keeps
  the Luau lexer, parser and bytecode compiler live where piccolo's adapter did
  not; see `docs/FOLLOWUPS.md`.

### Removed

- **`slotted-script-mlua`**, the facade's `script-mlua` feature, `MluaHostPlugin`
  and the `mlua` dependency. There is no fallback runtime; the conformance suite
  is what makes a future one cheap, and it still takes a
  `&mut dyn ScriptRuntime` rather than a concrete type for exactly that reason.
- `slotted-script-piccolo`, `vendor/piccolo` and the `[patch.crates-io]` section
  that pointed at it. With the patch gone nothing depends on a patched crate,
  and `cargo publish --dry-run` is green for all twelve publishable crates as
  one invocation, `slotted-script-luaur` among them.
- The `CC`/`CXX`/`CFLAGS` pin in `.cargo/config.toml`. It existed only for
  mlua's C++ Luau build, which the developer shell's Anaconda toolchain
  miscompiled into a binary that died with `SIGFPE` inside `Lua::new()`. The
  file records what would bring it back.
- The `getrandom_backend="wasm_js"` rustflag in `.cargo/config.toml`, which
  existed only for a transitive dependency of piccolo.

### Fixed

Four things play testing found that the harness had not.

- The quick-move flight came off the `slow` duration tier, so a shift-click
  animated for 320 ms over a model change that had already landed. `FlyToSlot`
  is a normal-tier motion now and every shipped theme pins it between 150 and
  180 ms. One gesture that lands in several slots flies to all of them on the
  same frame instead of only the first.
- The tooltip hover delay was twice the `normal` tier (360 ms in glass). It is
  its own token, `durations.hover_delay`, defaulting to 120 ms.
- A tooltip was spawned at the origin of the tooltip layer and moved into place
  a frame later, which read as a flash at the corner of the screen. It is now
  placed on the frame it is born, stays hidden until it has been clamped
  against its own measured size, and `place_tooltips` runs before the layout
  pass so the position it writes lands in the same frame.
- `bevy_picking` sends `Pointer<DragStart>` on the first pixel of movement, so
  a click with a shaky hand became a drag paint and picked nothing up. A drag
  now only becomes a paint once the pointer reaches a second slot; a gesture
  that ends where it started is the click it always was.
- The double-click window turned any second click on a slot into `PickupAll`,
  even with an empty cursor, where the model rejects it and the click does
  nothing. Collect-all is gated on the cursor already holding a stack, which is
  the vanilla rule.
- Opening a side tab widened the `tab.rail` column and pushed the machine panel
  sideways. The tab root is a fixed-size flow child now and the content opens
  into an absolutely positioned sibling anchored outside it, so nothing else on
  screen moves. The exclusion zone still publishes and the browser still
  re-docks around it.

### Changed

- `Durations` gains a `hover_delay` field (defaulted on deserialise, so
  existing theme files keep loading).
- `ClickInterpreter::interpret_with_time` takes a `carrying: bool`.
- `SideTabState` gains `open_height`; the tab's open box is a new
  `SideTabPanel` node.

## [0.1.0] - unreleased

The first version. It covers phases 0 to 7 of
[`docs/PLAN.md`](docs/PLAN.md): the model, the widgets, the browser, scripting,
the web playground, the machine widgets, and the second and third themes with
the icon bake and the documentation that came with them. Nothing is on
crates.io yet.

### The domain

- `slotted-model`: namespaced and interned ids, item stacks with component
  patches, inventories, menu definitions and menu state, and `apply_click`, the
  click state machine covering all seven modes plus the toolbar actions. No
  Bevy, no IO. Item conservation is asserted in debug builds and covered by a
  proptest.
- `slotted-registry`: namespaced registries for items, tags, recipes and recipe
  types; the `AssetSource` port with a `DirSource` adapter; a data stage that
  reads entry files and runs three patch rounds over them; `resolve_load_order`
  over `mod.toml` manifests; and a freeze that interns every id into a dense
  handle and builds the recipe index.

### The Bevy layer

- `slotted-ecs`: the model as components, entity events and systems, with the
  client half of the prediction loop (`Input`, `Predict`, `Submit`,
  `Reconcile`), the `Authority` port and a `LocalAuthority` adapter. Runs under
  `MinimalPlugins` and on wasm.
- `slotted-theme`: `Theme` as a `*.theme.ron` asset, 36 semantic roles with
  dotted parent fallback, ten material kinds, a token table for spacing, radii,
  elevation, durations, fonts, motion and blur, hot reload through the asset
  watcher, and `Motion` with a reduced-motion switch that lands every tween on
  its first frame. Three themes ship: **glass** (the default), **paper** and
  **neon**. Backdrop blur and the cut-corner shader are behind the `blur`
  feature.
- `slotted-ui`: screens as data. `ScreenDef` and 13 `UiNodeDef` node types
  (`panel`, `slot`, `slot_grid`, `virtual_grid`, `text`, `button`, `tank`,
  `bar`, `progress`, `side_tab`, `icon_button`, `viewport`, `anchor`, `custom`),
  one `spawn_screen`, the semantic layer that `bevy_a11y` announces and locators
  match on, tooltips with a compact and an expanded tier, the carried-stack
  layer, anchors and injection, exclusion zones, HUD layers with a dev-mode
  position editor, and input recording and replay.
- `slotted-icons`: the `IconSource` port with a baked-atlas adapter, a
  deterministic CPU bake that needs no camera and works headless and on wasm,
  an offscreen three-point GPU rig that renders into the atlas image itself
  (no readback, so WebGL2 is fine), and `LiveIcons`, which puts the hovered
  item in a turning viewport.
- `slotted-browser`: an item and recipe browser that layers over any screen
  through a screen-handler registry. Ingredient types, ordered registration
  phases, a search index built off the main thread, a prefix search grammar,
  recipe categories and pages, bookmarks, and recipe transfer with a dry run.
  Transfers go through `MenuAction`, never through inventories directly.

### Scripting

- `slotted-script`: the `ScriptRuntime` port, the `ScriptEvent` and
  `ScriptCommand` enums, the untagged `Value` form a Lua table maps onto, and
  the shared Lua prelude both adapters install, so `slotted.*` is the same
  surface everywhere.
- `slotted-script-luaur`: Luau through luaur, a pure-Rust line-for-line port of
  Luau. One sandboxed state per script, an interrupt budget, a memory limit and
  a serde bridge; the same runtime natively and on wasm. On `wasm32` an
  uncaught Lua error aborts the module; see ADR 0004.
- `slotted-packs`: mod discovery from `mod.toml`, a layered `AssetSource` and
  `pack://` asset reader over resource packs and mods, the data and control
  lifecycle that runs mod scripts and validates their commands, hot reload that
  remaps open inventories by name, and Fluent localisation.

### Testing

- `slotted-test`: a published headless UI harness. Locators over the semantic
  tree, synthetic input through the real `bevy_ui` layout and `bevy_picking`
  backend, virtual time with a `settle()` that waits for layout and motion,
  screen-tree snapshots, mod loading, a runner for a mod's `tests/*.lua`, and
  recorded-session replay.
- `slotted-testutils`: internal fakes, builders and the script conformance suite
  every adapter runs. Never published.
- `slotted.test`: the Lua side of the harness, so a mod's tests need no Rust.

### The facade

- `slotted`: `SlottedPlugins` for a windowed game and `SlottedPlugins::headless()`
  for a server or a test, the plugin group ADR 0002 proved sufficient without a
  renderer, a prelude, and feature flags for `ui`, `browser`, `packs`,
  `script-luaur`, `blur`, `dev` and `viewport`.

### Examples

- `chest`: the moodboard screen over a 3D scene, with tooltips, motion, the
  action rail and the browser.
- `machine`: a furnace with a tank, an energy bar, progress arrows and side
  tabs, and a `sorter` mod injecting a button into every container screen.
- `modded`: three mods loaded from disk, hot reloading, with a script console.
- `web-playground`: the modded demo in a browser tab with its Lua editable live
  beside the canvas.

### Tooling

- `cargo xtask`: `wasm-build`, `playground`, `serve`, `test-mods`, `gen-docs`
  and `luau-stubs`. No dependencies outside `std`.
- `just`: `check`, `test`, `lint`, `fmt`, `doc`, `ci`, `wasm-check`,
  `playground`, `serve`, `smoke`, `test-mods`, and a `run-` and `shot-` recipe
  per example.
- CI: native tests, clippy and rustfmt on Linux; a `wasm32` check of every crate
  that has to reach a browser; the luaur adapter's tests in headless Chrome;
  rustdoc with warnings fatal; and a Pages deploy of the playground, the guide
  and the API docs from `main`.

### Phase 7: themes, icons and documentation

- **Two more themes.** `paper` is an opaque cream sheet with a dotted grid, ink
  rules, square corners, hard offset shadows and hatched rarity stamps; `neon`
  is slate and lime with chamfered corners and a glowing rarity bar along each
  slot's bottom edge. Neither changed a widget: `screen_tree()` is byte for
  byte the same under all three, which is the property the phase existed to
  prove.
- **Four material kinds and the tokens to carry them.** `Tiled`, `Dashed`,
  `CutCorners`, and `Text` gaining `font` and `shadow`. `Elevation` gained an
  `x` offset, `Tokens` gained a `fonts` map and a `motion` table of per-preset
  durations and curves, and `Easing` gained `Linear`, `EaseInOut`, `Stamp`,
  `Snap` and `Overshoot` beside the original ease-out.
- **Item icons are lit 3D shapes.** `ItemDef.icon` is an `IconDef`: an image
  path, `(shape: "ingot", color: "#c9793f", metallic: 0.9)`, or a glTF path
  that parses and warns. Six shapes, one fixed three-point rig, baked on the
  CPU for a headless app and on the GPU into the same atlas image where there
  is a renderer. An item that declares nothing gets a cube in its hash colour,
  so no screen shows a grid of magenta.
- **Live viewport icons.** `LiveIcons` returns `IconRef::Live` for every item
  the atlas knows, and the tooltip draws that item's own mesh turning under the
  same rig. Slots and browser cards keep the flat atlas cell: one camera a
  tooltip, never one a slot (ADR 0003).
- **Polish.** A vertical tank's readout moved out of the fluid and under the
  well; progress arrows are 40x14 pills.
- **Every example takes `--theme <name>`**, and `just shot-chest-themes`
  captures the chest screen and a recipe page in paper and neon.
- **Documentation.** A README per crate, a seven-page guide, a Lua API
  reference and a `.d.luau` stub file both generated from the prelude's doc
  comments and checked in CI, this changelog, `CONTRIBUTING.md`, and the two
  licence files the manifests had always claimed.
- **Release prep.** Every publishable crate carries `readme`, `documentation`,
  `homepage`, `keywords` and `categories`, and `cargo publish --dry-run` passes
  for all thirteen as one invocation.

### Decisions

- [ADR 0001](docs/adr/0001-script-runtime.md): two adapters, mlua natively and
  piccolo on wasm. Superseded entirely by ADR 0004.
- [ADR 0002](docs/adr/0002-headless-ui-testing.md): the harness drives screens
  through Bevy's real picking backend, not a hand-rolled hit test.
- [ADR 0003](docs/adr/0003-glass-rendering.md): glass and backdrop blur are
  buildable on `bevy_ui` 0.19.1; blur stays an optional feature.
- [ADR 0004](docs/adr/0004-web-runtime-luaur.md): one script runtime, luaur, on
  every target; on wasm an uncaught Lua error aborts the module and the host
  restarts it.

### Known gaps

- Screen inheritance (`ScreenDef::inherits`) is parsed and not implemented.
- There is no `ScreenLoader` asset loader, so a game reads a `.screen.ron` file
  with `std::fs` and does not get hot reload for it.
- On `wasm32`, a Lua error a mod raises aborts the module rather than coming
  back as a `ScriptError`, and `pcall` in a script does not contain it. The host
  restarts; `examples/web-playground` shows how. ADR 0004.
- `slotted-test`'s `render` feature is declared and does nothing.
- `IconDef::Model` parses and warns; there is no glTF loader yet.
- No font files ship. Both new themes name their families in `tokens.fonts`
  and every text role points at a token, but the glyphs are Bevy's default
  face until a game drops the OFL files in or enables system font discovery.
- The GPU icon bake compiles for wasm and has not been run in a browser.
- `cargo deny check advisories` fails on `ttf-parser` (RUSTSEC-2026-0192),
  which arrives through Bevy's text stack. Left unignored on purpose.

[`docs/FOLLOWUPS.md`](docs/FOLLOWUPS.md) is the full list of what each phase
deferred.
