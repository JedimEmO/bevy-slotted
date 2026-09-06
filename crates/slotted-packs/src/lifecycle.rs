//! The mod lifecycle: discover, data, freeze, control, running, and reload.
//! Contract sections 2.3 and 2.6.

use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::sync::{Arc, Mutex};

use bevy::ecs::message::{Message, Messages};
use bevy::prelude::*;
use slotted_model::Namespaced;
use slotted_registry::defs::{
    FluidDef, HudLayerDef, ItemDef, RecipeDef, RecipeTypeDef, ScreenDef, TagDef, WidgetDef,
};
use slotted_registry::{AssetSource, DataStage, FrozenRegistries, LoadReport};
use slotted_script::{
    API_VERSION, LogLevel, ModId, ScriptCommand, ScriptError, ScriptEvent, ScriptId, ScriptRuntime,
    Stage,
};
use slotted_ui::def::{AnchorId, ScreenKind, UiNodeDef, WidgetKind};
use slotted_ui::tooltip::TooltipParts;
use slotted_ui::{Injection, Injections, Screens, WidgetRegistry};

use crate::tooltip::{ScriptTooltipPart, StaticPart, TemplateWidget};
use crate::{LayeredSource, ModEntry, ModError, ModSet, ModWatch, PackLayout};

/// Where the loader is. On native every stage completes inside `PreStartup`;
/// the state is recorded so Phase 5's asynchronous loader can step the same
/// machine one stage per frame.
#[derive(States, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ModStage {
    /// Nothing loaded.
    #[default]
    Idle,
    /// Reading manifests.
    Discover,
    /// RON and `data.lua` into open registries.
    Data,
    /// Interning.
    Freeze,
    /// Publishing screens, injections, tooltip parts; loading `control.lua`.
    Control,
    /// Events flow.
    Running,
}

/// The script runtime, shared by the lifecycle and the tooltip part.
///
/// The facade inserts an `MluaRuntime` here when its `script-mlua` feature is
/// on and nothing else did. Absent, data-only mods still load and scripted
/// ones report [`ModError::NoRuntime`].
#[derive(Resource, Clone)]
pub struct ScriptHost(pub Arc<Mutex<Box<dyn ScriptRuntime>>>);

impl ScriptHost {
    /// Wraps a runtime.
    pub fn new(runtime: impl ScriptRuntime + 'static) -> Self {
        Self(Arc::new(Mutex::new(Box::new(runtime))))
    }

    /// The runtime, recovering from a panic in another caller: a poisoned
    /// mutex would otherwise take the whole mod system down with it.
    pub fn lock(&self) -> std::sync::MutexGuard<'_, Box<dyn ScriptRuntime>> {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

/// One loaded control script and what it listens to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ControlScript {
    /// Handle in the runtime.
    pub id: ScriptId,
    /// Event names from its `Subscribe` command.
    pub events: BTreeSet<String>,
}

/// Every loaded control script, by mod, in load order.
#[derive(Resource, Debug, Default, Clone, PartialEq, Eq)]
pub struct ControlScripts {
    /// Scripts per mod.
    pub by_mod: BTreeMap<ModId, ControlScript>,
}

impl ControlScripts {
    /// Mods whose script handles `event`.
    pub fn subscribed<'a>(
        &'a self,
        event: &'a str,
    ) -> impl Iterator<Item = (&'a ModId, &'a ControlScript)> {
        self.by_mod
            .iter()
            .filter(move |(_, s)| s.events.contains(event))
    }
}

/// The previous frozen set, kept across a reload so inventories can be
/// remapped by name.
#[derive(Resource, Debug, Clone)]
pub struct FrozenSnapshot(pub Arc<FrozenRegistries>);

/// One console line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogEntry {
    /// Which mod, or `None` for the loader itself.
    pub mod_id: Option<ModId>,
    /// Severity.
    pub level: LogLevel,
    /// Text.
    pub message: String,
}

/// The console buffer. Capped; oldest lines drop first.
#[derive(Resource, Debug, Clone)]
pub struct ScriptLogs {
    /// Lines, oldest first.
    pub entries: VecDeque<LogEntry>,
    /// Maximum lines kept.
    pub cap: usize,
}

impl Default for ScriptLogs {
    fn default() -> Self {
        Self {
            entries: VecDeque::new(),
            cap: 500,
        }
    }
}

impl ScriptLogs {
    /// Appends, dropping the oldest past `cap`.
    pub fn push(&mut self, entry: LogEntry) {
        if self.entries.len() >= self.cap {
            self.entries.pop_front();
        }
        self.entries.push_back(entry);
    }
}

/// A script or the loader logged a line. Also appended to [`ScriptLogs`].
#[derive(Message, Debug, Clone, PartialEq, Eq)]
pub struct ScriptLog {
    /// Which mod.
    pub mod_id: Option<ModId>,
    /// Severity.
    pub level: LogLevel,
    /// Text.
    pub message: String,
}

/// Something failed. The previous state is kept.
#[derive(Message, Debug, Clone, PartialEq)]
pub struct ModFailed {
    /// Which mod, or `None` for the set.
    pub mod_id: Option<ModId>,
    /// What.
    pub error: ModError,
}

/// A reload finished.
#[derive(Message, Debug, Clone, PartialEq, Eq)]
pub struct ModReloaded {
    /// Which mod triggered it.
    pub mod_id: ModId,
}

/// Ask for a reload. The watcher, the console and the harness all send this.
#[derive(Message, Debug, Clone, PartialEq, Eq)]
pub struct ReloadMod {
    /// Which mod.
    pub mod_id: ModId,
}

