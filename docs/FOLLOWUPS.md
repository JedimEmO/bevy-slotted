# Follow-ups

Decisions deferred during phases. Each entry names the phase that should pick it up.

Entries the gap-closing round closed have been deleted rather than struck
through; what closed them is recorded in `docs/design/gaps-notes-{A,B,C}.md`
and summarised below. What is left here is open.

## Gap-closing round, 2026-09-06

Three packages landed together and were integrated and reviewed as one round.
The notes are `docs/design/gaps-notes-A.md` (authority, validation, the
networked adapter), `-B.md` (UI, theme, browser) and `-C.md` (icons, fonts,
advisories, wasm size). What changed, in one list:

- **`slotted-net`.** A new crate: `ClientMessage`/`ServerMessage`, a predicting
  `RemoteAuthority` that implements `slotted_model::Authority`, an
  authoritative `MenuServer` that applies every click to a scratch copy at
  `ValidationLevel::Always`, and a `Transport` port with an in-process
  `Loopback` that can add latency, reorder and drop. A `bevy_replicon` adapter
  is designed in notes A section 6 and not built.
- **Hints out of the inventories.** `MenuState::hints`, keyed by menu slot, and
  `slotted_model::slot_view` as the one accessor for "what does this slot
  show". A recipe filter no longer inflates `Inventories::count_of`.
- **`ValidationLevel`.** `Off`/`Debug`/`Always`, chosen by `Authority::validation`
  rather than by a global.
- **Resync.** `Authority::request_resync` and `AuthorityEvent::Slot`, so a
  refused submission can ask for the truth instead of repainting from a local
  copy.
- **`ComponentPatch` across a reload.** Patch keys are rebuilt by name beside
  the item; a key the new registry lost is reported as
  `ModError::ComponentVanished`.
- **Sizes are theme tokens.** `Tokens::sizes` carries `slot_size`, `slot_gap`,
  `panel_width`, `card_width`, `card_height` and `chrome_height`. `UiUnits`
  converts the things Bevy measures in another space; a widget never multiplies
  a token by `UiScale`.
- **One virtual grid.** The browser's card grid registers a `VirtualGridSource`
  and `slotted-ui` owns the pooling, so there is no second implementation.
- **Screen inheritance and the `.screen.ron` loader.** `inherits` and `remove`,
  merged by node id, with cycles and missing ancestors reported rather than
  panicked; `ScreenDef` is a Bevy asset and an edit respawns the open screen.
- **glTF icons.** `IconDef::Model { path, stand_in }` takes an atlas cell like
  any other icon; with the `gltf` feature the GPU rig loads the scene, and
  without one the declared stand-in (or a box hashed from the path) is what the
  CPU bake draws.
- **Fonts.** Five OFL families under `assets/fonts/<family>/` with the
  `OFL.txt` each was published with, wired into the three themes.
- **`cargo deny` in CI.** A `just deny` recipe, in `just ci` and in the
  workflow, with a dated single-id ignore for RUSTSEC-2026-0192.

### Found and fixed during the integration review

- **The atlas was published while the rig was clearing it.** The GPU bake's
  camera cleared its render target to transparent on every frame it drew, and
  that target *was* the image the UI sampled. A mesh pipeline compiles
  asynchronously, so on a cold shader cache the atlas was wiped and not
  refilled before the chest example's fixed two-second capture: five captures
  gave one blank and four good ones. The rig now warms its pipelines on a
  scratch target and only switches the atlas camera on once every pipeline
  behind the view is compiled, so the clear and the refill happen in one frame.
  Twenty-five captures over five cold-cache rounds of `just shot-chest` all
  show icons. `tools/check-shot.py` (`just shot-check`, and run by
  `just shot-chest`) measures the icons in the captured pixels, and
  `slotted-icons/tests/review_gaps.rs` holds the headless half: every baked
  cell has ink in it, and the rig's target starts as the CPU bake rather than
  as zeroes.
