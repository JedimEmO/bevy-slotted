//! The browser panel's own chrome, localised.
//!
//! The item names on cards already resolve through `slotted_ui::Localization`;
//! these tests cover the strings the panel writes itself — the search
//! placeholder, the footer, the category chips, the recipe tabs and the pill
//! buttons — and the two properties that matter for them: a catalogue renames
//! them, and no catalogue leaves them in the English they have always been.
//!
//! The catalogue here is a fake [`Localizer`] rather than a real `.ftl`,
//! because the Fluent loader lives in `slotted-packs`, which depends on this
//! crate: a test here cannot reach it without a dependency cycle. The
//! temp-`.ftl` half of the story is
//! `slotted-packs/tests/browser_chrome_locale.rs`, which drives
//! `LocaleTable::push_layer` over a file written at test time and asserts the
//! same keys resolve.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::sync::Arc;

use bevy::prelude::*;
use slotted_browser::ui::panel::keys;
use slotted_browser::{Categories, CategoryId, DefaultScreenHandler, ScreenHandlers};
use slotted_registry::defs::RecipeTypeDef;
use slotted_test::prelude::*;
use slotted_ui::{LocKey, Localization, Localizer, SemanticRole};

const CHEST: &str = "demo:chest";

/// A catalogue that answers a fixed table and nothing else, so a key with no
/// row still exercises the fallback.
struct Fake(Vec<(&'static str, &'static str)>);

impl Localizer for Fake {
    /// Substitutes `{ $name }` the way Fluent would, so the footer's count
    /// argument (menus M1 contract 2.3) reaches the string.
    fn resolve(&self, key: &LocKey, args: &slotted_ui::LocArgs) -> Option<String> {
        let (_, text) = self.0.iter().find(|(k, _)| *k == key.0)?;
        let mut out = (*text).to_owned();
        for (name, value) in args {
            let shown = match value {
                slotted_ui::Value::Int(i) => i.to_string(),
                slotted_ui::Value::Text(t) => t.clone(),
                other => format!("{other:?}"),
            };
            out = out.replace(&format!("{{ ${name} }}"), &shown);
        }
        Some(out)
    }
}

fn bare_chest() -> ScreenDef {
    ScreenDef {
        kind: ScreenKind::new(CHEST),
        initial_focus: None,
        presentation: slotted_ui::Presentation::default(),
        inherits: None,
        remove: vec![],
        root: UiNodeDef::Panel {
            role: slotted_theme::roles::PANEL,
            layout: slotted_ui::Layout::default(),
            children: vec![UiNodeDef::SlotGrid {
                inventory: MenuDef::CONTAINER,
                cols: 9,
                rows: 3,
                first: 0,
                tags: slotted_ui::Tags::default(),
            }],
            tags: slotted_ui::Tags::default(),
        },
        listring: vec![],
    }
}

fn harness() -> UiHarness {
    let mut h = UiHarness::builder()
        .plugins(SlottedPlugins::headless())
        .registries(TestRegistries::basic())
        .resolution(1280.0, 720.0)
        .theme("glass")
        .build();
    h.world_mut()
        .resource_mut::<Screens>()
        .register(bare_chest());
    h.world_mut()
        .resource_mut::<ScreenHandlers>()
        .register(ScreenKind::new(CHEST), Arc::new(DefaultScreenHandler));
    h
}

fn open(h: &mut UiHarness) {
    h.open_screen(ScreenKind::new(CHEST), ChestFixture::filled());
    h.browser().wait_for_index();
    h.settle();
}

/// The text a chip or a tab draws, which lives on its child rather than on the
/// node the locator finds.
fn labels(h: &UiHarness, role: SemanticRole) -> Vec<String> {
    h.find_all(&by::role(role))
        .into_iter()
        .filter_map(|node| {
            let world = h.world();
            world.get::<Children>(node).and_then(|children| {
                children
                    .iter()
                    .find_map(|child| world.get::<Text>(child).map(|t| t.0.clone()))
            })
        })
        .collect()
}

fn footer(h: &UiHarness) -> Option<String> {
    h.text_of(h.find(&by::test_id("browser.status")))
}

// ---------------------------------------------------------------------------
// With a catalogue
// ---------------------------------------------------------------------------

#[test]
fn a_renamed_category_chip_shows_the_catalogues_word() {
    let mut h = harness();
    h.world_mut().insert_resource(Localization::new(Fake(vec![(
        "category.demo.crafting",
        "Werkbank",
    )])));
    open(&mut h);
    let chips = labels(&h, SemanticRole::Chip);
    assert!(
        chips.contains(&"Werkbank".to_owned()),
        "the chip draws the catalogue's word, not the category path: {chips:?}"
    );
    assert!(
        chips.contains(&"smelting".to_owned()),
        "a category the catalogue says nothing about keeps its path: {chips:?}"
    );
}

