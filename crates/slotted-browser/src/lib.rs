//! The item and recipe browser: a JEI-style overlay for any slotted screen.
//!
//! Everything is registered in ordered phases at `Startup`
//! ([`BrowserPhase`]), indexed off the main thread ([`index`]), searched with
//! a prefix grammar ([`search`]) and shown in a panel docked beside the open
//! screen ([`ui`]). Transfers and cheat-gives never touch inventories
//! directly; they trigger `slotted_ecs::MenuAction`s so prediction and the
//! authority see ordinary clicks. See `docs/design/phase3-contract.md`.
//!
//! Package split for Phase 3: the logic modules ([`ingredient`], [`category`],
//! [`recipes`], [`index`], [`search`], [`bookmarks`], [`transfer`],
//! [`handlers`], [`runtime`], [`validate`]) contain no `bevy_ui`; the [`ui`]
//! module is the only one that spawns nodes.

pub mod bookmarks;
pub mod category;
pub mod events;
pub mod handlers;
pub mod index;
pub mod ingredient;
pub mod plugin;
pub mod recipes;
pub mod runtime;
pub mod search;
pub mod transfer;
pub mod ui;
pub mod validate;

pub use bookmarks::{Bookmark, Bookmarks};
pub use category::{
    CategoryId, CraftingCategory, LayoutBuilder, ProcessingCategory, RecipeCategory, RecipeLayout,
    RecipeRef, RecipeSlot, RecipeSlotIx, RecipeView, SlotRole,
};
pub use events::{
    BookmarkToggled, GhostHint, GiveRequested, IndexReady, OpenRecipes, OpenUses, RebuildBrowser,
    RecipeNav, SearchChanged, TransferFailed, TransferRequested,
};
pub use handlers::{
    ClickableArea, DefaultScreenHandler, GhostDrop, ScreenGeometry, ScreenHandler, ScreenHandlers,
};
pub use index::{BrowserIndex, Entry, EntryId, HiddenEntries, IndexState};
pub use ingredient::{
    FluidType, InfoPage, InfoPages, InfoType, Ingredient, IngredientCtx, IngredientType,
    IngredientTypeId, IngredientTypes, IngredientValue, ItemType, SubtypeInterpreter, SubtypeKey,
    Subtypes, TagType, types,
};
pub use plugin::{AttachPolicy, BrowserConfig, BrowserPhase, BrowserSet, SlottedBrowserPlugin};
pub use recipes::{Categories, RecipeStore};
pub use runtime::{BrowserRuntime, KeyMappings, PageMode, RecipePage};
pub use search::{
    Field, PrefixMode, SearchCache, SearchConfig, SortStage, Token, evaluate, sort, tokenize,
};
pub use transfer::{
    SimpleTransfer, TransferCtx, TransferError, TransferErrorKind, TransferHandler,
    TransferHandlers, TransferPlan,
};
pub use ui::{BrowserLayout, BrowserPanel, Card, CardGrid, panel_def};
pub use validate::{BrowserValidation, ValidationError};

/// The names a consumer needs to wire the browser and register into it.
pub mod prelude {
    pub use crate::{
        AttachPolicy, Bookmark, Bookmarks, BrowserConfig, BrowserPhase, BrowserRuntime, Categories,
        CategoryId, DefaultScreenHandler, Ingredient, IngredientType, IngredientTypes,
        LayoutBuilder, RecipeCategory, RecipeRef, RecipeView, ScreenHandler, ScreenHandlers,
        SimpleTransfer, SlotRole, SlottedBrowserPlugin, TransferHandler, TransferHandlers,
    };
}
