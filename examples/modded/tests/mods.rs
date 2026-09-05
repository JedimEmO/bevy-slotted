//! Phase 4 verification: three mods add an item, a recipe, a screen, a
//! tooltip part and a button to somebody else's screen without a line of
//! Rust, and a control script reloads without losing inventory state.
//!
//! Every test here drives the same `mods/` directory the windowed example
//! loads. The harness stages a copy of it, so the reload tests rewrite a
//! script the test owns rather than the one in the repository.

use std::sync::Arc;

use bevy::prelude::Entity;

use slotted::ui::{TooltipTier, UiNodeDef};
use slotted_model::{Inventory, ItemStack};
use slotted_script::ModId;
use slotted_test::prelude::*;

/// The chest item `copper_chest/data.lua` registers.
const CHEST_ITEM: &str = "copper_chest:copper_chest";
/// The food item `appleskin_like/data.lua` registers.
const APPLE: &str = "demo:apple";
/// The tooltip key the static part adds for anything tagged `c:foods`.
const FOOD_KEY: &str = "appleskin_like.food";
/// The `test_id` of the button the `sorter` mod injects.
const SORT_BUTTON: &str = "sorter_sort";

/// A harness with the example's mods staged and loaded.
fn loaded() -> UiHarness {
    let mut h = UiHarness::builder()
        .plugins(SlottedPlugins::headless())
        // For the browser handler the example registers for its screen; its
        // `Startup` opener no-ops because the mods load after `build`.
        .plugins(modded::ModdedDemoPlugin)
        .theme("glass")
        .mods_dir(modded::mods_dir())
        .build();
    let layout = h.mod_layout();
    h.load_mods(layout);
    h.settle();
    h
}

/// The example's menu and its seeded inventories, built against whatever the
/// mods just registered.
fn fixture(h: &UiHarness) -> (Arc<slotted_model::MenuDef>, Vec<Inventory>) {
    let registries = h.world().resource::<Registries>().0.clone();
    (modded::menu_def(), modded::inventories(&registries))
}

/// Opens the mod's screen over the seeded inventories.
fn open(h: &mut UiHarness) -> Opened {
    let fixture = fixture(h);
    let opened = h.open_mod_screen(modded::CHEST_SCREEN, fixture);
    h.settle();
    opened
}

/// The container slot holding `item`. Several inventories are seeded with the
/// same items, so a locator that names only the item is ambiguous.
fn chest_slot_with(h: &UiHarness, item: &str) -> Entity {
    h.find(
        &by::role(SemanticRole::Slot)
            .tag("region", "chest")
            .with_item(item)
            .index(0),
    )
}

/// The first container slot with nothing in it. Clicking one produces a
/// script log line without moving an item, which is what a reload test needs.
fn empty_chest_slot(h: &UiHarness) -> Entity {
    h.find_all(&by::role(SemanticRole::Slot).tag("region", "chest"))
        .into_iter()
        .find(|slot| h.stack_at(*slot).is_none())
        .expect("the seeded container has gaps")
}

/// Every string a card's text children actually draw. The label nodes carry no
/// semantic role of their own, so this reads the card's children directly.
fn card_text(h: &UiHarness, card: Entity) -> Vec<String> {
    h.world()
        .get::<bevy::prelude::Children>(card)
        .into_iter()
        .flatten()
        .filter_map(|child| h.text_of(*child))
        .filter(|text| !text.is_empty())
        .collect()
}

/// Which container slots hold something, in index order.
fn occupied(h: &UiHarness, opened: &Opened) -> Vec<usize> {
    let inventory = h
        .world()
        .get::<slotted::ecs::menu::Inventory>(opened.inventories[0])
        .expect("the container inventory entity");
    (0..inventory.len())
        .filter(|i| inventory.get(*i).is_some())
        .collect()
}

#[test]
fn mod_item_appears_in_a_slot() {
    let mut h = loaded();
    let opened = open(&mut h);

    // The item exists only because `data.lua` registered it, and the stack
    // exists only because the frozen registries interned that name.
    let slot = chest_slot_with(&h, CHEST_ITEM);
    let stack = h.stack_at(slot).expect("the slot holds a copper chest");
    assert_eq!(stack.count, 4);

    // The mod's screen, not the demo's: its title anchor is what the sorter
    // injects into.
    assert!(h.try_find(&by::anchor("title_end")).is_some());
    assert_eq!(
        h.world()
            .get::<slotted::ui::ScreenRoot>(opened.screen)
            .map(|root| root.kind.0.to_string()),
        Some(modded::CHEST_SCREEN.to_owned())
    );
    h.assert_conserved();
}

