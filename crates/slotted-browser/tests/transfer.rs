//! Recipe transfer and cheat-give: the plan a handler builds, what running it
//! through `MenuAction` does to the inventories, and who is allowed to cheat.

mod common;

use std::sync::Arc;

use bevy::prelude::*;
use slotted_browser::{
    Categories, CategoryId, CraftingCategory, IngredientCtx, IngredientTypes, RecipeStore,
    SimpleTransfer, Subtypes, TransferCtx, TransferErrorKind, TransferHandler,
};
use slotted_ecs::{Inventory, MenuAction, OpenMenu, Registries, SlottedEcsPlugin};
use slotted_model::{
    Actor, ClickAction, ClickError, GiveTarget, Inventories, ItemStack, MenuDef, MenuId, MenuState,
    apply_click,
};

/// The 2x2 category the player menu's crafting grid can take.
fn categories() -> Categories {
    let mut categories = Categories::default();
    categories.register(Arc::new(CraftingCategory::new(
        CategoryId::new("demo:crafting"),
        common::id("demo:crafting"),
        (2, 2),
    )));
    categories.bind_defaults(&common::registries());
    categories
}

/// The shapeless sword recipe: diamond, diamond, `#minecraft:planks`.
fn sword_layout(store: &RecipeStore, categories: &Categories) -> slotted_browser::RecipeLayout {
    let recipe = store
        .in_category(&CategoryId::new("demo:crafting"))
        .iter()
        .copied()
        .find(|r| {
            common::registries()
                .recipes
                .get(r.0)
                .is_some_and(|d| d.name == common::id("demo:diamond_sword"))
        })
        .expect("the sword is filed under demo:crafting");
    store
        .layout(
            recipe,
            None,
            &common::registries(),
            categories,
            &IngredientTypes::with_builtins(),
        )
        .expect("the sword lays out")
}

fn handler() -> SimpleTransfer {
    SimpleTransfer {
        grid: MenuDef::CRAFT_GRID,
        grid_cols: 2,
        sources: vec![MenuDef::PLAYER_MAIN, MenuDef::PLAYER_HOTBAR],
    }
}

