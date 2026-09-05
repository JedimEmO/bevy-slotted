//! The category registry and the recipe store built from the frozen
//! registries at the `Runtime` phase. Contract section 3. Package A.

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use bevy::prelude::Resource;
use slotted_model::Namespaced;
use slotted_registry::FrozenRegistries;

use crate::category::{
    CategoryId, CraftingCategory, LayoutBuilder, ProcessingCategory, RecipeCategory, RecipeLayout,
    RecipeRef, RecipeView,
};
use crate::ingredient::{Ingredient, IngredientTypes};

/// Registered categories, in tab order. Filled in
/// [`BrowserPhase::Categories`](crate::BrowserPhase::Categories).
#[derive(Resource, Default, Clone)]
pub struct Categories {
    ordered: Vec<Arc<dyn RecipeCategory>>,
    by_type: BTreeMap<Namespaced, CategoryId>,
}

impl Categories {
    /// Registers a category; the same id replaces in place. Every recipe
    /// type it claims is bound to it.
    pub fn register(&mut self, category: Arc<dyn RecipeCategory>) {
        let id = category.id();
        for ty in category.recipe_types() {
            self.by_type.insert(ty, id.clone());
        }
        match self.ordered.iter().position(|c| c.id() == id) {
            Some(i) => self.ordered[i] = category,
            None => self.ordered.push(category),
        }
    }

    /// The category by id.
    pub fn get(&self, id: &CategoryId) -> Option<&Arc<dyn RecipeCategory>> {
        self.ordered.iter().find(|c| &c.id() == id)
    }

    /// The category bound to a registry recipe type.
    pub fn for_recipe_type(&self, ty: &Namespaced) -> Option<&Arc<dyn RecipeCategory>> {
        self.by_type.get(ty).and_then(|id| self.get(id))
    }

    /// Categories in registration order (tab order).
    pub fn iter(&self) -> impl ExactSizeIterator<Item = &Arc<dyn RecipeCategory>> {
        self.ordered.iter()
    }

    /// Removes a category, for validation.
    pub fn remove(&mut self, id: &CategoryId) -> Option<Arc<dyn RecipeCategory>> {
        self.by_type.retain(|_, c| c != id);
        let i = self.ordered.iter().position(|c| &c.id() == id)?;
        Some(self.ordered.remove(i))
    }

    /// Binds every registry recipe type no category claims to a default:
    /// [`CraftingCategory`] when its size is not `(1, 1)`, else
    /// [`ProcessingCategory`], both under the recipe type's own id.
    pub fn bind_defaults(&mut self, registries: &FrozenRegistries) {
        for (_, name, def) in registries.recipe_types.iter() {
            if self.by_type.contains_key(name) {
                continue;
            }
            let id = CategoryId(name.clone());
            let category: Arc<dyn RecipeCategory> = if def.size == (1, 1) {
                Arc::new(ProcessingCategory::new(id, name.clone()))
            } else {
                Arc::new(CraftingCategory::new(id, name.clone(), def.size))
            };
            self.register(category);
        }
    }
}

/// Recipes grouped by category, with the two lookups the recipe view needs.
/// Built once at `Runtime` from `FrozenRegistries.recipes`.
#[derive(Resource, Debug, Default, Clone)]
pub struct RecipeStore {
    by_category: BTreeMap<CategoryId, Vec<RecipeRef>>,
    category_of: HashMap<RecipeRef, CategoryId>,
}

impl RecipeStore {
    /// Groups every registry recipe under the category bound to its type.
    /// Recipes of an unbound type are skipped (validation reported them).
    pub fn build(registries: &FrozenRegistries, categories: &Categories) -> Self {
        let mut store = Self::default();
        for (id, _, def) in registries.recipes.iter() {
            let Some(category) = categories.for_recipe_type(&def.recipe_type) else {
                continue;
            };
            let cid = category.id();
            store
                .by_category
                .entry(cid.clone())
                .or_default()
                .push(RecipeRef(id));
            store.category_of.insert(RecipeRef(id), cid);
        }
        store
    }

    /// Recipes in a category, registration order.
    pub fn in_category(&self, id: &CategoryId) -> &[RecipeRef] {
        self.by_category.get(id).map_or(&[], Vec::as_slice)
    }

    /// The category a recipe was filed under.
    pub fn category_of(&self, recipe: RecipeRef) -> Option<&CategoryId> {
        self.category_of.get(&recipe)
    }

    /// Recipes whose output is this ingredient (subtype ignored), grouped by
    /// category in tab order.
    pub fn recipes_for(
        &self,
        ingredient: &Ingredient,
        registries: &FrozenRegistries,
        categories: &Categories,
    ) -> Vec<(CategoryId, Vec<RecipeRef>)> {
        let Some(item) = ingredient.item_id() else {
            return Vec::new();
        };
        let ids = registries.recipe_index.for_output(item);
        self.group(ids.iter().map(|r| RecipeRef(*r)), categories)
    }

    /// Recipes that consume this ingredient (as input or catalyst), grouped
    /// by category in tab order.
    pub fn uses(
        &self,
        ingredient: &Ingredient,
        registries: &FrozenRegistries,
        categories: &Categories,
    ) -> Vec<(CategoryId, Vec<RecipeRef>)> {
        // PHASE3-IMPL: A — tag ingredients: union over members; catalysts.
        let Some(item) = ingredient.item_id() else {
            return Vec::new();
        };
        let ids = registries.recipe_index.uses(item);
        self.group(ids.iter().map(|r| RecipeRef(*r)), categories)
    }

    fn group(
        &self,
        recipes: impl Iterator<Item = RecipeRef>,
        categories: &Categories,
    ) -> Vec<(CategoryId, Vec<RecipeRef>)> {
        let mut by: BTreeMap<CategoryId, Vec<RecipeRef>> = BTreeMap::new();
        for r in recipes {
            if let Some(c) = self.category_of(r) {
                by.entry(c.clone()).or_default().push(r);
            }
        }
        categories
            .iter()
            .filter_map(|c| by.remove(&c.id()).map(|v| (c.id(), v)))
            .collect()
    }

    /// Lays out one recipe through its category.
    pub fn layout(
        &self,
        recipe: RecipeRef,
        focus: Option<&Ingredient>,
        registries: &FrozenRegistries,
        categories: &Categories,
        types: &IngredientTypes,
    ) -> Option<RecipeLayout> {
        // PHASE3-IMPL: A — memoise per (recipe, focus.is_some()).
        let def = registries.recipes.get(recipe.0)?;
        let category = categories.get(self.category_of(recipe)?)?;
        let view = RecipeView {
            id: recipe,
            def,
            registries,
            types,
            focus,
        };
        let mut builder = LayoutBuilder::new();
        category.layout(&view, &mut builder);
        Some(builder.finish())
    }

    /// Total recipes filed.
    pub fn len(&self) -> usize {
        self.category_of.len()
    }

    /// Whether nothing is filed.
    pub fn is_empty(&self) -> bool {
        self.category_of.is_empty()
    }
}
