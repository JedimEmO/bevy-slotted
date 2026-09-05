//! Building the index: pure function plus the systems that run it on
//! `AsyncComputeTaskPool` and poll the task. Package A.

use std::collections::HashMap;
use std::sync::Arc;

use bevy::prelude::*;
use bevy::tasks::{AsyncComputeTaskPool, block_on, futures_lite::future};
use slotted_registry::{FrozenRegistries, Rarity};

use super::{BrowserIndex, Entry, EntryId, IndexState, StringBitsetMap, SubstringIndex};
use crate::events::IndexReady;
use crate::ingredient::{IngredientCtx, IngredientTypes, Subtypes};
use crate::recipes::{Categories, RecipeStore};

/// Everything the build needs, cloned into the task.
#[derive(Clone)]
pub struct BuildInputs {
    /// Frozen registries.
    pub registries: Arc<FrozenRegistries>,
    /// Registered types.
    pub types: IngredientTypes,
    /// Registered subtype interpreters.
    pub subtypes: Subtypes,
    /// Categories, for the `%` field.
    pub categories: Categories,
    /// Recipes, for the `%` field.
    pub recipes: RecipeStore,
}

impl BrowserIndex {
    /// Builds the whole index. Pure; safe to run on any thread.
    pub fn build(inputs: &BuildInputs) -> Self {
        let ctx = IngredientCtx {
            registries: &inputs.registries,
            subtypes: &inputs.subtypes,
        };
        let mut entries = Vec::new();
        for ty in inputs.types.iter() {
            for ingredient in ty.entries(&ctx) {
                let rarity = ingredient
                    .item_id()
                    .and_then(|id| inputs.registries.items.get(id))
                    .map_or(Rarity::Common, |d| d.rarity);
                let categories = inputs
                    .recipes
                    .recipes_for(&ingredient, &inputs.registries, &inputs.categories)
                    .into_iter()
                    .map(|(c, _)| c.0.to_string())
                    .collect();
                entries.push(Entry {
                    display: ty.display_name(&ingredient, &ctx),
                    mod_ns: ty.mod_namespace(&ingredient, &ctx),
                    rarity,
                    tags: ty.tags(&ingredient, &ctx),
                    categories,
                    ingredient,
                });
            }
        }
        let n = entries.len();
        let id = |i: usize| u32::try_from(i).unwrap_or(u32::MAX);
        let names = SubstringIndex::build(
            n,
            entries
                .iter()
                .enumerate()
                .map(|(i, e)| (id(i), e.display.clone())),
        );
        let tooltips = SubstringIndex::build(
            n,
            entries.iter().enumerate().flat_map(|(i, e)| {
                let ty = inputs.types.get(&e.ingredient.ty);
                ty.map(|t| t.tooltip_text(&e.ingredient, &ctx))
                    .unwrap_or_default()
                    .into_iter()
                    .map(move |line| (id(i), line))
            }),
        );
        let ids = SubstringIndex::build(
            n,
            entries.iter().enumerate().filter_map(|(i, e)| {
                e.ingredient
                    .item_id()
                    .and_then(|item| inputs.registries.items.name_of(item))
                    .map(|name| (id(i), name.to_string()))
            }),
        );
        let mods = StringBitsetMap::build(
            n,
            entries
                .iter()
                .enumerate()
                .map(|(i, e)| (id(i), e.mod_ns.clone())),
        );
        let tags = StringBitsetMap::build(
            n,
            entries
                .iter()
                .enumerate()
                .flat_map(|(i, e)| e.tags.iter().cloned().map(move |t| (id(i), t))),
        );
        let categories = StringBitsetMap::build(
            n,
            entries
                .iter()
                .enumerate()
                .flat_map(|(i, e)| e.categories.iter().cloned().map(move |c| (id(i), c))),
        );
        let by_ingredient: HashMap<_, _> = entries
            .iter()
            .enumerate()
            .map(|(i, e)| (e.ingredient.clone(), EntryId(id(i))))
            .collect();
        Self {
            entries,
            names,
            tooltips,
            ids,
            mods,
            tags,
            categories,
            by_ingredient,
        }
    }
}

/// Starts a build on the compute pool and sets `IndexState::Building`.
pub fn start_index_build(world: &mut World) {
    let Some(registries) = world.get_resource::<slotted_ecs::Registries>() else {
        tracing::warn!("no Registries resource; the browser index stays empty");
        return;
    };
    let inputs = BuildInputs {
        registries: registries.0.clone(),
        types: world.resource::<IngredientTypes>().clone(),
        subtypes: world.resource::<Subtypes>().clone(),
        categories: world.resource::<Categories>().clone(),
        recipes: world.resource::<RecipeStore>().clone(),
    };
    let task = AsyncComputeTaskPool::get().spawn(async move { BrowserIndex::build(&inputs) });
    world.insert_resource(IndexState::Building(task));
}

/// `BrowserSet::Index`: moves a finished build into `Ready` and writes
/// [`IndexReady`].
pub fn poll_index_build(mut state: ResMut<IndexState>, mut ready: MessageWriter<IndexReady>) {
    let IndexState::Building(task) = &mut *state else {
        return;
    };
    if let Some(index) = block_on(future::poll_once(task)) {
        let entries = index.len();
        *state = IndexState::Ready(Arc::new(index));
        ready.write(IndexReady { entries });
    }
}
