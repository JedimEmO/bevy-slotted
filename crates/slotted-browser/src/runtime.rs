//! The `BrowserRuntime` resource sibling plugins read, and the `Apply`
//! systems that drain the UI's messages. Contract sections 5 and 6.
//! Package A, except `has_keyboard_focus`, which Package B writes.

use std::sync::Arc;

use bevy::input::keyboard::KeyCode;
use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::bookmarks::Bookmarks;
use crate::category::CategoryId;
use crate::events::{
    BookmarkToggled, GiveRequested, OpenRecipes, OpenUses, RecipeNav, SearchChanged,
    TransferFailed, TransferRequested,
};
use crate::index::{EntryId, HiddenEntries, IndexState};
use crate::ingredient::{Ingredient, IngredientCtx, IngredientTypes, Subtypes};
use crate::recipes::{Categories, RecipeStore};
use crate::search::{SearchCache, SearchConfig, evaluate, sort, tokenize};
use crate::transfer::{TransferCtx, TransferError, TransferErrorKind, TransferHandlers};

/// Recipes for, or uses of, the focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PageMode {
    /// Recipes that produce the focus.
    Recipes,
    /// Recipes that consume the focus.
    Uses,
}

/// One position in the recipe view's history.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecipePage {
    /// The open tab.
    pub category: CategoryId,
    /// What the player searched from.
    pub focus: Ingredient,
    /// Recipes or uses.
    pub mode: PageMode,
    /// Index into the category's recipe list for this focus.
    pub recipe: usize,
}

/// The hotkeys. Data only; Package B's `hotkeys.rs` reads them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyMappings {
    /// Show recipes for the hovered ingredient.
    pub recipes: KeyCode,
    /// Show uses of the hovered ingredient.
    pub uses: KeyCode,
    /// Toggle a bookmark on the hovered ingredient.
    pub bookmark: KeyCode,
    /// Focus the search field; `true` means Ctrl is held.
    pub focus_search: (bool, KeyCode),
    /// Recipe view back.
    pub back: KeyCode,
    /// Close the recipe view.
    pub close: KeyCode,
    /// Previous grid page.
    pub prev_page: KeyCode,
    /// Next grid page.
    pub next_page: KeyCode,
}

impl Default for KeyMappings {
    fn default() -> Self {
        Self {
            recipes: KeyCode::KeyR,
            uses: KeyCode::KeyU,
            bookmark: KeyCode::KeyA,
            focus_search: (true, KeyCode::KeyF),
            back: KeyCode::Backspace,
            close: KeyCode::Escape,
            prev_page: KeyCode::PageUp,
            next_page: KeyCode::PageDown,
        }
    }
}

/// What sibling plugins (storage terminals, HUDs, sorters) read to stay in
/// sync with the browser, and what the panel renders from.
#[derive(Resource, Debug, Clone, Default)]
pub struct BrowserRuntime {
    /// The current query.
    pub filter_text: String,
    /// The current result, in display order.
    pub visible: Arc<Vec<EntryId>>,
    /// Bumps whenever `Bookmarks` changes.
    pub bookmarks_version: u32,
    /// The hotkeys.
    pub keys: KeyMappings,
    /// True while any text field has `InputFocus`; hotkeys must not fire.
    /// Written by Package B every frame in `BrowserSet::Input`.
    pub has_keyboard_focus: bool,
    /// Pages behind the open one, oldest first.
    pub history: Vec<RecipePage>,
    /// Pages popped by `Back`, for `Forward`.
    pub forward: Vec<RecipePage>,
    /// The open recipe page, if the recipe view is showing.
    pub open: Option<RecipePage>,
    /// Bumps when visibility rules change (cheat mode, dev items).
    pub visibility_version: u32,
}

/// `BrowserSet::Apply`: re-evaluate the search when the field changes or the
/// index lands, and publish `visible`.
pub fn apply_search(
    mut changed: MessageReader<SearchChanged>,
    mut ready: MessageReader<crate::events::IndexReady>,
    index: Res<IndexState>,
    hidden: Res<HiddenEntries>,
    config: Res<SearchConfig>,
    mut cache: ResMut<SearchCache>,
    mut runtime: ResMut<BrowserRuntime>,
) {
    let mut dirty = ready.read().count() > 0;
    for SearchChanged { text } in changed.read() {
        if *text != runtime.filter_text {
            runtime.filter_text.clone_from(text);
            dirty = true;
        }
    }
    let Some(index) = index.ready() else {
        return;
    };
    if !dirty && !hidden.is_changed() {
        return;
    }
    let (query, hv, vv) = (
        runtime.filter_text.clone(),
        hidden.version,
        runtime.visibility_version,
    );
    let result = cache.get(&query, hv, vv).unwrap_or_else(|| {
        let tokens = tokenize(&query, &config);
        let set = evaluate(index, &tokens, &config, &hidden.set);
        let result = Arc::new(sort(index, &set, &config.sort));
        cache.put(&query, hv, vv, result.clone());
        result
    });
    runtime.visible = result;
}

