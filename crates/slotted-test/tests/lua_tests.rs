//! The Lua test runner: the protocol, the fixtures and the failure messages.
//! Phase 6 contract section 3.2.
#![cfg(feature = "script")]
#![allow(clippy::unwrap_used)]

use std::path::Path;

use slotted_model::Value;
use slotted_test::LuaFixture;
use slotted_test::prelude::*;
use slotted_ui::{Layout, ScreenDef, Screens, Tags, UiNodeDef};

/// A chest screen with nothing on it but the container grid: enough for a
/// locator to find a slot, which is all these tests ask of it.
fn chest_screen() -> ScreenDef {
    ScreenDef {
        kind: ScreenKind::new("demo:chest"),
        initial_focus: None,
        presentation: slotted_ui::Presentation::default(),
        inherits: None,
        remove: vec![],
        root: UiNodeDef::Panel {
            role: slotted_theme::roles::PANEL,
            layout: Layout::default(),
            children: vec![UiNodeDef::SlotGrid {
                inventory: slotted_model::InventoryRef::new(0),
                cols: 9,
                rows: 3,
                first: 0,
                tags: Tags::new().with("region", "chest"),
            }],
            tags: Tags::new(),
        },
        listring: Vec::new(),
    }
}

fn harness() -> UiHarness {
    let mut harness = UiHarness::builder()
        .plugins(SlottedPlugins::headless())
        .registries(TestRegistries::basic())
        .resolution(1280.0, 720.0)
        .build();
    harness
        .world_mut()
        .resource_mut::<Screens>()
        .register(chest_screen());
    harness
}

fn run(harness: &mut UiHarness, source: &str) -> slotted_test::LuaTestReport {
    harness.run_lua_tests("demo", Path::new("tests/case.lua"), source)
}

/// The whole loop: a test registers, opens a screen through a fixture alias,
/// reads a slot back and passes.
#[test]
fn a_test_opens_a_screen_and_reads_a_slot() {
    let mut harness = harness();
    harness.register_fixture("chest", ChestFixture::filled());
    let report = run(
        &mut harness,
        r#"
        local t = slotted.test
        t.test("the chest is filled", function()
            t.open_screen("demo:chest", "chest")
            t.expect_stack({ role = "slot", tag = { region = "chest" }, index = 0 },
                "minecraft:cobblestone", 64)
        end)
        "#,
    );
    assert!(
        report.all_passed(),
        "{:?}",
        report.results.first().and_then(|r| r.message.clone())
    );
    assert_eq!(report.results.len(), 1);
    assert!(report.results[0].steps >= 2, "one open and one query");
}

/// The gamepad API (menus M0, package C): `focused()` reads the focus the
/// screen opened with, `gamepad("DPadRight")` moves it, `gamepad("South")`
/// picks up, `action("back")` closes the screen through the stack. A bad
/// button name and a bad action name are the test's failure, not a panic.
#[test]
fn a_test_drives_the_screen_with_a_gamepad_and_reads_focus() {
    let mut harness = harness();
    harness.register_fixture("chest", ChestFixture::filled());
    let report = run(
        &mut harness,
        r#"
        local t = slotted.test
        t.test("the pad walks and picks up", function()
            t.open_screen("demo:chest", "chest")
            t.settle()
            t.expect_eq(t.focused().region, "chest", "focus starts in the chest grid")
            t.gamepad("DPadRight")
            t.settle()
            t.expect_eq(t.focused().region, "chest", "still in the grid")
            t.gamepad("DPadLeft")
            t.gamepad("South")
            t.settle()
            t.expect_stack({ role = "slot", tag = { region = "chest" }, index = 0 }, nil)
            -- Past the double-click window, or the second press collects.
            t.step(20)
            t.gamepad("South")
            t.settle()
            t.expect_stack({ role = "slot", tag = { region = "chest" }, index = 0 },
                "minecraft:cobblestone", 64)
            t.action("back")
            t.settle()
            t.expect_eq(t.focused(), nil, "nothing focused once the screen is gone")
        end)
        t.test("a bad button is a failure with a name", function()
            t.gamepad("Triangle")
        end)
        t.test("a bad action is a failure with a name", function()
            t.action("jump")
        end)
        "#,
    );
    assert_eq!(report.results.len(), 3);
    assert!(
        report.results[0].passed,
        "{:?}",
        report.results[0].message.clone()
    );
    assert_eq!(
        report.results[1].message.as_deref(),
        Some("no gamepad button named \"Triangle\"")
    );
    assert_eq!(
        report.results[2].message.as_deref(),
        Some("no UI action named \"jump\"")
    );
}

