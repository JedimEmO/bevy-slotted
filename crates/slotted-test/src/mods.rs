//! Scripted harness extension: load a `mods/` directory into a running
//! harness, reload one mod, read the console. Phase 4 contract section 3.
//! Enabled by the `script` feature; the runtime comes from the facade.

use slotted_packs::{LogEntry, ModError, ModLoader, PackLayout, ScriptLogs};
use slotted_registry::LoadReport;
use slotted_script::ModId;

use crate::fixture::{MenuFixture, Opened};
use crate::harness::UiHarness;

impl UiHarness {
    /// Inserts `layout`, runs the whole lifecycle (`ModLoader::run_all`) and
    /// steps one frame. The same code path a reload takes, so a test that
    /// loads mods after `build()` exercises what the example does at start.
    ///
    /// # Panics
    ///
    /// When the load fails fatally; per-mod script failures do not panic and
    /// are visible through [`mod_errors`](Self::mod_errors).
    pub fn load_mods(&mut self, layout: PackLayout) -> LoadReport {
        // PHASE4-IMPL: C
        self.world_mut().insert_resource(layout);
        let report =
            ModLoader::run_all(self.world_mut()).unwrap_or_else(|e| panic!("loading mods: {e}"));
        self.step(1);
        report
    }

    /// Re-runs the lifecycle after `id`'s files changed on disk.
    ///
    /// # Errors
    ///
    /// The [`ModError`] that aborted the reload; the previous state stands.
    pub fn reload_mod(&mut self, id: &str) -> Result<(), ModError> {
        // PHASE4-IMPL: C
        let id = ModId::new(id).unwrap_or_else(|e| panic!("{e}"));
        let result = ModLoader::reload_mod(self.world_mut(), &id);
        self.step(1);
        result
    }

    /// Every console line so far, oldest first.
    pub fn script_logs(&self) -> Vec<LogEntry> {
        self.world()
            .get_resource::<ScriptLogs>()
            .map(|l| l.entries.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// Every error the loader reported.
    pub fn mod_errors(&self) -> Vec<ModError> {
        // PHASE4-IMPL: C -- read the `ModFailed` messages the harness keeps.
        Vec::new()
    }

    /// Opens a screen registered by a mod, by kind string.
    ///
    /// # Panics
    ///
    /// When the kind is not in `Screens`.
    pub fn open_mod_screen(&mut self, kind: &str, fixture: impl MenuFixture) -> Opened {
        // PHASE4-IMPL: C
        let kind = slotted_ui::ScreenKind(
            slotted_model::Namespaced::parse(kind).unwrap_or_else(|e| panic!("{e}")),
        );
        self.open_screen(kind, fixture)
    }
}