#[test]
fn mod_recipe_is_listed_in_the_browser() {
    let mut h = loaded();
    open(&mut h);

    h.browser().wait_for_index();
    h.browser().search("copper");
    let cards = h.browser().visible_cards();
    assert!(
        cards.iter().any(|card| card.contains(CHEST_ITEM)),
        "the browser did not list the mod's chest; cards: {cards:?}"
    );

    // Its recipe is a mod recipe of a mod recipe type, so opening the page
    // proves the whole chain interned.
    // The search field still holds the keyboard after typing, and the recipe
    // hotkey is a plain letter.
    h.browser().blur_search();
    let card = h.find(&by::role(SemanticRole::Card).tag("entry", CHEST_ITEM));
    h.browser().open_recipes(card);
    let page = h
        .browser()
        .open_page()
        .expect("a recipe page for the chest");
    assert_eq!(page.category.0.to_string(), "copper_chest:assembly");
}

#[test]
fn a_card_shows_the_name_from_the_mods_ftl() {
    let mut h = loaded();
    open(&mut h);

    h.browser().wait_for_index();
    h.browser().search("copper");

    // `data.lua` gives the item `display_name = "copper_chest.item.copper_chest"`,
    // a Fluent key, and `locale/en-US.ftl` defines it as "Copper Chest". The
    // browser reaches the mod's bundle through `slotted_ui::Localization`,
    // which `slotted-packs` fills after each freeze; without the port the card
    // drew the key itself.
    let card = h.find(&by::role(SemanticRole::Card).tag("entry", CHEST_ITEM));
    let drawn = card_text(&h, card);
    assert!(
        drawn.iter().any(|text| text == "Copper Chest"),
        "the card did not resolve the mod's locale key; it drew {drawn:?}"
    );
    assert!(
        !drawn
            .iter()
            .any(|text| text.contains("copper_chest.item.copper_chest")),
        "the card still shows the raw key; it drew {drawn:?}"
    );

    // The search index reads the same resolved name through the same port, so
    // the localised words find the item too.
    h.browser().search("Copper Chest");
    assert!(
        h.browser().visible_cards().contains(&CHEST_ITEM.to_owned()),
        "searching the localised name found nothing"
    );
}

#[test]
fn the_shared_namespace_is_not_warned_about() {
    let h = loaded();

    // `c` is the Fabric-style common-tag namespace: `copper_chest` adds to
    // `c:ingots` and `appleskin_like` declares `c:foods`, which is what the
    // convention is for. `PacksConfig::shared_namespaces` lists it, so neither
    // is worth a line in the console.
    let shared: Vec<_> = h
        .script_logs_containing("is not a declared dependency")
        .into_iter()
        .filter(|entry| entry.message.contains("`c:"))
        .collect();
    assert!(
        shared.is_empty(),
        "the shared `c` namespace was warned about: {shared:?}"
    );

    // Reaching into a namespace that is neither shared nor a dependency still
    // is: `appleskin_like` registers `demo:apple` to give its tooltip part a
    // target, which is exactly the case the warning exists for.
    assert!(
        h.script_logs_containing("`demo:apple` is outside")
            .iter()
            .any(|entry| entry.message.contains("not a declared dependency")),
        "reaching into another mod's namespace stopped warning"
    );
}

#[test]
fn tooltip_part_renders_for_foods() {
    let mut h = loaded();
    open(&mut h);

    let apple = chest_slot_with(&h, APPLE);
    h.request_tooltip(apple, TooltipTier::Compact);
    h.settle();

    let tooltip = h.tooltip().expect("a tooltip over the apple");
    let keys: Vec<String> = tooltip
        .parts
        .iter()
        .filter_map(|part| match part {
            UiNodeDef::Text { key, .. } => Some(key.0.clone()),
            _ => None,
        })
        .collect();
    // The static part from `data.lua`...
    assert!(
        keys.iter().any(|k| k == FOOD_KEY),
        "the static food line is missing; keys: {keys:?}"
    );
    // ...and the per-item line `control.lua` returns for `tooltip_build`.
    assert!(
        keys.iter().any(|k| k == "appleskin_like.food.apple"),
        "the dynamic food line is missing; keys: {keys:?}"
    );

    // A non-food gets neither.
    let chest = chest_slot_with(&h, CHEST_ITEM);
    h.clear_tooltip(apple);
    h.request_tooltip(chest, TooltipTier::Compact);
    h.settle();
    let chest_tooltip = h.tooltip().expect("a tooltip over the chest");
    assert!(
        !chest_tooltip.parts.iter().any(|part| matches!(
            part,
            UiNodeDef::Text { key, .. } if key.0 == FOOD_KEY
        )),
        "the food line leaked onto an item that is not tagged c:foods"
    );
}

#[test]
fn injected_button_appears_at_the_anchor_and_excludes() {
    let mut h = loaded();
    let opened = open(&mut h);

    let anchor = h.find(&by::anchor("title_end"));
    let button = h.find(&by::test_id(SORT_BUTTON).within(anchor));

    // `exclusion = true` in the injection means the browser has to dock
    // clear of it.
    let rect = h.rect_of(button);
    let zones = h.exclusion_zones(opened.screen);
    assert!(
        zones.iter().any(|zone| zone.contains(rect.center())),
        "the injected button published no exclusion zone; zones: {zones:?}"
    );
}