/// A failed expectation is that test's message, with no chunk or line in it,
/// and the file's other tests still run.
#[test]
fn a_failed_expectation_names_itself_and_does_not_stop_the_file() {
    let mut harness = harness();
    harness.register_fixture("chest", ChestFixture::empty());
    let report = run(
        &mut harness,
        r#"
        local t = slotted.test
        t.test("this one fails", function()
            t.open_screen("demo:chest", "chest")
            t.expect(false, "the chest is not empty")
        end)
        t.test("this one passes", function()
            t.expect_eq(2, 2)
        end)
        "#,
    );
    assert_eq!(report.results.len(), 2);
    assert!(!report.results[0].passed);
    assert_eq!(
        report.results[0].message.as_deref(),
        Some("the chest is not empty")
    );
    assert!(report.results[1].passed);
}

/// A locator that matches nothing comes back as the harness's own message,
/// raised inside the test body.
#[test]
fn a_locator_that_matches_nothing_fails_the_test_with_why() {
    let mut harness = harness();
    let report = run(
        &mut harness,
        r#"
        local t = slotted.test
        t.test("no such node", function()
            t.click({ test_id = "nothing_here" })
        end)
        "#,
    );
    let message = report.results[0].message.clone().unwrap();
    assert!(
        message.contains("no node matches"),
        "unexpected message: {message}"
    );
}

/// The table fixture: sizes, filled slots, and the errors for a malformed one.
#[test]
fn a_fixture_table_types_against_the_registries() {
    let value = ron_value(
        r#"{ "slots": [3, 27, 9], "fill": { "1:0": { "item": "minecraft:coal", "count": 8 } } }"#,
    );
    let fixture = LuaFixture::from_value(&value).unwrap();
    assert_eq!(fixture.slots, vec![3, 27, 9]);
    let inventories = fixture.inventories(&TestRegistries::basic()).unwrap();
    assert_eq!(inventories.len(), 3);
    assert_eq!(inventories[1].get(0).unwrap().count, 8);
    // `{n, 27, 9}` is the standard container shape, so the def has the
    // player's routing rather than three unrelated runs of slots.
    assert_eq!(fixture.def().slots.len(), 3 + 27 + 9);

    let bad = LuaFixture::from_value(&ron_value(r#"{ "slots": [3], "fill": { "oops": {} } }"#));
    assert!(bad.unwrap_err().contains("inventory:slot"));
    let bad = LuaFixture::from_value(&ron_value(r#"{ "fill": {} }"#));
    assert!(bad.unwrap_err().contains("needs `slots`"));
}

/// The shape a Lua table arrives in, written as JSON-ish RON.
fn ron_value(text: &str) -> Value {
    let parsed: ron::Value = ron::from_str(text).expect("the literal parses");
    slotted_registry::to_model(&parsed).expect("a plain table converts")
}

/// `test-mods` on a directory whose mods bundle no `tests/*.lua` reports zero
/// tests and exits 0.
///
/// Nothing to run is not a failure: a workspace that runs `just test-mods` in
/// CI over a directory of content-only mods must stay green. The binary's exit
/// code is `failed == 0`, so this pins the whole path rather than the function
/// behind it: the real binary, a real directory, the real process exit.
#[test]
fn test_mods_on_mods_without_tests_reports_zero_and_exits_zero() {
    let dir = std::env::temp_dir().join(format!(
        "slotted-no-tests-{}",
        uuid::Uuid::new_v4().simple()
    ));
    let quiet = dir.join("mods").join("quiet");
    std::fs::create_dir_all(&quiet).expect("a staging directory");
    std::fs::write(
        quiet.join("mod.toml"),
        "id = \"quiet\"\nname = \"Quiet\"\nversion = \"1.0.0\"\napi_version = 1\n",
    )
    .expect("a manifest");
    // A mod with content and no `tests/` directory at all.
    std::fs::create_dir_all(quiet.join("data")).expect("a data directory");

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_test-mods"))
        .arg(dir.join("mods"))
        .output()
        .expect("the test-mods binary runs");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "nothing to run is not a failure\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(
        stdout.contains("0 passed, 0 failed in 0 file(s)"),
        "the summary counts zero of everything: {stdout}"
    );
    assert!(
        stdout.contains("no tests/*.lua"),
        "and says why there was nothing to run: {stdout}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}
