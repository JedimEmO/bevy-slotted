# Phase 5 notes, package B: wasm stack and web playground

Status: 2026-09-06. Bevy 0.19.1. Companions: `docs/PLAN.md` section 5.4 and
Phase 5, ADR 0001 (script runtime), ADR 0003 (glass rendering),
`docs/design/phase4-contract.md` sections 2.3 to 2.7.

Package B owns `examples/web-playground`, `tools/xtask`, the `wasm-check`,
`wasm-build`, `serve` and `playground` recipes, the CI `wasm` and `pages` jobs,
and the portability changes below.

## 1. What the wasm target already did

Every crate below the facade compiled for `wasm32-unknown-unknown` unchanged.
No feature gate, no `cfg`, nothing. The list `just wasm-check` runs:

| crate | features |
| --- | --- |
| `slotted-model`, `slotted-script` | default (none) |
| `slotted-registry` | `--no-default-features` (drops `std-fs`) |
| `slotted-ecs`, `slotted-theme`, `slotted-icons` | default |
| `slotted-ui`, `slotted-browser`, `slotted-packs` | default |
| `slotted` | `--no-default-features --features ui,browser,packs` |

Two things make that work and are worth not breaking:

- `std::fs` **compiles** on `wasm32-unknown-unknown`; every call just fails at
  runtime. So `LayeredSource`, `ModSet::discover` and the locale loader are
  compile-clean on wasm and merely useless there. The fix is a different source,
  not a `cfg`.
- The facade's `script-mlua` is a default feature, and mlua cannot target wasm
  at all (ADR 0001: Luau is C++, there is no C++ standard library for this
  target). A wasm build must pass `--no-default-features`. Cargo has no
  per-target defaults, so this stays a caller's job.

