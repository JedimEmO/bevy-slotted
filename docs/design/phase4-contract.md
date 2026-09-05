# Phase 4 contract: scripting, packs, modded example

Status: v1.1, 2026-09-05, amended in integration. Paragraphs marked **(amended in
integration)** replace what v1.0 said; everything else stands as written. Bevy 0.19.1, mlua 0.11 (Luau, vendored). Companion to `docs/PLAN.md`
4.8 to 4.10, 5.3 and Phase 4, ADR 0001, `docs/research/research-scripting.md` sections 4 and 5,
and the Phase 2 (v1.1) and Phase 3 contracts, which this builds on and does not restate. Three
packages implement it in parallel without talking to each other: **A** (`slotted-script`,
`slotted-script-mlua`, conformance suite), **B** (`slotted-packs`), **C** (`examples/modded`,
`slotted-test` scripted extension). The skeletons carry the real signatures; a body marked
`// PHASE4-IMPL: A|B|C` is that package's to fill. Signatures change only by amending this file.

## 0. Ground rules

- Dependency direction: `model <- script <- script-mlua`; `registry, ecs, ui, browser <- packs`;
  `packs` also depends on `script`. `slotted-script` depends on `serde`, `thiserror` and
  `slotted-model` only and must pass `cargo check -p slotted-script --target wasm32-unknown-unknown`.
  Nothing below the facade depends on `slotted-script-mlua`.
- Scripts never touch state. A script receives a `ScriptEvent` and returns `ScriptCommand`s; the
  host validates every command before applying it. Inventory mutation happens only by triggering
  `slotted_ecs::MenuAction`, so prediction, the authority and conservation are untouched.
- Factorio split: `data.lua` runs once per load with registries open and may only register;
  `control.lua` runs after the freeze and may only react. A `Register*` command from a control
  script, or a `MenuAction`-shaped command from a data script, is a `ModError::WrongStage` and is
  dropped.
- No wall clock. Budgets are interrupt ticks and bytes, never seconds.
- One Lua state per script. A mod's `data.lua` and `control.lua` share nothing but the prelude.

## 1. `slotted-script` (A)

### 1.1 Port

```rust
pub struct ScriptId(pub u32);
pub enum Stage { Data, Control }
pub struct Limits { pub budget: u64 /* interrupt ticks per call, default 1_000_000 */,
                    pub memory_bytes: usize /* default 64 MiB */ }
pub enum ScriptError {
    Compile  { name: String, message: String },
    Runtime  { name: String, message: String, traceback: String },
    BudgetExceeded { name: String },
    Memory   { name: String },
    Sandbox  { name: String, message: String },   // forbidden global reached, write to a frozen table
    Protocol { name: String, message: String },   // dispatch returned something that is not [command]
    UnknownScript(ScriptId),
}
pub trait ScriptRuntime: Send + Sync {
    fn load(&mut self, mod_id: &ModId, name: &str, source: &str, stage: Stage) -> Result<ScriptId, ScriptError>;
    fn unload(&mut self, id: ScriptId);
    fn call(&mut self, id: ScriptId, event: &ScriptEvent) -> Result<Vec<ScriptCommand>, ScriptError>;
    fn set_limits(&mut self, limits: Limits);
}
```

`ModId` is `slotted_script::ModId(String)`, a newtype with the same validation rule as the
registry's (`[a-z0-9_.-]+`); `slotted-packs` converts. `load` installs the prelude, sets
`slotted.mod_id`, `slotted.stage`, executes the chunk, and returns. A chunk that errors at load
is not registered. `call` on an unloaded id is `UnknownScript`. A `BudgetExceeded` or `Memory`
error leaves the state loaded; the host decides whether to unload.

### 1.2 Events and commands (`events.rs`, `commands.rs`)

Both enums are `#[serde(tag = "type", rename_all = "snake_case")]`, `Clone, Debug, PartialEq`.
Ids are strings in `namespace:path` form; menus are `MenuId` (u32); slots are `u16`;
`Button`, `Modifiers`, `TooltipTier`-like enums serialise as lowercase strings.

| `ScriptEvent` | Fields | When |
|---|---|---|
| `DataStage` | `api_version: u32` | once, right after the data chunk executed |
| `ControlStart` | `api_version: u32, mods: Vec<String>` | once, right after the control chunk executed |
| `SlotClick` | `menu: MenuId, screen: String, slot: u16, button: Button, modifiers: Modifiers, stack: Option<StackInfo>` | every `slotted_ecs::SlotClicked` |
| `WidgetActivate` | `menu: Option<MenuId>, screen: String, widget: String, tags: BTreeMap<String,String>` | `bevy_ui_widgets::Activate` on a `WidgetNode` inside a screen |
| `TooltipBuild` | `stack: StackInfo, tier: Tier` | while composing a tooltip, from `ScriptTooltipPart` |
| `RecipeLookup` | `item: String, mode: LookupMode::{Recipes, Uses}` | browser `OpenRecipes` / `OpenUses` |
| `ScreenOpened` | `menu: Option<MenuId>, screen: String` | `slotted_ui::ScreenSpawned` |
| `ScreenClosed` | `menu: Option<MenuId>, screen: String` | `slotted_ui::ScreenClosed` |
| `HudTick` | `elapsed_ms: u64` | every `PacksConfig.hud_tick` interval, off by default |
| `SearchChanged` | `text: String` | browser `SearchChanged` |

