//! Messages between the logic package (A) and the UI package (B).
//!
//! These are buffered `Message`s rather than entity events because neither
//! side owns an entity the other one knows about. Section 6 of the contract
//! lists writer and reader for each; this file is architect-owned.

use bevy::prelude::*;
use slotted_model::{GiveTarget, SlotIx};

use crate::bookmarks::Bookmark;
use crate::category::{CategoryId, RecipeRef};
use crate::ingredient::Ingredient;
use crate::transfer::TransferError;

/// B -> A: the search field's value differs from `BrowserRuntime::filter_text`.
#[derive(Message, Debug, Clone, PartialEq, Eq)]
pub struct SearchChanged {
    /// The new query text, verbatim.
    pub text: String,
}

/// A -> B: `IndexState` became `Ready`.
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct IndexReady {
    /// How many entries the index holds.
    pub entries: usize,
}

/// B -> A: show the recipes that produce this ingredient.
#[derive(Message, Debug, Clone, PartialEq, Eq)]
pub struct OpenRecipes(pub Ingredient);

/// B -> A: show the recipes that consume this ingredient.
#[derive(Message, Debug, Clone, PartialEq, Eq)]
pub struct OpenUses(pub Ingredient);

/// B -> A: navigate the recipe view.
#[derive(Message, Debug, Clone, PartialEq, Eq)]
pub enum RecipeNav {
    /// Pop one history entry.
    Back,
    /// Redo one popped entry.
    Forward,
    /// Close the recipe view, keeping history.
    Close,
    /// Move `n` recipes within the current category (negative goes back).
    Page(i32),
    /// Switch to another category tab for the same focus.
    Category(CategoryId),
}

/// B -> A: fill the open screen's grid from this recipe.
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransferRequested {
    /// Which recipe.
    pub recipe: RecipeRef,
    /// Shift-click: as many sets as fit.
    pub max: bool,
}

/// A -> B: a requested transfer was refused; paint `error.missing` red.
#[derive(Message, Debug, Clone, PartialEq, Eq)]
pub struct TransferFailed {
    /// Which recipe.
    pub recipe: RecipeRef,
    /// Why.
    pub error: TransferError,
}

/// B -> A: add or remove a bookmark.
#[derive(Message, Debug, Clone, PartialEq, Eq)]
pub struct BookmarkToggled(pub Bookmark);

/// B -> A: cheat-give. A resolves the stack and triggers
/// `MenuAction { Give }` only when the menu's actor may cheat.
#[derive(Message, Debug, Clone, PartialEq, Eq)]
pub struct GiveRequested {
    /// What to give.
    pub ingredient: Ingredient,
    /// How many; capped by the item's max stack size.
    pub count: u32,
    /// Cursor or an inventory.
    pub target: GiveTarget,
}

/// Anyone -> A: re-run the registration phases and rebuild the index.
#[derive(Message, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RebuildBrowser;

/// A -> ecs (Phase 6): set a ghost or filter slot's hint without an item.
/// Defined now so the default ghost-drop path has a destination; nothing
/// consumes it in Phase 3.
#[derive(Message, Debug, Clone, PartialEq, Eq)]
pub struct GhostHint {
    /// The menu entity.
    pub menu: Entity,
    /// The ghost or filter slot.
    pub slot: SlotIx,
    /// The hint; `None` clears it.
    pub hint: Option<Ingredient>,
}
