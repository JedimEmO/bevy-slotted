# Vendored piccolo

This is [piccolo](https://github.com/kyren/piccolo) 0.3.3, unpacked from
crates.io, with two patches. The root `Cargo.toml` wires it in with

```toml
[patch.crates-io]
piccolo = { path = "vendor/piccolo" }
```

so `slotted-script-piccolo` still declares `piccolo = "0.3.3"` and every build
in this workspace gets the patched source.

Only `src/`, `tests/`, the two licence files and the docs came across. The
examples, the `util` sub-crate and their `clap`/`rustyline` dev-dependencies
did not. `[workspace]` in the manifest makes this its own workspace root, which
is what a `[patch]` path inside another workspace's directory needs.

## Patch 1: a removed table key must not abort the process

`src/table/raw.rs`, in `RawTable::set_value`.

Removing a key from a piccolo table does not erase it. The entry becomes
`Key::Dead`, a tombstone holding the old allocation's address so that iteration
order survives. When the map part later grows, every key is rehashed, and
0.3.3 does that with

```rust
key.live_key().expect("all keys must be live when table is grown")
```

which panics on the tombstone. The array-growth branch a few lines above
already drops dead entries with a `retain`; the map-growth branch did not.

So in 0.3.3 this ordinary Lua aborts:

```lua
local t = {}
for i = 1, 64 do t["k" .. i] = i end
t.k1 = nil
for i = 65, 512 do t["k" .. i] = i end
```

That matters here more than it does in general. `slotted-script-piccolo` runs
untrusted mod scripts in a browser tab, and on `wasm32-unknown-unknown` a panic
is an abort of the whole module: one mod writing `t[k] = nil` would take down
the page. Recovering from a bad script is the entire reason ADR 0001 chose
piccolo over luaur.

The patch adds one line before the rehash:

```rust
self.map.retain(|_, v| !v.is_nil());
```

**This is upstream's own fix, backported.** piccolo `main` (checked at
`ce709eb1dae5c543cbc78e7e12bb80249d88c55f`) has restructured the code into
`RawTable::reserve_map`, which begins

```rust
// We always filter out all dead keys when growing the map.
self.map.retain(|_, v| !v.is_nil());
```

`tests/tombstone.rs` covers it: delete-then-grow, 10k insert/delete churn,
removing every key and refilling, and the same thing through the Rust API.

## Patch 2: a rustc lint that a vendored crate cannot ignore

`src/string.rs`, in `String::from_buffer`'s `Drop`.

`(*ptr).len()` on a `*const [u8]` autorefs through a raw pointer, which current
rustc rejects (`dangerous_implicit_autorefs`). A crates.io dependency is built
with `--cap-lints allow` and never sees it; a path dependency is treated as
local code and does. Rewritten as `ptr.len()`, the raw-slice length, which
reads the pointer metadata without forming a reference. No behaviour change.

## Patch 3: two style lints, allowed

`src/lib.rs`, a crate-level `#![allow(irrefutable_let_patterns,
mismatched_lifetime_syntaxes)]`. Same cause as patch 2 and no behaviour change:
uncapped lints on a path dependency, three warnings on every build of the
workspace. Allowed rather than fixed so the vendored source stays close to
upstream.

## How to get rid of this directory

Wait for the piccolo release that follows 0.3.3, point
`Cargo.toml`'s workspace dependency at it, delete `vendor/piccolo` and the
`[patch.crates-io]` section, and run
`cargo test -p slotted-script-piccolo`. Patch 1 is already upstream, so the
only question is whether patches 2 and 3 have landed too; if they have not,
patch 2 is a one-line PR and patch 3 disappears with it.

Nothing was reported upstream as part of this work: patch 1 was already fixed
there, and patch 2 is worth a PR if it is still present at release time.

## Licence

piccolo is MIT (`LICENSE-MIT`), with `LICENSE-CC0` covering the parts the
upstream README marks as public domain. Both files came across unchanged. The
patches above are contributed under the same terms.