`StackInfo { item: String, count: u32, components: Value }`, `Button::{Left, Right, Middle}`,
`Modifiers { shift, ctrl, alt }`, `Tier::{Compact, Expanded}`.

| `ScriptCommand` | Fields | Stage | Applied by B as |
|---|---|---|---|
| `RegisterItem` | `id: String, def: Value` | Data | `ItemDef` via `slotted_model::from_value` (amended in integration; was `Value -> ron::Value -> into_rust`), `items.replace` |
| `RegisterTag` | `id: String, def: Value` | Data | `TagDef`, `Registries::add_tag` (merges) |
| `RegisterRecipeType` | `id, def` | Data | `RecipeTypeDef`, replace |
| `RegisterRecipe` | `id, def` | Data | `RecipeDef`, replace |
| `RegisterScreen` | `id, def` | Data | `slotted_registry::ScreenDef { name, payload }`; the ui deserialises it to `slotted_ui::ScreenDef` in `Screens::load_from_registry` |
| `RegisterWidget` | `id, def` | Data | `WidgetDef` payload: a `UiNodeDef` template; B registers a `TemplateWidget { kind, template }` per entry in `WidgetRegistry` whose `Anchor { id: "children" }` receives the caller's children. No params in Phase 4. **(amended in integration)** `TemplateWidget` carries its `kind`, because `Widget::spawn` is not told which kind it was registered under and the spawned root has to carry `WidgetNode`. |
| `Inject` | `screen: String, anchor: String, node: Value, exclusion: bool` | Data | `slotted_ui::Injection` pushed to `Injections` |
| `AddTooltipPart` | `id: Option<String>, when: TooltipFilter, tier: TierFilter, nodes: Vec<Value>` | Data: static part; Control: reply to `TooltipBuild`, only `nodes` read | section 2.5 |
| `Sort` | `menu: MenuId, inventory: u16` | Control | `MenuAction { Toolbar(Sort) }` |
| `QuickStack` | `menu, from: u16, to: u16` | Control | `Toolbar(QuickStack)` |
| `Move` | `menu, from: u16, to: u16` | Control | `[Pickup{from,Left}, Pickup{to,Left}, Pickup{from,Left}]` in one frame, like a transfer plan |
| `ToggleFavorite` | `menu, slot: u16` | Control | `Toolbar(ToggleFavorite)` |
| `Click` | `menu, action: ClickAction` | Control | verbatim; the escape hatch |
| `SetHud` | `layer: String, value: Value` | Control | logged at debug level; Phase 6 |
| `Log` | `level: LogLevel::{Trace, Debug, Info, Warn, Error}, message: String` | any | `ScriptLog` message |
| `Deprecated` | `call: String, since: u32, hint: String` | any | one `ScriptLog` at Warn per `(mod, call)` |
| `Subscribe` | `events: Vec<String>` | Control | the host calls this script only for listed events |

`TooltipFilter { items: Vec<String>, tags: Vec<String> }` (empty means every stack),
`TierFilter::{Any, Compact, Expanded}`.

### 1.3 Value bridge (`value.rs`)

**(amended in integration)** `slotted_model::Value` gained a `Null` variant and, in
`slotted-model`'s new `value` module, a `serde::Deserializer`. `slotted_model::from_value` is now
the **only** way any untyped tree becomes a typed def, from a data file as much as from a script:
`slotted_registry::to_model` converts the `ron::Value` a `.ron` file parses to into a
`slotted_model::Value` first, and `typed_entry`, `slotted_ui::ScreenDef::from_value`,
`ScreenDef::from_ron` and the widget `params_of!` macro all read from there. The rules, in full, are
at the top of `slotted_model::value`:

- an **absent map key** is `None`, through the field's `#[serde(default)]`; a **`Value::Null`**
  (a RON `None`, a RON unit) is `None`; **any other present value is `Some(value)`**. There is no
  `Some` wrapper on the Lua side and none is needed. This is what v1.0 could not express: the
  `Value -> ron::Value -> into_rust` hop of section 2.3 accepted only `ron::Value::Option` for
  `deserialize_option`, so every `ItemDef::display_name`, `RecipeTypeDef::title_key` and
  `RecipeDef::shape` failed with *expected option*.
- an `Int` reads as any integer type and as a float; a `Float` reads only as a float; a `Str` of
  one character reads as a `char`; a `List` reads as a sequence, a tuple or an array; a `Map` reads
  as a struct, a map or an **internally tagged enum** (`type: "slot_grid"`, no special case: serde
  reads the tag through `deserialize_any`); a one-entry `Map` reads as an externally tagged variant
  and a bare string as a unit variant.
- a script still never produces a `Null`. A Lua table cannot hold a nil value and the adapter's
  bridge rejects an explicit nil, so a mod omits the key. `Null` exists for the RON side.

Two consequences. Phase 2's "no `Option` in widget params" deviation (phase2 contract section 8,
phase2 notes B item 5) is lifted: a params field may be an `Option`. And `IconDef`/`ViewSubject`
now *serialise* as the one-key map their data form always used, because RON's own variant syntax
(`image("x")`) has no `ron::Value` representation and so did not survive the untyped payload.