/// Every [`ModError`] the last load or reload produced, newest last.
///
/// Kept as a resource as well as a [`ModFailed`] message so the dev console
/// and the test harness can read the whole list at any time.
#[derive(Resource, Debug, Default, Clone, PartialEq)]
pub struct ModErrors(pub Vec<ModError>);

/// Marks the [`slotted_icons::Icons`] resource as one packs baked, so a
/// reload may replace it and a game-supplied one is left alone.
#[derive(Resource, Debug, Default, Clone, Copy)]
pub struct PacksBakedIcons;

/// Where packs' own entries sit in the shared UI registries, so a reload can
/// take exactly its own back out.
#[derive(Resource, Debug, Default, Clone)]
struct PacksOwned {
    injections: Vec<Injection>,
    tooltip_part: Option<usize>,
    widgets: Vec<WidgetKind>,
}

/// The interned form of a mod id on the script side.
pub(crate) fn script_mod_id(id: &slotted_registry::ModId) -> ModId {
    ModId::new(id.as_str()).unwrap_or_else(|e| unreachable!("registry ids are script ids: {e}"))
}

/// A runtime failure, mapped to the loader's error.
fn from_script_error(mod_id: &ModId, source: ScriptError) -> ModError {
    match source {
        ScriptError::BudgetExceeded { .. } | ScriptError::Memory { .. } => ModError::Budget {
            mod_id: mod_id.clone(),
        },
        source => ModError::Script {
            mod_id: mod_id.clone(),
            source,
        },
    }
}

/// Puts back the control script a mod already had, after its new chunk failed.
///
/// The handle is still live in the runtime, because nothing is unloaded until
/// its replacement is running, so the mod keeps answering events with the last
/// chunk that worked. Its `tooltip_build` subscription comes back too, or the
/// tooltip would lose a line that the mod never stopped providing.
fn keep_previous(
    previous: &ControlScripts,
    scripts: &mut ControlScripts,
    dynamic: &mut Vec<(ModId, ScriptId)>,
    mod_id: &ModId,
) {
    let Some(kept) = previous.by_mod.get(mod_id) else {
        return;
    };
    if kept.events.contains(crate::route::TOOLTIP_BUILD) {
        dynamic.push((mod_id.clone(), kept.id));
    }
    scripts.by_mod.insert(mod_id.clone(), kept.clone());
}

/// Writes `message` only when its `Messages` resource exists, so the loader
/// runs in a bare `World` as well as in a full app.
pub(crate) fn send<M: Message>(world: &mut World, message: M) {
    if let Some(mut messages) = world.get_resource_mut::<Messages<M>>() {
        messages.write(message);
    }
}

/// Appends to [`ScriptLogs`], writes a [`ScriptLog`] and mirrors to `tracing`.
pub(crate) fn log(world: &mut World, mod_id: Option<ModId>, level: LogLevel, message: String) {
    let owner = mod_id.as_ref().map_or("packs", ModId::as_str).to_owned();
    match level {
        LogLevel::Trace => {
            tracing::trace!(target: "slotted_packs::script", mod_id = %owner, "{message}");
        }
        LogLevel::Debug => {
            tracing::debug!(target: "slotted_packs::script", mod_id = %owner, "{message}");
        }
        LogLevel::Info => {
            tracing::info!(target: "slotted_packs::script", mod_id = %owner, "{message}");
        }
        LogLevel::Warn => {
            tracing::warn!(target: "slotted_packs::script", mod_id = %owner, "{message}");
        }
        LogLevel::Error => {
            tracing::error!(target: "slotted_packs::script", mod_id = %owner, "{message}");
        }
    }
    if let Some(mut logs) = world.get_resource_mut::<ScriptLogs>() {
        logs.push(LogEntry {
            mod_id: mod_id.clone(),
            level,
            message: message.clone(),
        });
    }
    send(
        world,
        ScriptLog {
            mod_id,
            level,
            message,
        },
    );
}

/// Records a failure in [`ModErrors`] and as a [`ModFailed`] message.
pub(crate) fn fail(world: &mut World, mod_id: Option<ModId>, error: ModError) {
    tracing::error!(%error, "mod error");
    if let Some(mut errors) = world.get_resource_mut::<ModErrors>() {
        errors.0.push(error.clone());
    }
    send(world, ModFailed { mod_id, error });
}

/// Moves the state machine on, when a state machine exists.
fn set_stage(world: &mut World, stage: ModStage) {
    if let Some(mut next) = world.get_resource_mut::<NextState<ModStage>>() {
        next.set(stage);
    }
    if let Some(mut state) = world.get_resource_mut::<State<ModStage>>() {
        // `run_all` completes inside one system, so the state transition
        // schedule never gets a turn; write the current state too.
        *state = State::new(stage);
    }
}

/// What one data stage produced.
struct DataOutcome {
    registries: slotted_registry::Registries,
    report: LoadReport,
    collected: CollectedUi,
    errors: Vec<(Option<ModId>, ModError)>,
    logs: Vec<LogEntry>,
}

/// The source the loader reads mods through: a host-supplied [`PackAssets`]
/// when there is one, otherwise a [`LayeredSource`] over the layout.
///
/// `PackAssets` is how a host with no filesystem (`wasm32-unknown-unknown`)
/// gets its mods in; native builds never insert it and read the disk as before.
fn pack_source(world: &World, layout: &PackLayout) -> crate::SharedSource {
    world.get_resource::<crate::PackAssets>().map_or_else(
        || Arc::new(LayeredSource::new(layout)) as _,
        |assets| assets.0.clone(),
    )
}

