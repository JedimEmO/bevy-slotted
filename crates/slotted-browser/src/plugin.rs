//! `SlottedBrowserPlugin`: system sets, registration phases, resources and
//! messages. Architect-owned; amend the contract before changing it.

use bevy::prelude::*;
use bevy::ui::UiSystems;
use slotted_ecs::SlottedEcsSet;
use slotted_ui::SlottedUiSet;

use crate::bookmarks::Bookmarks;
use crate::events::{
    BookmarkToggled, GhostHint, GiveRequested, IndexReady, OpenRecipes, OpenUses, RebuildBrowser,
    RecipeNav, SearchChanged, TransferFailed, TransferRequested,
};
use crate::handlers::ScreenHandlers;
use crate::index::build::{poll_index_build, start_index_build};
use crate::index::{HiddenEntries, IndexState};
use crate::ingredient::{InfoPages, IngredientTypes, Subtypes};
use crate::recipes::{Categories, RecipeStore};
use crate::runtime::{
    BrowserRuntime, apply_bookmarks, apply_navigation, apply_search, execute_gives,
    execute_transfers,
};
use crate::search::{SearchCache, SearchConfig};
use crate::transfer::TransferHandlers;
use crate::ui;
use crate::validate::{
    BrowserValidation, validate_categories, validate_ingredient_types, validate_recipes,
    validate_screen_handlers, validate_subtypes, validate_transfer,
};

/// Per-frame sets. See contract section 0 for where each runs.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BrowserSet {
    /// B: hotkeys, keyboard-focus flag, search field diff. Inside `SlottedUiSet::Input`.
    Input,
    /// A: drain messages, evaluate search, plan transfers, trigger actions.
    /// After `SlottedUiSet::Input`, before `SlottedEcsSet::Input`.
    Apply,
    /// A: poll the index build task. Inside `SlottedUiSet::Render`.
    Index,
    /// B: rebind cards, cycle alternatives, button states. Inside `SlottedUiSet::Render`.
    Render,
    /// B: docking. `PostUpdate`, after `SlottedUiSet::Layout`.
    Layout,
}

/// Ordered registration phases in `Startup`. Register into the phase's
/// resource from a system `in_set(BrowserPhase::X)`; the last system of each
/// phase validates against the earlier ones.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BrowserPhase {
    /// `Subtypes`.
    Subtypes,
    /// `IngredientTypes`.
    IngredientTypes,
    /// `Categories`.
    Categories,
    /// Extra recipes (Phase 6); `RecipeStore` is built at `Runtime`.
    Recipes,
    /// `TransferHandlers`.
    Transfer,
    /// `ScreenHandlers`.
    ScreenHandlers,
    /// `RecipeStore`, `BrowserRuntime`, index build start.
    Runtime,
}

impl BrowserPhase {
    /// All phases in order.
    pub const ALL: [Self; 7] = [
        Self::Subtypes,
        Self::IngredientTypes,
        Self::Categories,
        Self::Recipes,
        Self::Transfer,
        Self::ScreenHandlers,
        Self::Runtime,
    ];
}

/// Which screens get a panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum AttachPolicy {
    /// Only screen kinds with a registered `ScreenHandler`.
    #[default]
    Registered,
    /// Every screen root with a menu, through `DefaultScreenHandler`.
    AllMenus,
}

/// Plugin configuration.
#[derive(Resource, Debug, Clone, PartialEq, Eq)]
pub struct BrowserConfig {
    /// Which screens get a panel.
    pub attach: AttachPolicy,
    /// Panic on a validation error instead of dropping the entry.
    pub strict: bool,
    /// Spawn panels at all. Off for a dedicated server.
    pub ui: bool,
}

impl Default for BrowserConfig {
    fn default() -> Self {
        Self {
            attach: AttachPolicy::Registered,
            strict: cfg!(debug_assertions),
            ui: true,
        }
    }
}

/// The browser plugin. Requires `SlottedUiPlugin` (and through it the ecs
/// plugin) to be added first.
#[derive(Debug, Clone, Default)]
pub struct SlottedBrowserPlugin {
    /// Configuration.
    pub config: BrowserConfig,
}