Every `Value` field on a command or event is `#[serde(with = "slotted_script::value::untagged")]` (a `Vec<Value>` uses
the `UntaggedValue` newtype), implemented and tested in the skeleton with a `Visitor`: bool -> `Bool`; a number with zero fractional part inside `i64` -> `Int`, else
`Float`; string -> `Str`; sequence -> `List`; map with string keys -> `Map` (non-string keys are
an error); `null`/nil -> error (a script omits the field; `Value::Null` exists only for the
RON side, per the amendment above). Adapter rules for
Lua tables: consecutive integer keys `1..n` and nothing else is a `List`; anything else is a
`Map`; an **empty table is an empty `List`** (mlua: `DeserializeOptions::encode_empty_tables_as_array`).
Rust -> Lua: `Int` becomes a Lua number, `Map` a table with string keys, `List` a 1-based array,
`Option::None` is absent (`SerializeOptions::serialize_none_to_null(false)`).

### 1.4 Prelude (`prelude/slotted.lua`, `include_str!` as `PRELUDE`)

The skeleton ships the complete prelude; A amends it only to fix conformance failures. The
host sets `__slotted_mod_id`, `__slotted_stage` and `__slotted_api_version` before running it.

```lua
slotted.api_version      -- 1
slotted.mod_id           -- "copper_chest"
slotted.stage            -- "data" | "control"
slotted.register_item(id, def)   register_tag(id, def)   register_recipe_type(id, def)
slotted.register_recipe(id, def) register_screen(id, tree) register_widget(id, template)
slotted.inject(screen_kind, { anchor = "title_end", node = {...}, exclusion = false })
slotted.add_tooltip_part{ id = ..., items = {...}, tags = {...}, tier = "any", nodes = {...} }
slotted.on(event_name, handler)  -- handler(event) returns nil, one command table, or an array of them
slotted.cmd.sort(menu, inventory) .quick_stack(menu, from, to) .move(menu, from, to)
slotted.cmd.toggle_favorite(menu, slot) .click(menu, action) .set_hud(layer, value)
slotted.cmd.tooltip(nodes) .log(level, msg)
slotted.log(level, fmt, ...)   -- string.format; also slotted.info/warn/error
```

Register calls and `inject`/`add_tooltip_part` at the data stage append to a pending list;
`register_*` outside the data stage raises. Ids without a namespace get `mod_id .. ":"`
prepended; a `def.name` is filled from `id` when absent, so `register_item("copper_chest",
{ max_stack_size = 16 })` yields `def = { name = "copper_chest:copper_chest", ... }`.

**(amended in integration)** The prelude keeps **one** output buffer, not a pending list and a
separate log buffer: two buffers cannot represent the order the calls happened in. Chunk-time
emissions (`register_*`, `inject`, `add_tooltip_part`, `slotted.log`, `print`, `__deprecated`)
come out in call order, and inside the handler loop the buffer is drained after each handler's
`pcall` and before its return value is appended, so a handler that logs and then returns a command
produces the log first. A `data.lua` **may** call `slotted.on`; only `register_*` outside the data
stage raises.

Dispatch protocol. The host calls exactly one global, `__slotted_dispatch(event_table)`, and
receives an array of command tables. For `data_stage` the reply is the pending list followed by
what `on("data_stage")` handlers returned. For `control_start` the reply starts with
`{ type = "subscribe", events = {...} }` listing every name passed to `slotted.on`, then handler
output. Every other event returns handler output only; unhandled events return `{}`. A handler
error propagates as `ScriptError::Runtime` with the traceback and the remaining handlers of that
event still run. The prelude is plain Lua 5.1-compatible Luau using only `table`, `string`,
`math`, `pairs`, `ipairs`, `type`, `error`, `pcall`, `setmetatable`, so piccolo can host it later.

### 1.5 Sandbox

Removed before `sandbox(true)`, and asserted absent by the conformance suite: `io`, `os`,
`package`, `require`, `dofile`, `loadfile`, `load`, `loadstring`, `debug`, `collectgarbage`,
`getfenv`, `setfenv`, `newproxy`, `rawset`/`rawget` on `_G`. Kept: `table`, `string`, `math`,
`bit32`, `utf8`, `pairs`, `ipairs`, `next`, `select`, `type`, `tostring`, `tonumber`, `pcall`,
`xpcall`, `error`, `assert`, `unpack`, `setmetatable`, `getmetatable`, `print` (routed to
`Log{Info}`). The `slotted` table is read-only; `_G` is per state. Default `Limits`: 1 000 000
ticks per `call` and 64 MiB per state. Exceeding them returns the error and the host emits
`ModError::Budget`.

**(amended in integration)** `ScriptError::Sandbox` is raised for a write to a frozen table, which
Luau reports as an ordinary runtime error: the adapter classifies a runtime error whose text
mentions `readonly` or `read-only` as `Sandbox`. Reaching a *removed* global (`io.write(...)`) is a
plain `Runtime` error, because indexing `nil` is all Luau sees. Budget exhaustion leaves the state
loaded and usable: the interrupt counter and the budget are re-armed at the top of every `call`.

### 1.6 mlua adapter (`slotted-script-mlua`)

`MluaRuntime::new(limits)`, one `Lua::new_with(TABLE|STRING|MATH|BIT|UTF8)` per `load`, removals,
`sandbox(true)`, `set_interrupt` counting ticks per `call` (reset each call), `set_memory_limit`,
`to_value_with`/`from_value_with` via `LuaSerdeExt`. Compile errors (`mlua::Error::SyntaxError`)
map to `Compile`; `RuntimeError` to `Runtime` with `traceback` from the error chain; a
`MemoryError` to `Memory`; the interrupt raises a runtime error whose message starts with
`slotted:budget`, which the adapter maps to `BudgetExceeded`. Workspace dependency:
`mlua = { version = "0.11", features = ["luau", "vendored", "serialize", "send"] }`.
`.cargo/config.toml` already pins the C toolchain (ADR 0001).

