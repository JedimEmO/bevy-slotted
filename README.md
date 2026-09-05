# slotted

Minecraft's inventory model and Minecraft's modding freedom, with a modern skin.
slotted is a workspace of Bevy plugins built around one idea: the thing that
made Minecraft's UI endlessly extensible was not its graphics, it was that every
screen is a list of slots and every interaction is one of seven click modes over
that list. Implement that once, server-authoritative with client prediction, and
a chest, a furnace, a quest book and a machine with three fluid tanks are all the
same machinery wearing different clothes.

On top of that model sits a widget and theme layer covering what the mod
catalogue actually needs: slot grids, virtual grids, tanks and bars, side tabs,
composed tooltips, HUD layers, a 3D viewport widget and a carried-stack layer. An
item and recipe browser layers over any screen through a screen-handler registry.
A Lua modding surface gives mods the reach they have in Minecraft, including
injecting widgets into screens they do not own, and it runs both natively and in
the browser. Every screen is drivable headless, so a game's own test suite can
click a slot by semantic role and assert on the result.

## Crates

| Crate | What it does | Status |
|---|---|---|
| `slotted-model` | Domain: ids, stacks, inventories, menus, click state machine. No Bevy. | In progress |
| `slotted-registry` | Namespaced registries, data-stage loading, freeze, tags, recipes. No Bevy. | In progress |
| `slotted-ecs` | Bevy adapter of the model: components, events, systems, `LocalAuthority`. | Planned |
| `slotted-theme` | Tokens, materials, RON theme assets, hot reload, shipped themes. | Planned |
| `slotted-ui` | Widgets, screen trees, anchors, injection, exclusion zones, HUD layers. | Planned |
| `slotted-icons` | Offscreen icon baking to an atlas, cache, `IconSource` port. | Planned |
| `slotted-browser` | Ingredient registry, categories, search index, browser panel, transfer. | Planned |
| `slotted-script` | `ScriptRuntime` port, the `slotted.*` API surface, event and command types. | Planned |
| `slotted-script-mlua` | Luau via mlua, native only. | Planned |
| `slotted-script-piccolo` | Pure-Rust Lua 5.4, wasm and native, recoverable errors. | Planned |
| `slotted-script-luaur` | Pure-Rust Luau, feature-gated until wasm error handling lands. | Planned |
| `slotted-packs` | Layered `AssetReader` for mods and resource packs, `mod.toml`, Fluent. | Planned |
| `slotted` | Facade: `SlottedPlugins`, prelude, feature flags. | Planned |
| `slotted-test` | Public UI test harness: headless app, locators, synthetic input, snapshots. | Planned |
| `slotted-testutils` | Internal fakes, builders, script conformance suite. Dev-dependency only. | In progress |

## Getting around

- [`docs/PLAN.md`](docs/PLAN.md) is the implementation plan: crate-by-crate design, phases and risks.
- [`docs/moodboard.html`](docs/moodboard.html) is the visual direction for the glass theme.
- [`docs/adr/`](docs/adr/) holds the architecture decision records from the Phase 0 spikes.
- [`docs/research/`](docs/research/) holds the background research the plan is built on.

## Working on it

```
just check   # cargo check --workspace --all-targets
just test    # cargo test --workspace
just lint    # clippy, warnings fatal
just ci      # what CI runs: fmt-check, lint, test
```

`.cargo/config.toml` forces a plain `cc`/`c++` toolchain. If your shell exports
an Anaconda toolchain, that is deliberate: ADR 0001 records how it corrupts a
vendored C++ build into a runtime `SIGFPE` rather than a build error.

## Licence

MIT OR Apache-2.0, at your option.
