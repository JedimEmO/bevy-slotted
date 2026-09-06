# ADR 0004: luaur is the only script runtime, and an aborting wasm module is the price

Status: accepted
Date: 2026-09-06
Supersedes: [ADR 0001](0001-script-runtime.md) entirely
Spike: `spikes/script-runtimes/` (the luaur code that worked natively and in
Chrome, `tests/wasm_errors.rs`, and the README's notes on luaur's error model)

## Context

ADR 0001 put piccolo in the browser and mlua natively, and kept luaur in view as
"the intended endgame". Its reason for rejecting luaur was one finding:
luaur raises Lua errors by panicking and catches them with `catch_unwind`, and
`wasm32-unknown-unknown` has no unwinding, so on the web every raise aborts the
whole module. That was judged disqualifying for a modding surface.

Two things have changed since.

**The cost of piccolo turned out to be structural, not incidental.** piccolo has
no serde bridge, no `Function::call`, GC-arena lifetimes that stop values
crossing an `enter` boundary, untyped table access and interned string keys. The
adapter came to about 2,300 lines against mlua's 860 for the same port, with a
1,000-line stdlib polyfill of its own, because piccolo's Lua is not Luau and the
`slotted.*` prelude assumes Luau. Every prelude change had to be made twice, and
`docs/guide/modding.md` had to carry a section on where the two runtimes differ.
It also could not be published: piccolo 0.3.3 panics when a table that has had a
key removed is grown, so the workspace carried a vendored copy behind
`[patch.crates-io]`, and a patched dependency cannot go to crates.io.

**The owner has weighed the abort and accepted it.** A playground that dies on a
bad mod is recoverable by the page: catch the trap, build another module, put
the state back. A modding surface that is slow, or that behaves differently from
the Luau a mod was written against, is not recoverable by anyone.

luaur measures faster than mlua on this workspace's own benchmark, and is a
line-for-line port of Luau with an mlua-shaped API, so the two adapters differed
only in their imports.

| runtime | SlotClick round trip, release |
| --- | --- |
| luaur | 4.6 us |
| mlua | 7.2 us |

**And then the owner asked for one runtime, not two.** Keeping mlua as the
native default meant shipping two VMs, running the conformance suite twice to
stop the spare from rotting, and telling a mod author that the thing their
script runs on depends on where the game is played. mlua's only advantage was
maturity — it is the C++ Luau upstream ships — and it carried the vendored C++
build that ADR 0001 spent a paragraph warning about. With luaur passing the
whole suite, faster, and pure Rust, the second runtime was paying rent for a
guarantee the suite already provides.

## Decision

**`slotted-script-luaur` is the script runtime, on every target. There is no
other. `slotted-script-piccolo`, the vendored piccolo and
`slotted-script-mlua` are all removed from the tree.**

- One adapter, `slotted-script-luaur`, behind the facade's `script-luaur`
  feature, which is **on by default**, and `LuaurHostPlugin`, which is in the
  default plugin group. There is no per-target feature to set: a wasm build
  takes the same features a native one does.
- No C or C++ is compiled anywhere in the workspace any more.
  `cargo tree -e build --workspace --all-features` lists no build dependencies
  at all, so `.cargo/config.toml`'s `CC`/`CXX`/`CFLAGS` pin is gone and the
  Anaconda-toolchain failure ADR 0001 documents cannot recur. The file says why,
  and what would bring the pin back.
- The conformance suite in `slotted-testutils` stays generic over
  `&mut dyn ScriptRuntime` even with one implementation. It is the port's
  contract, it is what a game writing its own runtime checks itself against, and
  it is what made this swap cheap in the first place. All 32 cases pass.

**An uncaught Lua error aborts the wasm module, and the host restarts.** On
`wasm32`:

- `error(...)`, a runtime type error, an exhausted interrupt budget and a
  refused allocation all trap. They do not come back as `ScriptError`.
- `pcall` inside a script does not contain the error. luaur implements `pcall`
  through the same panic.
- A compile error is the one recoverable class, because that path never enters
  the VM.

Two pieces make that survivable, and `examples/web-playground` uses both:

1. `slotted_script_luaur::install_error_reporter` hands the host the error text
   and the traceback from inside the `xpcall` message handler, which Luau runs
   *before* it throws. The page therefore learns what crashed even though the
   module is about to die.
2. The page wraps every call into the module and watches for an uncaught
   `WebAssembly.RuntimeError` from the module's own animation frame. On one it
   says so, re-imports the glue under a cache-busting URL to get a fresh
   instance, and calls `restore_state` with the last `snapshot_state` it took.

## Consequences

- **Abort semantics, on wasm only.** One bad mod takes the page down and the
  page rebuilds it. A game shipping a browser build with untrusted mods must
  implement the restart path; the adapter's crate docs say so at the top and the
  playground is the reference. A native build is unaffected: luaur's
  `catch_unwind` works where unwinding exists, so `error()`, a runtime type
  error, an in-VM `pcall`, an exhausted budget and a refused allocation all come
  back as values and leave the state usable. That is not assumed — the adapter
  has a test per class, each followed by an event the same state must still
  answer.
- **One runtime, so no divergence to document.** A mod behaves the same in a
  browser and on a desktop, the `slotted.*` prelude is written once against
  Luau, and the guide has no "the two runtimes differ" section. The one
  remaining target difference is the abort above, and it is a property of the
  target rather than of the runtime.
- **No fallback if luaur breaks.** This is the real cost of deleting mlua. It is
  0.1.8 with one maintainer, and there is no second implementation to switch to
  in an afternoon. What there is instead is the conformance suite: 32 cases that
  any replacement has to pass, which is what turned the piccolo removal into a
  day's work. That is the mitigation, and it is deliberate rather than lucky.
- **The sandbox ordering is now a tested property, not a note.** luaur has the
  same hazard mlua had, reached differently: `sandbox(true)` installs a proxy
  global table whose `__index` falls through to the real environment, so setting
  a name to `nil` afterwards returns `Ok(())` and the script still sees the
  original. Adding a name after sealing *does* work, which is what makes the
  failure quiet. The adapter erases before sealing and pins both halves in a
  test, so the day luaur changes either one, the build says so.
- **Restart strategy.** The playground restores the chest from a named snapshot
  rather than a serialised world, because the restarted module rebuilds its
  registries from whatever Lua the editor holds now and an `ItemId` would not
  survive that. It re-applies the editor's scripts on the first restart and
  keeps the bundled ones on a second within twenty seconds, so a chunk that
  crashes while loading cannot spin the page forever.
- **The `pcall` caveat is a real limit on the modding contract.** A mod cannot
  defend itself on the web. `docs/guide/modding.md` says so.
- **Publishing is unblocked.** `[patch.crates-io]` is gone, so nothing in the
  workspace depends on a patched crate any more. The thirteen publishable crates
  dry-run green as one invocation, `slotted-script-luaur` among them; the
  single-crate form still fails for the reason `docs/design/phase7-notes-B.md`
  gives, which is that the sibling crates are not on crates.io yet. The list is
  twelve crates now, not thirteen.
- **The web bundle grew, which the spike did not predict.** The playground's
  `wasm-opt`-ed module went from 23.05 MiB to 26.46 MiB, about 3.4 MiB. The
  Phase 0 spike measured a standalone luaur binary at 743 KiB against piccolo's
  987 KiB, so this is not the VM core; it is the rest of luaur that a real
  embedding keeps live — the Luau lexer, parser and bytecode compiler, which
  piccolo's adapter never linked because the prelude was compiled a different
  way. Depending on `luaur-rt` rather than the `luaur` umbrella made no
  difference (26.47 MiB against 26.46), so the analyser and the module resolver
  were already being dropped by the linker. The module is a whole Bevy renderer
  either way and 3.4 MiB is 15% of it; it was not enough to change the
  decision, and it is recorded in `docs/FOLLOWUPS.md` as something to measure
  again if the bundle ever becomes the binding constraint.

## Not verified

- Whether a worker-per-script scheme could contain an abort rather than
  restarting the page. Not needed for a playground; it would matter for a game
  that ships many untrusted mods to a browser.
- luaur under a real mod workload larger than the three demo mods.