**(amended in integration)** mlua gives no traceback for an error raised in a Lua chunk called from
Rust, and `debug` is removed from the sandbox, so `call` invokes
`xpcall(__slotted_dispatch, __slotted_traceback, event)`. `__slotted_traceback` is a Rust function
installed before sandboxing that calls `Lua::traceback` while the erroring stack is live, stashes
the result and returns its argument unchanged. It is a global a mod can see and it does nothing
else.

### 1.7 Conformance suite (`slotted-testutils/conformance/*.lua`, `conformance.rs`)

```lua
-- STAGE: data                       (default data; `control` fires ControlStart first)
-- EVENTS: [ (type: "data_stage", api_version: 1) ]        RON Vec<ScriptEvent>, optional
-- EXPECT: [ (type: "register_item", id: "test:apple", def: {"name": "test:apple", "max_stack_size": 16}) ]
-- EXPECT_ERROR: budget | memory | compile | runtime | sandbox          (instead of EXPECT)
slotted.register_item("apple", { max_stack_size = 16 })
```

Comment blocks are contiguous `-- ` lines; the RON runs to the next directive. `ConformanceCase
::parse(name, source)`, `run_case(&mut dyn ScriptRuntime, &ConformanceCase) -> CaseResult`,
`run_all(&mut dyn ScriptRuntime) -> Vec<CaseResult>` reading the directory at test time, and
`assert_conformance(&mut dyn ScriptRuntime)` which panics with every failing case listed. Expected
commands are compared with `PartialEq` after the `Subscribe` command is stripped, so a case does
not restate it. `slotted-script-mlua/tests/conformance.rs` calls `assert_conformance` and is the
gate ADR 0001 requires. **(amended in integration)** alongside the named signatures the module also
has `Report { results }` with `total`/`passed`/`failures`, the directory-taking
`run_conformance_dir` and `load_cases_from`, and `ExpectedError::as_str`; `assert_conformance` also
fails when the suite holds fewer than 20 cases, so a case file that disappears is caught. The
`conformance` feature enables `dep:serde` as well, because the RON directive parser is generic over
`DeserializeOwned`. `STAGE` and `EXPECT_ERROR` take the rest of their line; `EVENTS` and `EXPECT`
accumulate following `-- ` lines until the joined text parses as RON, and any other leading comment
line is prose. The suite ships 30 cases. Minimum cases A ships: each `register_*`, id namespacing, `inject`,
`on` with one and several handlers, `cmd.*` shapes, `print` routing, each forbidden global,
budget exhaustion (`while true do end`), a runtime error with traceback, empty and mixed tables.

## 2. `slotted-packs` (B)

Bevy features: `std, bevy_asset, bevy_log, bevy_ui, bevy_text`. Features: `watch` =
`bevy/file_watcher` (hot reload on native). Depends on `slotted-{model,registry,ecs,ui,browser,
script}`, `fluent-bundle` 0.16 (`concurrent`), `unic-langid`, `toml`.

### 2.1 Discovery and layering (`modset.rs`, `source.rs`)

`ModSet::discover(mods_dir: &Path) -> Result<ModSet, ModError>` reads every `mods/<dir>/mod.toml`
into `slotted_registry::ModManifest`, sorts with `resolve_load_order`, and records
`ModEntry { manifest, root: PathBuf, id }` in that order. `ModSet::from_manifests(Vec<(PathBuf,
ModManifest)>)` is the in-memory constructor for tests. **(amended in integration)**
`AssetSource::list` lists files, never directories, so a source-backed host cannot enumerate
`mods/*`: `ModSet::discover_in(source, mods_dir, ids)` takes the ids explicitly and reads
`<mods_dir>/<id>/mod.toml` through the port, while `discover(&Path)` walks the filesystem itself.
A mod directory with no manifest, or one whose id does not match, is a
`ModError::Manifest(ManifestError { path, message })`; an unreadable directory is `ModError::Io`. `PackLayout { base: PathBuf, mods: ModSet,
resource_packs: Vec<PathBuf> }`.

`LayeredSource` implements `slotted_registry::AssetSource` over `PackLayout`: `read` tries
resource packs (last listed wins), then mods in reverse load order, then base; `list` is the
union. Physical roots: base is `<base>/`, a mod's is `<root>/<manifest.assets or ".">/`, so a
mod's `data/<id>/items/*.ron` is found at `mods/<id>/data/<id>/items/`. The same struct backs the
Bevy side: `LayeredAssetReader` (`AssetReader`) is the `pack://` source registered by
`PackSourcePlugin::new(PackLayout)`, which **must be added before `AssetPlugin`** (Bevy requires
sources to exist before the asset server is built); with `watch` it wires Bevy's `FileWatcher`
over every root. `pack://textures/gui/chest.png`, `pack://themes/glass.theme.ron`,
`pack://screens/x.screen.ron`, `pack://locale/en-US.ftl`, `pack://scripts/<mod>/data.lua` all
resolve through the same layering.

