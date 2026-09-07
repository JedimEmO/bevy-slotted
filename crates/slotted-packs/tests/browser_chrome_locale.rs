//! The browser's chrome keys, resolved through the real Fluent loader.
//!
//! `slotted-browser` cannot host this test: the loader lives here, and this
//! crate depends on the browser, so a test over there would close the
//! dependency cycle. The panel-level assertions are in
//! `slotted-browser/tests/locale.rs`, over a fake `Localizer`; what is left to
//! prove is that the keys the panel asks for are ones a `.ftl` file can
//! actually define — dots in the `LocKey`, dashes in the file.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use slotted_browser::ui::panel::keys;
use slotted_packs::Locales;
use slotted_packs::locale::LocaleTable;
use slotted_ui::LocKey;

const FTL: &str = "\
browser-search-placeholder = Gegenstände suchen
browser-status-hints = R Rezepte / U Verwendung / A Lesezeichen
browser-status-count = { $count ->
    [one] Ein Gegenstand
   *[other] { $count } Gegenstände
}
browser-uses-title = Verwendet in:
category-demo-crafting = Werkbank
";

#[test]
fn a_locale_file_written_at_test_time_renames_the_browsers_chrome() {
    let dir = std::env::temp_dir().join(format!(
        "slotted-browser-chrome-locale-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("en-US.ftl");
    std::fs::write(&path, FTL).unwrap();

    // The same two steps `load_locales` takes for `locale/<lang>.ftl`.
    let mut table = LocaleTable::default();
    table
        .push_layer(None, std::fs::read_to_string(&path).unwrap())
        .expect("the temp file is valid Fluent");
    let loc = Locales::new(table).port();

    for (key, want) in [
        (keys::SEARCH_PLACEHOLDER, "Gegenstände suchen"),
        (
            keys::STATUS_HINTS,
            "R Rezepte / U Verwendung / A Lesezeichen",
        ),
        (keys::USES_TITLE, "Verwendet in:"),
    ] {
        assert_eq!(
            loc.resolve(&LocKey(key.to_owned())).as_deref(),
            Some(want),
            "{key} is a key a `.ftl` file can define"
        );
    }
    // The count is one key with a `count` argument, so the file owns the
    // plural rule (menus M1 contract 2.3).
    let count = |n: i64| {
        let mut args = slotted_ui::LocArgs::new();
        args.insert("count".to_owned(), slotted_ui::Value::Int(n));
        loc.resolve_with(&LocKey(keys::STATUS_COUNT.to_owned()), &args)
    };
    assert_eq!(count(1).as_deref(), Some("Ein Gegenstand"));
    assert_eq!(count(7).as_deref(), Some("7 Gegenstände"));
    // The category chip's key, invented when a recipe type declares none.
    assert_eq!(
        loc.resolve(&LocKey("category.demo.crafting".to_owned()))
            .as_deref(),
        Some("Werkbank")
    );
    // Anything the file leaves out resolves to nothing, so the panel falls
    // back to its English literal rather than drawing the key.
    assert_eq!(loc.resolve(&LocKey(keys::STATUS_INDEXING.to_owned())), None);

    std::fs::remove_dir_all(&dir).ok();
}

/// The base pack ships the browser's English, so a game that loads
/// `assets/locale/en-US.ftl` sees exactly what an unlocalised panel draws.
#[test]
fn the_base_locale_file_defines_the_browsers_english() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/locale/en-US.ftl");
    let mut table = LocaleTable::default();
    table
        .push_layer(None, std::fs::read_to_string(path).unwrap())
        .expect("the base locale file is valid Fluent");
    let loc = Locales::new(table).port();
    for (key, want) in [
        (keys::SEARCH_PLACEHOLDER, "Search items"),
        (keys::STATUS_HINTS, "R recipes / U uses / A bookmark"),
        (keys::STATUS_INDEXING, "indexing…"),
        (keys::USES_TITLE, "Used in:"),
        (keys::USES_EMPTY, "Used in: nothing yet"),
    ] {
        assert_eq!(
            loc.resolve(&LocKey(key.to_owned())).as_deref(),
            Some(want),
            "{key} is missing from assets/locale/en-US.ftl"
        );
    }
    for (n, want) in [(1, "1 item"), (0, "0 items"), (42, "42 items")] {
        let mut args = slotted_ui::LocArgs::new();
        args.insert("count".to_owned(), slotted_ui::Value::Int(n));
        assert_eq!(
            loc.resolve_with(&LocKey(keys::STATUS_COUNT.to_owned()), &args)
                .as_deref(),
            Some(want)
        );
    }
}
