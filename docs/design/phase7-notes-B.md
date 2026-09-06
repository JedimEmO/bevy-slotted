# Phase 7, package B: documentation and release prep

What was written, what was found, and what is still in the way of a release.

## Written

**Root `README.md`.** Rewritten. The crate table was stale from Phase 1: every
row said "Planned" and it listed `slotted-script-luaur`, which does not exist.
Status is now the phase each crate landed in, and luaur is a named gap under the
table rather than a row pretending to be a crate. Added a 30-line quick start
for a game, a 20-line quick start for a mod, an examples table with `just`
recipes, six screenshot thumbnails, and a documentation index.

**One `README.md` per crate**, referenced from each `Cargo.toml`. Purpose, a
main-types table, a compiling example and the feature flags. They are not
`include_str!`-ed into the crate docs, so they carry no rustdoc-only syntax.

**`docs/guide/`.** Seven pages plus an index:

| Page | |
|---|---|
| `architecture.md` | The crate graph, the four ports, the five rules, the frame order, and a "where does this change belong" table. |
| `screens.md` | The `UiNodeDef` reference. Every node type and every field, from `def.rs`. |
| `themes.md` | Roles, materials, tokens, motion, and how to write a theme. |
| `modding.md` | The two stages, the events, the commands, injection, tooltips, the sandbox. |
| `testing.md` | The Rust harness for a game, the Lua harness for a mod, snapshots, recording. |
| `hot-reload.md` | What reloads, what survives, what happens when it fails. |
| `api/lua.md`, `api/slotted.d.luau` | Generated. |

**`CHANGELOG.md`** with a 0.1.0 section covering phases 0 to 6, the three ADRs,
and an explicit "known gaps" list. **`CONTRIBUTING.md`** with the contract-then-
implementation-then-review workflow, the four gates, the snapshot policy and the
FOLLOWUPS convention. **`LICENSE-MIT`** and **`LICENSE-APACHE`**, which did not
exist despite `license = "MIT OR Apache-2.0"` in every manifest.

**`docs/adr/README.md`** gained a note on what belongs in an ADR versus a doc
comment versus FOLLOWUPS, and on amending rather than rewriting a record. ADR
0001 moved from "proposed" to "accepted": both adapters shipped in Phases 4 and
5, so leaving it proposed was just stale.

## Generated documentation

The `slotted.*` surface is defined in one place, `crates/slotted-script/prelude/`,
and it now documents itself. Every public function carries a `---` block with
`@group`, `@signature`, `@stage`, `@param`, `@return`, `@example` and `@luau`.
Two new xtask subcommands parse them:

- `cargo xtask gen-docs` writes `docs/guide/api/lua.md`.
- `cargo xtask luau-stubs` writes `docs/guide/api/slotted.d.luau`.

Both take `--check`, `just gen-docs-check` runs both, and `just ci` and the CI
native job include it. A hand-edited or stale generated file fails the build.

The alternative was a hand-written reference beside the prelude. It would have
been wrong within a phase; this one cannot be, and writing the blocks caught
four errors in what a hand-written page would have said. `slotted.test`'s
`open_screen` takes an inline fixture table, not just a name; a Lua locator uses
a nested `tag` map and a snake-case `role`, not tag keys at the top level and a
PascalCase role; and `property_of` answers `{ id, value }`, not a bare number.

The stub file's event and command name unions are read out of `ScriptEvent::name`
and `ScriptCommand::name` rather than retyped. The per-event payload fields are a
hand-maintained table in `luau_stubs.rs`, because there is no derive to read them
from; the file says so.

`luau-analyze` is not installed here, so the stub file is unverified by a Luau
parser. It is generated from a fixed template with no free-form interpolation
except the declarations themselves, so the failure mode would be a bad `@luau`
tag rather than malformed syntax. Worth adding to CI when a Luau toolchain is
available.

## Release prep

Every publishable crate now has `readme`, `documentation`, `homepage`,
`keywords` and `categories`. `homepage` is inherited from `[workspace.package]`;
keywords and categories are per crate. Examples and `xtask` already had
`publish = false`.

`cargo publish --dry-run` **passes for all thirteen publishable crates**, run as
one multi-package invocation:

```
cargo publish --dry-run \
  -p slotted-model -p slotted-registry -p slotted-script -p slotted-ecs \
  -p slotted-theme -p slotted-icons -p slotted-ui -p slotted-browser \
  -p slotted-packs -p slotted-script-mlua -p slotted-script-piccolo \
  -p slotted-test -p slotted
```

