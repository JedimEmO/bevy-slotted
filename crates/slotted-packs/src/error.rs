//! [`ModError`]: everything the lifecycle can report.

use slotted_model::MenuId;
use slotted_registry::{LoadError, LoadOrderError, ManifestError, RegistryError};
use slotted_script::{ModId, ScriptError};

/// Why a mod, or the whole set, failed. Surfaces as a [`crate::ModFailed`]
/// message and in the dev console.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum ModError {
    /// No `PackLayout` resource.
    #[error("no PackLayout resource: insert one before SlottedPacksPlugin runs")]
    NoLayout,
    /// A mod has scripts but there is no `ScriptHost`.
    #[error("no script runtime: enable the facade's `script-mlua` feature or insert a ScriptHost")]
    NoRuntime,
    /// A `mod.toml` did not parse.
    #[error(transparent)]
    Manifest(#[from] ManifestError),
    /// Dependencies could not be ordered.
    #[error(transparent)]
    LoadOrder(#[from] LoadOrderError),
    /// The RON data stage failed.
    #[error(transparent)]
    Load(#[from] LoadError),
    /// A registration or the freeze failed.
    #[error(transparent)]
    Registry(#[from] RegistryError),
    /// A script failed.
    #[error("{mod_id}: {source}")]
    Script {
        /// Which mod.
        mod_id: ModId,
        /// What the runtime said.
        source: ScriptError,
    },
    /// A command from the wrong stage.
    #[error("{mod_id}: `{command}` is not allowed at this stage")]
    WrongStage {
        /// Which mod.
        mod_id: ModId,
        /// The command's `type`.
        command: String,
    },
    /// A command that did not validate.
    #[error("{mod_id}: `{command}`: {message}")]
    BadCommand {
        /// Which mod.
        mod_id: ModId,
        /// The command's `type`.
        command: String,
        /// Why.
        message: String,
    },
    /// The script's budget or memory limit was hit.
    #[error("{mod_id}: budget exceeded; the script is unloaded")]
    Budget {
        /// Which mod.
        mod_id: ModId,
    },
    /// A command named a menu that is not open.
    #[error("{mod_id}: menu {menu:?} is not open")]
    UnknownMenu {
        /// Which mod.
        mod_id: ModId,
        /// The id.
        menu: MenuId,
    },
    /// After a reload an item in an inventory no longer exists.
    #[error("{count} x `{item}` removed: the item no longer exists after reload")]
    ItemVanished {
        /// The old name.
        item: String,
        /// How many were dropped.
        count: u32,
    },
    /// Reading a file outside the registry path.
    #[error("{path}: {message}")]
    Io {
        /// Which file.
        path: String,
        /// What happened.
        message: String,
    },
}