- **A lost resync request was never repeated.** `Transport::send` reports `Ok`
  for a message the link then drops, which is what an unreliable transport
  does, and `RemoteAuthority` marked the menu as awaiting a resync on the send.
  One dropped `RequestResync` left the client waiting for a container that was
  never coming. Resync requests are now retransmitted on the same timer clicks
  are. Pinned by
  `slotted-net/tests/review_adversarial.rs::a_resync_request_the_link_eats_is_asked_for_again`.
- **`report_unmatched_injections` deduplicated with `Vec::dedup`,** which only
  collapses adjacent equal entries, so two mods aiming at the same missing
  anchor with a third injection registered between them were reported twice.
- **`resolve_loc_text` lived in `slotted-packs`.** `slotted-ui` owns `LocText`
  and the `Localization` port, but the system that repaints a label when the
  catalogue is replaced was registered by the packs plugin, so a game using
  `slotted-ui` with its own localiser got labels frozen at spawn time. The
  system moved to `slotted-ui::loc` and `SlottedUiPlugin` runs it;
  `slotted-packs` re-exports the name it published. It also now puts the key
  back when a key stops resolving, instead of leaving the previous language's
  text on screen.
- **The browser's status line resolved two locale keys and formatted a string
  every frame,** for every open browser, and threw all of it away: the write
  was guarded, the work was not. It is now composed only when the index state,
  the visible count or the catalogue moves.

### Left open by the round

- `bevy_replicon` has no adapter; the design is notes A section 6.
- The two bakes still author their *geometry* twice (see below).
- A model's stand-in cannot be derived from the glTF; it is declared or it is a
  box.
- The `ttf-parser` ignore has a review date (2027-03-06), not a fix.
- The GPU render tests in `slotted-icons` cannot run even with a GPU. libtest
  runs each test on a spawned thread, winit refuses to build an event loop off
  the main thread, and without `WinitPlugin` `RenderPlugin` leaves no
  `RenderDevice` in the main world, so `bevy_pbr`'s own systems panic on the
  first frame. `tests/gpu_bake.rs::the_rig_actually_draws_into_the_atlas` is
  marked `#[ignore = "needs a GPU"]` and fails when it is run with `--ignored`.
  The evidence for the render path is `just shot-chest` plus `just shot-check`
  until a headless render harness exists. **Phase 8.**

## From Phase 2

- **`SemanticLabel` on a slot reads the item's namespaced id, not its display name.** A screen
  reader hears "minecraft:cobblestone x64" while the tooltip beside it says "Cobblestone".
  `slotted_ui::item::semantic_label` should prefer `ItemDef::display_name` and fall back to the id,
  once localisation exists to say which name is the player-facing one. **Phase 3** (localisation),
  which also owns `LocKey` resolution: the same snapshot shows `Text` nodes labelled `chest.title`
  and rail buttons labelled `sort`. Changing this rewrites the three accepted `ScreenTree`
  snapshots in `slotted-test`.
- **`UiNodeDef::{Tank, Bar, SideTab, Viewport, VirtualGrid}` spawn a layout-only placeholder.**
  `SpawnCtx::spawn_placeholder` gives them a `Node` and `SemanticRole::Custom(<name>)` so a screen
  using one still opens and still lays out. **Phase 6** implements them; the property-bound ones
  (`Tank`, `Bar`) need the `MenuProperty` binding that Phase 2 only mirrors.
- **`slotted_icons::LiveIcons` returns `IconRef::Missing`.** The `live` feature compiles the type
  but there is no offscreen item-model bake yet. **Phase 6**, together with `Viewport`.
- **`Favorite` is mirrored as a marker component but is not a theme role swap.** The glass theme
  defines `slot.favorite` and nothing selects it. **Phase 3**, with the rest of the slot state
  machine.

## From the chest example

- **`SlottedPlugins::default()` is not enough on top of `DefaultPlugins`.** `slotted_ui`'s
  navigation systems need `TabNavigationPlugin` and `DirectionalNavigationPlugin`, and
  `DefaultPlugins` ships neither, so a windowed game panics on its first frame with
  `manual_directional_navigation ... Resource does not exist`. `examples/chest/src/main.rs` adds
  them by hand. The facade should either add them itself or ship a `SlottedPlugins::windowed()`
  companion to `headless()`. **Phase 3**, and it is a five-minute fix.
