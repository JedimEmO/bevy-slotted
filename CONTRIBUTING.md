# Contributing

## Getting set up

```
rustup toolchain install stable          # rust-toolchain.toml pins the rest
cargo install just
cargo check --workspace --all-targets
```

On Linux, Bevy needs `libasound2-dev`, `libudev-dev` and `pkg-config`.

`.cargo/config.toml` forces a plain `cc`/`c++` toolchain. That is deliberate: an
Anaconda toolchain on the `PATH` corrupts the vendored Luau build into a runtime
`SIGFPE` rather than a build error, which cost a day to find once. See
[ADR 0001](docs/adr/0001-script-runtime.md).

For the web playground you also need `cargo install wasm-bindgen-cli` at the
version the lockfile pins, and optionally
[binaryen](https://github.com/WebAssembly/binaryen) for `wasm-opt`.

## The gates

Nothing merges that does not pass these.

| | |
|---|---|
| `just ci` | `cargo fmt --all --check`, clippy with warnings fatal, and the whole test suite. |
| `just wasm-check` | Every crate that has to reach a browser, on `wasm32`, with the feature set a wasm build actually uses. |
| `just test-mods` | The demo mods' Lua tests, through the headless harness. |
| `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps` | Rustdoc, with broken intra-doc links fatal. |

If you touched the Lua prelude, also:

```
cargo xtask gen-docs
cargo xtask luau-stubs
```

Both take `--check`, which is what CI runs. A stale `docs/guide/api/lua.md` fails
the build.

## The workflow

Work happens in phases, each one ending with something runnable. A phase has
three parts, in this order:

1. **A contract.** `docs/design/phaseN-contract.md`, written before any code:
   the types, the systems, the frame order, and what each package owns. Several
   people can then work in parallel without colliding, because the contract says
   who owns what.
2. **The implementation**, usually split into packages A, B and C that touch
   disjoint files.
3. **A review**, whose findings land in `docs/design/phaseN-notes-X.md` and in
   `docs/FOLLOWUPS.md`.

Read the contract for the phase you are working in before changing anything it
covers. If the contract is wrong, change the contract in the same commit.

## Where a change belongs

[`docs/guide/architecture.md`](docs/guide/architecture.md) has the table. The two
rules that matter:

- **No arrow points from a lower crate to a higher one.** `slotted-ui` must not
  learn about scripting. `slotted-script` must not learn about `bevy_ui`. If a
  change seems to need one, the thing you want probably belongs in the facade or
  in `slotted-packs`.
- **The domain stays free of Bevy and IO.** `slotted-model` and
  `slotted-registry` have no Bevy dependency, and that is what makes them fast
  to test and usable on a headless server.

Adding a script command means four files: the `ScriptCommand` enum, the Lua
prelude's `slotted.cmd` table, the host's validation in `slotted-packs`, and the
conformance suite in `slotted-testutils`. Both runtimes have to agree, and the
conformance suite is what proves they do.

## Tests

Every crate has unit tests. Beyond that:

- **The model** gets a test per click mode plus proptests for conservation.
- **A widget** gets harness tests through `slotted-test` for hover, focus,
  activate and, where relevant, drag, going through real layout and picking.
  Never a hand-rolled hit test.
- **A shipped `ScreenDef`** gets a `screen_tree()` snapshot.
- **A script change** goes through the conformance suite on every adapter.

Tests use virtual time only. No wall clock anywhere in `slotted-ui`, and nothing
sleeps. `settle()` has a hard frame cap and panics rather than hanging.

## Snapshots

`ScreenTree` snapshots hold roles, labels, tags, test ids and item summaries.
They deliberately exclude colours, sizes and positions, so a theme change does
not touch them and a structural change does.

Review every snapshot diff before accepting it. `cargo insta review` shows them
one at a time. Accepting a whole run with `INSTA_UPDATE=always` is right after an
intentional restructure and wrong at any other time: a snapshot that changed in a
change that did not mean to change the tree is the finding, not the noise.

A commit that updates snapshots says in its message which tree changed and why.

## Follow-ups

When you defer something, write it in [`docs/FOLLOWUPS.md`](docs/FOLLOWUPS.md)
under the phase that found it, in this shape:

- **What is true today**, in one bold sentence.
- Why it is that way, and what the options are.
- The phase that should pick it up, in bold.
- The test that pins the current behaviour, if there is one.

That last part matters. A follow-up with a test pinning today's behaviour is a
decision that was recorded; one without is a thing someone will rediscover.

Do not leave a `TODO` in the source instead. The file is the list.

## Documentation

- Public items are documented. `missing_docs` is a warning at the workspace
  root and rustdoc runs with `-D warnings` in CI.
- Crate-level docs say what the crate is for and what its main types are, with a
  compiling example. Doctests run in `cargo test`.
- The Lua API is documented in the prelude's `---` blocks and nowhere else.
  `docs/guide/api/` is generated; editing it by hand is a mistake the
  `--check` gate will catch.
- Prose style: short sentences, concrete nouns, no marketing. Say what something
  does and what it costs. If a decision was expensive, say why it was made.

## Commits

One logical change per commit, with a message that says what changed and why the
alternative was not taken. Prefix with the phase when it is phase work
(`phase 6: machine widgets, HUD layers, ...`).

## Licence

By contributing you agree that your contribution is licensed under MIT OR
Apache-2.0, matching the project.
