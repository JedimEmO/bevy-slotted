//! The mod lifecycle: discover, data, freeze, control, running, and reload.
//! Contract sections 2.3 and 2.6.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::{Arc, Mutex};

use bevy::prelude::*;
use slotted_registry::{FrozenRegistries, LoadReport};
use slotted_script::{LogLevel, ModId, ScriptCommand, ScriptId, ScriptRuntime};

use crate::ModError;

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
        // PHASE4-IMPL: B -- contract 2.3 steps 1..5.
        let _ = world;
        Err(ModError::NoLayout)
    }

    /// Re-run data, freeze, control for the whole set after `id` changed,
    /// keeping menus open and remapping inventories by name.
    ///
    /// # Errors
    ///
    /// The [`ModError`] that aborted the reload; the previous registries and
    /// scripts stay in place.
    pub fn reload_mod(world: &mut World, id: &ModId) -> Result<(), ModError> {
        // PHASE4-IMPL: B -- contract 2.6 steps 1..5.
        let _ = (world, id);
        Err(ModError::NoLayout)
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
        // PHASE4-IMPL: B -- Value -> ron::Value -> into_rust::<Def>; replace
        // semantics; Inject and AddTooltipPart go to `collected`.
        let _ = (registries, collected);
        Err(ModError::WrongStage {
            mod_id: mod_id.clone(),
            command: command.name().to_owned(),
        })
    }

    /// Converts the untagged model value into the registry's `ron::Value`.
    pub fn to_ron_value(value: &slotted_model::Value) -> ron::Value {
        // PHASE4-IMPL: B
        let _ = value;
        ron::Value::Unit
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