/// The state machine. Pure functions over `&mut World` so `PreStartup`, the
/// reload system and the test harness run the identical code.
#[derive(Debug, Clone, Copy, Default)]
pub struct ModLoader;

impl ModLoader {
    /// Discover through Running, synchronously.
    ///
    /// # Errors
    ///
    /// The first fatal [`ModError`]. Per-mod script failures are not fatal:
    /// they become [`ModFailed`] messages and the mod's RON still loads.
    pub fn run_all(world: &mut World) -> Result<LoadReport, ModError> {
        let layout = world
            .get_resource::<PackLayout>()
            .cloned()
            .ok_or(ModError::NoLayout)?;
        set_stage(world, ModStage::Discover);
        world.insert_resource(layout.mods.clone());
        world.init_resource::<ModErrors>();

        set_stage(world, ModStage::Data);
        let source = pack_source(world, &layout);
        let source = &*source;
        let outcome = Self::run_data_stage(world, &layout.mods, source)?;
        let DataOutcome {
            registries,
            mut report,
            collected,
            errors,
            logs,
        } = outcome;
        for entry in logs {
            log(world, entry.mod_id, entry.level, entry.message);
        }
        for (mod_id, error) in errors {
            fail(world, mod_id, error);
        }

        set_stage(world, ModStage::Freeze);
        let (frozen, warnings) = registries.freeze()?;
        for warning in &warnings {
            log(world, None, LogLevel::Warn, warning.to_string());
        }
        report.warnings.extend(warnings);
        let frozen = Arc::new(frozen);
        Self::install_registries(world, frozen.clone());

        set_stage(world, ModStage::Control);
        Self::run_control_stage(world, &layout, source, &frozen, &collected);

        set_stage(world, ModStage::Running);
        Ok(report)
    }

    /// Re-run data, freeze, control for the whole set after `id` changed,
    /// keeping menus open and remapping inventories by name.
    ///
    /// # Errors
    ///
    /// The [`ModError`] that aborted the reload; the previous registries and
    /// scripts stay in place.
    pub fn reload_mod(world: &mut World, id: &ModId) -> Result<(), ModError> {
        let layout = world
            .get_resource::<PackLayout>()
            .cloned()
            .ok_or(ModError::NoLayout)?;
        let source = pack_source(world, &layout);
        let source = &*source;

        // Step 1: the whole data stage again. Anything at all that goes wrong
        // leaves the previous frozen set untouched.
        let outcome = match Self::run_data_stage(world, &layout.mods, source) {
            Ok(outcome) => outcome,
            Err(error) => {
                fail(world, Some(id.clone()), error.clone());
                return Err(error);
            }
        };
        let DataOutcome {
            registries,
            collected,
            errors,
            logs,
            ..
        } = outcome;
        for entry in logs {
            log(world, entry.mod_id, entry.level, entry.message);
        }
        if let Some((mod_id, error)) = errors.into_iter().next() {
            fail(world, mod_id, error.clone());
            return Err(error);
        }

        let previous = world
            .get_resource::<slotted_ecs::Registries>()
            .map(|r| r.0.clone());
        let (frozen, warnings) = match registries.freeze() {
            Ok(frozen) => frozen,
            Err(error) => {
                let error = ModError::Registry(error);
                fail(world, Some(id.clone()), error.clone());
                return Err(error);
            }
        };
        for warning in &warnings {
            log(world, None, LogLevel::Warn, warning.to_string());
        }
        let frozen = Arc::new(frozen);

        // Step 2: ids are dense and were re-interned, so every live stack has
        // to be looked up again by name before anything reads it.
        let before: HashMap<ScreenKind, Arc<slotted_ui::ScreenDef>> = world
            .get_resource::<Screens>()
            .map(|s| s.0.clone())
            .unwrap_or_default()
            .into_iter()
            .collect();
        Self::install_registries(world, frozen.clone());
        if let Some(previous) = previous {
            for error in remap_inventories(world, &previous, &frozen) {
                fail(world, Some(id.clone()), error);
            }
        }

        // Step 3: publish everything the frozen set feeds, control scripts
        // included.
        Self::run_control_stage(world, &layout, source, &frozen, &collected);

        // Step 4: respawn the screens whose tree actually changed.
        let mut changed: BTreeSet<ScreenKind> = world
            .get_resource::<Screens>()
            .map(|s| {
                s.0.iter()
                    .filter(|(kind, def)| before.get(*kind).is_none_or(|old| ***def != **old))
                    .map(|(kind, _)| kind.clone())
                    .collect()
            })
            .unwrap_or_default();
        for injection in &collected.injections {
            if let ScriptCommand::Inject { screen, .. } = &injection.1
                && let Ok(name) = Namespaced::parse(screen)
            {
                changed.insert(ScreenKind(name));
            }
        }
        respawn_screens(world, &changed);

        set_stage(world, ModStage::Running);
        send(world, ModReloaded { mod_id: id.clone() });
        Ok(())
    }