- **Nothing renders a rail button's or a text node's label in a human language.** `LocKey` is
  shown verbatim and rail buttons are labelled `quick_stack`, so the example carries a
  `fill_labels` system that substitutes strings by `test_id` and by the `action` tag. That is a
  reasonable thing for a game to own, but not for every game to have to write. **Phase 3**, with
  localisation; the same entry already appears above for `SemanticLabel`.
- **The durability bar reads `slotted:damage` and `slotted:max_damage` off a stack's patch, but a
  component key only exists if some `ItemDef` declares it.** The demo's three tools each list both
  keys in `components` purely so the freeze interns them. A first-class "component types" registry,
  or interning the keys the renderer needs unconditionally, would remove the trap. **Phase 3**.
- **`slotted_test::fixtures` cannot be reused by a data-driven example.** `TestRegistries::basic()`
  builds its item table in Rust and freezes it into its own dense ids, so an example that loads
  items from RON has to duplicate the table. The item rows want to live in a shared `assets/data/`
  directory that both the fixtures and the examples read. **Phase 3**.
- **`examples/chest/assets` is a symlink to the workspace `assets/`.** Bevy's `AssetPlugin`
  resolves against `CARGO_MANIFEST_DIR`, so without it `cargo test -p chest` cannot find
  `themes/glass.theme.ron`. A `UiHarnessBuilder::asset_root(path)` would let a consumer point the
  harness at their own asset directory instead. **Phase 3**.

## From the Phase 2 adversarial review

- **Bevy gives every `ImageNode` its own `AccessibilityNode`.** The contract calls the icon child
  of a slot purely visual and gives it no `SemanticRole`, but AccessKit still announces 27
  anonymous images inside the chest grid. The semantic tree and the AccessKit tree therefore have
  different node counts, and a screen reader hears the difference. Suppressing the child's node
  (or labelling it) belongs with the rest of the a11y pass. Pinned by
  `slotted-ui/tests/review_adversarial.rs::every_interactive_node_is_semantic_and_reaches_accesskit`.
  **Phase 3**.
- **Two motion presets are still unused.** `MotionPreset::Hover`, `Press`, `DropSquash` and
  `FlyToSlot` are wired into slots by `slotted_ui::motion`, but `Stagger` (children appearing one
  after another when a screen opens) and `Fade` (tooltip and panel entry) have no caller, and
  buttons animate nothing at all. `SQUASH_SCALE` and friends are constants in `slotted-ui` rather
  than theme tokens, so a theme cannot say how far a slot pops. **Phase 3**, with the slot state
  machine and the `SLOT_SIZE` token above.
- **A screen and its tooltips live in different trees.** Tooltips are spawned under
  `TooltipLayer`, not under the screen root, so despawning a screen used to leave its tooltip on
  screen forever. `despawn_orphan_tooltips` now sweeps them. The same shape applies to anything
  else a screen parks in a shared layer; a "owned by screen" relationship would be sturdier than
  a sweep. **Phase 3**.

## From Phase 3

- **`BrowserRuntime::visibility_version` invalidates nothing.** `apply_search` recomputes only
  when the query changed, the index landed or `HiddenEntries` changed; a bump of
  `visibility_version` alone leaves `visible` stale even though the field is part of the
  `SearchCache` key. Nothing in Phase 3 writes it (cheat mode and dev items are Phase 6), so no
  test can fail today. Add it to the dirty check when the first writer arrives. **Phase 6**.
- **The recipe view's header changes the contract's tree order.** Section 7 pins
  `RecipeView > Tab*, RecipeSlot*, Button[browser.transfer], Button[browser.back],
  Button[browser.forward], Panel[browser.uses]`. The moodboard's header puts the back button and
  the focus's title above the tabs, so `browser.back` now comes first and `browser.forward` sits
  beside `+` under the recipe. Amend section 7 to match, or move the header behind a theme
  option. **Phase 4**, with the first contract revision.
