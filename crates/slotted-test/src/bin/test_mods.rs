//! `test-mods <mods dir> [--filter <mod>]`: run every mod's `tests/*.lua`
//! through a headless harness and print a report. Phase 6 contract section
//! 3.2. `cargo xtask test-mods <dir>` is the front door.
//!
//! ```text
//! cargo xtask test-mods examples/machine/mods
//! cargo xtask test-mods examples/machine/mods --filter sorter
//! ```
//!
//! The harness the tests run against is the shipped one: the workspace
//! `assets/` as the base pack, every mod under the directory loaded through
//! the real lifecycle, and the game's own screens picked up from a `screens/`
//! directory beside `mods/` (that is where an example keeps them, and a mod
//! test that opens a host screen has no other way to reach it).

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use slotted_test::prelude::*;

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
fn run(dir: &Path, filter: Option<&str>) -> Result<bool, String> {
    if !dir.is_dir() {
        return Err(format!("{} is not a directory", dir.display()));
    }
    let mut harness = UiHarness::builder()
        .plugins(SlottedPlugins::headless())
        .resolution(1600.0, 900.0)
        .theme("glass")
        .mods_dir(dir)
        .build();

    // The `chest` alias every mod test may open against: a single chest plus
    // the player's pockets, all empty.
    harness.register_fixture("chest", ChestFixture::empty());
    for def in host_screens(dir) {
        harness.world_mut().resource_mut::<Screens>().register(def);
    }

    // Only the mods under `dir` have tests; `mod_layout_with_base` adds the
    // base pack's namespaces as content, and they are never asked for one.
    let ids: Vec<String> = harness
        .mod_layout()
        .mods
        .load_order()
        .iter()
        .map(ToString::to_string)
        .collect();
    let layout = harness.mod_layout_with_base();
    harness.load_mods(layout);
    for error in harness.mod_errors() {
        eprintln!("warning: {error}");
    }

    let wanted: Vec<String> = match filter {
        Some(one) => ids.iter().filter(|id| *id == one).cloned().collect(),
        None => ids.clone(),
    };
    if wanted.is_empty() {
        return Err(match filter {
            Some(one) => format!("no mod named {one} in {}", dir.display()),
            None => format!("no mods in {}", dir.display()),
        });
    }

    let mut passed = 0usize;
    let mut failed = 0usize;
    let mut files = 0usize;
    for id in &wanted {
        for report in harness.run_mod_tests(id) {
            files += 1;
            let file = report.file_name();
            for result in &report.results {
                if result.passed {
                    passed += 1;
                    println!("ok {id}/{file}: {}", result.name);
                } else {
                    failed += 1;
                    println!(
                        "FAIL {id}/{file}: {}: {}",
                        result.name,
                        result.message.as_deref().unwrap_or("no message")
                    );
                }
            }
        }
    }

    if files == 0 {
        println!("no tests/*.lua in {}", dir.display());
    }
    println!("{passed} passed, {failed} failed in {files} file(s)");
    Ok(failed == 0)
}

/// `<mods dir>/../screens/*.screen.ron`, parsed.
///
/// A mod's test opens a screen by kind, and a screen the *game* owns is not
/// in any mod. An example keeps those beside its `mods/`, so that is where
/// this looks; a directory that has none simply contributes nothing.
fn host_screens(mods_dir: &Path) -> Vec<ScreenDef> {
    let Some(dir) = mods_dir.parent().map(|p| p.join("screens")) else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut paths: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.to_string_lossy().ends_with(".screen.ron"))
        .collect();
    paths.sort();
    let mut out = Vec::new();
    for path in paths {
        match std::fs::read_to_string(&path)
            .map_err(|e| e.to_string())
            .and_then(|text| ScreenDef::from_ron(&text).map_err(|e| e.to_string()))
        {
            Ok(def) => out.push(def),
            Err(error) => eprintln!("warning: {}: {error}", path.display()),
        }
    }
    out
}