    /// The RON data stage plus every mod's `data.lua`, into one open set.
    fn run_data_stage(
        world: &mut World,
        mods: &ModSet,
        source: &dyn AssetSource,
    ) -> Result<DataOutcome, ModError> {
        let shared = world.get_resource::<crate::PacksConfig>().map_or_else(
            || vec![crate::plugin::SHARED_NAMESPACE.to_owned()],
            |config| config.shared_namespaces.clone(),
        );
        let mut registries = slotted_registry::Registries::new();
        let report = DataStage::new(mods.load_order()).load_into(source, &mut registries)?;
        let mut collected = CollectedUi::default();
        let mut errors = Vec::new();
        let mut logs = Vec::new();

        let host = world.get_resource::<ScriptHost>().cloned();
        for entry in mods.iter() {
            let Some(relative) = entry.manifest.entry.data.clone() else {
                continue;
            };
            let mod_id = script_mod_id(entry.id());
            let Some(host) = host.as_ref() else {
                errors.push((Some(mod_id), ModError::NoRuntime));
                continue;
            };
            let text = match read_script(source, entry, &relative) {
                Ok(text) => text,
                Err(error) => {
                    errors.push((Some(mod_id), error));
                    continue;
                }
            };
            let mut runtime = host.lock();
            let script = match runtime.load(&mod_id, &relative, &text, Stage::Data) {
                Ok(script) => script,
                Err(source) => {
                    errors.push((Some(mod_id.clone()), from_script_error(&mod_id, source)));
                    continue;
                }
            };
            let commands = runtime.call(
                script,
                &ScriptEvent::DataStage {
                    api_version: API_VERSION,
                },
            );
            // A data script has said everything it has to say; its state is
            // dead weight for the rest of the run.
            runtime.unload(script);
            drop(runtime);

            let commands = match commands {
                Ok(commands) => commands,
                Err(source) => {
                    errors.push((Some(mod_id.clone()), from_script_error(&mod_id, source)));
                    continue;
                }
            };
            for command in &commands {
                match command {
                    ScriptCommand::Log { level, message } => logs.push(LogEntry {
                        mod_id: Some(mod_id.clone()),
                        level: *level,
                        message: message.clone(),
                    }),
                    ScriptCommand::Deprecated { call, since, hint } => logs.push(LogEntry {
                        mod_id: Some(mod_id.clone()),
                        level: LogLevel::Warn,
                        message: format!("`{call}` is deprecated since api {since}: {hint}"),
                    }),
                    command => {
                        for warning in namespace_warnings(&mod_id, mods, &shared, command) {
                            logs.push(LogEntry {
                                mod_id: Some(mod_id.clone()),
                                level: LogLevel::Warn,
                                message: warning,
                            });
                        }
                        if let Err(error) = Self::apply_data_command(
                            &mut registries,
                            &mod_id,
                            command,
                            &mut collected,
                        ) {
                            errors.push((Some(mod_id.clone()), error));
                        }
                    }
                }
            }
        }

        Ok(DataOutcome {
            registries,
            report,
            collected,
            errors,
            logs,
        })
    }

    /// Replaces `slotted_ecs::Registries`, keeping the outgoing set so a
    /// reload can remap by name.
    fn install_registries(world: &mut World, frozen: Arc<FrozenRegistries>) {
        if let Some(previous) = world.get_resource::<slotted_ecs::Registries>() {
            let previous = previous.0.clone();
            world.insert_resource(FrozenSnapshot(previous));
        }
        world.insert_resource(slotted_ecs::Registries(frozen));
    }

    /// Contract 2.3 step 4: publish everything the frozen set feeds, then load
    /// the control scripts.
    fn run_control_stage(
        world: &mut World,
        layout: &PackLayout,
        source: &dyn AssetSource,
        frozen: &Arc<FrozenRegistries>,
        collected: &CollectedUi,
    ) {
        world.init_resource::<Screens>();
        world.init_resource::<Injections>();
        world.init_resource::<WidgetRegistry>();
        world.init_resource::<TooltipParts>();
        world.init_resource::<PacksOwned>();

        let owned = world.resource::<PacksOwned>().clone();

        // Screens come straight from the frozen registry payloads. Phase 6
        // adds fluids and HUD layers the same way (contract 1.1, 2.1).
        world.resource_mut::<Screens>().load_from_registry(frozen);
        if let Some(mut fluids) = world.get_resource_mut::<slotted_ui::Fluids>() {
            fluids.load_from_registry(frozen);
        }
        if let Some(mut hud) = world.get_resource_mut::<slotted_ui::HudLayers>() {
            hud.load_from_registry(frozen);
        }

        // Widget templates: one `TemplateWidget` per registry entry.
        let mut widgets = Vec::new();
        for (_, name, def) in frozen.widgets.iter() {
            match def.payload.clone().into_rust::<UiNodeDef>() {
                Ok(template) => widgets.push((
                    WidgetKind(name.clone()),
                    TemplateWidget {
                        kind: WidgetKind(name.clone()),
                        template,
                    },
                )),
                Err(error) => tracing::warn!(%name, %error, "widget payload is not a UiNodeDef"),
            }
        }
        let widget_kinds: Vec<WidgetKind> = widgets.iter().map(|(k, _)| k.clone()).collect();
        {
            let mut registry = world.resource_mut::<WidgetRegistry>();
            for kind in &owned.widgets {
                registry.0.remove(kind);
            }
            for (kind, widget) in widgets {
                registry.register(kind, widget);
            }
        }

        // Injections: take exactly what packs put in last time back out.
        let mut injections = Vec::new();
        for (mod_id, command) in &collected.injections {
            match to_injection(command) {
                Ok(injection) => injections.push(injection),
                Err(error) => fail(world, Some(mod_id.clone()), error),
            }
        }
        {
            let mut resource = world.resource_mut::<Injections>();
            resource
                .0
                .retain(|existing| !owned.injections.contains(existing));
            resource.0.extend(injections.iter().cloned());
        }

        // Control scripts, before the tooltip part, which asks them.
        let dynamic = Self::load_control_scripts(world, layout, source, frozen);

        let mut statics = Vec::new();
        for (mod_id, command) in &collected.tooltip_parts {
            match to_static_part(mod_id, command) {
                Ok(part) => statics.push(part),
                Err(error) => fail(world, Some(mod_id.clone()), error),
            }
        }
        let host = world.get_resource::<ScriptHost>().cloned();
        let part = ScriptTooltipPart {
            host,
            statics,
            dynamic,
        };
        let index = {
            let mut parts = world.resource_mut::<TooltipParts>();
            match owned.tooltip_part {
                Some(index) if index < parts.0.len() => {
                    parts.0[index] = Arc::new(part);
                    index
                }
                _ => {
                    parts.0.push(Arc::new(part));
                    parts.0.len() - 1
                }
            }
        };

        world.insert_resource(PacksOwned {
            injections,
            tooltip_part: Some(index),
            widgets: widget_kinds,
        });

        crate::locale::load_locales(world, layout, source);
        populate_watch(world, layout, source);
        rebake_icons(world, frozen);

        // A recipe type a mod just registered has no category, and
        // `RecipeStore` silently drops every recipe of an unbound type. The
        // browser binds defaults in its own `Startup`, which is too early for a
        // harness that loads mods after `build` and never runs again for a
        // reload, so the freeze does it here too. `bind_defaults` skips types a
        // category already claims, so running it twice is free.
        if let Some(mut categories) = world.get_resource_mut::<slotted_browser::Categories>() {
            categories.bind_defaults(frozen);
        }
        send(world, slotted_browser::RebuildBrowser);
    }

