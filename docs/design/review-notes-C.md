# Review notes C: facade features and CI gating

External review findings 7 and 8.

## 7. `default-features = false` now really is a server graph

The facade's `bevy` dependency enabled `bevy_ui`, `bevy_text`, `bevy_picking`,
`bevy_window`, `bevy_scene`, `default_font` and `ui_picking` unconditionally, so
the lightweight dedicated-server build its feature comments advertised did not
exist: turning `ui` off hid our wrappers and compiled Bevy's UI stack anyway.

**What changed.** `crates/slotted/Cargo.toml` keeps only `std`, `bevy_asset`,
`bevy_log` and `bevy_state` on the `bevy` dependency. Every Bevy UI feature moved
into the facade's own `ui` feature, which also turns on `slotted-packs?/ui`.
`packs` no longer implies `ui`. A new `server` feature is
`packs + script-luaur + net` and nothing else.

**`slotted-packs` gained a `ui` feature** (default on), because it depended on
`slotted-ui`, `slotted-icons` and `slotted-browser` unconditionally and those
three are where `bevy_ui` entered the graph. The three are now optional. The
gates are `#[cfg(feature = "ui")]` only; no logic moved except one split
described below.

| File | What is behind `ui` |
|---|---|
| `src/lib.rs` | the `tooltip` module, `resolve_loc_text`, the `tooltip::*` re-exports |
| `src/uidef.rs` | new. Re-exports `slotted_ui::def`'s `LocKey`, `ScreenKind`, `AnchorId`, `UiNodeDef`, `WidgetKind` with `ui`; declares `LocKey` and `ScreenKind` (same newtype shapes) without it |
| `src/locale.rs` | the `Localizer` impl, `Locales::port`, `resolve_or_warn`, the `Localization` insert |
| `src/route.rs` | `on_widget_activate`, `on_screen_spawned`, `on_screen_closed`, `on_property_changed`, `collect_browser_events`, `ingredient_name`, `apply_set_hud`, `apply_hud_update`, `hud_value`, `hud_def`, `typed`, and the `SetHud`/`HudUpdate` arms of `apply_control_command` |
| `src/plugin.rs` | the four UI observers, `collect_browser_events`, and ordering `SlottedPacksSet` against `SlottedUiSet`. Without `ui` the two sets keep only their relation to `SlottedEcsSet::Input` |
| `src/lifecycle.rs` | see below |

`lifecycle.rs` is the one place with a **structural** change rather than a plain
gate. `ModLoader::run_control_stage` interleaved UI publishing with the control
scripts, so its UI half is now two private methods:

- `publish_ui` — screens, fluids, HUD layers, widget templates, injections.
  Returns the previous `PacksOwned` so the tooltip slot can be reused.
- `publish_tooltip_part` — the statics and the `tooltip_build` subscribers.
  It now only writes `PacksOwned::tooltip_part`, because `publish_ui` has
  already recorded the injections and widgets of this load.

Also gated there: the `Screens` diff and `respawn_screens` on the reload path,
`to_injection`, `to_static_part`, `rebake_icons`, `PacksOwned`, and the four
data-stage arms that type a payload against `slotted_ui::ScreenDef`,
`UiNodeDef`, `FluidDef` and `HudLayerPayload`. A server stores those payloads
verbatim and does not type them; the client that draws the screen types it at
its own load and reports the same error against the same mod. A script calling
`set_hud` or `hud_update` on a server gets a named `BadCommand`, not a silent
success.

**Proof.** `just server-check` greps `cargo tree` and then builds the profile on
native and on wasm32; CI runs it as the `server` job.

| Build | Distinct crates in `cargo tree -e normal` |
|---|---|
| `slotted`, default features | 300 |
| `slotted --no-default-features --features server` | 160 |

Zero matches for `bevy_(ui|text|picking|winit|window|render)` in the second.

**Plugin groups.** `SlottedPlugins::headless()` is a UI helper and moved behind
`ui`, along with `HeadlessBevyPlugins`, `HeadlessStack` and
`HeadlessRenderAssets`. `SlottedPlugins::server()` is new: `ServerBevyPlugins`
(task pool, frame count, time, states, a 60 Hz runner, transforms, assets) plus
the group.

## 8. Pages cannot deploy over a red branch

`pages` depended on `wasm` and `docs` alone. It now needs `native`, `wasm`,
`browser`, `server` and `docs`.

The `browser` job could also pass without running anything: it looked for a
Chrome on the runner and skipped its own steps when it found none. It now
installs one with `browser-actions/setup-chrome`, so the wasm script-runtime
tests always run. `push` triggers on `master` as well as `main`, and the `pages`
condition is
`contains(fromJSON('["refs/heads/main","refs/heads/master"]'), github.ref)`.

## Handover to rev-ui: the gate inventory to preserve

The `ui` gating of `slotted-packs` is **complete**: `cargo check -p slotted-packs
--no-default-features` and `cargo clippy -p slotted-packs
--no-default-features` are both clean, and `just server-check` passes. There is
no remaining list of sites to apply.

