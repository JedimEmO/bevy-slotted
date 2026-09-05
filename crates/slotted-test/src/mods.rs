//! Scripted harness extension: load a `mods/` directory into a running
//! harness, reload one mod, read the console. Phase 4 contract section 3.
//! Enabled by the `script` feature; the runtime comes from the facade.
//!
//! ```no_run
//! use slotted_test::prelude::*;
//!
//! let mut h = UiHarness::builder()
//!     .plugins(SlottedPlugins::headless())
//!     .mods_dir("examples/modded/mods")
//!     .build();
//! let layout = h.mod_layout();
//! h.load_mods(layout);
//! ```
//!
//! [`UiHarnessBuilder::mods_dir`](crate::UiHarnessBuilder::mods_dir) copies
//! the directory to a temporary one first, so
//! [`edit_mod_file`](UiHarness::edit_mod_file) rewrites a script the test
//! owns rather than the one in the repository.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use slotted_packs::{LogEntry, ModError, ModFailed, ModLoader, PackLayout, ScriptLogs};
use slotted_registry::LoadReport;
use slotted_script::ModId;

use crate::fixture::{MenuFixture, Opened};
use crate::harness::UiHarness;

/// The mods directory a test works against, and the throwaway copy of it.
///
/// The copy is what every path the harness hands out points at. Nothing the
/// harness does can reach the original, which is usually a directory under
/// version control.
#[derive(Resource, Debug)]
pub struct ModsUnderTest {
    /// Where the mods were copied from.
    pub source: PathBuf,
    /// The copy, removed when the harness drops.
    work: TempDir,
}

impl ModsUnderTest {
    /// The directory to discover mods in.
    pub fn dir(&self) -> &Path {
        &self.work.0
    }
}

/// A directory removed on drop.
#[derive(Debug)]
struct TempDir(PathBuf);

