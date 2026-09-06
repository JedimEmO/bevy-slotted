//! The mod lifecycle: discover, data, freeze, control, running, and reload.
//! Contract sections 2.3 and 2.6.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::{Arc, Mutex};

use bevy::ecs::message::{Message, Messages};
use bevy::prelude::*;
use slotted_registry::{FrozenRegistries, LoadReport};
use slotted_script::{LogLevel, ModId, ScriptCommand, ScriptError, ScriptId, ScriptRuntime};
#[cfg(feature = "ui")]
use slotted_ui::ChangeSet;

use crate::install::remap_inventories;
#[cfg(feature = "ui")]
use crate::invalidate::apply_invalidation;
use crate::{LayeredSource, ModError, PackLayout};

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
/// The facade inserts a `LuaurRuntime` here when its `script-luaur` feature is
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

/// Marks the icon atlas as one packs baked, so a reload may replace it and a
/// game-supplied one is left alone. Only meaningful with the `ui` feature,
/// where there is an atlas at all.
#[derive(Resource, Debug, Default, Clone, Copy)]
pub struct PacksBakedIcons;

/// What [`ModLoader::run_control_stage`] reports back: the change set in a UI
/// build, nothing at all in a server build that has no screens to invalidate.
///
/// Ownership itself is not tracked here any more. It lives on the entries in
/// `slotted-ui`'s own registries as [`slotted_ui::Owner`], which is what lets
/// a reload *remove* what a mod stopped shipping instead of only adding to
/// what is already there.
#[cfg(feature = "ui")]
pub(crate) type StageChange = ChangeSet;
#[cfg(not(feature = "ui"))]
pub(crate) type StageChange = ();

/// The owner id every pack-provided tooltip part is registered under.
///
/// Packs contribute one composite part over every mod's statics and dynamic
/// subscriptions rather than one part per mod, so the owner names the loader
/// rather than a mod.
#[cfg(feature = "ui")]
pub(crate) const PACKS_TOOLTIP_OWNER: &str = "slotted:packs";

/// The interned form of a mod id on the script side.
pub(crate) fn script_mod_id(id: &slotted_registry::ModId) -> ModId {
    ModId::new(id.as_str()).unwrap_or_else(|e| unreachable!("registry ids are script ids: {e}"))
}

/// A runtime failure, mapped to the loader's error.
pub(crate) fn from_script_error(mod_id: &ModId, source: ScriptError) -> ModError {
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
pub(crate) fn keep_previous(
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
pub(crate) fn set_stage(world: &mut World, stage: ModStage) {
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
pub(crate) struct DataOutcome {
    pub(crate) registries: slotted_registry::Registries,
    pub(crate) report: LoadReport,
    pub(crate) collected: CollectedUi,
    pub(crate) errors: Vec<(Option<ModId>, ModError)>,
    pub(crate) logs: Vec<LogEntry>,
}

/// The source the loader reads mods through: a host-supplied [`PackAssets`]
/// when there is one, otherwise a [`LayeredSource`] over the layout.
///
/// `PackAssets` is how a host with no filesystem (`wasm32-unknown-unknown`)
/// gets its mods in; native builds never insert it and read the disk as before.
pub(crate) fn pack_source(world: &World, layout: &PackLayout) -> crate::SharedSource {
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
        #[cfg_attr(not(feature = "ui"), allow(clippy::let_unit_value))]
        let change = Self::run_control_stage(world, &layout, source, &frozen, &collected);

        // A first load usually has nothing open, but a harness that loads a
        // pack set after the game is running does, and it is the same rule
        // either way: what the install reached has to respawn.
        #[cfg(feature = "ui")]
        apply_invalidation(world, &change);
        #[cfg(not(feature = "ui"))]
        let () = change;

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
        Self::install_registries(world, frozen.clone());
        if let Some(previous) = previous {
            for error in remap_inventories(world, &previous, &frozen) {
                fail(world, Some(id.clone()), error);
            }
        }

        // Step 3: publish everything the frozen set feeds, control scripts
        // included. What that reconcile moved comes back as a change set.
        #[cfg_attr(not(feature = "ui"), allow(clippy::let_unit_value))]
        let change = Self::run_control_stage(world, &layout, source, &frozen, &collected);

        // Step 4: respawn every screen the change reaches. "Reaches" is the
        // whole point of the change set: a screen is affected by an edit to
        // the screen it inherits, to a widget template it spawns, and to an
        // injection aimed at it -- including one that went away, and including
        // the `slotted:any` wildcard. There are no screens spawned in a build
        // with no UI, so a reload there stops after the control scripts.
        #[cfg(feature = "ui")]
        apply_invalidation(world, &change);
        #[cfg(not(feature = "ui"))]
        let () = change;

        set_stage(world, ModStage::Running);
        send(world, ModReloaded { mod_id: id.clone() });
        Ok(())
    }
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
