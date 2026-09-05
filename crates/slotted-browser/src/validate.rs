//! Cross-reference validation at every phase boundary. Contract section 2.
//! Package A.

use bevy::prelude::*;
use slotted_model::Namespaced;
use slotted_ui::ScreenKind;

use crate::category::CategoryId;
use crate::handlers::ScreenHandlers;
use crate::ingredient::{IngredientTypeId, IngredientTypes, Subtypes};
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

/// Last system of `BrowserPhase::Subtypes`: an interpreter registered for an
/// item the registries do not have would silently never run, so it is dropped.
pub fn validate_subtypes(
    registries: Option<Res<slotted_ecs::Registries>>,
    mut subtypes: ResMut<Subtypes>,
    config: Res<BrowserConfig>,
    mut validation: ResMut<BrowserValidation>,
) {
    let Some(registries) = registries else { return };
    let dangling: Vec<Namespaced> = subtypes
        .items()
        .filter(|item| !registries.items.contains(item))
        .cloned()
        .collect();
    for item in dangling {
        subtypes.remove(&item);
        validation.report(ValidationError::UnknownItem(item), &config);
    }
}

/// Last system of `BrowserPhase::IngredientTypes`: every ingredient type an
/// `ingredient_types` registry entry names must have been registered in Rust.
/// Duplicate ids are collapsed by `IngredientTypes::register` itself.
pub fn validate_ingredient_types(
    registries: Option<Res<slotted_ecs::Registries>>,
    types: Res<IngredientTypes>,
    config: Res<BrowserConfig>,
    mut validation: ResMut<BrowserValidation>,
) {
    let Some(registries) = registries else { return };
    let missing: Vec<IngredientTypeId> = registries
        .ingredient_types
        .iter()
        .map(|(_, name, _)| IngredientTypeId(name.clone()))
        .filter(|id| types.get(id).is_none())
        .collect();
    for id in missing {
        validation.report(ValidationError::UnknownIngredientType(id), &config);
    }
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

/// Last system of `BrowserPhase::Recipes`: a recipe type no category claims
/// would hide every recipe of that type. `Categories::bind_defaults` has
/// already run, so this only fires when a default was refused.
pub fn validate_recipes(
    registries: Option<Res<slotted_ecs::Registries>>,
    categories: Res<Categories>,
    config: Res<BrowserConfig>,
    mut validation: ResMut<BrowserValidation>,
) {
    let Some(registries) = registries else { return };
    let unbound: Vec<Namespaced> = registries
        .recipe_types
        .iter()
        .map(|(_, name, _)| name.clone())
        .filter(|name| categories.for_recipe_type(name).is_none())
        .collect();
    for name in unbound {
        validation.report(ValidationError::UnboundRecipeType(name), &config);
    }
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

/// Last system of `BrowserPhase::ScreenHandlers`: a handler for a screen kind
/// nobody registered can never attach, so it is dropped.
///
/// An app with no screens at all (a headless test, a dedicated server) is not
/// an error, so an empty `Screens` skips the check entirely.
pub fn validate_screen_handlers(
    screens: Option<Res<slotted_ui::Screens>>,
    mut handlers: ResMut<ScreenHandlers>,
    config: Res<BrowserConfig>,
    mut validation: ResMut<BrowserValidation>,
) {
    let Some(screens) = screens else { return };
    if screens.0.is_empty() {
        return;
    }
    let unknown: Vec<ScreenKind> = handlers
        .kinds()
        .filter(|kind| screens.get(kind).is_none())
        .cloned()
        .collect();
    for kind in unknown {
        handlers.remove(&kind);
        validation.report(ValidationError::UnknownScreenKind(kind), &config);
    }
}
