//! Adversarial review of Phase 3: the cases the happy-path suites do not
//! reach.
//!
//! Every test here is a place the contract (`docs/design/phase3-contract.md`)
//! or the two implementation notes leave a sharp edge: a query the player can
//! type but nobody meant to parse, a frame that lands before the index does,
//! a transfer that must refuse without eating the items, a screen that closes
//! while its overlay is still holding entities. Where the current behaviour
//! looks wrong the assertion pins what the code does today and a
//! `SUSPECTED BUG` comment says what it should do instead, so the suite stays
//! green and the finding stays visible.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::collections::BTreeMap;
use std::sync::Arc;

use bevy::prelude::*;
use bevy::tasks::AsyncComputeTaskPool;
use slotted_browser::index::Bitset;
use slotted_browser::{
    BrowserPanel, BrowserRuntime, Card, Categories, CategoryId, DefaultScreenHandler, Field,
    GiveRequested, HiddenEntries, IndexState, Ingredient, IngredientCtx, IngredientTypes,
    PrefixMode, RecipeStore, ScreenHandlers, SearchConfig, SimpleTransfer, Subtypes, Token,
    TransferCtx, TransferErrorKind, TransferHandler, evaluate, tokenize,
};
use slotted_model::{Actor, GiveTarget, ItemId, MenuState};
use slotted_test::prelude::*;
use slotted_ui::{
    ExclusionZone, Layout, ScreenDef, Screens, Side, TooltipHost, TooltipTier, UiNodeDef,
};

const CHEST: &str = "demo:chest";
const WIDTH: f32 = 1280.0;
const HEIGHT: f32 = 720.0;

// ---------------------------------------------------------------------------
// Shared setup, mirroring `tests/panel.rs`.
// ---------------------------------------------------------------------------

fn bare_chest() -> ScreenDef {
    ScreenDef {
        kind: ScreenKind::new(CHEST),
        inherits: None,
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

/// A harness with the chest screen registered and a handler for it, so the
/// panel attaches. `theme` is `None` for the no-theme case.
fn harness_with(theme: Option<&str>, width: f32, height: f32) -> UiHarness {
    let mut builder = UiHarness::builder()
        .plugins(SlottedPlugins::headless())
        .registries(TestRegistries::basic())
        .resolution(width, height);
    if let Some(theme) = theme {
        builder = builder.theme(theme);
    }
    let mut h = builder.build();
    h.world_mut()
        .resource_mut::<Screens>()
        .register(bare_chest());
    h.world_mut()
        .resource_mut::<ScreenHandlers>()
        .register(ScreenKind::new(CHEST), Arc::new(DefaultScreenHandler));
    h
}

fn harness() -> UiHarness {
    harness_with(Some("glass"), WIDTH, HEIGHT)
}

/// Opens the chest and waits for the index.
fn open(h: &mut UiHarness) -> Opened {
    let opened = h.open_screen(ScreenKind::new(CHEST), ChestFixture::filled());
    h.browser().wait_for_index();
    h.settle();
    opened
}

/// The card for one item, with the search field blurred again so the hotkeys
/// fire. The same helper `tests/panel.rs` uses.
fn card_for(h: &mut UiHarness, item: &str) -> Entity {
    let words = item.split(':').next_back().unwrap().replace('_', " ");
    h.browser().search(&words);
    h.browser().blur_search();
    h.find(&by::role(SemanticRole::Card).tag("entry", item))
}

/// How many entities carry `C`.
fn count_of<C: Component>(h: &UiHarness) -> usize {
    let world = h.world();
    world
        .try_query::<(Entity, &C)>()
        .map_or(0, |mut q| q.iter(world).count())
}

/// A full-height rail down the window, the way an injected toolbar sits.
fn rail(h: &mut UiHarness, screen: Entity, left: f32, width: f32) -> Entity {
    let zone = h
        .world_mut()
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: bevy::ui::px(left),
                top: bevy::ui::px(0.0),
                width: bevy::ui::px(width),
                height: bevy::ui::px(HEIGHT),
                ..default()
            },
            ExclusionZone,
            ChildOf(screen),
        ))
        .id();
    h.settle();
    zone
}