`slotted-packs/watch` (Bevy's `file_watcher`) and `slotted-theme/blur` stay off
in a wasm build. Blur is a second scene render per frame and WebGL2 is where
that costs most, so the playground has it behind its own `blur` feature, off.

## 2. Portability changes made

Three files in `crates/slotted-packs`, all additive. Native behaviour is
unchanged: the new path is only taken when a host inserts a resource that no
native example inserts.

- **`src/source.rs`** — new `PackAssets(SharedSource)` resource, where
  `SharedSource = Arc<dyn AssetSource + Send + Sync>`; new `SourceAssetReader`,
  the `pack://` `AssetReader` over any `AssetSource` rather than over the
  filesystem. `LayeredAssetReader::logical` became `pub(crate)` so both readers
  share it.
- **`src/lifecycle.rs`** — `run_data_stage`, `run_control_stage`,
  `load_control_scripts`, `read_script` and `populate_watch` now take
  `&dyn AssetSource` instead of `&LayeredSource`. New `pack_source(world,
  layout)` picks `PackAssets` when it is there and builds a `LayeredSource`
  otherwise. Two existence checks that used `LayeredSource::resolve` (a
  `PathBuf`, which an in-memory source does not have) became `read(..).is_ok()`.
- **`src/plugin.rs`** — `PackSourcePlugin::with_assets(source)`. It registers
  `pack://` against `SourceAssetReader` and inserts `PackAssets`, so the
  lifecycle and the asset server read the same bundle. `PackSourcePlugin` lost
  its derived `Debug` (a `dyn AssetSource` has none) and gained a hand-written
  one.
- **`src/locale.rs`** — `load_locales` takes `&dyn AssetSource` and, when the
  disk read fails, tries the source. Base is `locale/<lang>.ftl`; a mod is
  `locale/<mod id>/<lang>.ftl`. The per-mod form exists because the layered
  logical path `locale/<lang>.ftl` names *every* mod's file at once, which is
  fine when each mod is its own physical root and impossible for one flat
  bundle. The disk is tried first, so nothing native changes.

Nothing was needed for `Instant` (only `slotted-script-mlua` uses one, and it is
never in a wasm build) or for thread pools (`bevy_tasks` handles the single
thread itself).

## 3. Playground architecture

```
page (index.html, playground.js)
  │  list_mods / get_mod_file / reload_mod / drain_console   ← wasm-bindgen
  ▼
bridge.rs ──► bus::Bus (Arc<Mutex<..>>: requests one way, console lines back)
                │ drained in PreUpdate            ▲ filled in PostUpdate
                ▼                                 │
        ReloadMod message ──► slotted-packs ──► ScriptLog / ModFailed / ModReloaded
                                   │
                            PackAssets ──► bundle::EditableSource
                                              (InMemorySource behind a Mutex)
```

- **One source of truth for the mods.** `build.rs` walks
  `examples/modded/mods/` and emits a table of `(logical path,
  include_str!(absolute path))`. There is no second copy of any mod file, and a
  `cargo:rerun-if-changed` per file means editing a mod on disk rebuilds the
  playground.
- **The reload is the shipped reload.** `reload_mod` from the page queues a
  `Write` then a `Reload`; `PreUpdate` applies the write to the
  `EditableSource` and writes a `ReloadMod` message; `slotted-packs` runs
  `ModLoader::reload_mod`, which re-runs the data stage, freezes, remaps every
  live `ItemStack` by name and re-runs the control stage. That remap is what
  keeps the chest's 64 copper ingots across an edit, and
  `a_stack_keeps_its_item_across_a_reload` in `tests/plumbing.rs` proves it by
  adding an item that shifts every interned id.
- **The bus is a `Mutex`, not an `mpsc`.** Both ends need both directions: the
  world drains requests, the page drains console lines. It is also what makes
  the plumbing testable natively, which is where six of the seven integration
  tests run.
- **Assets over HTTP, mods in the binary.** `assets/` is copied beside
  `index.html` and Bevy's web asset reader fetches it; only the mods go through
  `pack://`, because only they are editable.

## 4. One runtime behind the port

`slotted-script-piccolo` (package A) was being written in parallel with this
work and had no `lib.rs` when it started, so the playground was developed
against `LuaLite` (`src/lualite.rs`), a tree-walking interpreter over the Lua
subset the three demo mods are written in.

**It is gone.** piccolo landed, passed its conformance suite and is now the
only runtime the playground has, on wasm and natively alike; the crate has no
`fake-runtime` feature and no default features left. Keeping a second
interpreter meant a second set of error messages, a second `wasm-check` line
and a Lua subset nobody was allowed to leave — for a fallback the page never
took. The stand-in did its job, which was to prove the port is really a port,
and the seven `tests/plumbing.rs` cases that passed under both are the evidence
it left behind. (The `FakeRuntime` in the `slotted-packs` tests is unrelated
and stays: it replays scripted commands and has no parser at all.)

Running piccolo natively as well is deliberate. It means the fourteen native
integration tests exercise the runtime the browser gets, so a failure in
`review_adversarial.rs` is a failure in the tab.

## 5. Sizes

`cargo xtask wasm-build` reports all three numbers on every build.

| step | before | after |
| --- | --- | --- |
| `cargo build --profile wasm-release` | 54.45 MiB | 53.63 MiB |
| after `wasm-bindgen --target web` | 48.35 MiB | 46.86 MiB |
| after `wasm-opt -Oz` | 26.46 MiB | **23.05 MiB** |

"After" is two changes together: `opt-level = "z"` rather than `"s"` in the
`wasm-release` profile, and dropping `bevy_scene` from the playground's Bevy
feature list, which nothing in the workspace uses. The profile already had fat
LTO, one codegen unit, `panic = "abort"` and `strip = "debuginfo"`; `panic =
"abort"` does not reach `wasm-bindgen-test`, which builds under `test`. Audio,
glTF and gizmos were never in the list. That is where the cheap wins stop: the
remaining 23 MiB is a whole Bevy renderer, and the next cut is Bevy's feature
surface rather than the profile.

A plain `--release` module was **82 MiB**, because the release profile keeps
debuginfo and Bevy instantiates a great many generics. The workspace now has a
`wasm-release` profile (`opt-level = "z"`, fat LTO, one codegen unit,
`strip = "debuginfo"`, `panic = "abort"`) that `xtask` uses; native release
builds are untouched. `wasm-opt` is optional — without it the page still works,
it is just twice the size, and requiring a binstall to see the playground would
be a bad trade.

The module is large because it is a whole Bevy renderer. About 1 MiB of it is
the `tonemapping_luts` feature, which is not optional in practice: without it
the camera's default `TonyMcMapFace` tonemapper errors at runtime and the 3D
scene never draws. It needs `zstd_rust` alongside, because there is no C
toolchain for this target.

## 6. Deviations from the brief

- **CodeMirror comes from esm.sh, not cdnjs.** Two things forced it. cdnjs's
  `codemirror` package at 6.65.7 is the CodeMirror **5** API, a UMD bundle
  exporting a `CodeMirror` global, not CodeMirror 6. And CodeMirror 6 is half a
  dozen packages that must share one copy of `@codemirror/state`; jsDelivr's
  `+esm` bundles each inline their own, which makes every `instanceof` check
  fail at runtime (`Unrecognized extension value in extension set`) — observed
  in headless Chromium before the switch. esm.sh resolves a shared dependency
  to one URL, so the copies are one copy. The page races the imports against a
  six-second timeout and falls back to a plain `<textarea>` when the CDN is
  unreachable; that fallback is a tested path, because it is what the failed
  jsDelivr attempt exercised.
- **`smoke.html` ships in `dist/`.** The end-to-end check is a page rather than
  a `wasm-bindgen-test`, because what needs testing is the whole app running
  under `requestAnimationFrame` with a real WebGL2 context, which a
  `wasm-bindgen-test` does not give. Shipping it means the deployed artifact can
  be checked in place.
- **The in-canvas console overlay does not draw in a browser.** It is the only
  console a native run has, so natively it still starts visible and `F1` still
  toggles it. In a tab the page shows every line in its own pane beside the
  canvas, and a second copy floating over the chest was only covering the demo.
  Two ways to ask for it back on a page with no pane of its own: `?console=canvas`
  in the URL, read before the app starts, or the `set_canvas_console` export,
  which goes through the bus like every other request and therefore works at any
  time. `smoke.html` exercises the export.
- **`subscribe_console` and `drain_console` both exist.** The page polls
  `drain_console` on an animation frame; `subscribe_console` is the push form
  over the same queue, for a consumer that would rather be called. A page
  should use one or the other, not both.

## 7. What was verified

**Natively** (`cargo test --workspace --all-features`), `examples/web-playground`
adds 22 tests: eight unit tests over the bundle and the bus, seven in
`tests/plumbing.rs` and seven in `tests/review_adversarial.rs`. All of them run
the real `ModLoader` over the bundled mods through piccolo.

`plumbing.rs` is the happy path:

- the three mods' `data.lua` really register `copper_chest:copper_chest`,
  `copper_chest:copper_ingot` and `demo:apple`, the screen tree reaches
  `Screens`, and `sorter`'s injection reaches `Injections`;
- `copper_chest`'s `control.lua` answers a `slot_click` with the expected line;
- writing new text into the source and calling `ModLoader::reload_mod` runs the
  **new** chunk;
- a chunk that will not compile leaves the previous registries standing and
  reports the error;
- a stack survives a reload that adds an item and therefore shifts every
  interned id, still naming `copper_chest:copper_ingot` afterwards — the
  remap-by-name of contract section 2.6, on the browser's own path;
- a bus request becomes a write plus a `ReloadMod` message, and a malformed mod
  id from the page is reported rather than panicking.

`review_adversarial.rs` is everything a person can do to the page by accident,
run through the playground's own two systems plus a transcription of
`slotted-packs`' `apply_reloads`, all in one `Update` so the ordering under
test is the ordering the browser gets:

- a control chunk that will not compile leaves the mod answering with the last
  good chunk, and the error reaches `drain_console`;
- `while true do end` in `data.lua` runs out of fuel, is reported with the word
  *budget*, and the registries and the control script are the ones from before;
- two reloads queued in the same turn both run, in order, each over its own
  text;
- an unknown mod id or file name is an `Err`, not a panic and not an empty
  string;
- a rejected mod id does not eat the requests queued behind it;
- the mod list and the bundled text are unchanged after two failed reloads;
- five reloads that each add and then remove an item definition conserve every
  stack in the chest, summed by item **name** — an `assert_conserved` in the
  playground's own terms, because the ids are re-interned ten times over.

Two of those failed when they were first written, and both were real:

1. **A control chunk that would not compile left the mod with no control script
   at all.** `load_control_scripts` unloaded every previous script up front and
   then loaded the new ones, so a mod whose new chunk failed had nothing left.
   In a tab that meant one typo silently stopped every click in the game, with
   only a console line to say so. It now loads first and unloads only what was
   really replaced, keeping the previous script — and its `tooltip_build`
   subscription — when the new one fails. `slotted-packs`' `lifecycle.rs` tests
   pin both halves: the kept script is still live in the runtime, and a
   *successful* reload still frees the chunk it replaced rather than leaking it.
2. **Two reloads in one frame collapsed into one.** `drain_requests` applied the
   whole queue before the reload ran, so the second edit's text overwrote the
   file the first reload had not read yet, and `apply_reloads`' `dedup` then
   made it one reload of the second chunk. `drain_requests` now stops at the
   first reload and requeues the rest for the next frame.

**In headless Chromium** (`just smoke`, `examples/web-playground/tests/smoke.mjs`,
a dependency-free CDP client over Node's global `WebSocket`), against
`dist/web-playground` served by `cargo xtask serve`. Nine checks on
`smoke.html`, all passing: the three original ones (`list_mods`, `get_mod_file`,
a reload whose chunk logs through `drain_console`) and six adversarial twins of
the native cases above, including the compile error, the runaway loop, the two
reloads in order and `get_mod_file` throwing on a name the page made up. Then a
tenth on `index.html` itself: replacing the editor's text and pressing the
page's own Run puts the new line into the page's console pane.

The browser log confirms the renderer came up on SwiftShader
(`AdapterInfo { .. backend: Gl, .. }`, `WebGL 2.0`) and that the mods loaded
(`opened copper_chest:chest`).

**By eye**, `just shot-playground` captures `index.html` at 1760x900 after 12
seconds. The screenshot shows the 3D scene, the glass chest screen with the
mod's items and their counts, the `Sort` button `sorter` injected into a screen
it does not own, the action rail, the item browser docked in the strip beside
the chest, the CodeMirror editor open on `copper_chest/control.lua` with its
lines wrapped, and the page's console pane. Three things were fixed by looking
at it: the editor opened on `appleskin_like/data.lua` rather than the file whose
edits change what is on screen; long comment lines ran off under a horizontal
scrollbar; and the stage was narrow enough (1.25fr of a 1400 px window) that the
item browser had no strip to dock in and simply vanished. Chromium here is
snap-confined, so both the profile and the screenshots are written under
`~/snap/chromium/common`.

Not verified: any browser other than Chromium; a real GPU (everything above ran
on SwiftShader); and the Pages deploy, which cannot run outside `main`.