**(amended in integration)** A mod's asset root *is* its own directory, so the logical path
`data.lua` would resolve to whichever mod sorts first. `read_script` tries
`scripts/<mod id>/<entry>` through `LayeredSource` first, which is unambiguous and lets a resource
pack override a script, and falls back to reading `<mod root>/<entry>` directly. `ModWatch` only
registers a handle for the first form, so a script at the mod root reloads through an explicit
`ReloadMod` rather than the file watcher.

### 2.2 Registry extension (shared, in the skeleton)

`slotted_registry::DataStage::load_into(&self, source, &mut Registries) -> Result<LoadReport,
LoadError>` does everything `load` does except freeze; `load` now calls it. This is the one change
below `packs` and it is already implemented.

### 2.3 Lifecycle (`lifecycle.rs`, `plugin.rs`)

```rust
#[derive(States)] pub enum ModStage { Idle, Discover, Data, Freeze, Control, Running }
pub struct ModLoader;                 // the state machine, pure functions over &mut World
impl ModLoader {
    pub fn run_all(world: &mut World) -> Result<LoadReport, ModError>;      // Discover..Running, synchronously
    pub fn reload_mod(world: &mut World, id: &ModId) -> Result<(), ModError>;
}
```

`SlottedPacksPlugin { config: PacksConfig { hud_tick: Option<Duration>, reload_on_change: bool } }`
runs `ModLoader::run_all` in `PreStartup`, so `slotted_ecs::Registries` exists before `slotted-ui`'s
and `slotted-browser`'s `Startup` systems read it. The stages, in order:

1. **Discover**: `PackLayout` resource present (the example inserts it; the harness's `load_mods`
   does). Missing: `ModError::NoLayout`.
2. **Data**: `Registries::new()`; `DataStage::new(order).load_into(&LayeredSource, &mut regs)`;
   then for each mod in load order with `entry.data` set: read the script through
   `LayeredSource`, `runtime.load(.., Stage::Data)`, `call(DataStage)`, apply every command
   (section 1.2 table) into the same `Registries`; a failing script skips that mod's script
   registrations, records `ModError::Script`, and keeps its RON. **(amended in integration)** the
   host fills `name` (items, tags, recipes, recipe types) and `kind` (screens) into the payload
   from the registered id when the script left them out, then overwrites the typed field from that
   id anyway, so a missing field is never a deserialisation error a modder cannot read. A
   `RegisterScreen` payload is typed to a `slotted_ui::ScreenDef` at registration time so the error
   names the screen. Registering into a namespace that is neither the mod's own nor a declared
   dependency is a `ScriptLog { level: Warn }`, **not** an error: that is how a compatibility mod
   tags somebody else's item, and this contract's own `appleskin_like` does it. A `data.lua` state
   is unloaded once its `DataStage` reply is in.
3. **Freeze**: `regs.freeze()`; `Registries(Arc)` resource replaced; `FrozenSnapshot` keeps the
   previous `Arc` for remapping. Warnings become `ScriptLog { level: Warn }`.
4. **Control**: `Injections` and `TooltipParts` entries owned by packs are removed and re-added
   from the collected `Inject`/`AddTooltipPart` commands; `Screens::load_from_registry` is
   re-run; `RebuildBrowser` is written; `Icons` is re-baked (`bake_placeholder_atlas`) unless a
   game replaced the resource. Then each mod's `control.lua` is loaded, `ControlStart` is
   called, and its `Subscribe` list is stored in `ControlScripts { by_mod: BTreeMap<ModId,
   ControlScript { id: ScriptId, events: BTreeSet<String> }> }`.
5. **Running**: events flow (section 2.4).

Resources: `ScriptHost(Arc<Mutex<Box<dyn ScriptRuntime>>>)` (the facade inserts the mlua one when
`script-mlua` is on and none exists; a missing host makes scripted mods `ModError::NoRuntime`
and data-only mods still load), `ModSet`, `PackLayout`, `ControlScripts`, `ScriptLogs { entries:
VecDeque<LogEntry>, cap: 500 }`, `ModErrors(Vec<ModError>)`. Messages: `ScriptLog { mod_id,
level, message }`, `ModError`-carrying `ModFailed { mod_id: Option<ModId>, error: ModError }`,
`ModReloaded { mod_id }`. **(amended in integration)** `ModErrors(pub Vec<ModError>)` is filled
alongside every `ModFailed` and initialised by the plugin. `run_all` completes inside one system, so
the state transition schedule never runs between stages: `set_stage` writes `NextState` and `State`
together and both are optional, so the loader also works in a bare `World`. A private `PacksOwned`
resource records the exact `Injection` values, the index of packs' `TooltipPart` and the
`WidgetKind`s registered last time, so a reload removes packs' own entries and not a game's;
`Icons` is re-baked only when packs baked it, marked by `PacksBakedIcons`.

`ModError` variants: `NoLayout`, `NoRuntime`, `Manifest(ManifestError)`, `LoadOrder(LoadOrderError)`,
`Load(LoadError)`, `Registry(RegistryError)`, `Script { mod_id, source: ScriptError }`,
`WrongStage { mod_id, command: String }`, `BadCommand { mod_id, command: String, message: String }`,
`Budget { mod_id }`, `UnknownMenu { mod_id, menu: MenuId }`, `ItemVanished { item: String, count: u32 }`,
and **(amended in integration)** `Io`, which the variant list left out.

### 2.4 Event routing (`route.rs`)