#[test]
fn the_status_line_and_the_hotkey_hint_resolve_through_the_catalogue() {
    let mut h = harness();
    h.world_mut().insert_resource(Localization::new(Fake(vec![
        (keys::STATUS_COUNT, "{ $count } Gegenstände"),
        (
            keys::STATUS_HINTS,
            "R Rezepte / U Verwendung / A Lesezeichen",
        ),
        (keys::SEARCH_PLACEHOLDER, "Gegenstände suchen"),
    ])));
    open(&mut h);

    let count = footer(&h).expect("the footer has a count");
    assert!(
        count.ends_with(" Gegenstände") && count.starts_with(|c: char| c.is_ascii_digit()),
        "the count reaches the catalogue's string as an argument: {count:?}"
    );
    let texts: Vec<String> = h
        .world()
        .iter_entities()
        .filter_map(|e| e.get::<Text>().map(|t| t.0.clone()))
        .collect();
    assert!(
        texts.contains(&"R Rezepte / U Verwendung / A Lesezeichen".to_owned()),
        "the hotkey hint is localised: {texts:?}"
    );
    assert!(
        texts.contains(&"Gegenstände suchen".to_owned()),
        "the search placeholder is localised: {texts:?}"
    );
}

#[test]
fn a_recipe_tab_shows_the_categorys_localised_title() {
    let mut h = harness();
    h.world_mut().insert_resource(Localization::new(Fake(vec![(
        "category.demo.crafting",
        "Werkbank",
    )])));
    open(&mut h);
    let card = h.find(&by::role(SemanticRole::Card).tag("entry", "minecraft:iron_pickaxe"));
    h.browser().open_recipes(card);
    h.settle();
    let tabs = labels(&h, SemanticRole::Tab);
    assert_eq!(
        tabs,
        vec!["Werkbank".to_owned()],
        "the open recipe page's tab draws the same key the chip does"
    );
}

#[test]
fn replacing_the_catalogue_repaints_the_panel() {
    let mut h = harness();
    open(&mut h);
    assert!(
        labels(&h, SemanticRole::Chip).contains(&"crafting".to_owned()),
        "the panel starts in English"
    );
    // A language switch at runtime: `slotted-packs` replaces the port rather
    // than mutating it, and every chrome string re-resolves off the change.
    h.world_mut().insert_resource(Localization::new(Fake(vec![
        ("category.demo.crafting", "Werkbank"),
        (keys::STATUS_HINTS, "R Rezepte"),
    ])));
    h.settle();
    let chips = labels(&h, SemanticRole::Chip);
    assert!(
        chips.contains(&"Werkbank".to_owned()),
        "the chip repainted without being respawned: {chips:?}"
    );
    let texts: Vec<String> = h
        .world()
        .iter_entities()
        .filter_map(|e| e.get::<Text>().map(|t| t.0.clone()))
        .collect();
    assert!(
        texts.contains(&"R Rezepte".to_owned()),
        "the footer hint repainted too: {texts:?}"
    );
}

// ---------------------------------------------------------------------------
// Without one
// ---------------------------------------------------------------------------

#[test]
fn with_no_catalogue_every_chrome_string_is_the_english_it_always_was() {
    let mut h = harness();
    open(&mut h);
    let texts: Vec<String> = h
        .world()
        .iter_entities()
        .filter_map(|e| e.get::<Text>().map(|t| t.0.clone()))
        .collect();
    for expected in [
        "Search items",
        "R recipes / U uses / A bookmark",
        "+",
        "<",
        ">",
    ] {
        assert!(
            texts.contains(&expected.to_owned()),
            "{expected:?} falls back to the literal, not to its key: {texts:?}"
        );
    }
    assert!(
        labels(&h, SemanticRole::Chip).contains(&"crafting".to_owned()),
        "a chip falls back to the category path"
    );
    let count = footer(&h).expect("the footer has a count");
    assert!(
        count.ends_with(" items") || count == "1 item",
        "the count reads as it always did: {count:?}"
    );
}

// ---------------------------------------------------------------------------
// The declared key
// ---------------------------------------------------------------------------

#[test]
fn a_recipe_types_declared_title_key_reaches_its_category() {
    let mut registries = slotted_registry::registry::Registries::new();
    let mut declared = RecipeTypeDef::new(common::id("beta:assembly"));
    declared.title_key = Some("beta.recipe_type.assembly".to_owned());
    registries
        .add_recipe_type(declared)
        .expect("beta:assembly is fresh");
    registries
        .add_recipe_type(RecipeTypeDef::new(common::id("beta:silent")))
        .expect("beta:silent is fresh");
    let (frozen, _) = registries.freeze().expect("the fixture freezes");

    let mut categories = Categories::default();
    categories.bind_defaults(&frozen);

    let declared = categories
        .get(&CategoryId::new("beta:assembly"))
        .expect("a default category was bound");
    assert_eq!(
        declared.title_key(),
        LocKey("beta.recipe_type.assembly".to_owned()),
        "the mod's own key survives the trip into the category"
    );
    let silent = categories
        .get(&CategoryId::new("beta:silent"))
        .expect("a default category was bound");
    assert_eq!(
        silent.title_key(),
        LocKey("category.beta.silent".to_owned()),
        "a recipe type that declares nothing keeps the invented key"
    );
}
