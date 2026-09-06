//! Phase 3 skeleton smoke test: the browser plugin builds headless through the
//! facade, indexes the fixture registries off the main thread, files the three
//! fixture recipes under default categories, and attaches no panel unless a
//! screen handler is registered. Package B replaces this with `tests/browser.rs`
//! driven through `h.browser()`.
#![allow(clippy::unwrap_used)]

use std::sync::Arc;

use slotted_browser::{
    BrowserRuntime, CategoryId, DefaultScreenHandler, IndexState, Ingredient, RecipeStore,
    ScreenHandlers, SearchChanged,
};
use slotted_test::prelude::*;
use slotted_ui::{Layout, ScreenDef, Screens, UiNodeDef};

const CHEST: &str = "demo:chest";

fn bare_chest() -> ScreenDef {
    ScreenDef {
        kind: ScreenKind::new(CHEST),
        inherits: None,
        remove: vec![],
        root: UiNodeDef::Panel {
            role: slotted_theme::roles::PANEL,
            layout: Layout::default(),
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
        .theme("glass")
        .build();
    h.world_mut()
        .resource_mut::<Screens>()
        .register(bare_chest());
    h
}

fn wait_for_index(h: &mut UiHarness) -> Arc<slotted_browser::BrowserIndex> {
    for _ in 0..600 {
        if let Some(index) = h.world().resource::<IndexState>().ready() {
            return index.clone();
        }
        h.step(1);
    }
    panic!("the browser index never became Ready");
}

#[test]
fn the_index_lands_with_every_fixture_item() {
    let mut h = harness();
    let index = wait_for_index(&mut h);
    let registries = TestRegistries::basic();
    // Eleven items plus the tags, all without subtypes.
    assert!(index.len() >= registries.items.len());
    let coal = Ingredient::item(TestRegistries::item("minecraft:coal"));
    let id = index.id_of(&coal).expect("coal is indexed");
    assert_eq!(index.get(id).unwrap().display, "Coal");
    assert_eq!(index.get(id).unwrap().mod_ns, "minecraft");
    assert!(
        index
            .get(id)
            .unwrap()
            .categories
            .contains(&"demo:smelting".to_owned())
    );
}

#[test]
fn default_categories_file_the_three_recipes() {
    let mut h = harness();
    wait_for_index(&mut h);
    let store = h.world().resource::<RecipeStore>();
    assert_eq!(store.len(), 3);
    assert_eq!(
        store.in_category(&CategoryId::new("demo:crafting")).len(),
        2
    );
    assert_eq!(
        store.in_category(&CategoryId::new("demo:smelting")).len(),
        1
    );
}

#[test]
fn a_search_narrows_the_visible_entries() {
    let mut h = harness();
    wait_for_index(&mut h);
    h.step(1);
    let all = h.world().resource::<BrowserRuntime>().visible.len();
    assert!(all > 0, "an empty query shows everything");
    h.world_mut().write_message(SearchChanged {
        text: "ingot".to_owned(),
    });
    h.step(1);
    let runtime = h.world().resource::<BrowserRuntime>();
    assert_eq!(runtime.filter_text, "ingot");
    assert_eq!(
        runtime.visible.len(),
        3,
        "iron ingot, gold ingot and the #c:ingots tag entry"
    );
}

#[test]
fn no_panel_attaches_without_a_screen_handler() {
    let mut h = harness();
    h.open_screen(ScreenKind::new(CHEST), ChestFixture::empty());
    h.settle();
    assert!(h.try_find(&by::role(SemanticRole::Browser)).is_none());
}

#[test]
fn a_registered_screen_handler_attaches_a_panel() {
    let mut h = harness();
    h.world_mut()
        .resource_mut::<ScreenHandlers>()
        .register(ScreenKind::new(CHEST), Arc::new(DefaultScreenHandler));
    h.open_screen(ScreenKind::new(CHEST), ChestFixture::empty());
    h.settle();
    let panel = h.find(&by::role(SemanticRole::Browser));
    assert!(h.try_find(&by::test_id("browser.search")).is_some());
    assert!(h.try_find(&by::test_id("browser.cards")).is_some());
    let _ = panel;
}
