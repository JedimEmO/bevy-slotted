//! Mods and resource packs for slotted.
//!
//! Three jobs, one crate, because they share the layering rule "resource
//! packs, then mods in reverse load order, then base":
//!
//! - **Discovery and layering** ([`ModSet`], [`PackLayout`], [`LayeredSource`],
//!   [`LayeredAssetReader`]): read every `mods/<id>/mod.toml`, sort with
//!   `resolve_load_order`, and serve one logical path across several roots,
//!   both to the registry's `DataStage` and to Bevy as the `pack://` source.
//! - **Lifecycle** ([`ModLoader`], [`ModStage`], [`SlottedPacksPlugin`]): run
//!   the data stage (RON and `data.lua` into the same builders), freeze,
//!   publish screens, injections and tooltip parts, load `control.lua`, then
//!   route UI events to scripts and their commands back through validation.
//!   Hot reload re-runs the same steps and remaps inventory ids by name.
//! - **Localisation** ([`Locales`], [`FtlAsset`]): `locale/<lang>.ftl` per mod,
//!   layered, resolving `LocText` nodes.
//!
//! `docs/design/phase4-contract.md` section 2 is the contract this crate
//! implements.

pub mod assets;
pub mod error;
pub mod install;
#[cfg(feature = "ui")]
pub mod invalidate;
pub mod lifecycle;
pub mod locale;
pub mod modset;
pub mod plugin;
pub mod prepare;
pub mod route;
pub mod source;
#[cfg(feature = "ui")]
pub mod tooltip;
pub mod uidef;

pub use assets::{DataFile, DataFileLoader, ModWatch, ScriptAsset, ScriptLoader};
pub use error::ModError;
pub use lifecycle::{
    ControlScript, ControlScripts, FrozenSnapshot, LogEntry, ModErrors, ModFailed, ModLoader,
    ModReloaded, ModStage, PacksBakedIcons, ReloadMod, ScriptHost, ScriptLog, ScriptLogs,
};
#[cfg(feature = "ui")]
pub use locale::resolve_loc_text;
pub use locale::{FtlAsset, FtlLoader, Locales};
pub use modset::{MANIFEST_FILE, ModEntry, ModPaths, ModSet, PackLayout};
pub use plugin::{PackSourcePlugin, PacksConfig, SlottedPacksPlugin, SlottedPacksSet};
pub use route::{
    MAX_SCRIPT_CALLS_PER_FRAME, OpenScreens, PendingScriptEvents, TOOLTIP_BUILD,
    WarnedDeprecations, apply_control_command,
};
pub use source::{
    LayeredAssetReader, LayeredSource, PACK_SOURCE, PackAssets, SharedSource, SourceAssetReader,
};
#[cfg(feature = "ui")]
pub use tooltip::{CHILDREN_ANCHOR, ScriptTooltipPart, StaticPart, TemplateWidget};

/// Everything a game needs to wire mods in.
pub mod prelude {
    pub use crate::{
        LogEntry, ModError, ModErrors, ModFailed, ModLoader, ModReloaded, ModSet, ModStage,
        PackAssets, PackLayout, PackSourcePlugin, PacksConfig, ReloadMod, ScriptHost, ScriptLog,
        ScriptLogs, SlottedPacksPlugin,
    };
}