    /// Loads every control script again, returning the `(mod, script)` pairs
    /// subscribed to `tooltip_build`.
    ///
    /// A mod whose new chunk will not compile, will not read or throws out of
    /// `control_start` **keeps the script it already had**, and the previous
    /// chunk is only unloaded once its replacement is running. The failure is
    /// still reported. The alternative, which this used to do, was to unload
    /// everything up front and leave that mod with no control script at all:
    /// in the web playground a single typo then stopped every click in the
    /// game until the page was reloaded, with nothing on screen to say why.
    fn load_control_scripts(
        world: &mut World,
        layout: &PackLayout,
        source: &dyn AssetSource,
        frozen: &Arc<FrozenRegistries>,
    ) -> Vec<(ModId, ScriptId)> {
        let _ = frozen;
        let host = world.get_resource::<ScriptHost>().cloned();
        let previous = world
            .get_resource::<ControlScripts>()
            .cloned()
            .unwrap_or_default();

        let mods: Vec<String> = layout
            .mods
            .load_order()
            .iter()
            .map(ToString::to_string)
            .collect();
        let mut scripts = ControlScripts::default();
        let mut dynamic = Vec::new();
        let entries: Vec<ModEntry> = layout.mods.iter().cloned().collect();
        for entry in &entries {
            let Some(relative) = entry.manifest.entry.control.clone() else {
                continue;
            };
            let mod_id = script_mod_id(entry.id());
            let Some(host) = host.as_ref() else {
                fail(world, Some(mod_id), ModError::NoRuntime);
                continue;
            };
            let text = match read_script(source, entry, &relative) {
                Ok(text) => text,
                Err(error) => {
                    fail(world, Some(mod_id.clone()), error);
                    keep_previous(&previous, &mut scripts, &mut dynamic, &mod_id);
                    continue;
                }
            };
            let mut runtime = host.lock();
            let script = match runtime.load(&mod_id, &relative, &text, Stage::Control) {
                Ok(script) => script,
                Err(source) => {
                    let error = from_script_error(&mod_id, source);
                    drop(runtime);
                    fail(world, Some(mod_id.clone()), error);
                    keep_previous(&previous, &mut scripts, &mut dynamic, &mod_id);
                    continue;
                }
            };
            let commands = runtime.call(
                script,
                &ScriptEvent::ControlStart {
                    api_version: API_VERSION,
                    mods: mods.clone(),
                },
            );
            drop(runtime);
            let commands = match commands {
                Ok(commands) => commands,
                Err(source) => {
                    let error = from_script_error(&mod_id, source);
                    fail(world, Some(mod_id.clone()), error);
                    host.lock().unload(script);
                    keep_previous(&previous, &mut scripts, &mut dynamic, &mod_id);
                    continue;
                }
            };
            let mut events = BTreeSet::new();
            for command in &commands {
                match command {
                    ScriptCommand::Subscribe { events: names } => {
                        events.extend(names.iter().cloned());
                    }
                    ScriptCommand::Log { level, message } => {
                        let (level, message) = (*level, message.clone());
                        log(world, Some(mod_id.clone()), level, message);
                    }
                    other => tracing::debug!(
                        mod_id = %mod_id,
                        command = other.name(),
                        "command at control_start is ignored"
                    ),
                }
            }
            if events.contains(crate::route::TOOLTIP_BUILD) {
                dynamic.push((mod_id.clone(), script));
            }
            scripts
                .by_mod
                .insert(mod_id, ControlScript { id: script, events });
        }

        // Only now, and only what was really replaced: a mod that kept its
        // previous chunk kept the handle with it, and unloading that handle
        // would leave the table pointing at a script the runtime has freed.
        if let Some(host) = host.as_ref() {
            let mut runtime = host.lock();
            for (mod_id, script) in &previous.by_mod {
                if scripts.by_mod.get(mod_id).map(|kept| kept.id) != Some(script.id) {
                    runtime.unload(script.id);
                }
            }
        }
        world.insert_resource(scripts);
        dynamic
    }

