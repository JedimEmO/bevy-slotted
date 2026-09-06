# slotted-packs

Mods and resource packs for
[slotted](https://github.com/JedimEmO/bevy-slotted).

Three jobs, one crate, because they share the layering rule "resource packs,
then mods in reverse load order, then base":

- **Discovery and layering.** Read every `mods/<id>/mod.toml`, sort with
  `resolve_load_order`, and serve one logical path across several roots, both to
  the registry's `DataStage` and to Bevy as the `pack://` asset source.
- **Lifecycle.** Run the data stage (RON and `data.lua` into the same builders),
  freeze, publish screens, injections and tooltip parts, load `control.lua`,
  then route UI events to scripts and their commands back through validation.
  Hot reload re-runs the same steps and remaps inventory ids by name.
- **Localisation.** `locale/<lang>.ftl` per mod, layered, resolving `LocText`
  nodes through Fluent.

## Main types

| Type | What it is |
|---|---|
| `SlottedPacksPlugin`, `PacksConfig`, `SlottedPacksSet` | The plugin, where it looks for mods, and its frame order. |
| `ModSet`, `ModEntry`, `ModPaths`, `PackLayout` | The discovered mods and the resolved layer stack. |
| `LayeredSource`, `LayeredAssetReader`, `PackSourcePlugin` | The `AssetSource` adapter and the `pack://` reader. |
| `ModLoader`, `ModStage` | The data/control lifecycle and the state it runs in. |
| `ScriptAsset`, `ScriptLoader`, `DataFile`, `ModWatch` | The assets and the hot-reload watcher. |
| `Locales`, `FtlAsset`, `resolve_loc_text` | Fluent localisation. |
| `ScriptTooltipPart`, `StaticPart`, `TemplateWidget` | Tooltip parts a mod contributes. |

## The manifest

```toml
id = "copper_chest"
name = "Copper Chest"
version = "0.1.0"
api_version = 1

[entry]
data = "data.lua"
control = "control.lua"
```

## Feature flags

| Feature | Default | Effect |
|---|---|---|
| `watch` | no | Hot reload of scripts, RON and locale files through Bevy's file watcher. |

## Licence

MIT OR Apache-2.0, at your option.
