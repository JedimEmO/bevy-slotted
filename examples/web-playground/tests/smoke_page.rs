//! What `web/smoke.html` believes about the bundle, checked against the Rust.
//!
//! The browser page asserts the raw exports by hand, so it carries its own
//! copy of what `list_mods()` should answer. A copy drifts: adding
//! `hud_clock` to `examples/modded/mods/` left the page asserting three mods
//! against a bundle of four, and nothing failed until `just smoke` ran in a
//! browser -- the slowest signal this repository has.
//!
//! These tests read the page's declarations out of the file and compare them
//! with [`bundle`], so the drift is a `cargo test` failure that names the mod.
//! They deliberately parse the source rather than run it: the alternative is a
//! JavaScript engine in the test harness for two array literals.

use web_playground::bundle;

/// `web/smoke.html`, beside this crate's manifest.
fn smoke_html() -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("web/smoke.html");
    std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

/// The single-quoted strings of the page's `const <name> = [...]` array.
fn declared_list(source: &str, name: &str) -> Vec<String> {
    let needle = format!("const {name} = [");
    let start = source
        .find(&needle)
        .unwrap_or_else(|| panic!("smoke.html no longer declares `{name}`"))
        + needle.len();
    let body = &source[start..];
    let end = body
        .find(']')
        .unwrap_or_else(|| panic!("`{name}` in smoke.html is not closed on one line"));
    body[..end]
        .split(',')
        .map(|item| item.trim().trim_matches('\'').to_owned())
        .filter(|item| !item.is_empty())
        .collect()
}

/// The integer of the page's `const <name> = <n>;`.
fn declared_number(source: &str, name: &str) -> usize {
    let needle = format!("const {name} = ");
    let start = source
        .find(&needle)
        .unwrap_or_else(|| panic!("smoke.html no longer declares `{name}`"))
        + needle.len();
    let body = &source[start..];
    let end = body
        .find(';')
        .unwrap_or_else(|| panic!("`{name}` in smoke.html has no `;`"));
    body[..end]
        .trim()
        .parse()
        .unwrap_or_else(|error| panic!("`{name}` in smoke.html is not a number: {error}"))
}

/// The regression: a mod added to the bundle and not to the page.
#[test]
fn the_smoke_page_expects_the_mods_the_bundle_actually_carries() {
    let declared = declared_list(&smoke_html(), "EXPECTED_MODS");
    assert_eq!(
        declared,
        bundle::script_mod_ids(),
        "web/smoke.html asserts a different mod list than `bundle::script_mod_ids()`; \
         update `EXPECTED_MODS` in the page"
    );
}

/// The page's second clause: every mod it lists opens two editor tabs.
#[test]
fn the_smoke_page_expects_the_number_of_editor_tabs_each_mod_opens() {
    let declared = declared_number(&smoke_html(), "EXPECTED_FILES");
    for id in bundle::script_mod_ids() {
        assert_eq!(
            bundle::script_files(&id).len(),
            declared,
            "`{id}` opens a different number of editor tabs than web/smoke.html expects"
        );
    }
}

/// The page parses `list_mods()` with `JSON.parse` and reads `.id` and
/// `.files`, so those two field names are load-bearing across the boundary.
#[test]
fn list_mods_answers_the_shape_the_smoke_page_reads() {
    let json = bundle::mods_json();
    for id in bundle::script_mod_ids() {
        assert!(
            json.contains(&format!("\"id\":\"{id}\"")),
            "`{id}` is missing from `mods_json()`"
        );
    }
    assert!(
        json.contains("\"files\":["),
        "`mods_json()` lost its `files`"
    );
}
