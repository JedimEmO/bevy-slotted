# ADR 0001: Script runtime strategy

Status: superseded by [ADR 0004](0004-web-runtime-luaur.md)
Date: 2026-09-05, superseded 2026-09-06

> **None of this decision holds any more.** ADR 0004 made luaur the only script
> runtime on every target and removed `slotted-script-piccolo`, the vendored
> piccolo and `slotted-script-mlua` from the tree. The owner weighed the abort
> described below and accepted it in exchange for one faithful, fast Luau
> everywhere; the page restarts the module instead. The toolchain warning below
> is history too: nothing in the workspace compiles C or C++ now, so
> `.cargo/config.toml` no longer pins `CC`/`CXX`.
>
> What survives is the reasoning, and one finding ADR 0004 builds on rather than
> contradicts: luaur raises Lua errors by panicking, and `wasm32-unknown-unknown`
> cannot unwind. Everything below is left as it was written.
Spike: `spikes/script-runtimes/` (README there has commands, versions, raw numbers)

## Context

`slotted-script` defines a `ScriptRuntime` port. PLAN.md 4.9 assumed two
adapters: `mlua` with the `luau` feature natively, and `luaur` 0.1.8 (pure-Rust
Luau) for `wasm32-unknown-unknown`, with luaur-only as the preferred outcome if
it held up. Scripts must run natively and in a browser tab, and the web
playground depends on the browser path.

Versions measured: luaur 0.1.8, mlua 0.11.6 with luau0-src 0.18.3+luau709,
piccolo 0.3.3, bevy 0.19.1, rustc 1.96.1.

## What the spike found

**luaur is a good native runtime.** All four operations work: load a chunk, call
a registered Rust function from Lua, call a Lua function from Rust, round-trip a
nested struct through serde. It works inside a Bevy 0.19 `MinimalPlugins` app
with the state in a `Resource`, over 100 frames. It is *faster* than mlua here.

| backend | cold start | call with a small table | serde round trip |
| --- | --- | --- | --- |
| luaur | 39.8 us | 265 ns | 4956 ns |
| mlua | 40.7 us | 308 ns | 7034 ns |

At 265 ns a call, per-hover and per-click Lua hooks are free.

**luaur's sandbox and limits are better than PLAN.md 4.9 assumed.**
`set_interrupt` (instruction budget) and `set_memory_limit` both work and both
abort a runaway script. The plan's "missing debug hooks, budgets via cooperative
yields" workaround is unnecessary.

**luaur is broken on wasm in one specific, disqualifying way.** It raises Lua
errors by panicking and catches them with `catch_unwind`.
`wasm32-unknown-unknown` has no unwinding, so in headless Chrome every raise
aborts the module: `error('boom')`, a runtime type error, and an in-VM `pcall`
around either. Only compile errors are recoverable. There is no Lua-level
workaround, and the `panic=unwind` build needs `-Z build-std` on nightly. For a
modding surface, where scripts are untrusted and buggy by definition, one bad mod
taking down the page is not acceptable.

**mlua cannot target wasm at all**, as expected. Luau is C++ and
`wasm32-unknown-unknown` has no C++ standard library: `fatal error: 'iterator'
file not found`.

**piccolo 0.3 works in the browser, including errors.** All three operations
plus fuel-limited execution pass in headless Chrome, and a runtime error comes
back as `StaticError` with the state still usable. It costs 987 KiB of wasm
against luaur's 743 KiB, and a much thicker adapter: no serde bridge, no
`Function::call`, GC-arena lifetimes, untyped table access, interned string keys.

**The mlua and luaur APIs are effectively identical.** The two backend modules in
the spike differ only in their imports, one `?` on `create_table`, and sandbox
setup. One shared Lua prelude and one shared serde event/command bridge serve
both. piccolo does not fit that shape.

**Both mlua and luaur share a sandbox footgun**: after `Lua::sandbox(true)`, a
write to `globals()` returns `Ok(())` and silently does nothing. Removals must
happen before the sandbox call. Separately, luaur ignores `StdLib` flags
entirely; anything but `StdLib::NONE` opens the whole safe stdlib.

## Options

1. **luaur only, native and web.** Simplest, one adapter. Rejected: script errors
   abort the browser module.
2. **mlua native, luaur web.** The plan's fallback. Rejected for the same reason.
   The web path is the one that has to tolerate bad scripts.
3. **luaur native, piccolo web.** Two adapters with genuinely different APIs, two
   preludes, a conformance suite that has to paper over a different value model.
4. **mlua native, piccolo web.** Same adapter count as 3, but the native side is
   the battle-tested C Luau and the web side is the one that recovers from
   errors.
5. **Wait for luaur.** The abort is a fixable bug upstream: catching errors at
   the VM boundary without unwinding is possible. luaur is 0.1.8 and one
   maintainer.

## Decision

**Recommend option 4 for now, and revisit as soon as luaur handles errors without
unwinding.**

- Native: `slotted-script-mlua`, mlua with `luau`, `sandbox(true)`,
  `set_interrupt` budget, `set_memory_limit`, dangerous globals removed before
  sandboxing.
- Web: `slotted-script-piccolo`, fuel-limited, manual value bridge.
- Keep `slotted-script-luaur` in the tree as a third adapter. It passes the
  native suite today and is the intended endgame: one pure-Rust runtime on both
  targets, no vendored C++ build, and it is the faster of the two natively.
- The conformance suite in `slotted-testutils` becomes load-bearing rather than
  nice to have. It is what makes the eventual swap to luaur-everywhere cheap, and
  it is what will tell us when luaur is ready.

The native choice is close and reversible. mlua wins on maturity only; luaur
wins on speed and on having no C++ build. If the toolchain fragility below bites
in CI, switch the native adapter to luaur.

## Consequences

- Two value models to maintain until luaur is fixed, and a prelude that cannot
  be literally shared with piccolo. This is the main cost of the decision.
- The web playground gets a runtime that survives a buggy mod, which is the
  point of a playground.
- A vendored C++ build on the native path. The spike hit this hard: an Anaconda
  toolchain on `PATH` produced a binary that dies with `SIGFPE` inside
  `Lua::new()`, because Luau's `kSizeClassConfig` global constructor never ran.
  CI must pin a known-good toolchain, and the failure mode is a crash, not a
  build error. Choosing luaur natively would remove this class of problem
  entirely.
- Web bundle carries ~987 KiB of wasm for the VM before wasm-opt.
- piccolo call cost was not measured, so the web path's per-call budget is
  unknown. Measure it before the playground ships.
- If luaur fixes error handling on wasm, collapsing to luaur-only removes an
  adapter, the C++ build, and 244 KiB of wasm. Worth tracking.

Not verified: piccolo call cost, cold start and memory limits; whether a
worker-per-script scheme could make luaur's aborts survivable; any backend under
a real mod workload; `wasm-opt` on the reported sizes.