/// The pages of one focus: every category with at least one recipe, in tab
/// order, with the recipes it holds.
fn pages_for(
    focus: &Ingredient,
    mode: PageMode,
    store: &RecipeStore,
    registries: &slotted_ecs::Registries,
    categories: &Categories,
) -> Vec<(CategoryId, Vec<crate::category::RecipeRef>)> {
    match mode {
        PageMode::Recipes => store.recipes_for(focus, registries, categories),
        PageMode::Uses => store.uses(focus, registries, categories),
    }
}

/// `BrowserSet::Apply`: open recipe pages and navigate history.
///
/// A page opens on the first category that has any recipe for the focus; a
/// focus nothing produces or consumes leaves the view as it was. `Page` walks
/// the recipes of the open category and wraps at both ends.
pub fn apply_navigation(
    mut recipes: MessageReader<OpenRecipes>,
    mut uses: MessageReader<OpenUses>,
    mut nav: MessageReader<RecipeNav>,
    mut runtime: ResMut<BrowserRuntime>,
    store: Res<RecipeStore>,
    categories: Res<Categories>,
    registries: Option<Res<slotted_ecs::Registries>>,
) {
    let Some(registries) = registries else { return };
    let open = |runtime: &mut BrowserRuntime, focus: Ingredient, mode: PageMode| {
        let pages = pages_for(&focus, mode, &store, &registries, &categories);
        let Some((category, _)) = pages.into_iter().next() else {
            tracing::debug!(?focus, ?mode, "no recipes to open");
            return;
        };
        if let Some(prev) = runtime.open.take() {
            runtime.history.push(prev);
        }
        runtime.forward.clear();
        runtime.open = Some(RecipePage {
            category,
            focus,
            mode,
            recipe: 0,
        });
    };
    for OpenRecipes(focus) in recipes.read() {
        open(&mut runtime, focus.clone(), PageMode::Recipes);
    }
    for OpenUses(focus) in uses.read() {
        open(&mut runtime, focus.clone(), PageMode::Uses);
    }
    for event in nav.read() {
        match event {
            RecipeNav::Back => {
                if let Some(open) = runtime.open.take() {
                    runtime.forward.push(open);
                }
                runtime.open = runtime.history.pop();
            }
            RecipeNav::Forward => {
                if let Some(next) = runtime.forward.pop() {
                    if let Some(open) = runtime.open.take() {
                        runtime.history.push(open);
                    }
                    runtime.open = Some(next);
                }
            }
            RecipeNav::Close => {
                if let Some(open) = runtime.open.take() {
                    runtime.history.push(open);
                }
            }
            RecipeNav::Page(delta) => {
                let Some(page) = runtime.open.as_mut() else {
                    continue;
                };
                let pages = pages_for(&page.focus, page.mode, &store, &registries, &categories);
                let len = pages
                    .iter()
                    .find(|(c, _)| *c == page.category)
                    .map_or(0, |(_, r)| r.len());
                if len == 0 {
                    continue;
                }
                let len = i64::try_from(len).unwrap_or(i64::MAX);
                let now = i64::try_from(page.recipe).unwrap_or(0);
                let next = (now + i64::from(*delta)).rem_euclid(len);
                page.recipe = usize::try_from(next).unwrap_or(0);
            }
            RecipeNav::Category(category) => {
                let Some(page) = runtime.open.as_mut() else {
                    continue;
                };
                let pages = pages_for(&page.focus, page.mode, &store, &registries, &categories);
                if pages.iter().any(|(c, _)| c == category) {
                    page.category = category.clone();
                    page.recipe = 0;
                }
            }
        }
    }
}

/// `BrowserSet::Apply`: toggle bookmarks.
pub fn apply_bookmarks(
    mut toggled: MessageReader<BookmarkToggled>,
    mut bookmarks: ResMut<Bookmarks>,
    mut runtime: ResMut<BrowserRuntime>,
) {
    for BookmarkToggled(bookmark) in toggled.read() {
        bookmarks.toggle(bookmark.clone());
        runtime.bookmarks_version = runtime.bookmarks_version.wrapping_add(1);
    }
}

/// The open game screen and its menu: the first screen root that drives a
/// menu and is not the browser's own panel.
fn active_menu(
    screens: &Query<&slotted_ui::ScreenRoot>,
) -> Option<(slotted_ui::ScreenKind, Entity)> {
    let panel = slotted_ui::ScreenKind(
        slotted_model::Namespaced::new("slotted", "browser").expect("static id"),
    );
    screens
        .iter()
        .filter(|root| root.kind != panel)
        .find_map(|root| root.menu.map(|menu| (root.kind.clone(), menu)))
}

