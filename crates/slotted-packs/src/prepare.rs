//! Stage one of a load: discover, read and freeze.
//!
//! What this module guarantees when it returns without an error:
//!
//! * every enabled mod's RON and `data.lua` has been read in load order and
//!   applied into one open [`slotted_registry::Registries`], so the last mod
//!   in the order wins a conflict and every patch has seen the entry it
//!   patches;
//! * every command a `data.lua` issued has been either applied, collected for
//!   the install stage ([`CollectedUi`]), or reported as an error against the
//!   mod that issued it -- nothing is dropped silently;
//! * nothing has been published. The world's registries, screens, widgets and
//!   injections are exactly what they were before the call.
//!
//! That last point is the reason this stage is separate: a mod set that fails
//! to load must leave the running game alone, and it can only do that while
//! its work is still confined to values this module owns.

use std::collections::BTreeSet;

use bevy::prelude::*;
use slotted_model::Namespaced;
use slotted_registry::defs::{
    FluidDef, HudLayerDef, ItemDef, RecipeDef, RecipeTypeDef, ScreenDef, TagDef, WidgetDef,
};
use slotted_registry::{AssetSource, DataStage, FrozenRegistries};
use slotted_script::{API_VERSION, LogLevel, ModId, ScriptCommand, ScriptEvent, Stage};

use crate::lifecycle::{
    CollectedUi, DataOutcome, LogEntry, ModLoader, ScriptHost, from_script_error, script_mod_id,
};
#[cfg(feature = "ui")]
use crate::uidef::UiNodeDef;
use crate::{ModEntry, ModError, ModSet};

impl ModLoader {
    /// The RON data stage plus every mod's `data.lua`, into one open set.
    pub(crate) fn run_data_stage(
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
                // A server has no `slotted-ui` to type the tree against. The
                // payload is still stored verbatim, so the client that draws
                // this screen types it at its own load and reports the same
                // error against the same mod.
                #[cfg(feature = "ui")]
                {
                    let _typed: slotted_ui::ScreenDef =
                        slotted_model::from_value(value.clone()).map_err(|e| bad(e.to_string()))?;
                }
                let payload = slotted_registry::from_model(&value);
                registries
                    .screens
                    .replace(name.clone(), ScreenDef { name, payload });
            }
            ScriptCommand::RegisterWidget { id, def } => {
                let name = name(id)?;
                #[cfg(feature = "ui")]
                {
                    let _typed: UiNodeDef =
                        slotted_model::from_value(def.clone()).map_err(|e| bad(e.to_string()))?;
                }
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
                #[cfg(feature = "ui")]
                slotted_ui::FluidDef::from_payload(&name, &payload).map_err(&bad)?;
                registries
                    .fluids
                    .replace(name.clone(), FluidDef { name, payload });
            }
            ScriptCommand::RegisterHudLayer { id, def } => {
                let name = name(id)?;
                let payload = slotted_registry::from_model(def);
                #[cfg(feature = "ui")]
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
pub(crate) fn into_def<T: serde::de::DeserializeOwned>(
    value: &slotted_model::Value,
    id: &str,
    bad: &impl Fn(String) -> ModError,
) -> Result<T, ModError> {
    let mut value = value.clone();
    value.or_insert("name", id);
    slotted_model::from_value(value).map_err(|e| bad(e.to_string()))
}

/// Warns when a mod registers an id in a namespace that is neither its own, one
/// of its dependencies, nor a shared one.
///
/// Reaching into another namespace is how a compatibility mod adds a tag to
/// somebody else's item, so this is a warning and never an error. A namespace
/// in `PacksConfig::shared_namespaces` (`c` by default, the Fabric common-tag
/// convention) is not even that: every mod is meant to write there.
pub(crate) fn namespace_warnings(
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
pub(crate) fn read_script(
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
/// Contract 2.6 step 2, the component half.
///
/// The dense ids inside a `ComponentPatch` are interned by the same freeze
/// that numbers items, and a reload renumbers both. A patch left alone would
/// therefore read some other component's value out of the same slot, with no
/// error anywhere. Rebuilds `patch` by name; a key the new registry has lost
/// is dropped and recorded in `dropped` against the item that carried it.
/// Returns whether anything moved.
pub(crate) fn remap_patch(
    old: &FrozenRegistries,
    new: &FrozenRegistries,
    item: &slotted_model::Namespaced,
    patch: &mut slotted_model::ComponentPatch,
    dropped: &mut BTreeSet<(String, String)>,
) -> bool {
    if patch.is_empty() {
        return false;
    }
    let mut rebuilt = slotted_model::ComponentPatch::new();
    let mut changed = false;
    for (id, value) in patch.iter() {
        let Some(key) = old.components.name(id) else {
            dropped.insert((format!("{id:?}"), item.to_string()));
            changed = true;
            continue;
        };
        if let Some(fresh) = new.components.get(key) {
            changed |= fresh != id;
            rebuilt.insert(fresh, value.clone());
        } else {
            dropped.insert((key.to_string(), item.to_string()));
            changed = true;
        }
    }
    if changed {
        *patch = rebuilt;
    }
    changed
}
