# Phase 4 notes: package B (`slotted-packs`)

Deviations, gaps and small extensions found while implementing contract section 2.
Nothing here changes a signature the contract fixes; each item says what was done
instead and what a contract amendment would look like.

## 1. `ModErrors` was missing from the skeleton

Contract 2.3 lists `ModErrors(Vec<ModError>)` among the resources, but neither
`lifecycle.rs` nor `lib.rs` declared it, and package C's harness needs
`mod_errors()`. Added as `slotted_packs::ModErrors(pub Vec<ModError>)`, filled
alongside every `ModFailed` message and initialised by `SlottedPacksPlugin`.

## 2. No `ModError` variant for a bad mod directory

The contract's variant list has no id-mismatch or missing-manifest case.
Discovery reports both as `ModError::Manifest(ManifestError { path, message })`,
which already carries the file and a message. Unreadable directories use the
skeleton's `ModError::Io`, which is likewise absent from the contract's list but
present in `error.rs`.

## 3. Discovery through an `AssetSource` needs the ids

`slotted_registry::AssetSource::list` lists files, never directories, so a
source-backed host cannot enumerate `mods/*`. `ModSet::discover_in(source,
mods_dir, ids)` takes the ids explicitly and reads `<mods_dir>/<id>/mod.toml`
through the port. `ModSet::discover(&Path)` is the filesystem case and walks
directories itself.

## 4. Where a mod's script text comes from

Contract 2.1 says `pack://scripts/<mod>/data.lua` resolves through the layering,
but a mod's asset root *is* its own directory, so the logical path `data.lua`
would resolve to whichever mod happens to sort first. `read_script` therefore
tries `scripts/<mod id>/<entry>` through `LayeredSource` first, which is
unambiguous and lets a resource pack override a script, and falls back to
reading `<mod root>/<entry>` directly. `ModWatch` only registers a script handle
for the first form, so a script that lives at the mod root reloads through an
explicit `ReloadMod` rather than the file watcher.

## 5. Data scripts are unloaded after the data stage

A `data.lua` state may only register and is asked exactly one event, so
`run_data_stage` calls `ScriptRuntime::unload` once the `DataStage` reply is in.
Static tooltip parts and injections are already Rust values by then, so nothing
depends on the state surviving.

## 6. Namespace validation is a warning, not an error

The brief asked for ids to be namespaced under the mod's own id. Registering
into another namespace is how a compatibility mod tags somebody else's item, and
the contract's own example (`appleskin_like` registering `demo:apple` and
`c:foods`) does exactly that. `namespace_warnings` therefore emits a
`ScriptLog { level: Warn }` when the namespace is neither the mod's own nor a
declared dependency, and the registration proceeds.

## 7. `Value -> ron::Value -> into_rust` cannot express an optional field

Contract 2.3 step 2 routes a script's registration through `ron::Value`. That
hop cannot populate any `Option` field: RON writes `Some("x")`, `ron::Value`'s
deserializer accepts only `ron::Value::Option` for `deserialize_option`, and a
Lua table has no `Some` wrapper the prelude could produce. Every
`ItemDef::display_name`, `RecipeTypeDef::title_key` and `RecipeDef::shape`
failed with "expected option", which package C hit on the `examples/modded`
mods.

The fix landed below packs, as a shared amendment: `slotted_model::from_value`
is a deserializer over `Value` whose `deserialize_option` calls `visit_some`, so
a present value is `Some` and an absent key falls to serde's `None`, with
`slotted_registry::{to_model, from_model}` bridging the untyped registry payload
and `slotted_ui::ScreenDef::from_value` reading back through the same rules.
Packs reads every def, injected node and tooltip node through it, and
`ModLoader::to_ron_value` keeps its contract signature but is no longer on the
def path.

Contract section 1.3 should gain a sentence: an absent table key is `None`, and
any present value is `Some(value)`; there is no wrapper on the Lua side.

## 7a. A Fluent identifier may not contain a dot

Contract 2.7's convention key `copper_chest.item.copper_chest` is not a legal
Fluent identifier (`[A-Za-z][A-Za-z0-9_-]*`), and one of them makes
`FluentResource::try_new` reject the whole file with "Expected a token starting
with =". Rather than push the dash form onto modders, `locale::normalise_ids`
rewrites the identifier of each message and term definition when a layer is
parsed, and `Locales::resolve` tries the key as written and then normalised.
Values, continuations, attributes and comments are untouched. A mod may write
the key either way; the contract should say the dot is the convention and the
dash is what Fluent stores.

## 7b. Recipe categories have to be re-bound after every freeze

`slotted_browser`'s `validate_categories` calls `Categories::bind_defaults` once
in `BrowserPhase::Categories`. That is too early for a harness that loads mods
after `build` and never runs again for a hot reload, and a recipe type with no
category has every one of its recipes dropped by `RecipeStore`. The control
stage now calls `bind_defaults` beside the icon re-bake and the browser rebuild
of contract 2.3 step 4; it skips types a category already claims, so running it
twice costs nothing. Package C's stopgap copy in `UiHarness::load_mods` and
`reload_mod` can go.

## 8. `def.name` and `kind` are filled from the id

The prelude fills `def.name` (contract 1.4), but a host that trusts it turns a
missing field into a deserialisation error a modder cannot read. `apply_data_command`
inserts `name` (items, tags, recipes, recipe types) and `kind` (screens) into the
payload when absent, then still overwrites the typed field from the registered
id. A `RegisterScreen` payload is deserialised to `slotted_ui::ScreenDef` at
registration time so the error names the screen rather than surfacing at spawn.

## 9. `TemplateWidget` carries its kind

`Widget::spawn` is not told which kind it was registered under, and the contract
requires the spawned root to carry `WidgetNode`. `TemplateWidget` gained a
`kind: WidgetKind` field alongside `template`.

## 10. Per-frame script budget is a constant

Contract 2.4 asks for a per-frame budget but `PacksConfig` has no field for one
and its shape is shared. `route::MAX_SCRIPT_CALLS_PER_FRAME` (512) bounds the
calls one `Dispatch` may make, in calls rather than seconds. Making it
configurable means adding a field to `PacksConfig`.

## 11. Ownership of `Injections`, `TooltipParts`, `WidgetRegistry` and `Icons`

None of these carry an owner, so a reload cannot tell packs' entries from a
game's. A private `PacksOwned` resource records the exact `Injection` values, the
index of packs' `TooltipPart` and the `WidgetKind`s registered last time, and the
control stage removes those before adding the new ones. `Icons` is only re-baked
when packs itself baked it, marked by the `PacksBakedIcons` resource.

## 12. Reload treats a per-mod script failure as fatal

Contract 2.6 step 1 says RON and script errors keep the previous registries.
`reload_mod` therefore aborts on the first error of any kind, where `run_all`
records a per-mod script failure and keeps going with that mod's RON.

## 13. Component ids are not remapped

`remap_inventories` remaps `ItemStack::id` by name across every `Inventory` and
`Carried`. A stack's `ComponentPatch` is keyed by `ComponentId`, which is
re-interned by the same freeze; those keys are left as they are. No Phase 4
example writes a component patch, but this is a real gap for Phase 5.

## 14. `ModStage` is written directly

`run_all` completes inside one system, so the state transition schedule never
runs between stages. `set_stage` writes `NextState` and `State` together, and
both are optional so the loader works in a bare `World`.