`SlottedPacksSet::{Collect, Dispatch, Apply}` in `Update`: `Collect` inside `SlottedUiSet::Input`
(observers only enqueue into `PendingScriptEvents(VecDeque<ScriptEvent>)`); `Dispatch` after
`SlottedUiSet::Input` and before `SlottedEcsSet::Input`, so a script's `MenuAction` is predicted
the same frame as the click that caused it; `Apply` is the same system's tail. Observers:
`SlotClicked` (resolve `SlotRef -> OpenMenu.id`, kind from the `ScreenRoot` ancestor recorded in
`OpenScreens { by_menu: HashMap<Entity, (MenuId, ScreenKind)> }`, which `ScreenSpawned`/
`ScreenClosed` maintain), `Activate`, `ScreenSpawned`, `ScreenClosed`; message readers for the
browser's `OpenRecipes`, `OpenUses`, `SearchChanged`. `Dispatch` calls only the scripts whose
`Subscribe` list has the event's `type`, in load order, and applies commands in return order.
Validation: a `MenuId` must match an `OpenMenu` in the world, inventories and slots must be in
range of its `def` (else `BadCommand`), the stage rule of section 0, and one `Deprecated` warning
per `(mod, call)`. **(amended in integration)** the per-frame budget is
`route::MAX_SCRIPT_CALLS_PER_FRAME` (512), a constant in calls rather than seconds, because
`PacksConfig`'s shape is shared and has no field for one; making it configurable means adding one.

### 2.5 Tooltips and widgets

`ScriptTooltipPart { host: ScriptHost, statics: Vec<StaticPart>, dynamic: Vec<(ModId, ScriptId)> }`
is one `TooltipPart` packs pushes after the built-ins. `build` appends every static part whose
filter matches the stack (`items` by id, `tags` through `FrozenRegistries.tag_index`) and tier,
then fires `TooltipBuild` into each control script subscribed to `tooltip_build` and appends the
`nodes` of every `AddTooltipPart` it returns. Node values deserialise to `UiNodeDef`; a bad one is
a `BadCommand` and is skipped. `TemplateWidget` (section 1.2 `RegisterWidget`) is registered in
`WidgetRegistry` for every `widgets` entry after each freeze.

### 2.6 Hot reload

Triggers: `AssetEvent::Modified` for a `ScriptAsset` (`.lua`, loader in packs) or `DataFile`
(`.ron` under `pack://data/`) handle recorded in `ModWatch { by_mod: HashMap<ModId,
Vec<UntypedHandle>> }` (packs loads every file `LayeredSource::list` finds, purely to be
notified); or an explicit `ReloadMod { mod_id }` message (what the harness and the console send).
`ModLoader::reload_mod`:

1. Re-run **Data** for the whole set (registries are one interned space; a per-mod incremental
   rebuild is Phase 5) but re-read only from `LayeredSource`; RON and script errors keep the
   **previous** `Registries`, emit `ModFailed`, and stop here. **(amended in integration)**
   `reload_mod` aborts on the first error of any kind, where `run_all` records a per-mod script
   failure and keeps going with that mod's RON.
2. **Freeze**; then remap every `Inventory` component and every `Carried` stack by name:
   `old.items.name_of(id) -> new.items.id_of(name)`; a stack whose item no longer exists is
   removed, counted in `ModError::ItemVanished`, and the slot resyncs. Menus stay open,
   `OpenMenu.state` is untouched.
3. Re-run **Control** step 4 (injections, tooltip parts, screens, browser rebuild, icons), unload
   every control script, reload all of them.
4. For every `ScreenRoot` whose kind's `ScreenDef` changed (payload inequality) or whose kind has
   an injection from the reloaded mod: `close_screen` then `spawn_screen` with the same `menu`
   entity. The slot entities are new; `register_slot_refs` re-seeds them, so contents reappear
   without any inventory write. Focus and hover reset; tooltips close.
5. `ModReloaded { mod_id }`.

**(amended in integration)** A `control.lua` that does not compile is only found at step 3, after
the freeze, and step 3 unloads every control script before reloading them. The previous control
script therefore does **not** stay loaded: the mod keeps its data-stage registrations, its failure
reaches `ModErrors`/`ModFailed`, no handler from the broken file runs, and the reload still returns
`Ok`. `examples/modded/tests/mods.rs::a_broken_script_is_reported_and_the_old_one_stays` pins that.
Separately, a stack's `ComponentPatch` is keyed by `ComponentId`, which the same freeze re-interns;
`remap_inventories` remaps `ItemStack::id` by name and leaves those keys alone. No Phase 4 example
writes a component patch; this is a gap for Phase 5 and is in `docs/FOLLOWUPS.md`.

**(amended in integration)** A `RebuildBrowser` also re-runs `slotted-browser`'s six phase
validators, so a reload that adds a recipe type gets a category bound for it and one that removes
an item drops the interpreter registered for it.

### 2.7 Localisation (`locale.rs`)