    /// Applies one data-stage command into open registries.
    ///
    /// # Errors
    ///
    /// [`ModError::WrongStage`], [`ModError::BadCommand`] (the def did not
    /// deserialise) or [`ModError::Registry`].
    pub fn apply_data_command(
        registries: &mut slotted_registry::Registries,
        mod_id: &ModId,
        command: &ScriptCommand,
        collected: &mut CollectedUi,
    ) -> Result<(), ModError> {
        if command.allowed_stage() == Some(Stage::Control) {
            return Err(ModError::WrongStage {
                mod_id: mod_id.clone(),
                command: command.name().to_owned(),
            });
        }
        let bad = |message: String| ModError::BadCommand {
            mod_id: mod_id.clone(),
            command: command.name().to_owned(),
            message,
        };
        let name = |id: &str| Namespaced::parse(id).map_err(|e| bad(e.to_string()));

        match command {
            ScriptCommand::RegisterItem { id, def } => {
                let name = name(id)?;
                let mut def: ItemDef = into_def(def, id, &bad)?;
                def.name = name.clone();
                registries.items.replace(name, def);
            }
            ScriptCommand::RegisterTag { id, def } => {
                let name = name(id)?;
                let mut def: TagDef = into_def(def, id, &bad)?;
                def.name = name;
                registries.add_tag(def);
            }
            ScriptCommand::RegisterRecipeType { id, def } => {
                let name = name(id)?;
                let mut def: RecipeTypeDef = into_def(def, id, &bad)?;
                def.name = name.clone();
                registries.recipe_types.replace(name, def);
            }
            ScriptCommand::RegisterRecipe { id, def } => {
                let name = name(id)?;
                let mut def: RecipeDef = into_def(def, id, &bad)?;
                def.name = name.clone();
                registries.recipes.replace(name, def);
            }
            ScriptCommand::RegisterScreen { id, def } => {
                let name = name(id)?;
                let mut value = def.clone();
                value.or_insert("kind", id.as_str());
                // Type it here rather than at spawn time, where the modder has
                // no way to tell which of their screens is malformed. The
                // payload the registry keeps is the untyped value as it stands:
                // `ScreenDef::from_value` reads it back under the same rules.
                let _typed: slotted_ui::ScreenDef =
                    slotted_model::from_value(value.clone()).map_err(|e| bad(e.to_string()))?;
                let payload = slotted_registry::from_model(&value);
                registries
                    .screens
                    .replace(name.clone(), ScreenDef { name, payload });
            }
            ScriptCommand::RegisterWidget { id, def } => {
                let name = name(id)?;
                let _typed: UiNodeDef =
                    slotted_model::from_value(def.clone()).map_err(|e| bad(e.to_string()))?;
                let payload = slotted_registry::from_model(def);
                registries
                    .widgets
                    .replace(name.clone(), WidgetDef { name, payload });
            }
            ScriptCommand::RegisterFluid { id, def } => {
                let name = name(id)?;
                let payload = slotted_registry::from_model(def);
                // Type it here for the same reason a screen is typed here: a
                // colour the modder mistyped must name their fluid, not turn
                // into magenta three frames into the game.
                slotted_ui::FluidDef::from_payload(&name, &payload).map_err(&bad)?;
                registries
                    .fluids
                    .replace(name.clone(), FluidDef { name, payload });
            }
            ScriptCommand::RegisterHudLayer { id, def } => {
                let name = name(id)?;
                let payload = slotted_registry::from_model(def);
                slotted_ui::HudLayerPayload::from_value(payload.clone())
                    .map_err(|e| bad(e.to_string()))?;
                registries
                    .hud_layers
                    .replace(name.clone(), HudLayerDef { name, payload });
            }
            ScriptCommand::Inject { .. } => {
                collected.injections.push((mod_id.clone(), command.clone()));
            }
            ScriptCommand::AddTooltipPart { .. } => {
                collected
                    .tooltip_parts
                    .push((mod_id.clone(), command.clone()));
            }
            // Handled by the caller, which owns the log buffer.
            ScriptCommand::Log { .. } | ScriptCommand::Deprecated { .. } => {}
            other => {
                return Err(ModError::WrongStage {
                    mod_id: mod_id.clone(),
                    command: other.name().to_owned(),
                });
            }
        }
        Ok(())
    }

    /// Converts the untagged model value into the registry's `ron::Value`.
    ///
    /// Kept as the contract names it; the conversion itself lives in
    /// [`slotted_registry::from_model`], which is also what the registry's own
    /// loader reverses.
    pub fn to_ron_value(value: &slotted_model::Value) -> ron::Value {
        slotted_registry::from_model(value)
    }
}

/// A script `Value` as a typed def, with `name` filled in from the id.
///
/// [`slotted_model::from_value`] is the same deserializer the registry's own
/// RON loader reads a data file through, so a def registered from Lua and a def
/// written in a file follow one set of rules. See [`slotted_model::value`].
fn into_def<T: serde::de::DeserializeOwned>(
    value: &slotted_model::Value,
    id: &str,
    bad: &impl Fn(String) -> ModError,
) -> Result<T, ModError> {
    let mut value = value.clone();
    value.or_insert("name", id);
    slotted_model::from_value(value).map_err(|e| bad(e.to_string()))
}