- **`chrome_height` is a declared number, not a measurement.** The card grid's row count is
  derived from a 190-unit allowance for the search field, chips, bookmarks and footer. It is a
  theme token now, so a theme that changes those font sizes can say so, but it still has to know
  to; deriving rows from the grid's own `ComputedNode` after the first layout would be exact and
  would need no token at all. Half closed in the gaps pass, package B. **Phase 8**.
- **The dev feature is still empty.** `slotted-browser`'s `dev` feature is declared but the
  exclusion highlighter, the id tooltips and copy-recipe-id are not implemented (notes B, section
  10). **Phase 6**.
- **`ScreenHandler::clickable_areas` has no caller.** No Phase 3 handler returns one and nothing
  reads them; the furnace arrow they exist for is a Phase 6 screen. **Phase 6**.

## From Phase 4

- **The per-frame script budget is a constant, not configuration.** `route::MAX_SCRIPT_CALLS_PER_FRAME`
  is 512 calls. `PacksConfig` has no field for it and its shape is shared, so making it
  configurable means amending the contract. **Phase 5** (packs notes B, item 10).
- **An untyped payload loses the width of its numbers.** Every def is now read through
  `slotted_model::Value`, whose only numeric variants are `i64` and `f64`, so a widget `params`
  payload that RON parsed as `U8(54)` comes back as `I64(54)`. Nothing reads a payload's numeric
  width, and both sides of a payload comparison go through the same conversion, so this shows only
  if something starts comparing a payload against a freshly parsed `ron::Value`. **Phase 6**.
- **A resource pack cannot override a script that lives at a mod's root.** `read_script` tries
  `scripts/<mod id>/<entry>` through the layering first and falls back to `<mod root>/<entry>`;
  `ModWatch` registers a handle only for the first form, so a root-level script reloads through an
  explicit `ReloadMod` and not the file watcher. **Phase 5** (packs notes B, item 4).

## From Phase 5

- **The item browser's status line wraps when the panel is squeezed.** On a canvas narrow enough
  that the free strip beside the chest is under about 260 px, `dock::choose` still docks the panel
  and shrinks it to one or two card columns; "6 items" and "R recipes / U uses / A bookmark" then
  wrap onto three lines each and the footer looks broken. It is legible, not wrong, and the fix is
  in `slotted-browser`'s status line rather than in the playground: either a narrow form of the
  hint text or a minimum width below which the panel hides. Seen in the playground screenshot at a
  1400 px window. **Phase 6**.
- **`wasm-opt` is still where most of the module goes.** Re-measured in the gap pass, package C:
  62.24 MiB out of `cargo build`, 54.39 after `wasm-bindgen`, 26.57 after `wasm-opt -Oz`. The
  profile is as small as the knobs make it (`opt-level = "z"`, fat LTO, one codegen unit,
  `panic = "abort"`, debuginfo stripped); `strip = "symbols"` is not one of them, because
  wasm-bindgen reads the symbol names and `wasm-opt` drops the name section afterwards anyway.
  Splitting the Bevy feature list by target was tried and saved ten kilobytes. The module is a
  whole Bevy renderer and the next real cut is Bevy's own feature surface. **Phase 8**.

## From the Phase 6 design

- **`Injections` has one wildcard (`slotted:any`) and no per-screen-type filter.** A mod that
  wants "every container screen but not settings pages" has no way to say so. Add an
  `InjectionTarget` enum when the second filter shape appears. **Phase 7**.

## From Phase 6