fn texts(tokens: &[Token]) -> Vec<&str> {
    tokens.iter().map(|t| t.text.as_str()).collect()
}

// ---------------------------------------------------------------------------
// 1. The tokenizer, given input no one meant to write.
// ---------------------------------------------------------------------------

/// Defends the tokenizer against a quote the player never closed: a query is
/// typed one character at a time, so every prefix of `"iron ingot"` reaches
/// `tokenize` and none of them may be lost or panic.
#[test]
fn an_unclosed_quote_swallows_the_rest_of_the_query_without_losing_it() {
    let cfg = SearchConfig::default();

    let tokens = tokenize("\"iron", &cfg);
    assert_eq!(texts(&tokens), ["iron"], "the open quote is not text");
    assert_eq!(tokens[0].field, Field::Name);

    // A quote in the middle closes nothing and is simply dropped, so the two
    // halves fuse into one term rather than splitting.
    let tokens = tokenize("iron\"ore", &cfg);
    assert_eq!(texts(&tokens), ["ironore"]);

    // Everything after an open quote is one term, spaces included.
    let tokens = tokenize("\"iron ingot", &cfg);
    assert_eq!(texts(&tokens), ["iron ingot"]);

    // An open quote makes the leading `-` literal, exactly as a closed one
    // does, so a half-typed `"-iron"` never flips to a negation mid-keystroke.
    let tokens = tokenize("\"-iron", &cfg);
    assert!(!tokens[0].negate);
    assert_eq!(texts(&tokens), ["-iron"]);
}

/// Defends the tokenizer against a backslash with nothing to escape, which is
/// what every query containing one looks like halfway through being typed.
#[test]
fn a_backslash_at_the_end_of_the_query_is_dropped() {
    let cfg = SearchConfig::default();
    assert!(tokenize("\\", &cfg).is_empty(), "a lone `\\` is no term");
    assert_eq!(texts(&tokenize("iron \\", &cfg)), ["iron"]);
    assert_eq!(
        texts(&tokenize("iron\\", &cfg)),
        ["iron"],
        "the dangling escape does not extend the term"
    );
    // The same, inside a quoted phrase.
    assert_eq!(texts(&tokenize("\"iron ingot\\", &cfg)), ["iron ingot"]);
}

