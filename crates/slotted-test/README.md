# slotted-test

Drive a [slotted](https://github.com/JedimEmO/bevy-slotted) screen from
`cargo test`: no window, no GPU, virtual time.

This is a normal published crate, not a dev-only helper. A game depends on it
from its own `[dev-dependencies]` and writes tests against its own screens.

Clicks go through the real `bevy_ui` layout and the real `bevy_picking` backend,
so a test that passes here exercises the same path a player does. The harness
owns the three workarounds ADR 0002 found necessary without a renderer: it fills
the UI camera's `target_info`, relies on the facade's `HeadlessRenderAssets`, and
drives a `PointerId::Mouse` pointer because `Hovered` is hard-wired to it.

Nothing in `slotted-ui` knows this crate exists. Locators read the same semantic
components `bevy_a11y` reads.

## Main types

| Type | What it is |
|---|---|
| `UiHarness`, `UiHarnessBuilder` | The headless app. `settle()` advances virtual time until layout and motion are quiet, with a hard frame cap. |
| `Locator`, `by`, `describe` | Find a node by role, test id, tag, anchor, text or index. |
| `ScreenTree`, `TreeNode`, `ItemSummary` | The snapshot form, for `assert_tree_snapshot!`. |
| `MenuFixture`, `Opened`, `ScreenSource` | Opening a screen over a menu. |
| `ChestFixture`, `PlayerFixture`, `TestRegistries` | Ready-made content, so a test does not have to build a registry set. |
| `Browser` | Driving the item browser. |
| `LuaFixture`, `TestDriver`, `LuaTestReport` | Running a mod's `tests/*.lua` through the same harness. |
| `ReplayReport` | Replaying a recorded input session. |

## Example

```rust
use slotted_test::prelude::*;

let mut harness = UiHarness::builder()
    .plugins(SlottedPlugins::headless())
    .resolution(1280.0, 720.0)
    .theme("glass")
    .build();
harness.settle();

let slot = harness.find(&by::role(SemanticRole::Slot).tag("region", "chest").index(0));
harness.click(slot);
harness.settle();
```

## The `test-mods` binary

```
cargo run -p slotted-test --features script-luaur --bin test-mods -- examples/modded/mods
```

Runs every mod's `tests/*.lua` and prints a report. `cargo xtask test-mods <dir>`
and `just test-mods` are the shorthands.

It registers the game's own screens from a `screens/` directory beside the mods
directory, so a mod test can open one. `--screens <dir>` adds another, and may
be repeated: `sorter` injects into `slotted:any` and has a test file for the
chest and one for the furnace, whose screens live in two different examples.

## Feature flags

| Feature | Default | Effect |
|---|---|---|
| `snapshots` | yes | `assert_tree_snapshot!` through `insta`. Turn it off for a wasm build. |
| `script` | no | `UiHarness::load_mods`, the Lua `slotted.test` runner and the `test-mods` binary. The runtime comes from the consumer's own `slotted` dependency. |
| `script-luaur` | no | A runtime of this crate's own, so `cargo run --bin test-mods` works with no consumer to inherit from. |
| `render` | no | Run the real renderer with a software adapter and compare pixels. Not implemented yet. |

## Licence

MIT OR Apache-2.0, at your option.