Own loader over `fluent-bundle`, because `bevy_fluent` 0.15 (Bevy 0.19, confirmed) needs
`*.ftl.ron` bundle descriptors modders should not have to write. `FtlAsset { source: String }`
loaded from `.ftl`; `Locales { lang: LanguageIdentifier, layers: Vec<(ModId, FluentBundle)> }`
resource built from `pack://locale/<lang>.ftl` of base and every mod (`mods/<id>/locale/<lang>.ftl`),
searched in **reverse load order**, base last. Keys are namespaced by convention,
`copper_chest.item.copper_chest`, and unqualified keys are looked up as written. **(amended in
integration)** a Fluent message id is `[A-Za-z][A-Za-z0-9_-]*`, so the convention's dots are not
legal and one bad id rejects a whole file. `locale.rs` therefore rewrites the identifier of each
*definition* as a layer is parsed (`normalise_ids`, dots to dashes, comments and values untouched)
and tries a lookup as written and then normalised. A modder may write either form in either place;
the convention stays dotted. `Locales::resolve
(&LocKey, &FluentArgs) -> Option<String>`. `slotted-ui` `spawn_text` now attaches `LocText(LocKey)`
(shared amendment, in the skeleton); packs' `resolve_loc_text` (`SlottedUiSet::Render`; one `(Entity, &LocText, &mut Text)` query
plus an `Added<LocText>` entity query, since two `&mut Text` queries conflict) writes
`Text` for `Added<LocText>` and for every `LocText` when `Locales` changes, and leaves the key
verbatim when unresolved. `SemanticLabel` keeps the key, so Phase 2/3 snapshots stand.

**(amended in integration) the localisation port.** `LocText` covers a screen tree, but the item
browser draws names too and `slotted-packs` depends on `slotted-browser`, so the browser could not
reach `Locales` and its cards drew the key. `slotted-ui` now owns the port:

```rust
pub trait Localizer: Send + Sync + 'static { fn resolve(&self, key: &LocKey) -> Option<String>; }
#[derive(Resource, Clone)] pub struct Localization(pub Arc<dyn Localizer>);   // default: NoLocalization
```

`Locales` is a handle on an `Arc<LocaleTable>` and `load_locales` inserts the same table as both the
resource and the port, so the two can never disagree; `resolve_loc_text` reads the port as well.
`slotted-browser`'s `IngredientCtx` carries a `&Localization`, which is where `ItemType::display_name`
resolves `ItemDef::display_name` ("a localisation key **or** a literal display name") for a card,
the search index and a recipe page title alike. `NoLocalization` resolves nothing and every reader
falls back to the key, so a crate that loads no mods, and every Phase 2 and Phase 3 snapshot, is
unchanged. `examples/modded/tests/mods.rs::a_card_shows_the_name_from_the_mods_ftl` pins it. The
browser's own chrome and a category chip's label are still English literals; see
`docs/FOLLOWUPS.md`.

## 3. `examples/modded` and the harness (C)

**(amended in integration) the mods are three, not two, and they carry more than v1.0 asked.**
The paragraphs below stand except where this list changes them.

- **`sorter`** is a third mod whose only job is to put a button on a screen it does not own:
  `data.lua` injects a `slotted:button` at `copper_chest:chest`'s `title_end` with
  `exclusion = true`, and `control.lua` answers `widget_activate` with `slotted.cmd.sort`. It
  declares a required dependency on `copper_chest` so the load order is stable. `UiNodeDef` has no
  icon variant outside `SideTab`, so the injected node is a button labelled from its params rather
  than the icon button the brief sketched.
- **`copper_chest` registers two items and a 3x3 recipe type.** Eight ingots do not fit a 2x2
  grid. `copper_chest:copper_chest` keeps its contract id, gains `rarity = "uncommon"` and is in
  both `c:storage` and `c:chests`; `copper_chest:copper_ingot` is new and joins `c:ingots`, which
  `assets/data/demo/tags/ingots.ron` already owns, which is the one line that proves tags merge
  across a mod boundary; `copper_chest:assembly` is 3x3 and the recipe is a ring of `#c:ingots`.
- **`appleskin_like` has a `control.lua` as well**, so the tooltip API shows both halves:
  `data.lua` adds a static part filtered by the tag `c:foods` (`appleskin_like-food`) and
  `control.lua` answers `tooltip_build` with a per-item line (`appleskin_like-food-apple`),
  because a control script sees the stack but not the tag index.
- **The console toggles on `F1` and `F8`.** v1.0 said `F8`, the brief said `F1`; both are bound and
  neither is a key `slotted-ui` uses. `--no-console` starts it hidden, which is how
  `shots/modded.png` is captured without the overlay over the chest. `ScriptLogs` is a resource but
  a `ModFailed` is a message that lives two frames, so the example keeps its own `ConsoleErrors`
  list; errors draw first and in red, then the last log lines.
- **`load_mods` still takes a `PackLayout`.** The builder's new `mods_dir(path)` copies the
  directory to a temporary one **at build time** and `UiHarness::mod_layout()` returns a layout over
  that copy, which is what the reload tests edit. A lazy copy would be invisible to a `PackLayout`
  an earlier `load_mods` had already inserted.
- **`mod_errors` reads two sources**: the `ModErrors` resource if it exists, plus every `ModFailed`
  message seen since the harness was built, minus duplicates, so a test can assert on a failure
  from many frames ago.
- **Base data is not in the load order.** `DataStage::new(order)` is built from the mod ids, so
  `assets/data/demo/` is not loaded by the example and the seeded inventories are made entirely of
  mod items. The modded example cannot reuse `chest::inventories`.

