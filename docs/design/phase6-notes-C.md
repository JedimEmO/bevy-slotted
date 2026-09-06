# Phase 6 notes: package C

Mod tests, `test-mods`, the playground's Tests tab and `examples/machine`.
Everything here follows `docs/design/phase6-contract.md` section 3; this file
records the decisions the contract left open and every shared file C touched.

## Shared files touched (all additive)

| File | What |
|---|---|
| `crates/slotted-script-mlua/src/lib.rs` | loads `TEST_PRELUDE` after `PRELUDE` for `Stage::Test`, and adds `StdLib::COROUTINE` for that stage only (a test body is a coroutine). Data and control keep the surface they had; piccolo's `load_core` has always installed `coroutine`, so this is the one place the two sandboxes differ and it is invisible to a mod. |
| `crates/slotted-script-piccolo/src/lib.rs` | the same load, in the same env, before the readonly wrappers go on. |
| `crates/slotted-testutils/src/conformance.rs` | `STAGE: test` is now a legal directive. |
| `crates/slotted-testutils/conformance/test_stage{,_failure}.lua` | one case per direction of the protocol; both adapters run both. |
| `crates/slotted-test/Cargo.toml` | new `script-mlua` feature (`script` + `slotted/script-mlua`) so `cargo run -p slotted-test --bin test-mods` has a runtime; a consumer still inherits one through its own `slotted` dependency. |
| `examples/machine/Cargo.toml` | `slotted-ecs` and `slotted-ui` as dev-dependencies for `tests/ui.rs`. |
| `examples/web-playground/Cargo.toml` | `slotted-test` with `default-features = false, features = ["script"]`. |
| `examples/modded/mods/sorter/tests/sort.lua` | new file, so the playground's Tests tab has something real to run. Nothing else about that example changed. |

## Decisions

**A failed expectation is raised as a table, not a string.** `error("msg")`
prefixes the chunk and line, and the two adapters name chunks differently, so
a shared conformance case could not pin a message and a mod's report would
read differently depending on which runtime ran it. `slotted_test.lua` raises
`{ __slotted_test = message }` and unwraps it in `finish`; a real Lua error (a
typo in a test) still arrives with its position.

**`test-mods` loads the base pack's `data/<namespace>/` directories as
script-less mods.** A running game registers its own content before any mod
does. A harness has no game, so `assets/data/demo` would never be read and a
mod test's fixture could not name a single item. `UiHarness::mod_layout_with_base`
adds one synthetic manifest per base namespace (parsed from TOML, so the crate
needs no `semver`), skipping any namespace a real mod owns.
`UiHarness::mod_layout` is unchanged, so no existing test's load order moves.

**`test-mods` registers `<mods dir>/../screens/*.screen.ron`.** A mod's test
opens a screen by kind; a screen the *game* owns is in no mod. An example keeps
those beside its `mods/`, so that is where the binary looks. A directory
without them contributes nothing.

**`open_screen`'s fixture shapes.** `"empty"` is `MenuDef::generic(0)`; a
`{ slots, fill }` table with the shape `{n, 27, 9}` becomes `MenuDef::generic(n)`,
so a fixture that names a container plus the player's pockets gets the
quick-move routing and the hotbar a screen expects. Any other shape is a plain
run of slots per inventory with no routing.

**The machine's `sorter` sorts inventory 1, not 0.** The copy is otherwise the
modded example's mod. Inventory 0 of a machine is its own three slots, two of
which are not `Normal`; the player's pockets are inventory 1, which is the half
a sort button is for. `tests/sort.lua` names `minecraft:cobblestone`, the id
`assets/data/demo` actually registers.

**The furnace conserves items.** The result of cooking is the input item. A
demo furnace with no recipe table still has to conserve, and
`assert_conserved` in `tests/ui.rs` asserts exactly that. Burning a fuel item
does destroy one, so the conservation test sets `burn` through `SetProperty`
first and no coal is consumed while it runs.

**The playground runs tests against the live app** (the contract's choice).
`LiveTestRunner` performs one op per frame; `Settle` waits for two quiet
frames. Stated cost: playground tests are not isolated, and `open_screen` is
"the screen you have open", failing with `expected screen X, found Y`
otherwise.

## Assets added

`assets/locale/en-US.ftl` (base pack strings; the machine's screen is host
content, so its keys have nowhere else to live) and eight 16px placeholder
PNGs under `assets/icons/` for the redstone and face buttons. Both are new
files, neither replaces anything.

## Open

- `examples/machine/tests/snapshots/ui__the_furnace_screen_matches_its_snapshot.snap`
  pins package A's widget trees. If A changes a widget's children the snapshot
  moves with it; `cargo insta review` is the whole fix.
- `shots/machine.png` and `shots/machine-tab.png` are captured from
  `cargo run -p machine -- --shot`, which needs a display. In both, the
  `tab.rail` column sits detached to the right of the panel and each side tab
  header draws as a thin line: the tank, the bars, the icon buttons and the
  injected sort button all render, so this is package A's side-tab header
  sizing and rail role, not the example's layout. Reshoot with
  `just shot-machine` once that lands.