/// An `Inject` command as a `slotted-ui` injection.
fn to_injection(command: &ScriptCommand) -> Result<Injection, ModError> {
    let ScriptCommand::Inject {
        screen,
        anchor,
        node,
        exclusion,
    } = command
    else {
        unreachable!("only Inject commands are collected as injections");
    };
    let bad = |message: String| ModError::BadCommand {
        mod_id: ModId::new("packs").expect("a literal id"),
        command: "inject".to_owned(),
        message,
    };
    let target = ScreenKind(Namespaced::parse(screen).map_err(|e| bad(e.to_string()))?);
    let node: UiNodeDef =
        slotted_model::from_value(node.clone()).map_err(|e| bad(e.to_string()))?;
    Ok(Injection {
        target,
        anchor: AnchorId::new(anchor.clone()),
        node,
        exclusion: *exclusion,
    })
}

/// An `AddTooltipPart` command as a static part.
fn to_static_part(mod_id: &ModId, command: &ScriptCommand) -> Result<StaticPart, ModError> {
    let ScriptCommand::AddTooltipPart {
        id,
        when,
        tier,
        nodes,
    } = command
    else {
        unreachable!("only AddTooltipPart commands are collected as tooltip parts");
    };
    let mut parsed = Vec::with_capacity(nodes.len());
    for node in nodes {
        parsed.push(
            slotted_model::from_value::<UiNodeDef>(node.0.clone()).map_err(|e| {
                ModError::BadCommand {
                    mod_id: mod_id.clone(),
                    command: "add_tooltip_part".to_owned(),
                    message: e.to_string(),
                }
            })?,
        );
    }
    Ok(StaticPart {
        mod_id: mod_id.clone(),
        id: id.clone(),
        when: when.clone(),
        tier: *tier,
        nodes: parsed,
    })
}

/// Warns when a mod registers an id in a namespace that is neither its own, one
/// of its dependencies, nor a shared one.
///
/// Reaching into another namespace is how a compatibility mod adds a tag to
/// somebody else's item, so this is a warning and never an error. A namespace
/// in `PacksConfig::shared_namespaces` (`c` by default, the Fabric common-tag
/// convention) is not even that: every mod is meant to write there.
fn namespace_warnings(
    mod_id: &ModId,
    mods: &ModSet,
    shared: &[String],
    command: &ScriptCommand,
) -> Vec<String> {
    let (ScriptCommand::RegisterItem { id, .. }
    | ScriptCommand::RegisterTag { id, .. }
    | ScriptCommand::RegisterRecipeType { id, .. }
    | ScriptCommand::RegisterRecipe { id, .. }
    | ScriptCommand::RegisterScreen { id, .. }
    | ScriptCommand::RegisterWidget { id, .. }
    | ScriptCommand::RegisterFluid { id, .. }
    | ScriptCommand::RegisterHudLayer { id, .. }) = command
    else {
        return Vec::new();
    };
    let Ok(name) = Namespaced::parse(id) else {
        return Vec::new();
    };
    let namespace = name.namespace();
    if namespace == mod_id.as_str() || shared.iter().any(|s| s == namespace) {
        return Vec::new();
    }
    let entry = mods
        .iter()
        .find(|entry| entry.id().as_str() == mod_id.as_str());
    let declared = entry.is_some_and(|entry| {
        entry
            .manifest
            .dependencies
            .iter()
            .any(|dep| dep.id.as_str() == namespace)
    });
    if declared {
        return Vec::new();
    }
    vec![format!(
        "`{id}` is outside the `{mod_id}` namespace and `{namespace}` is not a declared dependency"
    )]
}

/// A mod's script text, through the layered source when a pack overrides it
/// and from the mod's own directory otherwise.
fn read_script(
    source: &dyn AssetSource,
    entry: &ModEntry,
    relative: &str,
) -> Result<String, ModError> {
    let logical = format!("scripts/{}/{relative}", entry.id());
    let bytes = if let Ok(bytes) = source.read(&logical) {
        bytes
    } else {
        let path = entry.root.join(relative);
        std::fs::read(&path).map_err(|err| ModError::Io {
            path: path.to_string_lossy().into_owned(),
            message: err.to_string(),
        })?
    };
    String::from_utf8(bytes).map_err(|err| ModError::Io {
        path: logical,
        message: err.to_string(),
    })
}

/// Contract 2.6 step 2: every live stack looked up again by name.
fn remap_inventories(
    world: &mut World,
    old: &FrozenRegistries,
    new: &FrozenRegistries,
) -> Vec<ModError> {
    let mut vanished: BTreeMap<String, u32> = BTreeMap::new();
    let mut remap = |stack: &mut Option<slotted_model::ItemStack>| {
        let Some(current) = stack.as_ref() else {
            return false;
        };
        let Some(name) = old.items.name_of(current.id) else {
            let entry = vanished.entry(format!("{:?}", current.id)).or_default();
            *entry += current.count;
            *stack = None;
            return true;
        };
        match new.items.id_of(name) {
            Some(id) if id == current.id => false,
            Some(id) => {
                if let Some(stack) = stack.as_mut() {
                    stack.id = id;
                }
                true
            }
            None => {
                *vanished.entry(name.to_string()).or_default() += current.count;
                *stack = None;
                true
            }
        }
    };

    let entities: Vec<Entity> = world
        .query_filtered::<Entity, With<slotted_ecs::Inventory>>()
        .iter(world)
        .collect();
    for entity in entities {
        let Some(mut inventory) = world.get_mut::<slotted_ecs::Inventory>(entity) else {
            continue;
        };
        for index in 0..inventory.0.len() {
            let mut slot = inventory.0.get(index).cloned();
            if remap(&mut slot) {
                inventory.0.set(index, slot);
            }
        }
    }

    let carried: Vec<Entity> = world
        .query_filtered::<Entity, With<slotted_ecs::Carried>>()
        .iter(world)
        .collect();
    for entity in carried {
        let Some(mut held) = world.get_mut::<slotted_ecs::Carried>(entity) else {
            continue;
        };
        let mut slot = held.0.clone();
        if remap(&mut slot) {
            held.0 = slot;
        }
    }

    vanished
        .into_iter()
        .map(|(item, count)| ModError::ItemVanished { item, count })
        .collect()
}

