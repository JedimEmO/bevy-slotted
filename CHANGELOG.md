# Changelog

Notable changes to the slotted workspace. Every crate shares one version.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and
the project uses [semantic versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

Nothing yet.

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
- `slotted-script-mlua`: Luau through mlua, one sandboxed state per script, with
  interrupt budgets and a memory limit. Native only.
- `slotted-script-piccolo`: pure-Rust Lua through piccolo, fuel-budgeted, with a
  stdlib polyfill, a read-only-table sandbox and a manual value bridge. The only
  runtime that reaches wasm, because it reports a Lua error as a `Result`
  instead of aborting the module.
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
  `script-mlua`, `script-piccolo`, `blur`, `dev` and `viewport`.

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
  that has to reach a browser; the piccolo adapter's tests in headless Chrome;
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

- [ADR 0001](docs/adr/0001-script-runtime.md): mlua (Luau) natively, piccolo on
  wasm, luaur kept as the intended endgame.
- [ADR 0002](docs/adr/0002-headless-ui-testing.md): the harness drives screens
  through Bevy's real picking backend, not a hand-rolled hit test.
- [ADR 0003](docs/adr/0003-glass-rendering.md): glass and backdrop blur are
  buildable on `bevy_ui` 0.19.1; blur stays an optional feature.

### Known gaps

- Screen inheritance (`ScreenDef::inherits`) is parsed and not implemented.
- There is no `ScreenLoader` asset loader, so a game reads a `.screen.ron` file
  with `std::fs` and does not get hot reload for it.
- `slotted-script-piccolo` cannot be published while the workspace patches
  `piccolo` to a vendored copy; see `vendor/piccolo/VENDOR.md`.
- `slotted-script-luaur` is planned and not written.
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
