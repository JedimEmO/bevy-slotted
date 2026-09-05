//! Cross-reference validation at every phase boundary. Contract section 2.
//! Package A.

use bevy::prelude::*;
use slotted_model::Namespaced;
use slotted_ui::ScreenKind;

use crate::category::CategoryId;
use crate::ingredient::IngredientTypeId;
use crate::plugin::BrowserConfig;
use crate::recipes::Categories;
use crate::transfer::TransferHandlers;

/// One thing a registration got wrong. The entry is removed; `strict` panics.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ValidationError {
    /// A subtype interpreter for an item the registries do not have.
    #[error("subtype interpreter for unknown item {0}")]
    UnknownItem(Namespaced),
    /// A category claims a recipe type the registries do not have.
    #[error("category {category:?} claims unknown recipe type {recipe_type}")]
    UnknownRecipeType {
        /// The category.
        category: CategoryId,
        /// The missing type.
        recipe_type: Namespaced,
    },
    /// Recipes of this type have no category and will not be shown.
    #[error("no category for recipe type {0}")]
    UnboundRecipeType(Namespaced),
    /// A transfer handler names a category nobody registered.
    #[error("transfer handler for {screen:?} names unknown category {category:?}")]
    UnknownCategory {
        /// The screen.
        screen: ScreenKind,
        /// The missing category.
        category: CategoryId,
    },
    /// A screen handler for a kind the `Screens` resource does not know.
    #[error("screen handler for unregistered screen kind {0:?}")]
    UnknownScreenKind(ScreenKind),
    /// An ingredient references an unregistered type.
    #[error("unknown ingredient type {0:?}")]
    UnknownIngredientType(IngredientTypeId),
}

/// Everything validation found, in phase order.
#[derive(Resource, Debug, Default, Clone, PartialEq, Eq)]
pub struct BrowserValidation(pub Vec<ValidationError>);

impl BrowserValidation {
    /// Records an error, logging it, or panics under `strict`.
    pub fn report(&mut self, error: ValidationError, config: &BrowserConfig) {
        assert!(!config.strict, "browser registration error: {error}");
        tracing::error!(%error, "browser registration");
        self.0.push(error);
    }
}

/// Last system of `BrowserPhase::Subtypes`.
pub fn validate_subtypes(_config: Res<BrowserConfig>, _validation: ResMut<BrowserValidation>) {
    // PHASE3-IMPL: A — `UnknownItem` for every registered interpreter.
}

/// Last system of `BrowserPhase::IngredientTypes`.
pub fn validate_ingredient_types() {
    // PHASE3-IMPL: A — duplicate ids are already collapsed by `register`.
}

/// Last system of `BrowserPhase::Categories`: binds defaults for unclaimed
/// recipe types, then reports categories claiming unknown types.
pub fn validate_categories(
    registries: Option<Res<slotted_ecs::Registries>>,
    mut categories: ResMut<Categories>,
    config: Res<BrowserConfig>,
    mut validation: ResMut<BrowserValidation>,
) {
    let Some(registries) = registries else { return };
    categories.bind_defaults(&registries);
    let bad: Vec<(CategoryId, Namespaced)> = categories
        .iter()
        .flat_map(|c| {
            c.recipe_types()
                .into_iter()
                .filter(|t| !registries.recipe_types.contains(t))
                .map(move |t| (c.id(), t))
        })
        .collect();
    for (category, recipe_type) in bad {
        validation.report(
            ValidationError::UnknownRecipeType {
                category,
                recipe_type,
            },
            &config,
        );
    }
}

/// Last system of `BrowserPhase::Recipes`.
pub fn validate_recipes() {
    // PHASE3-IMPL: A — `UnboundRecipeType` for every type without a category.
}

/// Last system of `BrowserPhase::Transfer`.
pub fn validate_transfer(
    categories: Res<Categories>,
    mut handlers: ResMut<TransferHandlers>,
    config: Res<BrowserConfig>,
    mut validation: ResMut<BrowserValidation>,
) {
    let bad: Vec<(ScreenKind, CategoryId)> = handlers
        .keys()
        .filter_map(|(screen, category)| {
            let category = category.clone()?;
            (categories.get(&category).is_none()).then(|| (screen.clone(), category))
        })
        .collect();
    for (screen, category) in bad {
        handlers.remove(&screen, Some(&category));
        validation.report(
            ValidationError::UnknownCategory { screen, category },
            &config,
        );
    }
}

/// Last system of `BrowserPhase::ScreenHandlers`.
pub fn validate_screen_handlers() {
    // PHASE3-IMPL: A — `UnknownScreenKind` against `slotted_ui::Screens`.
}