/// Contract 2.6 step 4: close and re-open every screen of a changed kind on
/// the same menu entity, so the slots re-seed without an inventory write.
fn respawn_screens(world: &mut World, changed: &BTreeSet<ScreenKind>) {
    if changed.is_empty() {
        return;
    }
    let roots: Vec<(Entity, ScreenKind, Option<Entity>)> = world
        .query::<(Entity, &slotted_ui::ScreenRoot)>()
        .iter(world)
        .filter(|(_, root)| changed.contains(&root.kind))
        .map(|(entity, root)| (entity, root.kind.clone(), root.menu))
        .collect();
    if roots.is_empty() {
        return;
    }
    let screens = world.resource::<Screens>().clone();
    for (root, kind, menu) in roots {
        let Some(def) = screens.get(&kind).cloned() else {
            continue;
        };
        let mut commands = world.commands();
        slotted_ui::close_screen(&mut commands, root);
        slotted_ui::spawn_screen(&mut commands, def, menu);
        world.flush();
    }
}

/// Loads every file a mod ships as a Bevy asset, purely so the file watcher
/// reports changes to it (contract 2.6).
///
/// A mod's scripts are watched when they live at `scripts/<mod id>/`, the one
/// layout where a logical `pack://` path names exactly one mod's file.
fn populate_watch(world: &mut World, layout: &PackLayout, source: &dyn AssetSource) {
    let Some(server) = world.get_resource::<AssetServer>().cloned() else {
        return;
    };
    let mut watch = ModWatch::default();
    for entry in layout.mods.iter() {
        let mod_id = script_mod_id(entry.id());
        let handles = watch.by_mod.entry(mod_id).or_default();
        for relative in entry
            .manifest
            .entry
            .data
            .iter()
            .chain(entry.manifest.entry.control.iter())
        {
            let logical = format!("scripts/{}/{relative}", entry.id());
            if source.read(&logical).is_ok() {
                handles.push(
                    server
                        .load::<crate::ScriptAsset>(format!("{}://{logical}", crate::PACK_SOURCE))
                        .untyped(),
                );
            }
        }
        for kind in slotted_registry::RegistryKind::ALL {
            let dir = format!("data/{}/{}", entry.id(), kind.dir());
            for path in source.list(&dir).unwrap_or_default() {
                handles.push(
                    server
                        .load::<crate::DataFile>(format!("{}://{path}", crate::PACK_SOURCE))
                        .untyped(),
                );
            }
        }
        let locale = format!(
            "locale/{}.ftl",
            world
                .get_resource::<crate::Locales>()
                .map_or_else(|| unic_langid::langid!("en-US"), |l| l.lang.clone())
        );
        if source.read(&locale).is_ok() {
            handles.push(
                server
                    .load::<crate::FtlAsset>(format!("{}://{locale}", crate::PACK_SOURCE))
                    .untyped(),
            );
        }
    }
    world.insert_resource(watch);
}

/// Re-bakes the placeholder atlas, but only over an `Icons` packs itself put
/// there: a game that supplied its own icon source keeps it.
fn rebake_icons(world: &mut World, frozen: &FrozenRegistries) {
    let ours = world.contains_resource::<PacksBakedIcons>();
    if world.contains_resource::<slotted_icons::Icons>() && !ours {
        return;
    }
    if !world.contains_resource::<Assets<Image>>()
        || !world.contains_resource::<Assets<bevy::image::TextureAtlasLayout>>()
    {
        return;
    }
    // Shape-aware: a mod that declared `icon = { shape = "cube", .. }` gets
    // the same lit primitive the base game's items get.
    let items: Vec<slotted_icons::IconItem<'_>> = frozen
        .items
        .iter()
        .map(|(id, name, def)| slotted_icons::IconItem {
            id,
            name,
            icon: def.icon.as_ref(),
        })
        .collect();
    let baked = slotted_icons::bake_icon_atlas(&items, slotted_icons::CELL);
    let images = baked
        .images
        .iter()
        .filter_map(|(id, path)| {
            world
                .get_resource::<bevy::asset::AssetServer>()
                .map(|server| (*id, server.load::<Image>(path.clone())))
        })
        .collect();
    let image = world.resource_mut::<Assets<Image>>().add(baked.image);
    let layout = world
        .resource_mut::<Assets<bevy::image::TextureAtlasLayout>>()
        .add(baked.layout);
    world.insert_resource(slotted_icons::Icons::new(slotted_icons::AtlasIcons {
        image,
        layout,
        index: baked.index,
        images,
        missing: baked.missing,
    }));
    world.insert_resource(PacksBakedIcons);
    // Tell `slotted-icons` this atlas is a baked one, not a game's own
    // source, so a later change to `Registries` rebakes over it. Without
    // this, a game that installs its own registries after the pack load
    // keeps an atlas indexed by somebody else's item ids.
    world.insert_resource(slotted_icons::BakedByPlugin);
}

/// Data-stage commands that are not registry entries: injections and static
/// tooltip parts, collected per load and published in the Control stage.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct CollectedUi {
    /// `Inject` commands with their mod.
    pub injections: Vec<(ModId, ScriptCommand)>,
    /// `AddTooltipPart` commands with their mod.
    pub tooltip_parts: Vec<(ModId, ScriptCommand)>,
}