/// Defends the escape rule: `\` makes the next character text, whatever that
/// character means to the grammar. A player looking for a literal `"` or `#`
/// has no other way to ask.
#[test]
fn a_backslash_escapes_a_quote_and_a_prefix_character() {
    let cfg = SearchConfig::default();

    let tokens = tokenize(r#"\"quoted\""#, &cfg);
    assert_eq!(texts(&tokens), [r#""quoted""#]);
    assert_eq!(tokens[0].field, Field::Name, "the quotes are content");

    let tokens = tokenize(r"\#c:tools \@demo \%demo:crafting", &cfg);
    assert_eq!(texts(&tokens), ["#c:tools", "@demo", "%demo:crafting"]);
    assert!(
        tokens.iter().all(|t| t.field == Field::Name),
        "an escaped prefix selects no field"
    );

    // An escaped `-` is text, and an escaped `|` does not open an OR group.
    let tokens = tokenize(r"\-iron a\|b", &cfg);
    assert!(!tokens[0].negate);
    assert_eq!(texts(&tokens), ["-iron", "a|b"]);
    assert_eq!(tokens[1].or_group, 1, "the escaped pipe kept the AND split");
}

/// Defends the interaction of `-` with a `Disabled` prefix. A disabled prefix
/// is literal text, so `-&id` must negate a *name* term whose text still
/// carries the `&`; reading the prefix anyway would silently search a field
/// the config switched off.
#[test]
fn negating_a_disabled_prefix_keeps_the_prefix_character_in_the_text() {
    let cfg = SearchConfig::default();
    assert_eq!(cfg.mode(Field::Id), PrefixMode::Disabled);

    let tokens = tokenize("-&minecraft:coal", &cfg);
    assert!(tokens[0].negate);
    assert_eq!(tokens[0].field, Field::Name);
    assert_eq!(tokens[0].text, "&minecraft:coal");

    // Any field can be switched off, and `-` behaves the same way for it.
    let mut cfg = SearchConfig::default();
    cfg.modes.insert(Field::Mod, PrefixMode::Disabled);
    let tokens = tokenize("-@demo", &cfg);
    assert!(tokens[0].negate);
    assert_eq!(tokens[0].field, Field::Name);
    assert_eq!(tokens[0].text, "@demo");

    // And the negation is harmless: no display name holds a `&`, so the term
    // subtracts nothing and the result is still everything.
    let index = common::index();
    let cfg = SearchConfig::default();
    let tokens = tokenize("-&minecraft:coal", &cfg);
    let set = evaluate(&index, &tokens, &cfg, &Bitset::new(index.len()));
    assert_eq!(
        set.count(),
        index.len(),
        "a negated term that matches nothing removes nothing"
    );
}

// ---------------------------------------------------------------------------
// 2. A frame that lands before the index does.
// ---------------------------------------------------------------------------

/// Defends the panel against being driven while `IndexState` is still
/// `Building`: the screen can open, the player can type and page, and none of
/// it may panic or bind a card to an entry that does not exist yet.
#[test]
fn the_panel_survives_every_frame_while_the_index_is_still_building() {
    let mut h = harness();
    // A build that never lands, so the whole test runs in `Building`.
    let task = AsyncComputeTaskPool::get()
        .spawn(async { std::future::pending::<slotted_browser::BrowserIndex>().await });
    h.world_mut().insert_resource(IndexState::Building(task));

    let opened = h.open_screen(ScreenKind::new(CHEST), ChestFixture::filled());
    h.settle();
    assert!(
        h.world().resource::<IndexState>().is_building(),
        "the test is only meaningful while the build is in flight"
    );
    assert!(h.browser().is_attached(opened.screen), "the panel attached");

    // Nothing to show, and nothing pretending to show something.
    assert!(h.browser().visible_entries().is_empty());
    assert!(h.browser().visible_cards().is_empty());
    assert!(h.world().resource::<BrowserRuntime>().visible.is_empty());
    let cards = h.find_all(&by::role(SemanticRole::Card));
    assert!(!cards.is_empty(), "the pool is spawned before the index");
    for card in cards {
        assert!(!h.is_visible(card), "an unbound card stays hidden");
        assert_eq!(
            h.world()
                .get::<slotted_ui::Tags>(card)
                .and_then(|t| t.get("entry")),
            None,
            "an unbound card claims no entry"
        );
    }

    // Typing and paging while the index builds is ordinary input, not a crash.
    h.browser().search("iron");
    h.key(KeyCode::PageDown);
    h.settle();
    assert_eq!(h.world().resource::<BrowserRuntime>().filter_text, "iron");
    assert!(h.browser().visible_entries().is_empty());

    // The loading affordance: the footer says the index is still building
    // rather than leaving an empty grid to read as "no items". A 50k-entry
    // pack spends most of a second here (notes A, "Measured").
    let status = h.find(&by::test_id("browser.status"));
    assert_eq!(
        h.text_of(status).as_deref(),
        Some("indexing…"),
        "the footer says the index has not landed"
    );
}

/// Defends the OR operator against the spaces a player types around it.
/// `a | b` and `a|b` are the same query: whitespace ends a term but must not
/// forget the `|` that came before or after it.
#[test]
fn spaces_around_the_or_operator_do_not_break_the_group() {
    let cfg = SearchConfig::default();
    let tight = tokenize("iron|coal", &cfg);
    for spaced in ["iron | coal", "iron| coal", "iron |coal"] {
        let tokens = tokenize(spaced, &cfg);
        assert_eq!(tokens, tight, "`{spaced}` is the same query as `iron|coal`");
    }
    assert_eq!(tight.len(), 2);
    assert_eq!(tight[0].or_group, tight[1].or_group, "one OR group");

    // A third, unjoined term still opens a group of its own.
    let mixed = tokenize("iron | coal ingot", &cfg);
    assert_eq!(mixed.len(), 3);
    assert_eq!(mixed[0].or_group, mixed[1].or_group);
    assert_ne!(mixed[1].or_group, mixed[2].or_group);
}

// ---------------------------------------------------------------------------
// 3. Hiding an entry under an active query.
// ---------------------------------------------------------------------------

/// Defends the edit-mode blacklist: hiding an entry while a query is running
/// must drop exactly that entry, and unhiding must put the result set back the
/// way it was, order included. The search cache is keyed on the hidden
/// version, so a stale hit here would show a hidden entry.
#[test]
fn hiding_an_entry_during_a_query_and_unhiding_restores_the_order() {
    let mut h = harness();
    open(&mut h);
    h.browser().search("iron");
    h.browser().blur_search();

    let before = h.browser().visible_entries();
    assert!(before.len() >= 2, "iron matches several entries");
    let index = h
        .world()
        .resource::<IndexState>()
        .ready()
        .cloned()
        .expect("the index landed");
    let victim = before[0].clone();

    let id = index.id_of(&victim).expect("a visible entry is indexed");

    // The blacklist starts as a `Bitset::default()`, which addresses zero
    // bits. `set_hidden` grows it to fit the id, so it works whether it is
    // edited before or after the index lands.
    h.world_mut()
        .resource_mut::<HiddenEntries>()
        .set_hidden(id, true);
    h.settle();
    let during = h.browser().visible_entries();
    assert_eq!(during.len(), before.len() - 1);
    assert!(!during.contains(&victim), "the hidden entry is gone");
    assert_eq!(
        during,
        before[1..].to_vec(),
        "the survivors keep their order"
    );

    h.world_mut()
        .resource_mut::<HiddenEntries>()
        .set_hidden(id, false);
    h.settle();
    assert_eq!(
        h.browser().visible_entries(),
        before,
        "unhiding restores the original result, order included"
    );
}

// ---------------------------------------------------------------------------
// 4 and 5. Transfer: tag alternatives, and a grid that is not empty.
// ---------------------------------------------------------------------------

/// The smelting category as the `Categories` phase leaves it: `demo:smelting`
/// is 1x1, so `bind_defaults` gives it a `ProcessingCategory`.
fn smelting() -> (Categories, RecipeStore) {
    let categories = common::categories();
    let store = RecipeStore::build(&common::registries(), &categories);
    (categories, store)
}

/// The coal recipe's layout: one input, `#minecraft:planks`.
fn coal_layout(store: &RecipeStore, categories: &Categories) -> slotted_browser::RecipeLayout {
    let recipe = store
        .in_category(&CategoryId::new("demo:smelting"))
        .first()
        .copied()
        .expect("the coal recipe is filed under demo:smelting");
    store
        .layout(
            recipe,
            None,
            &common::registries(),
            categories,
            &IngredientTypes::with_builtins(),
        )
        .expect("the coal recipe lays out")
}

fn handler() -> SimpleTransfer {
    SimpleTransfer {
        grid: MenuDef::CRAFT_GRID,
        grid_cols: 2,
        sources: vec![MenuDef::PLAYER_MAIN, MenuDef::PLAYER_HOTBAR],
    }
}

/// A player menu with `main` filled from `(slot, item, count)` triples.
fn menu(contents: &[(usize, &str, u32)]) -> (MenuDef, slotted_model::Inventories, MenuState) {
    let def = MenuDef::player();
    let mut inventories = slotted_model::Inventories::for_menu(&def);
    let main = inventories
        .get_mut(MenuDef::PLAYER_MAIN)
        .expect("the player menu has a main inventory");
    for (i, item, count) in contents {
        main.set(*i, Some(common::stack(item, *count)));
    }
    let state = MenuState::new(&def);
    (def, inventories, state)
}

fn with_ctx<R>(
    def: &MenuDef,
    inventories: &slotted_model::Inventories,
    state: &MenuState,
    f: impl FnOnce(&TransferCtx<'_>) -> R,
) -> R {
    let registries = common::registries();
    let lookup = slotted_ecs::RegistryLookup(&registries);
    let types = IngredientTypes::with_builtins();
    let subtypes = Subtypes::default();
    let ctx = TransferCtx {
        def,
        inventories,
        state,
        actor: Actor::SURVIVAL,
        lookup: &lookup,
        types: &types,
        ctx: IngredientCtx {
            loc: &slotted_ui::Localization::default(),
            registries: &registries,
            subtypes: &subtypes,
        },
    };
    f(&ctx)
}

/// Every item in an `Inventories`, counted per kind.
fn census(inventories: &slotted_model::Inventories) -> BTreeMap<ItemId, u64> {
    let mut out: BTreeMap<ItemId, u64> = BTreeMap::new();
    for (_, inventory) in inventories.iter() {
        for slot in inventory.slots().iter().flatten() {
            *out.entry(slot.id).or_default() += u64::from(slot.count);
        }
    }
    out
}

/// Defends tag ingredients on the source side: the recipe asks for
/// `#minecraft:planks` and the player owns oak planks, which is a member of
/// that tag and nothing else. Matching by tag membership is the whole point of
/// expanding alternatives, so a dry run over it must succeed.
#[test]
fn a_dry_run_accepts_a_stack_that_only_matches_through_a_tag() {
    let (categories, store) = smelting();
    let layout = coal_layout(&store, &categories);
    let handler = handler();

    // Oak planks are in `#minecraft:planks` and match no recipe input by id.
    let (def, inventories, state) = menu(&[(3, "minecraft:oak_planks", 1)]);
    with_ctx(&def, &inventories, &state, |ctx| {
        handler
            .dry_run(ctx, &layout)
            .expect("a tag member feeds a tag input");
    });

    // A non-member of the tag does not, and the refusal names the input.
    let (def, inventories, state) = menu(&[(3, "minecraft:dirt", 64)]);
    let error = with_ctx(&def, &inventories, &state, |ctx| {
        handler
            .dry_run(ctx, &layout)
            .expect_err("dirt is in no planks tag")
    });
    assert_eq!(error.kind, TransferErrorKind::MissingIngredients);
    assert_eq!(error.missing.iter().map(|ix| ix.0).collect::<Vec<_>>(), [0]);
}

/// Defends item conservation across a refused transfer: a grid holding a stack
/// the recipe does not want must be refused with `GridNotEmpty`, and the
/// refusal must leave every item exactly where it was. A handler that "cleared
/// the way" would destroy the player's stack.
#[test]
fn a_conflicting_grid_stack_refuses_the_transfer_without_destroying_items() {
    let (categories, store) = smelting();
    let layout = coal_layout(&store, &categories);
    let handler = handler();

    let (def, mut inventories, state) = menu(&[(3, "minecraft:oak_planks", 8)]);
    inventories
        .get_mut(MenuDef::CRAFT_GRID)
        .expect("the player menu has a grid")
        .set(0, Some(common::stack("minecraft:ender_pearl", 16)));
    let before = census(&inventories);

    for max in [false, true] {
        let error = with_ctx(&def, &inventories, &state, |ctx| {
            handler
                .plan(ctx, &layout, max)
                .expect_err("the grid cell holds something else")
        });
        assert_eq!(
            error.kind,
            TransferErrorKind::GridNotEmpty,
            "a dirty grid is refused, not emptied (max = {max})"
        );
        assert!(
            error.missing.is_empty(),
            "nothing is missing; the grid is in the way"
        );
    }
    with_ctx(&def, &inventories, &state, |ctx| {
        handler
            .dry_run(ctx, &layout)
            .expect_err("the dry run agrees with the plan");
    });

    assert_eq!(
        census(&inventories),
        before,
        "planning is pure: a refusal creates and destroys nothing"
    );
    assert_eq!(
        inventories
            .get(MenuDef::CRAFT_GRID)
            .and_then(|g| g.get(0))
            .map(|s| s.count),
        Some(16),
        "the stack in the way is untouched"
    );

    // The counterpart, so the refusal is not simply "any occupied cell":
    // a cell already holding a stack the recipe wants there is accepted.
    let (def, mut inventories, state) = menu(&[(3, "minecraft:oak_planks", 8)]);
    inventories
        .get_mut(MenuDef::CRAFT_GRID)
        .expect("grid")
        .set(0, Some(common::stack("minecraft:oak_planks", 1)));
    with_ctx(&def, &inventories, &state, |ctx| {
        handler
            .plan(ctx, &layout, false)
            .expect("a matching stack in the cell is fine");
    });
}

// ---------------------------------------------------------------------------
// 6. Cheat-give as a survival actor.
// ---------------------------------------------------------------------------

/// Defends the cheat gate at both of its layers: Ctrl+clicking a card as a
/// survival player must write no `GiveRequested`, and a `GiveRequested` forged
/// past the UI must still be refused by `execute_gives`. Either way no item
/// appears in an inventory or on the cursor.
#[test]
fn a_survival_actor_gets_no_items_out_of_the_browser() {
    let mut h = harness();
    let opened = h.open_screen(ScreenKind::new(CHEST), ChestFixture::filled());
    h.browser().wait_for_index();
    h.settle();
    let card = card_for(&mut h, "minecraft:diamond");

    h.hold(KeyCode::ControlLeft);
    h.click(card);
    h.release(KeyCode::ControlLeft);
    h.settle();
    h.assert_conserved();
    assert!(
        h.browser().open_page().is_none(),
        "a Ctrl+click is a give attempt, not a recipe click"
    );

    // The same request, written straight into the message queue, so the gate
    // in `execute_gives` is tested and not just the one in the card observer.
    let diamond = Ingredient::item(TestRegistries::item("minecraft:diamond"));
    h.world_mut().write_message(GiveRequested {
        ingredient: diamond,
        count: 7,
        target: GiveTarget::Cursor,
    });
    h.settle();

    h.assert_conserved();
    let carried = h
        .world()
        .get::<slotted_ecs::Carried>(opened.menu)
        .and_then(|c| c.0.clone());
    assert_eq!(carried, None, "nothing landed on the cursor");
    let counted: u64 = opened
        .inventories
        .iter()
        .filter_map(|e| h.world().get::<slotted_ecs::Inventory>(*e))
        .map(|inv| {
            inv.0.count_of(&ItemStack::new(
                TestRegistries::item("minecraft:diamond"),
                1,
            ))
        })
        .sum();
    assert_eq!(counted, 7, "the chest's seven diamonds, and no more");
}

// ---------------------------------------------------------------------------
// 7. Teardown.
// ---------------------------------------------------------------------------

/// Defends teardown: closing the screen must take the panel, the screen's
/// exclusion zones and any tooltip a card raised with it. A tooltip lives
/// under the shared tooltip layer, not under the panel, so it outlives its
/// host unless something despawns it.
#[test]
fn closing_the_screen_leaves_no_panel_exclusion_zone_or_tooltip_behind() {
    let mut h = harness();
    let opened = open(&mut h);
    rail(&mut h, opened.screen, WIDTH - 200.0, 180.0);
    assert_eq!(count_of::<ExclusionZone>(&h), 1);

    let card = card_for(&mut h, "minecraft:coal");
    h.request_tooltip(card, TooltipTier::Compact);
    h.settle();
    assert!(
        count_of::<TooltipHost>(&h) > 0,
        "the card raised a tooltip to clean up"
    );

    let screen = opened.screen;
    let mut commands = h.world_mut().commands();
    slotted_ui::close_screen(&mut commands, screen);
    h.world_mut().flush();
    h.settle();

    assert!(!h.browser().is_attached(screen));
    assert_eq!(count_of::<BrowserPanel>(&h), 0, "no orphan panel root");
    assert_eq!(count_of::<Card>(&h), 0, "the card pool went with it");
    assert_eq!(
        count_of::<ExclusionZone>(&h),
        0,
        "the screen's exclusion zones went with it"
    );
    assert_eq!(
        count_of::<TooltipHost>(&h),
        0,
        "no tooltip is left hanging over an empty window"
    );
    assert!(h.try_find(&by::role(SemanticRole::Browser)).is_none());
}

// ---------------------------------------------------------------------------
// 8. Docking against an injected exclusion zone.
// ---------------------------------------------------------------------------

/// Defends docking against a toolbar that appears on the side the panel is
/// already using: the panel must move to the other side, and must move again
/// if the next zone lands there too. A dock that only reads the screen rect
/// would sit underneath the rail.
#[test]
fn an_exclusion_zone_on_the_docked_side_flips_the_panel_over() {
    let mut h = harness();
    let opened = open(&mut h);
    assert_eq!(
        h.browser().dock_side(),
        Some(Side::Right),
        "a centred screen leaves the wider strip on the right"
    );

    let right = rail(&mut h, opened.screen, WIDTH - 200.0, 180.0);
    assert_eq!(
        h.browser().dock_side(),
        Some(Side::Left),
        "a rail on the right pushes the panel left"
    );
    let layout = h.browser().layout(opened.screen).expect("still docked");
    assert!(
        layout.rect.max.x <= WIDTH - 200.0 + 1.0,
        "the panel stays clear of the rail"
    );

    // Take the right rail away and put one on the left instead: the panel has
    // to come back.
    h.world_mut().entity_mut(right).despawn();
    h.settle();
    rail(&mut h, opened.screen, 0.0, 300.0);
    assert_eq!(
        h.browser().dock_side(),
        Some(Side::Right),
        "a rail on the left pushes the panel back"
    );
    assert_eq!(h.browser().dock_tag().as_deref(), Some("right"));
}

// ---------------------------------------------------------------------------
// 9. Hotkeys under keyboard focus.
// ---------------------------------------------------------------------------

/// Defends the keyboard-focus guard for every hotkey at once: R, U, A and
/// Backspace must do nothing while the search field holds the keyboard, and
/// all of them must work again once Esc blurs it. Backspace is the sharp one:
/// it both edits text and navigates history.
#[test]
fn the_hotkeys_are_deaf_while_the_search_field_has_focus_and_hear_again_after_escape() {
    let mut h = harness();
    open(&mut h);

    let coal = card_for(&mut h, "minecraft:coal");
    h.browser().open_recipes(coal);
    let first = h.browser().open_page().expect("the coal page opened");
    // The recipe page replaces the card grid, so browsing to a second item
    // means closing the first page first.
    h.browser().close();
    let sword = card_for(&mut h, "minecraft:diamond_sword");
    h.browser().open_recipes(sword);
    let second = h.browser().open_page().expect("the sword page opened");
    assert_ne!(first.focus, second.focus);

    h.hover(sword);
    h.browser().focus_search();
    assert!(h.browser().search_has_focus());
    for key in [
        KeyCode::KeyR,
        KeyCode::KeyU,
        KeyCode::KeyA,
        KeyCode::Backspace,
    ] {
        h.key(key);
    }
    h.settle();
    assert_eq!(
        h.browser().open_page(),
        Some(second.clone()),
        "neither R, U nor Backspace moved the recipe view while typing"
    );
    assert_eq!(
        h.world()
            .resource::<slotted_browser::Bookmarks>()
            .entries
            .len(),
        0,
        "A is a letter while the field has the keyboard"
    );

    // Esc blurs the field rather than closing the view, and hands the keys
    // back.
    h.key(KeyCode::Escape);
    h.settle();
    assert!(!h.browser().search_has_focus());
    assert_eq!(
        h.browser().open_page(),
        Some(second),
        "the Esc that blurs is not also the Esc that closes"
    );

    h.key(KeyCode::Backspace);
    h.settle();
    assert_eq!(
        h.browser().open_page(),
        Some(first),
        "Backspace navigates again once the field is blurred"
    );
    // Back on the card grid, which the recipe page was covering, the
    // bookmark key reaches the card again.
    h.browser().close();
    h.settle();
    h.hover(sword);
    h.key(KeyCode::KeyA);
    h.settle();
    assert_eq!(
        h.world()
            .resource::<slotted_browser::Bookmarks>()
            .entries
            .len(),
        1,
        "so does the bookmark key"
    );
}

// ---------------------------------------------------------------------------
// 10. A second screen.
// ---------------------------------------------------------------------------

/// Defends what a second screen inherits from the first.
///
/// `BrowserRuntime` is a single app-wide resource (contract section 5) and
/// nothing in the plugin resets it on `ScreenClosed`, so the query, the
/// recipe history and the open page are deliberately *persisted* across
/// screens, the way JEI keeps its search text when you walk from a chest to a
/// furnace. What must not persist is entity state: the closed screen's panel,
/// its card pool and its layout all belong to that screen and have to be gone,
/// with the new panel rebound from the surviving runtime.
#[test]
fn a_second_screen_inherits_the_runtime_but_none_of_the_first_panel() {
    let mut h = harness();
    let first = open(&mut h);
    let card = card_for(&mut h, "minecraft:coal");
    h.browser().open_recipes(card);
    let page = h.browser().open_page().expect("a page opened");
    let cards_per_panel = count_of::<Card>(&h);
    assert!(cards_per_panel > 0);

    let screen = first.screen;
    let mut commands = h.world_mut().commands();
    slotted_ui::close_screen(&mut commands, screen);
    h.world_mut().flush();
    h.settle();
    assert_eq!(count_of::<BrowserPanel>(&h), 0);
    assert_eq!(count_of::<Card>(&h), 0);

    let second = h.open_screen(ScreenKind::new(CHEST), ChestFixture::filled());
    h.settle();
    assert!(h.browser().is_attached(second.screen));
    assert_eq!(count_of::<BrowserPanel>(&h), 1, "exactly one panel");
    assert_eq!(
        count_of::<Card>(&h),
        cards_per_panel,
        "one card pool, not two"
    );

    // Persisted, by design: the query, the visible set and the history.
    assert_eq!(h.world().resource::<BrowserRuntime>().filter_text, "coal");
    assert_eq!(h.browser().open_page(), Some(page));
    // The page the first screen left open is still open, so the new panel
    // shows the recipe view rather than the grid; closing it hands the cards
    // back, rebound from the runtime the new panel inherited.
    h.browser().close();
    h.settle();
    assert!(
        h.browser()
            .visible_cards()
            .iter()
            .any(|c| c == "minecraft:coal"),
        "the new panel rebound from the runtime it inherited"
    );

    // The new panel is its own screen root with its own placement.
    assert!(h.browser().layout(second.screen).is_some());
    assert!(
        h.browser().layout(screen).is_none(),
        "nothing still points at the closed screen"
    );
}

// ---------------------------------------------------------------------------
// 11. Theme independence.
// ---------------------------------------------------------------------------

/// Defends the promise the `ScreenTree` snapshots rest on: the semantic tree
/// is theme-independent. Roles, test ids, tags, labels and bound items come
/// from the widgets; only colours and spacing come from the theme, so a panel
/// built with the glass theme and one built with no theme at all must produce
/// the same tree.
#[test]
fn the_browser_subtree_is_the_same_tree_with_and_without_a_theme() {
    fn subtree(theme: Option<&str>) -> String {
        let mut h = harness_with(theme, 900.0, 400.0);
        open(&mut h);
        h.browser().search("coal");
        h.browser().blur_search();
        let tree = h.screen_tree();
        tree.roots
            .iter()
            .find(|root| root.test_id.as_deref() == Some("browser"))
            .expect("the panel is a root of its own")
            .to_string()
    }

    let themed = subtree(Some("glass"));
    let bare = subtree(None);
    assert!(themed.contains("#browser.cards"), "the tree is the panel's");
    assert_eq!(
        themed, bare,
        "the theme changed the semantic tree, which the snapshots assume it cannot"
    );
}
