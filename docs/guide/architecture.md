# Architecture

slotted is sixteen crates. The shape is deliberate: the rules of an inventory
are a plain Rust library, everything that touches a frame buffer or a file is an
adapter behind a port, and no arrow points from a lower crate to a higher one.

## The graph

```
slotted-model ──► slotted-registry ──► slotted-ecs ──► slotted-ui ──► slotted-browser
      │                                     │              ▲
      │                                     │              ├── slotted-theme
      │                                     │              └── slotted-icons
      │
      ├──► slotted-net (client and server halves of the Authority port)
      │
      └──► slotted-script ──► slotted-script-luaur       slotted-packs
                                                             │
                                                             ▼
                                        slotted (facade, wires everything)
                                        slotted-test (depends on the facade)
```

Read it as "the crate on the left knows nothing about the crate on the right".
`slotted-ui` does not know that scripting exists. `slotted-script` does not know
that `bevy_ui` exists. The facade and the examples are the only places that see
both.

`slotted-test` sits above the facade, not below it. A game depends on it from
its own `[dev-dependencies]`, which is why it is a published crate rather than a
test helper.

## The five rules

**The domain never knows about IO or rendering.** `slotted-model` has two
dependencies, `serde` and `thiserror`. Every click mode, every stack merge and
every conservation rule is a pure function over value types, testable in
milliseconds, and equally usable in a headless authoritative server.

**Every boundary with two implementations is a port.** There are five:

| Port | Lives in | Implementations |
|---|---|---|
| `Authority` | `slotted-model`, resource in `slotted-ecs` | `LocalAuthority` for single-player; `slotted_net::RemoteAuthority` for a client of a `MenuServer`. |
| `Transport` | `slotted-net` | `Loopback` in process, with latency, reordering and loss for tests. A `bevy_replicon` adapter is designed, not built. |
| `ScriptRuntime` | `slotted-script` | `slotted-script-luaur`, the same runtime natively and in a browser. |
| `AssetSource` | `slotted-registry` | `DirSource` for a plain directory, `LayeredSource` in `slotted-packs` for mods and resource packs. |
| `IconSource` | `slotted-icons` | `AtlasIcons` over a baked atlas; `LiveIcons` behind the `live` feature. |

A port exists because there are genuinely two implementations, not because a
trait might be useful one day.

**Screens are data.** A screen is a `ScreenDef`: a tree of `UiNodeDef`s.
`spawn_screen` is the single place that turns one into entities. Rust builds the
tree with struct literals, a `.screen.ron` file deserialises into it, and a Lua
table becomes it through the untagged `Value` form. Injection is a patch onto
that tree at a named anchor, which is what lets a mod change a screen it does
not own. See [screens.md](screens.md).

**Registries freeze.** The data stage fills registries with `Namespaced` string
ids. Freezing interns each id into a dense handle and hands back an immutable
`FrozenRegistries`. Everything after that is a pure function over fixed data,
which is why the browser can index 50,000 entries off the main thread and why a
recipe lookup is an array index.

**Scripts declare and react, never mutate.** A script receives a `ScriptEvent`
and returns `ScriptCommand`s. The host validates each command and applies it as
an ordinary `MenuAction`, so prediction, conservation and the authority see the
same thing they see from a human click. This is the property that makes the
identical script safe on a server and in a browser tab.

## The frame

Inside `Update`, in order:

1. `SlottedEcsSet::Input` drains the actions observers collected from pointer
   and keyboard events.
2. `SlottedEcsSet::Predict` runs `apply_click` locally, so the screen responds
   this frame.
3. `SlottedEcsSet::Submit` hands the delta to the `Authority`.
4. `SlottedEcsSet::Reconcile` polls the authority for acks, single-slot pushes
   and resyncs. A resync overwrites local state; prediction is a guess, not a
   decision. A submission the authority refuses outright asks it for a
   snapshot rather than trusting the guess that was just rejected.
5. `SlottedUiSet` lays out, picks, composes tooltips and syncs the semantic
   layer.
6. `SlottedThemeSet::Motion` advances tweens against `Time<Virtual>`, then
   `SlottedThemeSet::Apply` repaints changed roles.

Nothing in `slotted-ui` reads a wall clock. That is what lets the test harness
run on virtual time and `settle()` terminate.

## The semantic layer

Every spawned node carries a `SemanticRole`, an optional `SemanticLabel`, a
`Tags` map and a `TestId`. `bevy_a11y` reads it to announce the screen, and
`slotted-test` reads it to locate nodes. Accessibility and testability are the
same feature, built once, which is why a locator never depends on layout
coordinates or on a widget's internals.

## Where a change belongs

| You want to | Change |
|---|---|
| Add a click mode or fix a stacking rule | `slotted-model` |
| Add a def type or a data-stage rule | `slotted-registry` |
| Add a component, event or prediction rule | `slotted-ecs` |
| Add a node type or a widget | `slotted-ui`, and the `UiNodeDef` enum |
| Add a colour, a state or a shipped theme | `slotted-theme`, and `roles::ALL` |
| Add a script event or command | `slotted-script`, then the Lua prelude, then both adapters |
| Add a mod-loading rule | `slotted-packs` |
| Add a wire message or change what a server validates | `slotted-net` |
| Add a harness action or query | `slotted-test`, and `TestOp` if a mod's tests need it |

Adding a script command means touching four files: the `ScriptCommand` enum, the
prelude's `slotted.cmd` table, the host's validation in `slotted-packs`, and the
conformance suite in `slotted-testutils`. That is the cost of two runtimes
behaving identically, and the conformance suite is what proves they do.

## Playing over a network

`slotted-net` is the second implementation of the `Authority` port, and it is
the reason the port exists. A game turns on the facade's `net` feature, inserts
a `ClientTransport` resource and changes nothing else: prediction, correction
and the round-trip accounting were already written against the port, not
against a local authority.

The server half, `MenuServer`, runs the same `apply_click` the client ran,
against its own copy of the menu, at `ValidationLevel::Always`. A client that
predicted right gets an ack of a few bytes. A client that predicted wrong, or
whose state id says its copy has drifted, gets the whole container back. The
menu's other viewers get one `SetSlot` per slot that changed, taken off the
inventories' dirty masks.

Anything above the transport is testable without a socket: `Loopback` is
tick-driven, so a test decides exactly when the link is slow, shuffled or
lossy. See `docs/design/gaps-notes-A.md`.

## Bevy version

Pinned at the workspace root, currently 0.19.1. An upgrade is one line plus the
migration guide. `bsn!` and any `bevy_ui_widgets` churn land in `slotted-ui`
only; nothing else imports those types.