- **The side tab's header and content were spawned as siblings of the tab root, not its
  children.** `spawn_side_tab` used `SpawnCtx::spawn_node`, which parents to the *rail*, so a
  closed tab was a 44x2 line with two loose nodes beside it and an open tab's content sat in the
  rail's flow. Fixed here by parenting both to the root. What made it survive review is that every
  assertion the contract names (`side_tab_open`, the content's `Visibility`, the exclusion zone)
  is true of a detached content node too; only the screenshot showed it. A widget whose children
  matter should assert its own tree shape, not only its state. **Phase 7**: a
  `assert_widget_tree!`-style helper, or a debug check that a widget's spawned entities all
  descend from the root it returned.
- **`measure_side_tabs` writes the tab root's height every frame it disagrees.** A closed tab's
  content still takes part in layout (it is hidden and clipped, not removed), so the root cannot
  inherit its height and the system has to assert it. Removing the content from layout while
  closed (`Display::None`) would be cheaper and would drop the system, but it also drops the
  measurement `open_width` is computed from. **Phase 7**, with a measure-once cache.
- **A recording refuses to replay at another resolution or scale factor.** Contract 2.3 asked for
  a warning; a warning let a drifted replay report every input delivered and pass green while
  clicking the wrong nodes. `ReplayError` gained `Resolution` and `ScaleFactor`. The real fix is a
  recording that stores a locator beside each pointer position, so a replay can say "this click
  was meant for the sort button" and survive a layout change. **Phase 7**.
- **The built-in `hotbar` HUD layer now sets `hide_with_screen`.** A screen draws the player's
  hotbar row itself, so leaving the HUD one up put two hotbars on the glass. The contract listed
  `hide_with_screen` only against the crosshair; this is the deviation.
- **A tank carries a `BarText` readout child.** Contract 1.2 gave the tank a `SemanticLabel` and a
  tooltip and no visible reading, which left the machine screenshot with an unlabelled column of
  blue. The stacked three-line label (`tank_label`) is what fits a well one slot wide. A tank wide
  enough for the one-line form should use it; nothing measures that yet. **Phase 7**.
- **`open_screen` inside a Lua mod test closes whatever screen was open.** Two tests in one file
  each opening the same screen left two screens in the world, and every locator in the second test
  matched twice. `just test-mods examples/modded/mods` was red on exactly this. The headless driver
  now closes first; `LiveDriver` still means "the screen you have open", which is the contract's
  choice and unaffected.
- **`UiHarness::mod_layout` panics on a malformed `mod.toml`.** `test-mods` therefore aborts with a
  panic message rather than printing an error and exiting 1. The message names the file and the
  missing field, so it is actionable, and the exit code is still non-zero. Turning it into a
  reported error means a fallible `mod_layout`, which every existing caller would have to unwrap.
  **Phase 7**.
- **A tank warns once per unknown fluid id, through a marker component.** `UnknownFluidWarned`
  exists because the warning lives on the render path, which runs every frame. A crate-wide
  warn-once utility would be better than a component per case. **Phase 7**.

## From Phase 7 package C

- **The CPU bake and the GPU bake still author their *geometry* twice.** Narrowed in the gap pass,
  package C. What a cell draws is now one description, `shape::CellDraw`, read by both bakes, and
  the view angle (`shape::view_rotation`) and the cell fill (`shape::cell_fill`) moved beside the
  polygons that are authored to match them, so colour, orientation and size are decided once.
  What is left is the drawing itself: flat polygons in `shape.rs` against `Mesh` primitives in
  `gpu.rs`, so a seventh `ShapeKind` has to be added to both. Generating the silhouette by
  projecting the mesh would remove that, and it means moving `Mesh` construction into the
  no-renderer path and re-accepting every atlas snapshot. **Phase 8**.

## From the Phase 7 review

- **`Theme::missing_roles()` cannot see a missing child role.** A role falls back to its parent, so
  a theme that drops `tank.fill` still paints it, in the tank's own material, and completeness
  reports nothing. Only the cross-theme key-set comparison catches it, and nothing would catch a
  role all three themes dropped together. A `roles::ALL`-shaped check that distinguishes "resolved
  through a parent" from "declared" would. **Phase 8**, and documented in the guide meanwhile.
  Pinned by `slotted-theme/tests/review_adversarial.rs::a_theme_missing_a_role_is_caught_and_the_role_is_named`.
- **The machine's energy bar draws its readout over its own fill.** White text over the orange fill
  is legible but low contrast where the fill edge crosses a digit. A `bar.text` role that swaps
  colour over the filled part, or a readout beside the bar the way the tank's now is, would fix
  it. Cosmetic, present since Phase 6. **Phase 8**.

## From the gaps pass (package B)

- **A Lua test's pointer gesture now settles first.** `t.click`/`t.hover` in a mod's test file
  aimed at the node's rect as it stood, and the frame the fonts landed every label re-measured and
  the button moved out from under the pointer: `examples/machine`'s `sorter/sort.lua` clicked
  nothing and reported "control.lua logs the sort". `LuaTestDriver::act` settles before it
  resolves a locator. The deeper shape is that a gesture is delivered at a position while an
  assertion is written against a node, so any asynchronous relayout can separate them. A pointer
  action that re-reads the rect between the move and the press would be sturdier than settling
  first, and a recording that stores a locator beside each position (the Phase 6 entry above) is
  the same fix seen from the replay side.
- **`slotted_ui::Decorative` is honoured by the harness, not by the renderer.** It says "leave this
  node out of the settled fingerprint", which is a testing concept living on a rendering
  component. Nothing else reads it yet. If a second consumer appears (a screenshot that waits for
  motion to stop, say), it wants a name that is about the node rather than about the harness.

## From the luaur swap (ADR 0004)

- **There is no second script runtime any more, and luaur is 0.1.8 with one
  maintainer.** ADR 0004 deleted mlua deliberately, and the mitigation is the
  32-case conformance suite: it takes a `&mut dyn ScriptRuntime`, not a concrete
  type, so a replacement has a definition of done on day one. That is what made
  the piccolo removal a day's work. Worth revisiting only if luaur goes
  unmaintained or a case it cannot pass turns up; the thing to watch is the
  upstream repository, not this file.
- **luaur costs about 3.4 MiB of optimised wasm more than piccolo did.** Investigated in the gap
  pass, package C, and not attempted: `luaur-rt` compiles every chunk from text
  (`ChunkMode::Binary` is documented as unsupported) and depends on `luaur-compiler`
  unconditionally, so shipping a compiler-less VM needs two upstream changes; and the playground
  edits Lua in the browser, so it needs the compiler at runtime regardless. The design, and what
  would have to land upstream first, is in `docs/design/gaps-notes-C.md` section 6. The
  playground's module went from 23.05 MiB to 26.46 MiB. It is not the VM core
  (the Phase 0 spike measured luaur smaller than piccolo standalone) but the
  Luau lexer, parser and bytecode compiler a real embedding keeps live.
  Depending on `luaur-rt` instead of the `luaur` umbrella changed nothing, so
  the analyser was already being dropped. If the bundle ever becomes the binding
  constraint, the thing to try is precompiling each mod's chunk to bytecode at
  build time and shipping a VM without the compiler, which luaur's layering
  (`luaur-compiler` beside `luaur-vm`) would allow.
- **A trap inside the module's own animation frame is detected by a `window`
  error listener, not by the call wrapper.** `web/playground.js` guards every
  call it makes into the module, but Bevy drives its loop from a
  `requestAnimationFrame` the module registered itself, so the common case
  surfaces as an uncaught error and is matched by a string test
  (`/RuntimeError|unreachable|out of bounds|table index/i`). A page that threw an
  unrelated error whose text matched would restart for nothing. A narrower
  signal would be the module marking itself dead before it traps, which needs a
  hook luaur does not currently offer.
- **The restart rebuilds the whole app, not just the Lua state.** Everything the
  visitor did outside the chest — the camera, the console history, an open
  browser panel — is gone. Only the open menu's inventories are carried across.
  Restoring more would mean serialising more of the world, which is a bigger
  contract than a playground needs; a game that ships a browser build with
  untrusted mods will want to decide this for itself.
- **`install_error_reporter` is a process-wide `OnceLock`.** One host, one
  reporter, set at start-up and never replaced. That is the right shape for a
  page and the wrong one for a test that wants to assert on reports, which is
  why the adapter has exactly one test that installs one.
- **The reporter fires natively too, for errors that are also returned as
  `Err`.** A native host that installs one sees each error twice. Gating it on
  `cfg(target_arch = "wasm32")` inside the adapter would fix that and would also
  stop a native test from covering the path, so it is documented rather than
  gated.