`examples/modded`: windowed app as `chest` is, `PackSourcePlugin` before `DefaultPlugins`,
`SlottedPlugins` with `packs` and `script-mlua`, `SlottedPacksPlugin` with `reload_on_change: true`.
`PackLayout { base: assets/, mods: examples/modded/mods/, resource_packs: [] }`. Opens the
`copper_chest:chest` screen at start with the same inventories as the chest example.

`mods/copper_chest/`: `mod.toml` (`id = "copper_chest"`, `api_version = 1`, `entry.data =
"data.lua"`, `entry.control = "control.lua"`), `data.lua` registers item `copper_chest`
(tags `#c:storage`), tag `c:storage`, recipe type `copper_chest:assembly` (2x2), recipe
`copper_chest` from `#demo:ingots`, and screen `copper_chest:chest` (the demo chest tree with a
`Text` title `copper_chest.screen.title` and an `Anchor { id: "title_end" }`); `control.lua`
subscribes to `slot_click`: logs every click at Info, returns `toggle_favorite` on Alt+left;
`locale/en-US.ftl` has the title and item name. `mods/appleskin_like/`: data-only, `mod.toml`
with `entry.data`, `data.lua` calls `add_tooltip_part{ tags = {"c:foods"}, tier = "any", nodes
= {{ type = "text", key = "appleskin_like.food", style = "muted" }} }` and `register_tag("c:foods",
{ values = {"demo:apple"} })` plus `register_item("demo:apple", ...)` so the part has a target.

Dev console: a `Panel` with up to 12 `Text` children at `GlobalZIndex(zbands::DEV)`, rebuilt when
`ScriptLogs` changes, toggled with F8, red rows for `ModFailed`. `--shot <path>` as in `chest`;
`--reload <mod>` sends `ReloadMod` after the first frame (for the screenshot of a reload).

`slotted-test` feature `script` adds `slotted-packs` and `slotted-script`; the facade's
`script-mlua` feature (on by default; wasm builds pass `--no-default-features`) supplies the
runtime, so the harness never names mlua. `impl UiHarness`, in `src/mods.rs`:
`load_mods(&mut self, layout: PackLayout) -> LoadReport` (inserts the layout, runs
`ModLoader::run_all`, one frame), `reload_mod(&mut self, id: &str) -> Result<(), ModError>`,
`script_logs(&self) -> Vec<LogEntry>`, `mod_errors(&self) -> Vec<ModError>`,
`open_mod_screen(&mut self, kind: &str, fixture) -> Opened` (looks the kind up in `Screens`).
`examples/modded/tests/mods.rs`: copper item in a slot (`with_item("copper_chest:copper_chest")`),
recipe listed in the browser for that item, `appleskin_like.food` in `TooltipContent` for an apple,
and the reload test: copy `mods/` to a temp dir, load, click a slot, assert the log line, rewrite
`control.lua` to log a different string, `reload_mod`, click again, assert the new line and
`assert_conserved()` with `stack_at` unchanged.

## 4. Ownership

| Package | Files |
|---|---|
| A | `slotted-script-mlua/src/*` bodies; `slotted-testutils/src/conformance.rs` bodies and `conformance/*.lua` (four seed cases exist); `slotted-script-mlua/tests/conformance.rs` (un-ignore); prelude fixes |
| B | `slotted-packs/src/*` bodies; `slotted-test` snapshot re-acceptance if `LocText` changes a tree |
| C | `examples/modded/**`, `slotted-test/src/mods.rs`, `slotted-test` feature `script` wiring |
| shared (amend here first) | `slotted-script/src/*` (complete; signatures only change here), `slotted-packs/src/{lib,plugin}.rs` signatures, `slotted-registry` `load_into` (done), `slotted-ui` `LocText` (done), facade features and `MluaHostPlugin` (done), `Cargo.toml`s |

`ScriptAsset` and `DataFile` are packs' (they are Bevy assets); `ScriptRuntime`, events, commands,
`Value` bridge and prelude are script's. Facade: `packs` (default) adds `SlottedPacksPlugin` to
`SlottedPlugins`; `script-mlua` (default) adds `MluaHostPlugin`, which inserts `ScriptHost` with
`MluaRuntime` at plugin build when absent. `HeadlessBevyPlugins` now includes `StatesPlugin`.
`PackSourcePlugin` also inserts the `PackLayout` resource, so the example inserts nothing else.

## 5. Deviations from PLAN.md

- `ScriptRuntime` is exactly 4.8; subscriptions ride a `Subscribe` command instead of a method.
- `Move`, `QuickStack`, `Sort` are separate commands plus a `Click` escape hatch; `Move` is a
  click plan, since the model has no move action.
- Menus are identified to scripts by `MenuId`, never `Entity`.
- The lifecycle runs synchronously in `PreStartup` on native; `ModStage` records it and Phase 5's
  async loader steps the same machine per frame.
- Hot reload re-runs the whole data stage and remaps ids by name rather than rebuilding one mod
  incrementally; inventory state survives by remap and re-seed, not by diffing.
- Own Fluent loader instead of `bevy_fluent`; `RegisterWidget` is a template without params.
- `slotted-script-piccolo` and `-luaur` are not started in Phase 4.
- **(amended in integration)** There is one untyped-to-typed deserializer for the whole workspace,
  in `slotted-model`, and the RON data stage goes through it too. Section 1.3 has the rules. The
  cost is that an untyped payload keeps its numbers as `i64`/`f64` rather than the narrowest RON
  type that fit, and that `IconDef`/`ViewSubject` serialise as one-key maps.
