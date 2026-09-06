# slotted-registry

Namespaced registries for [slotted](https://github.com/JedimEmO/bevy-slotted),
and the data stage that fills them.

Registries are open during the data stage and frozen before the control stage
runs. Freezing interns every `Namespaced` id into a dense numeric handle and
returns an immutable registry, so every later lookup is a pure function over
fixed data.

No Bevy. The crate owns the port and the pure logic; `slotted-packs` owns the
adapter that layers mods and resource packs behind the same trait.

## Main types

| Type | What it is |
|---|---|
| `Registries` | The mutable side. `add_item`, `add_tag`, `add_recipe`, `add_recipe_type`. |
| `FrozenRegistries` | The immutable side: dense ids, resolved tags, a recipe index. |
| `DataStage`, `Round` | Reads entry files from an `AssetSource` and runs three patch rounds over them. |
| `AssetSource`, `DirSource` | The port that supplies data files, and the filesystem adapter. |
| `ModManifest`, `resolve_load_order` | `mod.toml` parsing and dependency-ordered load. |
| `defs::*` | `ItemDef`, `TagDef`, `RecipeDef`, `RecipeTypeDef`, `Ingredient`, `ItemResult`. |
| `Value` | The untyped value RON and Lua both deserialise into. |

## Example

A game with no mod support registers everything in Rust and freezes.

```rust
use slotted_model::Namespaced;
use slotted_registry::defs::ItemDef;
use slotted_registry::registry::Registries;

let id = |s: &str| Namespaced::parse(s).expect("a valid id");
let mut registries = Registries::new();
registries.add_item(ItemDef::new(id("demo:oak_plank"))).expect("a fresh id");
let (frozen, _warnings) = registries.freeze().expect("a consistent registry set");
assert!(frozen.item_id(&id("demo:oak_plank")).is_some());
```

## Feature flags

| Feature | Default | Effect |
|---|---|---|
| `std-fs` | yes | The `DirSource` adapter. Turn it off for wasm, where there is no filesystem to read. |

## Licence

MIT OR Apache-2.0, at your option.