/// Clones the menu's component inventories into a model `Inventories`.
fn gather(
    menu: &slotted_ecs::OpenMenu,
    inventories: &Query<&slotted_ecs::Inventory>,
) -> Option<slotted_model::Inventories> {
    let mut out = slotted_model::Inventories::new();
    for entity in &menu.inventories {
        out.push(inventories.get(*entity).ok()?.0.clone());
    }
    Some(out)
}

/// `BrowserSet::Apply`: plan transfers through the handler and trigger the
/// resulting `MenuAction`s, or write `TransferFailed`.
///
/// The plan is a sequence of ordinary clicks, so prediction, the authority and
/// item conservation see nothing unusual.
#[allow(clippy::too_many_arguments)]
pub fn execute_transfers(
    mut requests: MessageReader<TransferRequested>,
    mut failed: MessageWriter<TransferFailed>,
    mut commands: Commands,
    runtime: Res<BrowserRuntime>,
    store: Res<RecipeStore>,
    categories: Res<Categories>,
    handlers: Res<TransferHandlers>,
    types: Res<IngredientTypes>,
    subtypes: Res<Subtypes>,
    loc: Res<slotted_ui::Localization>,
    registries: Option<Res<slotted_ecs::Registries>>,
    screens: Query<&slotted_ui::ScreenRoot>,
    menus: Query<&slotted_ecs::OpenMenu>,
    inventories: Query<&slotted_ecs::Inventory>,
) {
    let requests: Vec<TransferRequested> = requests.read().copied().collect();
    if requests.is_empty() {
        return;
    }
    let Some(registries) = registries else { return };
    for request in requests {
        let error = |kind, message: &str| TransferFailed {
            recipe: request.recipe,
            error: TransferError::new(kind, message),
        };
        let Some((kind, menu_entity)) = active_menu(&screens) else {
            failed.write(error(TransferErrorKind::NoHandler, "no open menu"));
            continue;
        };
        let Ok(menu) = menus.get(menu_entity) else {
            failed.write(error(TransferErrorKind::NoHandler, "no open menu"));
            continue;
        };
        let Some(category) = store.category_of(request.recipe).cloned() else {
            failed.write(error(
                TransferErrorKind::NoHandler,
                "recipe has no category",
            ));
            continue;
        };
        let Some(handler) = handlers.get(&kind, &category) else {
            failed.write(error(
                TransferErrorKind::NoHandler,
                "this screen cannot take a recipe",
            ));
            continue;
        };
        let focus = runtime.open.as_ref().map(|p| &p.focus);
        let Some(layout) = store.layout(request.recipe, focus, &registries, &categories, &types)
        else {
            failed.write(error(TransferErrorKind::NoHandler, "recipe has no layout"));
            continue;
        };
        let Some(invs) = gather(menu, &inventories) else {
            failed.write(error(
                TransferErrorKind::NoHandler,
                "menu is missing an inventory",
            ));
            continue;
        };
        let lookup = registries.lookup();
        let ctx = TransferCtx {
            def: &menu.def,
            inventories: &invs,
            state: &menu.state,
            actor: menu.actor,
            lookup: &lookup,
            types: &types,
            ctx: IngredientCtx {
                registries: &registries,
                subtypes: &subtypes,
                loc: &loc,
            },
        };
        match handler.plan(&ctx, &layout, request.max) {
            Ok(plan) => {
                for action in plan.actions {
                    commands.trigger(slotted_ecs::MenuAction {
                        entity: menu_entity,
                        action,
                    });
                }
            }
            Err(error) => {
                failed.write(TransferFailed {
                    recipe: request.recipe,
                    error,
                });
            }
        }
    }
}

/// `BrowserSet::Apply`: cheat-give through `MenuAction { Give }`.
///
/// The permission check lives in `apply_click`, which refuses a `Give` from an
/// actor without `can_cheat`; this only avoids triggering an action that is
/// certain to be refused, and says so in the log.
pub fn execute_gives(
    mut requests: MessageReader<GiveRequested>,
    mut commands: Commands,
    types: Res<IngredientTypes>,
    screens: Query<&slotted_ui::ScreenRoot>,
    menus: Query<&slotted_ecs::OpenMenu>,
) {
    for request in requests.read() {
        let Some((_, menu_entity)) = active_menu(&screens) else {
            tracing::debug!("cheat-give with no open menu");
            continue;
        };
        let Ok(menu) = menus.get(menu_entity) else {
            continue;
        };
        if !menu.actor.can_cheat {
            tracing::debug!(?request, "cheat-give refused: the actor may not cheat");
            continue;
        }
        let Some(stack) = types
            .get(&request.ingredient.ty)
            .and_then(|ty| ty.as_stack(&request.ingredient, request.count))
        else {
            tracing::debug!(?request, "cheat-give: this ingredient has no stack");
            continue;
        };
        commands.trigger(slotted_ecs::MenuAction {
            entity: menu_entity,
            action: slotted_model::ClickAction::Give {
                item: stack.id,
                count: stack.count,
                target: request.target,
            },
        });
    }
}
