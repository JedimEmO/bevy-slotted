# slotted-ecs

The Bevy adapter of the [slotted](https://github.com/JedimEmO/bevy-slotted)
domain model.

This crate turns `slotted-model` into components, entity events and systems, and
owns the client side of the prediction loop: gather a `ClickAction`, apply it
locally, submit it to the `Authority`, then reconcile acks and resyncs. It knows
nothing about rendering or `bevy_ui`. The ui crate attaches a `SlotRef` to
whatever entity draws a slot and this crate does the rest.

It runs under `MinimalPlugins` and on wasm.

## Main types

| Type | What it is |
|---|---|
| `SlottedEcsPlugin`, `SlottedEcsSet` | The plugin, and the frame order: `Input`, `Predict`, `Submit`, `Reconcile`. |
| `Inventory`, `SlotRef`, `Carried`, `MenuProperty` | The components a menu's entities carry. |
| `MenuAction`, `SlotClicked`, `SlotChanged`, `PropertyChanged` | The events in and out. |
| `Authority`, `LocalAuthority`, `PendingRoundTrips` | The port, the single-player adapter, and the in-flight queue. |
| `Registries`, `RegistryLookup` | The frozen registries as a resource, and the `LookupCtx` bridge. |
| `open_menu`, `open_container_menu`, `close_menu` | Spawn and despawn a menu from a `MenuDef`. |

## Example

```rust
use bevy::prelude::*;
use slotted_ecs::prelude::*;

App::new()
    .add_plugins(MinimalPlugins)
    .add_plugins(SlottedEcsPlugin)
    .run();
```

## Feature flags

None.

## Licence

MIT OR Apache-2.0, at your option.