Two manifest fixes were needed to get there, and both are the same shape.

**A dev-dependency carrying a version is written into the packaged manifest.**
`slotted-testutils` is `publish = false`, and the workspace dependency gave it
`version = "0.1.0"`, so packaging any crate that tests against it produced a
manifest demanding a crate crates.io will never have. Dropping the version makes
it path-only, which packaging strips.

**`slotted-test` was the same problem plus a cycle.** `slotted-ui` and
`slotted-browser` dev-depend on the harness, the harness depends on the facade,
and the facade depends on `slotted-ui`. Cargo allows that in a workspace because
dev-dependencies never enter a library's build graph, but a version in a
packaged manifest demands that `slotted-test` reach crates.io before
`slotted-ui`, which is impossible. Path-only fixes it, and nothing publishable
depends on `slotted-test` as a normal dependency.

Both wildcards are covered by `allow-wildcard-paths = true` in `deny.toml`,
which permits a version-less requirement only for a path dependency. A wildcard
on a registry crate still fails. `cargo deny check bans` passes.

Note that a single-crate `cargo publish --dry-run -p slotted-registry` still
fails with "no matching package named `slotted-model`", which is correct: none
of these are on crates.io yet. Cargo 1.96 resolves in-workspace when several
crates are published in one invocation, which is how the real publish will run.

**`slotted-script-piccolo` must not be published yet, even though it dry-runs
green.** The workspace patches `piccolo` to `vendor/piccolo`, which backports an
upstream fix: 0.3.3 panics when a table that has had a key removed is grown, and
a panic on wasm aborts the module. `[patch.crates-io]` is a workspace-local
thing that no consumer inherits, so a published 0.1.0 would resolve real
piccolo 0.3.3 and carry the bug. Verification passes because the code compiles
against it, not because it works.

The plan: publish the other twelve, and publish `slotted-script-piccolo` when
piccolo releases past 0.3.3 with the `set_value` fix, at which point the
`[patch]` block and `vendor/piccolo/` both go. Until then, `slotted` published
with default features is fine (`script-mlua` is the default) and the
`script-piccolo` feature is only usable from a git dependency.

## Rustdoc and CI

`RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps` passes with no
changes needed. `missing_docs` was already a workspace warning and the crates
were already compliant, which is why there was nothing to fix.

CI gained a `docs` job that builds rustdoc with warnings fatal and assembles a
site: the playground stays at the root so its URL does not move, rustdoc goes
under `/doc/` and the guide under `/guide/` with a generated index linking each
Markdown page to its GitHub-rendered version. The `pages` job now needs both
`wasm` and `docs`. The native job runs the two `--check` generators.

## Gates

| | |
|---|---|
| `cargo fmt` on this package's files | pass |
| `cargo clippy -p xtask --all-targets -- -D warnings` | pass |
| `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps` | pass |
| `cargo test -p slotted-script -p slotted-script-mlua -p slotted-script-piccolo` | pass, 53 tests, both runtimes |
| `cargo xtask test-mods examples/machine/mods` | pass |
| `cargo xtask gen-docs --check`, `luau-stubs --check` | pass |
| `cargo deny check bans` | pass |
| `cargo publish --dry-run`, 13 crates | pass |

`just ci` as a whole was red when this package finished, in
`crates/slotted-registry/src/icon.rs`, `crates/slotted-icons/src/plugin.rs` and
`examples/chest/tests/ui.rs`. Those are packages A and C mid-flight; none of
them is a file this package touched.

## For a later phase

- **The guide on Pages is raw Markdown behind a generated index.** A static site
  generator would render it properly. Not worth a dependency until the guide is
  bigger than seven pages.
- **The stub file is unverified.** Add a `luau-analyze --definitions` step to the
  docs job once a Luau toolchain is in the runner image.
- **`gen-docs` has no test.** The parser is exercised only by running it. A
  fixture test over a small Lua file would catch a regression in tag parsing.
- **The per-event payload fields in `luau_stubs.rs` are hand-maintained.** They
  will drift from `events.rs` eventually. Deriving them would mean parsing Rust
  struct variants in a dependency-free xtask, which is more machinery than the
  drift is worth today; a doc comment on the constant says so.
- **`ScreenDef::inherits` is documented as unimplemented in two places now**, the
  guide and the changelog. When it lands, both need editing.
