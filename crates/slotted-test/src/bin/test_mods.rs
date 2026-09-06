//! `test-mods <mods dir> [--filter <mod>]`: run every mod's `tests/*.lua`
//! through a headless harness and print a report. Phase 6 contract section
//! 3.2. `cargo xtask test-mods <dir>` is the front door.

use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(dir) = args.first().map(PathBuf::from) else {
        eprintln!("usage: test-mods <mods dir> [--filter <mod>]");
        return ExitCode::FAILURE;
    };
    let filter = args
        .windows(2)
        .find(|w| w[0] == "--filter")
        .map(|w| w[1].clone());
    match run(&dir, filter.as_deref()) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(message) => {
            eprintln!("test-mods: {message}");
            ExitCode::FAILURE
        }
    }
}

/// Builds the harness, loads the mods, runs the tests. `Ok(true)` when every
/// test passed.
fn run(dir: &std::path::Path, filter: Option<&str>) -> Result<bool, String> {
    // PHASE6-IMPL: C. `UiHarness::builder().plugins(SlottedPlugins::headless())
    // .mods_dir(dir).build()`, `load_mods(mod_layout())`, register the
    // `"chest"` fixture alias (27+27+9 empty), then for each mod (filtered)
    // `run_mod_tests`; print `ok <mod>/<file>: <name>` or `FAIL ...: <message>`
    // and a summary line.
    let _ = (dir, filter);
    Err("test-mods is not implemented yet".to_owned())
}