impl Plugin for SlottedBrowserPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(self.config.clone())
            .insert_resource(IngredientTypes::with_builtins())
            // The browser draws names, so it needs the localisation port to
            // exist whether or not `SlottedUiPlugin` was added; both calls are
            // `init_resource`, so whichever runs first wins and the other is a
            // no-op.
            .init_resource::<slotted_ui::Localization>()
            .init_resource::<Subtypes>()
            .init_resource::<InfoPages>()
            .init_resource::<Categories>()
            .init_resource::<RecipeStore>()
            .init_resource::<TransferHandlers>()
            .init_resource::<ScreenHandlers>()
            .init_resource::<IndexState>()
            .init_resource::<HiddenEntries>()
            .init_resource::<SearchConfig>()
            .init_resource::<SearchCache>()
            .init_resource::<Bookmarks>()
            .init_resource::<BrowserRuntime>()
            .init_resource::<BrowserValidation>()
            .add_message::<SearchChanged>()
            .add_message::<IndexReady>()
            .add_message::<OpenRecipes>()
            .add_message::<OpenUses>()
            .add_message::<RecipeNav>()
            .add_message::<TransferRequested>()
            .add_message::<TransferFailed>()
            .add_message::<BookmarkToggled>()
            .add_message::<GiveRequested>()
            .add_message::<RebuildBrowser>()
            .add_message::<GhostHint>();

        app.configure_sets(
            Startup,
            (
                BrowserPhase::Subtypes,
                BrowserPhase::IngredientTypes,
                BrowserPhase::Categories,
                BrowserPhase::Recipes,
                BrowserPhase::Transfer,
                BrowserPhase::ScreenHandlers,
                BrowserPhase::Runtime,
            )
                .chain(),
        )
        .add_systems(
            Startup,
            (
                validate_subtypes.in_set(BrowserPhase::Subtypes),
                validate_ingredient_types.in_set(BrowserPhase::IngredientTypes),
                validate_categories.in_set(BrowserPhase::Categories),
                validate_recipes.in_set(BrowserPhase::Recipes),
                validate_transfer.in_set(BrowserPhase::Transfer),
                validate_screen_handlers.in_set(BrowserPhase::ScreenHandlers),
                (build_recipe_store, start_index_build)
                    .chain()
                    .in_set(BrowserPhase::Runtime),
            ),
        );

        app.configure_sets(
            Update,
            (
                BrowserSet::Input.in_set(SlottedUiSet::Input),
                BrowserSet::Apply
                    .after(SlottedUiSet::Input)
                    .before(SlottedEcsSet::Input),
                BrowserSet::Index.in_set(SlottedUiSet::Render),
                BrowserSet::Render
                    .in_set(SlottedUiSet::Render)
                    .after(BrowserSet::Index),
            ),
        )
        .configure_sets(
            PostUpdate,
            BrowserSet::Layout
                .after(SlottedUiSet::Layout)
                .after(UiSystems::Layout),
        )
        .add_systems(
            Update,
            (
                (
                    apply_search,
                    apply_navigation,
                    apply_bookmarks,
                    execute_transfers,
                    execute_gives,
                    rebuild_on_request,
                )
                    .chain()
                    .in_set(BrowserSet::Apply),
                poll_index_build.in_set(BrowserSet::Index),
            ),
        );

        if self.config.ui {
            ui::register(app);
            // The panel's own chrome. It runs after everything else in
            // `Render`, so a chip or a tab spawned this pass is repainted in
            // the same frame it is laid out, and a replaced `Localization`
            // repaints every label that was written once at spawn time.
            app.add_systems(
                Update,
                ui::panel::render_chrome
                    .in_set(BrowserSet::Render)
                    .after(ui::recipe_view::render_recipe_view),
            );
        }
    }
}

/// `BrowserPhase::Runtime`: file every registry recipe under its category.
pub fn build_recipe_store(
    registries: Option<Res<slotted_ecs::Registries>>,
    categories: Res<Categories>,
    mut store: ResMut<RecipeStore>,
) {
    if let Some(registries) = registries {
        *store = RecipeStore::build(&registries, &categories);
    }
}

/// `BrowserSet::Apply`: a `RebuildBrowser` message restarts the index build.
///
/// A rebuild follows a freeze, so the registries the `Startup` phase validated
/// are not the ones in the world any more: a mod's hot reload can add a recipe
/// type that has no category yet, or remove the item an interpreter was
/// registered for. Every phase validator therefore runs again, in phase order,
/// before the store and the index are rebuilt. They are idempotent; the only
/// one that writes is `validate_categories`, which binds a default category to
/// each unclaimed recipe type.
pub fn rebuild_on_request(world: &mut World) {
    let requested = world
        .resource_mut::<Messages<RebuildBrowser>>()
        .drain()
        .count()
        > 0;
    if requested {
        revalidate(world);
        build_recipe_store_world(world);
        start_index_build(world);
    }
}

/// Re-runs the `Startup` phase validators against the current registries.
fn revalidate(world: &mut World) {
    // Errors from the previous registries are stale; a validator that still
    // disagrees reports again.
    if let Some(mut validation) = world.get_resource_mut::<BrowserValidation>() {
        validation.0.clear();
    }
    let _ = world.run_system_cached(validate_subtypes);
    let _ = world.run_system_cached(validate_ingredient_types);
    let _ = world.run_system_cached(validate_categories);
    let _ = world.run_system_cached(validate_recipes);
    let _ = world.run_system_cached(validate_transfer);
    let _ = world.run_system_cached(validate_screen_handlers);
}

fn build_recipe_store_world(world: &mut World) {
    let Some(registries) = world.get_resource::<slotted_ecs::Registries>() else {
        return;
    };
    let store = RecipeStore::build(registries, world.resource::<Categories>());
    world.insert_resource(store);
}
