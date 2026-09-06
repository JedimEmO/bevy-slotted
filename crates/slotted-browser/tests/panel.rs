//! The browser panel, driven through `slotted_test`'s harness.
//!
//! Every assertion goes through something a player can reach: the docked
//! panel's own components, the search field's keystrokes, the hotkeys, and a
//! real pointer click on the `+` button. Package A's logic is exercised where
//! it exists and stubbed with in-test fakes where the contract only names a
//! trait.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use bevy::prelude::*;
use bevy::ui::ui_transform::UiGlobalTransform;
use slotted_browser::ui::dock::PANEL_PADDING;
use slotted_browser::{
    Bookmarks, BrowserRuntime, CategoryId, DefaultScreenHandler, Ingredient, PageMode,
    RecipeLayout, ScreenHandlers, TransferCtx, TransferError, TransferErrorKind, TransferHandler,
    TransferHandlers, TransferPlan,
};
use slotted_ecs::Authority;
use slotted_model::{Button, ClickAction, SlotIx};
use slotted_test::prelude::*;
use slotted_testutils::RecordingAuthority;
use slotted_ui::{ExclusionZone, Layout, ScreenDef, Screens, Side, UiNodeDef};

const CHEST: &str = "demo:chest";
const WIDTH: f32 = 1280.0;
const HEIGHT: f32 = 720.0;

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
/// panel attaches.
fn harness() -> UiHarness {
    let mut h = UiHarness::builder()
        .plugins(SlottedPlugins::headless())
        .registries(TestRegistries::basic())
        .resolution(WIDTH, HEIGHT)
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

/// Opens the chest and waits for the index, which is what every test needs
/// before the cards mean anything.
fn open(h: &mut UiHarness) -> Opened {
    let opened = h.open_screen(ScreenKind::new(CHEST), ChestFixture::filled());
    h.browser().wait_for_index();
    h.settle();
    opened
}

/// Shifts the screen's def root sideways by giving it a margin, so the free
/// strips stop being symmetric.
fn shift_screen(h: &mut UiHarness, screen: Entity, margin: UiRect) {
    let root = h.world().get::<Children>(screen).unwrap().iter().next();
    let root = root.expect("the screen root has a def root child");
    h.world_mut().get_mut::<Node>(root).unwrap().margin = margin;
    h.settle();
}

// ---------------------------------------------------------------------------
// Docking
// ---------------------------------------------------------------------------

#[test]
fn the_panel_docks_right_when_the_screen_sits_left_of_centre() {
    let mut h = harness();
    let opened = open(&mut h);
    shift_screen(&mut h, opened.screen, UiRect::right(bevy::ui::px(600.0)));
    assert_eq!(h.browser().dock_side(), Some(Side::Right));
}

#[test]
fn the_panel_docks_left_when_the_screen_sits_right_of_centre() {
    let mut h = harness();
    let opened = open(&mut h);
    shift_screen(&mut h, opened.screen, UiRect::left(bevy::ui::px(600.0)));
    assert_eq!(h.browser().dock_side(), Some(Side::Left));
}

#[test]
fn an_exclusion_zone_pushes_the_dock_to_the_other_side() {
    let mut h = harness();
    let opened = open(&mut h);
    assert_eq!(
        h.browser().dock_side(),
        Some(Side::Right),
        "a centred screen leaves the wider strip on the right"
    );
    // A rail down the right of the screen, the way an injected toolbar sits.
    h.world_mut().spawn((
        Node {
            position_type: PositionType::Absolute,
            left: bevy::ui::px(WIDTH - 200.0),
            top: bevy::ui::px(0.0),
            width: bevy::ui::px(180.0),
            height: bevy::ui::px(HEIGHT),
            ..default()
        },
        ExclusionZone,
        ChildOf(opened.screen),
    ));
    h.settle();
    assert_eq!(h.browser().dock_side(), Some(Side::Left));
}

/// A side tab opening beside a machine panel must not move the panel, and
/// the browser has to re-dock around the strip the open tab now occupies.
/// Both halves of the "side tabs shift the main UI layout" report.
#[test]
fn an_opening_side_tab_leaves_the_panel_alone_and_the_browser_docks_around_it() {
    let mut h = harness();
    h.world_mut()
        .resource_mut::<Screens>()
        .register(tabbed_chest());
    h.world_mut()
        .resource_mut::<ScreenHandlers>()
        .register(ScreenKind::new(TABBED), Arc::new(DefaultScreenHandler));
    let opened = h.open_screen(ScreenKind::new(TABBED), ChestFixture::filled());
    h.browser().wait_for_index();
    h.settle();

    let panel = h.find(&by::test_id("panel"));
    let panel_before = h.rect_of(panel);
    let dock_before = h.browser().layout(opened.screen).expect("docked");

    let tab = h.find(&by::test_id("tab"));
    h.toggle_side_tab(tab);
    h.settle();

    assert_eq!(
        h.rect_of(panel),
        panel_before,
        "opening a tab moved the panel it sits beside"
    );

    let zones = h.exclusion_zones(opened.screen);
    let dock_after = h.browser().layout(opened.screen).expect("still docked");
    for zone in &zones {
        assert!(
            dock_after.rect.intersect(*zone).is_empty(),
            "the browser overlaps the open tab: {:?} against {zone:?}",
            dock_after.rect
        );
    }
    assert_ne!(
        dock_after.rect, dock_before.rect,
        "the browser did not move out of the tab's way"
    );
}

const TABBED: &str = "demo:tabbed";

/// The machine shape: a panel and a tab rail side by side.
fn tabbed_chest() -> ScreenDef {
    ScreenDef {
        kind: ScreenKind::new(TABBED),
        inherits: None,
        root: UiNodeDef::Panel {
            role: slotted_theme::roles::PANEL,
            layout: Layout {
                direction: slotted_ui::def::LayoutDirection::Row,
                ..Layout::default()
            },
            children: vec![
                UiNodeDef::Panel {
                    role: slotted_theme::roles::PANEL,
                    layout: Layout::default(),
                    children: vec![UiNodeDef::SlotGrid {
                        inventory: MenuDef::CONTAINER,
                        cols: 9,
                        rows: 3,
                        first: 0,
                        tags: slotted_ui::Tags::default(),
                    }],
                    tags: slotted_ui::Tags::new().with("test_id", "panel"),
                },
                UiNodeDef::Panel {
                    role: slotted_theme::roles::TAB_RAIL,
                    layout: Layout::default(),
                    children: vec![UiNodeDef::SideTab {
                        icon: slotted_ui::IconDef::Image("icons/tab.png".to_owned()),
                        side: Side::Right,
                        label: Some(slotted_ui::LocKey("tab".to_owned())),
                        open: false,
                        children: vec![UiNodeDef::Text {
                            key: slotted_ui::LocKey("a body wide enough to notice".to_owned()),
                            style: slotted_ui::TextRole::Body,
                            tags: slotted_ui::Tags::default(),
                        }],
                        tags: slotted_ui::Tags::new().with("test_id", "tab"),
                    }],
                    tags: slotted_ui::Tags::new().with("test_id", "rail"),
                },
            ],
            tags: slotted_ui::Tags::default(),
        },
        listring: vec![],
    }
}

#[test]
fn a_screen_that_fills_the_window_hides_the_panel() {
    let mut h = harness();
    let opened = open(&mut h);
    h.world_mut().spawn((
        Node {
            position_type: PositionType::Absolute,
            left: bevy::ui::px(0.0),
            top: bevy::ui::px(0.0),
            width: bevy::ui::px(WIDTH),
            height: bevy::ui::px(HEIGHT),
            ..default()
        },
        ExclusionZone,
        ChildOf(opened.screen),
    ));
    h.settle();
    assert_eq!(h.browser().dock_side(), None);
    assert_eq!(h.browser().dock_tag().as_deref(), Some("none"));
    let panel = h.find(&by::test_id("browser"));
    assert!(!h.is_visible(panel));
}

#[test]
fn the_panel_stays_inside_the_free_strip() {
    let mut h = harness();
    let opened = open(&mut h);
    let layout = h.browser().layout(opened.screen).expect("docked");
    let screen_rect = h.rect_of(
        h.world()
            .get::<Children>(opened.screen)
            .unwrap()
            .iter()
            .next()
            .unwrap(),
    );
    assert!(layout.rect.min.x >= screen_rect.max.x - 1.0);
    assert!(layout.rect.max.x <= WIDTH + 1.0);
    assert!(layout.cols >= 2, "a strip that docks fits two columns");
    let _ = PANEL_PADDING;
}

#[test]
fn closing_the_screen_takes_the_panel_with_it() {
    let mut h = harness();
    let opened = open(&mut h);
    assert!(h.browser().is_attached(opened.screen));
    let screen = opened.screen;
    let mut commands = h.world_mut().commands();
    slotted_ui::close_screen(&mut commands, screen);
    h.world_mut().flush();
    h.settle();
    assert!(!h.browser().is_attached(screen));
    assert!(h.try_find(&by::role(SemanticRole::Browser)).is_none());
}

// ---------------------------------------------------------------------------
// Search
// ---------------------------------------------------------------------------

#[test]
fn typing_in_the_search_field_narrows_the_cards() {
    let mut h = harness();
    open(&mut h);
    let all = h.browser().visible_entries().len();
    assert!(all > 3, "an empty query shows every entry");

    h.browser().search("ingot");
    let narrowed = h.browser().visible_entries();
    assert_eq!(
        narrowed.len(),
        3,
        "iron ingot, gold ingot and the #c:ingots tag entry"
    );
    assert_eq!(
        h.world().resource::<BrowserRuntime>().filter_text,
        "ingot",
        "the field's value reached the runtime"
    );
    let cards = h.browser().visible_cards();
    assert!(
        cards.iter().any(|c| c == "minecraft:iron_ingot"),
        "the iron ingot card is bound, got {cards:?}"
    );
    assert!(cards.len() <= narrowed.len());
}

#[test]
fn a_narrower_query_hides_the_cards_it_drops() {
    let mut h = harness();
    open(&mut h);
    h.browser().search("coal");
    let cards = h.browser().visible_cards();
    assert!(!cards.is_empty());
    assert!(
        cards.iter().all(|c| c.contains("coal")),
        "every bound card matches, got {cards:?}"
    );
}

// ---------------------------------------------------------------------------
// Hotkeys and the recipe view
// ---------------------------------------------------------------------------

/// The card for one item, once the query has narrowed to it.
///
/// Searching leaves the field focused, exactly as typing does on screen, so
/// this blurs it again before handing the card back: the hotkeys are gated on
/// keyboard focus and a test that means to press R has to stop typing first.
fn card_for(h: &mut UiHarness, item: &str) -> Entity {
    let words = item.split(':').next_back().unwrap().replace('_', " ");
    h.browser().search(&words);
    h.browser().blur_search();
    h.find(&by::role(SemanticRole::Card).tag("entry", item))
}

#[test]
fn the_recipes_key_over_a_card_opens_that_recipe() {
    let mut h = harness();
    open(&mut h);
    let card = card_for(&mut h, "minecraft:coal");
    h.browser().open_recipes(card);

    let page = h.browser().open_page().expect("a page opened");
    assert_eq!(page.mode, PageMode::Recipes);
    assert_eq!(
        page.focus,
        Ingredient::item(TestRegistries::item("minecraft:coal"))
    );
    assert_eq!(page.category, CategoryId::new("demo:smelting"));

    let view = h.find(&by::test_id("browser.recipes"));
    assert!(h.is_visible(view), "the recipe view is showing");
    let output = h.find(
        &by::role(SemanticRole::RecipeSlot)
            .tag("role", "output")
            .index(0),
    );
    assert_eq!(
        h.world()
            .get::<slotted_ui::Tags>(output)
            .and_then(|t| t.get("entry"))
            .map(ToOwned::to_owned),
        Some("minecraft:coal".to_owned()),
        "the output slot shows what the recipe makes"
    );
}

#[test]
fn the_uses_key_opens_the_recipes_that_consume_the_card() {
    let mut h = harness();
    open(&mut h);
    let card = card_for(&mut h, "minecraft:oak_planks");
    h.browser().open_uses(card);
    let page = h.browser().open_page().expect("a page opened");
    assert_eq!(page.mode, PageMode::Uses);
}

#[test]
fn backspace_returns_to_the_previous_page() {
    let mut h = harness();
    open(&mut h);
    let coal = card_for(&mut h, "minecraft:coal");
    h.browser().open_recipes(coal);
    let first = h.browser().open_page().expect("the first page");

    // The recipe page replaces the card grid, as the moodboard's browser does,
    // so browsing to a second item means closing the first page first.
    h.browser().close();
    let sword = card_for(&mut h, "minecraft:diamond_sword");
    h.browser().open_recipes(sword);
    let second = h.browser().open_page().expect("the second page");
    assert_ne!(first.focus, second.focus);

    h.browser().back();
    assert_eq!(h.browser().open_page(), Some(first));
}

#[test]
fn escape_closes_the_recipe_view() {
    let mut h = harness();
    open(&mut h);
    let card = card_for(&mut h, "minecraft:coal");
    h.browser().open_recipes(card);
    assert!(h.browser().open_page().is_some());

    h.browser().close();
    assert!(h.browser().open_page().is_none());
    let view = h.find(&by::test_id("browser.recipes"));
    assert!(!h.is_visible(view), "the view hides when the page closes");
}

#[test]
fn the_bookmark_key_over_a_card_adds_and_removes_it() {
    let mut h = harness();
    open(&mut h);
    let card = card_for(&mut h, "minecraft:coal");
    h.browser().bookmark(card);
    assert_eq!(h.world().resource::<Bookmarks>().entries.len(), 1);
    assert!(
        h.try_find(&by::role(SemanticRole::Bookmark)).is_some(),
        "the strip drew the bookmark"
    );

    h.browser().bookmark(card);
    assert_eq!(h.world().resource::<Bookmarks>().entries.len(), 0);
}

#[test]
fn the_hotkeys_do_not_fire_while_the_search_field_has_focus() {
    let mut h = harness();
    open(&mut h);
    let card = card_for(&mut h, "minecraft:coal");
    h.hover(card);
    h.browser().focus_search();
    assert!(h.browser().search_has_focus());

    // R, U and A are ordinary characters while the field has the keyboard.
    for key in [KeyCode::KeyR, KeyCode::KeyU, KeyCode::KeyA] {
        h.key(key);
    }
    h.settle();
    assert!(
        h.browser().open_page().is_none(),
        "R over a card must not open a page while typing"
    );
    assert_eq!(h.world().resource::<Bookmarks>().entries.len(), 0);

    // Blurring hands the keys back.
    h.browser().blur_search();
    h.hover(card);
    h.browser().open_recipes(card);
    assert!(h.browser().open_page().is_some());
}

// ---------------------------------------------------------------------------
// Transfer
// ---------------------------------------------------------------------------

/// A handler that always plans the same two clicks.
struct FixedPlan(Vec<ClickAction>);

impl TransferHandler for FixedPlan {
    fn plan(
        &self,
        _ctx: &TransferCtx<'_>,
        _layout: &RecipeLayout,
        _max: bool,
    ) -> Result<TransferPlan, TransferError> {
        Ok(TransferPlan {
            actions: self.0.clone(),
        })
    }
}

/// A handler that always refuses, naming the slots it could not fill.
struct AlwaysMissing;

impl TransferHandler for AlwaysMissing {
    fn plan(
        &self,
        _ctx: &TransferCtx<'_>,
        _layout: &RecipeLayout,
        _max: bool,
    ) -> Result<TransferPlan, TransferError> {
        Err(TransferError {
            kind: TransferErrorKind::MissingIngredients,
            missing: vec![slotted_browser::RecipeSlotIx(0)],
            message: "nothing to put in".to_owned(),
        })
    }
}

fn register_handler(h: &mut UiHarness, handler: Arc<dyn TransferHandler>) {
    h.world_mut().resource_mut::<TransferHandlers>().register(
        ScreenKind::new(CHEST),
        None,
        handler,
    );
}

#[test]
fn the_transfer_button_is_disabled_with_no_handler() {
    let mut h = harness();
    open(&mut h);
    let card = card_for(&mut h, "minecraft:coal");
    h.browser().open_recipes(card);
    assert!(
        !h.browser().transfer_enabled(),
        "a chest has no grid, so nothing can be transferred into it"
    );
}

#[test]
fn the_transfer_button_is_disabled_when_the_dry_run_reports_missing() {
    let mut h = harness();
    open(&mut h);
    register_handler(&mut h, Arc::new(AlwaysMissing));
    let card = card_for(&mut h, "minecraft:coal");
    h.browser().open_recipes(card);
    assert!(!h.browser().transfer_enabled());
}

#[test]
fn the_transfer_button_is_enabled_when_the_dry_run_passes() {
    let mut h = harness();
    open(&mut h);
    register_handler(&mut h, Arc::new(FixedPlan(Vec::new())));
    let card = card_for(&mut h, "minecraft:coal");
    h.browser().open_recipes(card);
    assert!(h.browser().transfer_enabled());
}

#[test]
fn a_transfer_click_sends_the_planned_actions() {
    let mut h = harness();
    let authority = RecordingAuthority::new();
    h.world_mut().insert_resource(Authority(authority.clone()));
    let opened = open(&mut h);
    let actions = vec![
        ClickAction::Pickup {
            slot: SlotIx(0),
            button: Button::Left,
        },
        ClickAction::Pickup {
            slot: SlotIx(1),
            button: Button::Left,
        },
    ];
    register_handler(&mut h, Arc::new(FixedPlan(actions.clone())));
    let card = card_for(&mut h, "minecraft:coal");
    h.browser().open_recipes(card);
    assert!(h.browser().transfer_enabled());

    h.browser().transfer();

    let submitted: Vec<ClickAction> = authority
        .submitted()
        .into_iter()
        .map(|entry| entry.action)
        .collect();
    assert_eq!(
        submitted, actions,
        "the plan reached the authority in order"
    );
    let _ = opened;
    h.assert_conserved();
}

#[test]
fn a_refused_transfer_paints_the_missing_slots() {
    let mut h = harness();
    open(&mut h);
    // Enabled by the dry run, refused by the plan: the button is clickable and
    // the failure comes back as a message.
    register_handler(&mut h, Arc::new(SometimesMissing));
    let card = card_for(&mut h, "minecraft:coal");
    h.browser().open_recipes(card);
    assert!(h.browser().transfer_enabled());
    h.browser().transfer();

    let painted = h.find_all(&by::role(SemanticRole::RecipeSlot));
    let missing = painted
        .into_iter()
        .filter(|e| {
            h.world()
                .get::<slotted_theme::Themed>(*e)
                .is_some_and(|t| t.0 == slotted_browser::ui::roles::SLOT_MISSING)
        })
        .count();
    assert_eq!(missing, 1, "exactly the slot the error named turned red");
}

/// Passes the dry run, refuses the real plan. The state a grid reaches when
/// something changes between the two.
struct SometimesMissing;

impl TransferHandler for SometimesMissing {
    fn plan(
        &self,
        _ctx: &TransferCtx<'_>,
        _layout: &RecipeLayout,
        _max: bool,
    ) -> Result<TransferPlan, TransferError> {
        Err(TransferError {
            kind: TransferErrorKind::MissingIngredients,
            missing: vec![slotted_browser::RecipeSlotIx(0)],
            message: "gone".to_owned(),
        })
    }

    fn dry_run(&self, _ctx: &TransferCtx<'_>, _layout: &RecipeLayout) -> Result<(), TransferError> {
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Shape
// ---------------------------------------------------------------------------

#[test]
fn the_browser_subtree_matches_the_snapshot() {
    let mut h = UiHarness::builder()
        .plugins(SlottedPlugins::headless())
        .registries(TestRegistries::basic())
        // Narrow enough that the card pool stays readable in the snapshot.
        .resolution(900.0, 400.0)
        .theme("glass")
        .build();
    h.world_mut()
        .resource_mut::<Screens>()
        .register(bare_chest());
    h.world_mut()
        .resource_mut::<ScreenHandlers>()
        .register(ScreenKind::new(CHEST), Arc::new(DefaultScreenHandler));
    open(&mut h);
    h.browser().search("coal");

    let tree = h.screen_tree();
    let browser = tree
        .roots
        .iter()
        .find(|root| root.test_id.as_deref() == Some("browser"))
        .expect("the panel is a root of its own")
        .clone();
    insta::assert_snapshot!(browser.to_string());
}

#[test]
fn every_interactive_node_carries_a_role_and_an_id() {
    let mut h = harness();
    open(&mut h);
    for id in [
        "browser",
        "browser.search",
        "browser.chips",
        "browser.cards",
        "browser.bookmarks",
        "browser.recipes",
    ] {
        assert!(
            h.try_find(&by::test_id(id)).is_some(),
            "{id} is missing from the panel"
        );
    }
    let card = card_for(&mut h, "minecraft:coal");
    h.browser().open_recipes(card);
    for id in [
        "browser.transfer",
        "browser.back",
        "browser.forward",
        "browser.uses",
    ] {
        assert!(
            h.try_find(&by::test_id(id)).is_some(),
            "{id} is missing from the recipe view"
        );
    }
    let _ = UiGlobalTransform::default();
}