#[test]
fn activating_the_injected_button_sorts_the_chest() {
    let mut h = loaded();
    let opened = open(&mut h);

    let before = occupied(&h, &opened);
    assert!(
        before.iter().any(|i| *i > before.len()),
        "the seeded container should start with gaps; occupied: {before:?}"
    );

    let button = h.find(&by::test_id(SORT_BUTTON));
    h.activate(button);
    h.settle();

    // The script answered with `sort`, which the host turned into the same
    // `MenuAction` the built-in rail sends: stacks merge and compact.
    let after = occupied(&h, &opened);
    assert_eq!(
        after,
        (0..after.len()).collect::<Vec<_>>(),
        "sorting left gaps; occupied: {after:?}"
    );
    assert!(
        h.script_logs_containing("sorting the container")
            .iter()
            .any(|entry| entry.mod_id.as_ref().map(ModId::as_str) == Some("sorter")),
        "the sorter script logged nothing"
    );
    h.assert_conserved();
}

#[test]
fn a_slot_click_reaches_the_control_script() {
    let mut h = loaded();
    open(&mut h);

    let slot = chest_slot_with(&h, CHEST_ITEM);
    h.click_slot(slot, Button::Left, Modifiers::default());
    h.settle();

    let lines = h.script_logs_containing("clicked");
    assert!(
        lines.iter().any(|entry| entry.message.contains(CHEST_ITEM)),
        "control.lua logged no click; lines: {lines:?}"
    );
    // The click was a pickup, so the stack moved to the cursor rather than
    // vanishing.
    h.assert_conserved();
}

/// A `control.lua` that logs a different string on every slot click.
const RELOADED_CONTROL: &str = r#"
slotted.on("slot_click", function(ev)
    slotted.info("reloaded handler saw slot %d", ev.slot)
end)
"#;

/// A `control.lua` that does not compile.
const BROKEN_CONTROL: &str = r#"
slotted.on("slot_click", function(ev
    slotted.info("never runs")
end)
"#;

#[test]
fn reloading_a_control_script_changes_behaviour_and_keeps_state() {
    let mut h = loaded();
    let opened = open(&mut h);

    // Clicking an empty slot moves nothing, so the only thing that changes
    // across the reload is what the script said.
    let empty = empty_chest_slot(&h);
    h.click_slot(empty, Button::Left, Modifiers::default());
    h.settle();
    assert!(!h.script_logs_containing("clicked").is_empty());

    let before = occupied(&h, &opened);
    let stacks: Vec<Option<ItemStack>> = h
        .find_all(&by::role(SemanticRole::Slot).tag("region", "chest"))
        .into_iter()
        .map(|slot| h.stack_at(slot))
        .collect();

    h.edit_mod_file("copper_chest", "control.lua", RELOADED_CONTROL);
    h.reload_mod("copper_chest").expect("the reload succeeds");
    h.settle();

    // The screen was respawned around the same menu, so every slot entity is
    // new and `register_slot_refs` re-seeded them from inventories nobody
    // wrote to.
    assert_eq!(occupied(&h, &opened), before, "the reload moved stacks");
    let after: Vec<Option<ItemStack>> = h
        .find_all(&by::role(SemanticRole::Slot).tag("region", "chest"))
        .into_iter()
        .map(|slot| h.stack_at(slot))
        .collect();
    assert_eq!(after, stacks, "the reload changed what the slots show");

    let empty = empty_chest_slot(&h);
    h.click_slot(empty, Button::Left, Modifiers::default());
    h.settle();
    assert!(
        !h.script_logs_containing("reloaded handler saw slot")
            .is_empty(),
        "the reloaded script did not run"
    );
    h.assert_conserved();
}

#[test]
fn a_broken_script_is_reported_and_the_old_one_stays() {
    let mut h = loaded();
    let opened = open(&mut h);
    let before = occupied(&h, &opened);

    h.edit_mod_file("copper_chest", "control.lua", BROKEN_CONTROL);
    let _ = h.reload_mod("copper_chest");
    h.settle();

    let errors = h.mod_errors();
    assert!(
        errors
            .iter()
            .any(|error| error.to_string().contains("copper_chest")),
        "a syntax error in control.lua was not reported; errors: {errors:?}"
    );

    // Nothing was lost to the failed reload, and no handler from the broken
    // file is running.
    assert_eq!(occupied(&h, &opened), before);
    let empty = empty_chest_slot(&h);
    h.click_slot(empty, Button::Left, Modifiers::default());
    h.settle();
    assert!(
        h.script_logs_containing("never runs").is_empty(),
        "the broken handler ran"
    );
    h.assert_conserved();
}