/// A player menu with `main` filled from `(slot, item, count)` triples.
fn menu(contents: &[(usize, &str, u32)]) -> (MenuDef, Inventories, MenuState) {
    let def = MenuDef::player();
    let mut inventories = Inventories::for_menu(&def);
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
    inventories: &Inventories,
    state: &MenuState,
    actor: Actor,
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
        actor,
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

#[test]
fn a_dry_run_reports_exactly_the_missing_slots() {
    let categories = categories();
    let store = RecipeStore::build(&common::registries(), &categories);
    let layout = sword_layout(&store, &categories);
    let handler = handler();

    // Nothing in the inventory: all three inputs are missing.
    let (def, inventories, state) = menu(&[]);
    let error = with_ctx(&def, &inventories, &state, Actor::SURVIVAL, |ctx| {
        handler.dry_run(ctx, &layout).expect_err("nothing to take")
    });
    assert_eq!(error.kind, TransferErrorKind::MissingIngredients);
    let missing: Vec<u16> = error.missing.iter().map(|ix| ix.0).collect();
    assert_eq!(missing, [0, 1, 2]);

    // One diamond covers the first input only; the planks are still missing.
    let (def, inventories, state) = menu(&[(0, "minecraft:diamond", 1)]);
    let error = with_ctx(&def, &inventories, &state, Actor::SURVIVAL, |ctx| {
        handler.dry_run(ctx, &layout).expect_err("still short")
    });
    let missing: Vec<u16> = error.missing.iter().map(|ix| ix.0).collect();
    assert_eq!(missing, [1, 2], "one diamond feeds one input, not two");

    // Everything present: the dry run succeeds.
    let (def, inventories, state) =
        menu(&[(0, "minecraft:diamond", 5), (1, "minecraft:oak_planks", 3)]);
    with_ctx(&def, &inventories, &state, Actor::SURVIVAL, |ctx| {
        handler.dry_run(ctx, &layout).expect("everything is there");
    });
}

#[test]
fn a_full_cursor_and_a_dirty_grid_are_refused() {
    let categories = categories();
    let store = RecipeStore::build(&common::registries(), &categories);
    let layout = sword_layout(&store, &categories);
    let handler = handler();

    let (def, inventories, mut state) =
        menu(&[(0, "minecraft:diamond", 5), (1, "minecraft:oak_planks", 3)]);
    state.carried = Some(common::stack("minecraft:dirt", 1));
    let error = with_ctx(&def, &inventories, &state, Actor::SURVIVAL, |ctx| {
        handler
            .plan(ctx, &layout, false)
            .expect_err("cursor is full")
    });
    assert_eq!(error.kind, TransferErrorKind::CursorNotEmpty);

    let (def, mut inventories, state) =
        menu(&[(0, "minecraft:diamond", 5), (1, "minecraft:oak_planks", 3)]);
    inventories
        .get_mut(MenuDef::CRAFT_GRID)
        .expect("the player menu has a grid")
        .set(0, Some(common::stack("minecraft:dirt", 1)));
    let error = with_ctx(&def, &inventories, &state, Actor::SURVIVAL, |ctx| {
        handler
            .plan(ctx, &layout, false)
            .expect_err("grid is dirty")
    });
    assert_eq!(error.kind, TransferErrorKind::GridNotEmpty);
}

#[test]
fn a_plan_is_clicks_that_conserve_items_and_fill_the_grid() {
    let categories = categories();
    let store = RecipeStore::build(&common::registries(), &categories);
    let layout = sword_layout(&store, &categories);
    let handler = handler();

    let (def, mut inventories, mut state) =
        menu(&[(0, "minecraft:diamond", 5), (1, "minecraft:oak_planks", 3)]);
    let plan = with_ctx(&def, &inventories, &state, Actor::SURVIVAL, |ctx| {
        handler.plan(ctx, &layout, false).expect("plan")
    });
    assert!(
        plan.actions
            .iter()
            .all(|a| matches!(a, ClickAction::Pickup { .. })),
        "a transfer is ordinary clicks and nothing else"
    );

    let registries = common::registries();
    let lookup = slotted_ecs::RegistryLookup(&registries);
    let diamond = common::stack("minecraft:diamond", 1);
    let planks = common::stack("minecraft:oak_planks", 1);
    let before = (
        inventories.count_of(&diamond),
        inventories.count_of(&planks),
    );
    for action in plan.actions {
        // `apply_click` asserts conservation itself in debug builds.
        apply_click(
            &def,
            &mut inventories,
            &mut state,
            action,
            &Actor::SURVIVAL,
            &lookup,
        )
        .expect("every planned click is legal");
    }
    assert_eq!(state.carried, None, "the cursor ends empty");
    assert_eq!(
        (
            inventories.count_of(&diamond),
            inventories.count_of(&planks)
        ),
        before,
        "nothing was created or destroyed"
    );

    let grid = inventories.get(MenuDef::CRAFT_GRID).expect("grid");
    assert_eq!(grid.get(0).map(|s| (s.id, s.count)), Some((diamond.id, 1)));
    assert_eq!(grid.get(1).map(|s| (s.id, s.count)), Some((diamond.id, 1)));
    assert_eq!(grid.get(2).map(|s| (s.id, s.count)), Some((planks.id, 1)));
    assert_eq!(grid.get(3), None, "the fourth cell stays empty");

    let main = inventories.get(MenuDef::PLAYER_MAIN).expect("main");
    assert_eq!(main.get(0).map(|s| s.count), Some(3), "remainder returned");
    assert_eq!(main.get(1).map(|s| s.count), Some(2));
}

#[test]
fn max_fills_as_many_sets_as_the_inventory_allows() {
    let categories = categories();
    let store = RecipeStore::build(&common::registries(), &categories);
    let layout = sword_layout(&store, &categories);
    let handler = handler();

    let (def, mut inventories, mut state) =
        menu(&[(0, "minecraft:diamond", 5), (1, "minecraft:oak_planks", 3)]);
    let plan = with_ctx(&def, &inventories, &state, Actor::SURVIVAL, |ctx| {
        handler.plan(ctx, &layout, true).expect("plan")
    });
    let registries = common::registries();
    let lookup = slotted_ecs::RegistryLookup(&registries);
    for action in plan.actions {
        apply_click(
            &def,
            &mut inventories,
            &mut state,
            action,
            &Actor::SURVIVAL,
            &lookup,
        )
        .expect("every planned click is legal");
    }
    let grid = inventories.get(MenuDef::CRAFT_GRID).expect("grid");
    // Five diamonds feed two cells per set, so two full sets; the planks
    // stack of three is enough for both.
    assert_eq!(grid.get(0).map(|s| s.count), Some(2));
    assert_eq!(grid.get(1).map(|s| s.count), Some(2));
    assert_eq!(grid.get(2).map(|s| s.count), Some(2));
    assert_eq!(state.carried, None);
}

// ---------------------------------------------------------------------------
// Cheat-give.
// ---------------------------------------------------------------------------

#[test]
fn give_is_refused_for_a_survival_actor_and_allowed_when_cheating() {
    let registries = common::registries();
    let lookup = slotted_ecs::RegistryLookup(&registries);
    let action = ClickAction::Give {
        item: common::item("minecraft:diamond"),
        count: 3,
        target: GiveTarget::Cursor,
    };

    let (def, mut inventories, mut state) = menu(&[]);
    let refused = apply_click(
        &def,
        &mut inventories,
        &mut state,
        action,
        &Actor::SURVIVAL,
        &lookup,
    );
    assert_eq!(refused, Err(ClickError::Permission));
    assert_eq!(state.carried, None, "nothing was created");

    apply_click(
        &def,
        &mut inventories,
        &mut state,
        action,
        &Actor::CREATIVE,
        &lookup,
    )
    .expect("a cheating actor may give");
    assert_eq!(
        state.carried,
        Some(common::stack("minecraft:diamond", 3)),
        "the stack lands on the cursor"
    );
}

#[test]
fn give_into_an_inventory_goes_through_the_menu_action_path() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_plugins(SlottedEcsPlugin)
        .insert_resource(Registries(common::registries()));

    let def = Arc::new(MenuDef::player());
    let sizes = def.inventory_sizes();
    let world = app.world_mut();
    let inventories: Vec<Entity> = sizes
        .iter()
        .map(|n| world.spawn(Inventory::new(*n)).id())
        .collect();
    let menu = world
        .spawn((
            OpenMenu {
                def: def.clone(),
                state: MenuState::new(&def),
                inventories: inventories.clone(),
                id: MenuId(1),
                actor: Actor::CREATIVE,
            },
            slotted_ecs::Carried::default(),
            slotted_ecs::SlotEntities::default(),
        ))
        .id();
    app.update();

    app.world_mut().trigger(MenuAction {
        entity: menu,
        action: ClickAction::Give {
            item: common::item("minecraft:coal"),
            count: 7,
            target: GiveTarget::Inventory(MenuDef::PLAYER_MAIN),
        },
    });
    app.update();

    let main = inventories[MenuDef::PLAYER_MAIN.index()];
    let inventory = app.world().get::<Inventory>(main).expect("main inventory");
    assert_eq!(
        inventory.0.get(0).cloned(),
        Some(ItemStack::new(common::item("minecraft:coal"), 7)),
        "the give landed in the first empty slot"
    );
}