What follows is the inventory to carry through the `lifecycle.rs` split, so the
server graph does not silently regain `bevy_ui`. `just server-check` is the
assertion; run it after the restructure.

### `src/lib.rs`

`#[cfg(feature = "ui")]` on `pub mod tooltip`, on `pub use locale::resolve_loc_text`,
and on `pub use tooltip::{CHILDREN_ANCHOR, ScriptTooltipPart, StaticPart, TemplateWidget}`.

### `src/uidef.rs` (new, ungated module)

With `ui`, re-exports `slotted_ui::def`'s `AnchorId`, `LocKey`, `ScreenKind`,
`UiNodeDef`, `WidgetKind`. Without it, declares `LocKey(pub String)` and
`ScreenKind(pub Namespaced)`, the same newtype shapes, so the non-UI half of the
lifecycle still has a key type and a screen-kind type. Everything in the crate
names these through `crate::uidef`, never through `slotted_ui::def`.

### `src/locale.rs`

Gated: the `use slotted_ui::{Localization, Localizer}` import, `LocaleTable::resolve_or_warn`,
`impl Localizer for LocaleTable`, `Locales::port`, the `pub use slotted_ui::resolve_loc_text`,
and the `world.insert_resource(locales.port())` line in `load_locales`.

### `src/route.rs`

Gated whole items: `on_widget_activate`, `on_screen_spawned`, `on_screen_closed`,
`on_property_changed`, `collect_browser_events`, `ingredient_name`,
`apply_hud_update`, `apply_set_hud`, `hud_value`, `hud_def`, `typed`, and the
`use slotted_ui::{ScreenClosed, ScreenRoot, ScreenSpawned}` import.

In `apply_control_command`: the `SetHud` and `HudUpdate` arms are gated, and a
`#[cfg(not(feature = "ui"))]` arm over both returns a named `BadCommand` rather
than letting them fall through to the wrong-stage arm. `on_slot_clicked`,
`dispatch_script_events`, `resolve_menu`, `check_inventory`, `check_slot`,
`stack_info`, `OpenScreens`, `PendingScriptEvents`, `WarnedDeprecations` and
`TOOLTIP_BUILD` are **not** gated: they are the server's dispatch path.

### `src/plugin.rs`

`SlottedPacksPlugin::build` ends in two blocks. The `ui` one adds the four UI
observers, `collect_browser_events`, and `configure_sets` ordering
`SlottedPacksSet` against `slotted_ui::SlottedUiSet`. The `not(ui)` one keeps
only `Dispatch.after(Collect).before(SlottedEcsSet::Input)`.

### `src/lifecycle.rs`

Gated imports: `std::collections::HashMap`, `slotted_ui::tooltip::TooltipParts`,
`slotted_ui::{Injection, Injections, Screens, WidgetRegistry}`,
`crate::tooltip::{ScriptTooltipPart, StaticPart, TemplateWidget}`, and
`crate::uidef::{AnchorId, ScreenKind, UiNodeDef, WidgetKind}`.

Gated whole items: `PacksOwned`, `ModLoader::publish_ui`,
`ModLoader::publish_tooltip_part`, `to_injection`, `to_static_part`,
`respawn_screens`, `rebake_icons`.

Gated blocks:

| Where | What |
|---|---|
| `reload_mod` | the `before: HashMap<ScreenKind, Arc<ScreenDef>>` snapshot, and the whole step-4 block that diffs `Screens`, folds in the `Inject` targets and calls `respawn_screens` |
| `run_control_stage` | the `publish_ui` call, the `publish_tooltip_part` call (with a `let _ = dynamic;` on the other side), the `rebake_icons` call, and the trailing `slotted_browser::Categories` / `RebuildBrowser` block. Its `collected` parameter carries `#[cfg_attr(not(feature = "ui"), allow(unused_variables))]` |
| the data stage | the four arms that type a payload against `slotted_ui::ScreenDef`, `UiNodeDef`, `FluidDef::from_payload` and `HudLayerPayload::from_value`. Without `ui` the payload is stored verbatim and the client that draws the screen types it at its own load |

`publish_ui` returns the *previous* `PacksOwned` so the tooltip slot index can be
reused; it also writes the new injections and widgets. `publish_tooltip_part`
therefore only assigns `PacksOwned::tooltip_part`, and must not rebuild the whole
resource, or a reload loses the injection bookkeeping.

## For the owner of `slotted-ui` / `slotted-packs`

`slotted-packs` builds clean with `--no-default-features`. Its **`ui` build is
currently broken by in-flight `slotted-ui` changes that are not mine**:
`Screens`, `WidgetRegistry` and `TooltipParts` became named-field structs and
`Injection` gained an `owner` field, while `slotted-packs` still uses `.0` and
builds an `Injection` without it.

| `crates/slotted-packs/src/lifecycle.rs` | Needs |
|---|---|
| 446, 469 | `Screens` field access, was `.0` |
| 688 | `WidgetRegistry` field access, was `.0` |
| 744, 745, 749, 750 | `TooltipParts` field access, was `.0` |
| 1051 | `Injection { .. }` is missing `owner` |

None of those lines are touched by this change.
