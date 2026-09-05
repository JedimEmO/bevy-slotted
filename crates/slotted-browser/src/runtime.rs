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
use crate::ingredient::Ingredient;
use crate::search::{SearchCache, SearchConfig, evaluate, sort, tokenize};

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

/// `BrowserSet::Apply`: open recipe pages and navigate history.
pub fn apply_navigation(
    mut recipes: MessageReader<OpenRecipes>,
    mut uses: MessageReader<OpenUses>,
    mut nav: MessageReader<RecipeNav>,
    mut runtime: ResMut<BrowserRuntime>,
) {
    // PHASE3-IMPL: A — pick the first category with results via RecipeStore;
    // Page/Category navigation. The skeleton records history and open/close.
    let mut open = |focus: Ingredient, mode: PageMode| {
        if let Some(prev) = runtime.open.take() {
            runtime.history.push(prev);
        }
        runtime.forward.clear();
        runtime.open = Some(RecipePage {
            category: CategoryId::new("slotted:crafting"),
            focus,
            mode,
            recipe: 0,
        });
    };
    for OpenRecipes(focus) in recipes.read() {
        open(focus.clone(), PageMode::Recipes);
    }
    for OpenUses(focus) in uses.read() {
        open(focus.clone(), PageMode::Uses);
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
            RecipeNav::Page(_) | RecipeNav::Category(_) => {}
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

/// `BrowserSet::Apply`: plan transfers through the handler and trigger the
/// resulting `MenuAction`s, or write `TransferFailed`.
pub fn execute_transfers(
    mut requests: MessageReader<TransferRequested>,
    mut failed: MessageWriter<TransferFailed>,
) {
    // PHASE3-IMPL: A — assemble `TransferCtx` from the open screen's menu,
    // look the handler up by (ScreenKind, category), trigger the plan.
    for request in requests.read() {
        failed.write(TransferFailed {
            recipe: request.recipe,
            error: crate::transfer::TransferError::new(
                crate::transfer::TransferErrorKind::NoHandler,
                "transfer is not implemented yet",
            ),
        });
    }
}

/// `BrowserSet::Apply`: cheat-give through `MenuAction { Give }` when the
/// menu's actor may cheat.
pub fn execute_gives(mut requests: MessageReader<GiveRequested>) {
    // PHASE3-IMPL: A — resolve `as_stack`, find the active menu, check
    // `OpenMenu::actor.can_cheat`, `commands.trigger(MenuAction { Give })`.
    for request in requests.read() {
        tracing::debug!(?request, "give requested; not implemented yet");
    }
}