impl Drop for TempDir {
    fn drop(&mut self) {
        // A test that already failed should not fail again over cleanup.
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Every [`ModFailed`] message seen since the harness was built.
///
/// Packs reports a per-mod failure as a message rather than a return value,
/// so a test that wants to assert on one needs somewhere for it to land.
#[derive(Resource, Debug, Default, Clone)]
struct ModErrorLog(Vec<ModError>);

/// `Last`: drains `ModFailed` into [`ModErrorLog`].
fn collect_mod_errors(mut reader: MessageReader<ModFailed>, mut log: ResMut<ModErrorLog>) {
    for failure in reader.read() {
        log.0.push(failure.error.clone());
    }
}

/// Copies `dir` to a temporary directory and records both on the harness.
///
/// Called by the builder; a test names the directory with
/// [`UiHarnessBuilder::mods_dir`](crate::UiHarnessBuilder::mods_dir).
///
/// # Panics
///
/// When the directory cannot be copied: a test that cannot stage its mods
/// has nothing left to prove.
pub(crate) fn install_mods_dir(harness: &mut UiHarness, dir: &Path) {
    let work = TempDir(
        std::env::temp_dir().join(format!("slotted-mods-{}", uuid::Uuid::new_v4().simple())),
    );
    copy_dir(dir, &work.0)
        .unwrap_or_else(|e| panic!("copying {} to {}: {e}", dir.display(), work.0.display()));
    harness.world_mut().insert_resource(ModsUnderTest {
        source: dir.to_path_buf(),
        work,
    });
}

/// Recursive directory copy. `std::fs` has none.
fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

impl UiHarness {
    /// A [`PackLayout`] over the workspace `assets/` and the staged mods
    /// directory.
    ///
    /// # Panics
    ///
    /// When no `mods_dir` was set on the builder, or a `mod.toml` is
    /// malformed.
    pub fn mod_layout(&mut self) -> PackLayout {
        let dir = self
            .world()
            .get_resource::<ModsUnderTest>()
            .expect("mod_layout needs UiHarness::builder().mods_dir(..)")
            .dir()
            .to_path_buf();
        PackLayout::new(assets_dir())
            .with_mods(&dir)
            .unwrap_or_else(|e| panic!("discovering {}: {e}", dir.display()))
    }

    /// Rewrites one file of one staged mod, for a reload test.
    ///
    /// `rel_path` is relative to the mod's own directory
    /// (`control.lua`, `data/copper_chest/items/x.ron`). Only the copy the
    /// harness made is touched.
    ///
    /// # Panics
    ///
    /// When no `mods_dir` was set on the builder, or the write fails.
    pub fn edit_mod_file(&mut self, id: &str, rel_path: &str, contents: &str) {
        let dir = self
            .world()
            .get_resource::<ModsUnderTest>()
            .expect("edit_mod_file needs UiHarness::builder().mods_dir(..)")
            .dir()
            .join(id);
        let path = dir.join(rel_path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .unwrap_or_else(|e| panic!("creating {}: {e}", parent.display()));
        }
        std::fs::write(&path, contents)
            .unwrap_or_else(|e| panic!("writing {}: {e}", path.display()));
    }

    /// Inserts `layout`, runs the whole lifecycle (`ModLoader::run_all`) and
    /// steps one frame. The same code path a reload takes, so a test that
    /// loads mods after `build()` exercises what the example does at start.
    ///
    /// # Panics
    ///
    /// When the load fails fatally; per-mod script failures do not panic and
    /// are visible through [`mod_errors`](Self::mod_errors).
    pub fn load_mods(&mut self, layout: PackLayout) -> LoadReport {
        self.arm_error_log();
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
    ///
    /// # Panics
    ///
    /// When `id` is not a valid mod id.
    pub fn reload_mod(&mut self, id: &str) -> Result<(), ModError> {
        self.arm_error_log();
        let id = ModId::new(id).unwrap_or_else(|e| panic!("{e}"));
        let result = ModLoader::reload_mod(self.world_mut(), &id);
        if let Err(error) = &result {
            self.world_mut()
                .get_resource_or_init::<ModErrorLog>()
                .0
                .push(error.clone());
        }
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

    /// Every [`ScriptLogs`] line whose message contains `needle`.
    pub fn script_logs_containing(&self, needle: &str) -> Vec<LogEntry> {
        self.script_logs()
            .into_iter()
            .filter(|entry| entry.message.contains(needle))
            .collect()
    }

    /// Every error the loader reported since the harness was built.
    ///
    /// Packs keeps the last load's failures in its own `ModErrors` resource
    /// and also writes each one as a [`ModFailed`] message, which lives for
    /// two frames. The harness keeps every message it ever saw, and returns
    /// the union, so a test can assert on a failure several frames later.
    pub fn mod_errors(&self) -> Vec<ModError> {
        let mut out: Vec<ModError> = self
            .world()
            .get_resource::<slotted_packs::lifecycle::ModErrors>()
            .map(|errors| errors.0.clone())
            .unwrap_or_default();
        if let Some(log) = self.world().get_resource::<ModErrorLog>() {
            for error in &log.0 {
                if !out.contains(error) {
                    out.push(error.clone());
                }
            }
        }
        out
    }

    /// Opens a screen registered by a mod, by kind string.
    ///
    /// # Panics
    ///
    /// When the kind is not a namespaced id, or is not in `Screens`.
    pub fn open_mod_screen(&mut self, kind: &str, fixture: impl MenuFixture) -> Opened {
        let kind = slotted_ui::ScreenKind(
            slotted_model::Namespaced::parse(kind).unwrap_or_else(|e| panic!("{e}")),
        );
        self.open_screen(kind, fixture)
    }

    /// Makes sure [`ModFailed`] messages are being collected.
    ///
    /// The harness has no plugin of its own, so the system goes into the
    /// world's `Last` schedule the first time mods are loaded.
    fn arm_error_log(&mut self) {
        if self.world().contains_resource::<ModErrorLog>() {
            return;
        }
        let world = self.world_mut();
        world.init_resource::<ModErrorLog>();
        world
            .get_resource_or_init::<Schedules>()
            .add_systems(Last, collect_mod_errors);
    }
}

/// The workspace `assets/` directory, resolved from this crate's root so a
/// test binary finds it whatever the working directory is.
fn assets_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets")
}
