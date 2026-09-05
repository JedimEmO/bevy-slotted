//! The lookups built at freeze time.
//!
//! Authored data says what a mod wrote; an index says what the game needs to
//! know. [`TagIndex`] flattens tag inheritance so `has_tag` is a set lookup
//! rather than a graph walk, and [`RecipeIndex`] answers the browser's two
//! questions, "what makes this" and "what uses this", without scanning the
//! recipe list.
//!
//! Both are built once, inside
//! [`Registries::freeze`](crate::registry::Registries::freeze), and are
//! immutable afterwards.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use slotted_model::{ItemId, Namespaced};

use crate::defs::{Ingredient, ItemDef, RecipeDef, RecipeTypeDef, TagDef, TagEntry};
use crate::registry::{MultiMap, RecipeId, RecipeTypeId, Registry, RegistryError, TagId, Warning};

/// Tag membership with `#other:tag` inheritance already resolved.
///
/// A tag that inherits another contains everything that one contains, however
/// deep the chain goes. Diamonds are fine, loops are a
/// [`RegistryError::TagCycle`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TagIndex {
    members: BTreeMap<Namespaced, BTreeSet<ItemId>>,
    tags_of: BTreeMap<ItemId, BTreeSet<Namespaced>>,
}

static NO_ITEMS: BTreeSet<ItemId> = BTreeSet::new();
static NO_TAGS: BTreeSet<Namespaced> = BTreeSet::new();

impl TagIndex {
    /// Every item in `tag`, including everything inherited. Empty if the tag
    /// is not registered.
    pub fn items_with(&self, tag: &Namespaced) -> &BTreeSet<ItemId> {
        self.members.get(tag).unwrap_or(&NO_ITEMS)
    }

    /// Whether `item` is in `tag`.
    pub fn has_tag(&self, item: ItemId, tag: &Namespaced) -> bool {
        self.members.get(tag).is_some_and(|set| set.contains(&item))
    }

    /// Every tag `item` belongs to.
    pub fn tags_of(&self, item: ItemId) -> &BTreeSet<Namespaced> {
        self.tags_of.get(&item).unwrap_or(&NO_TAGS)
    }

    /// How many tags have at least one member.
    pub fn len(&self) -> usize {
        self.members.len()
    }

    /// Whether no tag has a member.
    pub fn is_empty(&self) -> bool {
        self.members.is_empty()
    }

    /// Every `(tag, members)` pair.
    pub fn iter(&self) -> impl ExactSizeIterator<Item = (&Namespaced, &BTreeSet<ItemId>)> {
        self.members.iter()
    }

    /// Resolves `tags` against `items`.
    ///
    /// Unknown members and unknown parent tags are [`Warning`]s: a pack that
    /// lists an item from a mod the player did not install still has to boot.
    ///
    /// # Errors
    ///
    /// [`RegistryError::TagCycle`] if tags inherit from each other in a loop.
    pub(crate) fn build(
        tags: &Registry<TagDef, TagId>,
        items: &Registry<ItemDef, ItemId>,
        warnings: &mut Vec<Warning>,
    ) -> Result<Self, RegistryError> {
        let mut members: BTreeMap<Namespaced, BTreeSet<ItemId>> = BTreeMap::new();

        // An item may claim membership from its own side, which is how a mod
        // joins a tag it does not own without shipping a tag file.
        for (id, _, item) in items.iter() {
            for tag in &item.tags {
                members.entry(tag.clone()).or_default().insert(id);
            }
        }

        // Resolve each tag depth-first, memoising as we go. `resolving` is the
        // current path, so a repeat in it is a cycle and names the loop.
        let mut resolved: BTreeMap<Namespaced, BTreeSet<ItemId>> = BTreeMap::new();
        let mut resolving: Vec<Namespaced> = Vec::new();
        for (_, name, _) in tags.iter() {
            resolve(
                name,
                tags,
                items,
                &members,
                &mut resolved,
                &mut resolving,
                warnings,
            )?;
        }

        // Tags that only ever got members from the item side never appear in
        // the tag registry, so fold them in too.
        for (name, direct) in members {
            resolved.entry(name).or_default().extend(direct);
        }
        resolved.retain(|_, set| !set.is_empty());

        let mut tags_of: BTreeMap<ItemId, BTreeSet<Namespaced>> = BTreeMap::new();
        for (tag, set) in &resolved {
            for item in set {
                tags_of.entry(*item).or_default().insert(tag.clone());
            }
        }

        Ok(Self {
            members: resolved,
            tags_of,
        })
    }
}

/// Depth-first resolution of one tag, with the current path as the cycle check.
fn resolve(
    name: &Namespaced,
    tags: &Registry<TagDef, TagId>,
    items: &Registry<ItemDef, ItemId>,
    from_items: &BTreeMap<Namespaced, BTreeSet<ItemId>>,
    resolved: &mut BTreeMap<Namespaced, BTreeSet<ItemId>>,
    resolving: &mut Vec<Namespaced>,
    warnings: &mut Vec<Warning>,
) -> Result<BTreeSet<ItemId>, RegistryError> {
    if let Some(done) = resolved.get(name) {
        return Ok(done.clone());
    }
    if let Some(start) = resolving.iter().position(|seen| seen == name) {
        let mut path: Vec<String> = resolving[start..].iter().map(ToString::to_string).collect();
        path.push(name.to_string());
        return Err(RegistryError::TagCycle(path));
    }

    let Some(def) = tags.get_by_name(name) else {
        return Ok(from_items.get(name).cloned().unwrap_or_default());
    };

    resolving.push(name.clone());
    let mut set = from_items.get(name).cloned().unwrap_or_default();
    for entry in &def.values {
        match entry {
            TagEntry::Item(item) => match items.id_of(item) {
                Some(id) => {
                    set.insert(id);
                }
                None => warnings.push(Warning::UnknownTagMember {
                    tag: name.clone(),
                    item: item.clone(),
                }),
            },
            TagEntry::Tag(parent) => {
                if tags.contains(parent) {
                    let inherited = resolve(
                        parent, tags, items, from_items, resolved, resolving, warnings,
                    )?;
                    set.extend(inherited);
                } else {
                    warnings.push(Warning::UnknownParentTag {
                        tag: name.clone(),
                        parent: parent.clone(),
                    });
                }
            }
        }
    }
    resolving.pop();

    resolved.insert(name.clone(), set.clone());
    Ok(set)
}

/// Which recipes make an item, and which recipes consume it.
///
/// `uses` follows tags: a recipe whose input is `#slotted:planks` shows up
/// under every plank. That is what makes the browser's "usage" tab useful
/// rather than a list of literal ids nobody wrote.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecipeIndex {
    for_output: MultiMap<ItemId, RecipeId>,
    uses: MultiMap<ItemId, RecipeId>,
    by_type: MultiMap<RecipeTypeId, RecipeId>,
}

static NO_RECIPES: &[RecipeId] = &[];

impl RecipeIndex {
    /// Every recipe that produces `item`, in registration order.
    pub fn for_output(&self, item: ItemId) -> &[RecipeId] {
        self.for_output.get(&item).map_or(NO_RECIPES, Vec::as_slice)
    }

    /// Every recipe that can consume `item`, directly or through a tag.
    pub fn uses(&self, item: ItemId) -> &[RecipeId] {
        self.uses.get(&item).map_or(NO_RECIPES, Vec::as_slice)
    }

    /// Every recipe of one category, which is one browser tab.
    pub fn by_type(&self, recipe_type: RecipeTypeId) -> &[RecipeId] {
        self.by_type
            .get(&recipe_type)
            .map_or(NO_RECIPES, Vec::as_slice)
    }

    /// How many items have at least one recipe producing them.
    pub fn output_count(&self) -> usize {
        self.for_output.len()
    }

    /// Builds the index.
    ///
    /// # Errors
    ///
    /// [`RegistryError::UnknownRecipeType`] if a recipe names a recipe type
    /// nobody registered. An unknown *item* is only a [`Warning`], because a
    /// recipe referring to an absent mod's item is a normal pack state, while
    /// an unknown recipe type leaves the browser with no way to draw anything.
    pub(crate) fn build(
        recipes: &Registry<RecipeDef, RecipeId>,
        recipe_types: &Registry<RecipeTypeDef, RecipeTypeId>,
        items: &Registry<ItemDef, ItemId>,
        tags: &TagIndex,
        warnings: &mut Vec<Warning>,
    ) -> Result<Self, RegistryError> {
        let mut index = Self::default();

        for (recipe_id, name, recipe) in recipes.iter() {
            let Some(type_id) = recipe_types.id_of(&recipe.recipe_type) else {
                return Err(RegistryError::UnknownRecipeType {
                    recipe: name.clone(),
                    recipe_type: recipe.recipe_type.clone(),
                });
            };
            index.by_type.entry(type_id).or_default().push(recipe_id);

            match items.id_of(&recipe.result.item) {
                Some(output) => push_once(&mut index.for_output, output, recipe_id),
                None => warnings.push(Warning::UnknownRecipeItem {
                    recipe: name.clone(),
                    item: recipe.result.item.clone(),
                }),
            }

            for ingredient in recipe.all_ingredients() {
                let mut missing = Vec::new();
                ingredient.walk(&mut |part| match part {
                    Ingredient::Item(id) => match items.id_of(id) {
                        Some(input) => push_once(&mut index.uses, input, recipe_id),
                        None => missing.push(id.clone()),
                    },
                    Ingredient::Tag(tag) => {
                        for input in tags.items_with(tag) {
                            push_once(&mut index.uses, *input, recipe_id);
                        }
                    }
                    Ingredient::AnyOf(_) => {}
                });
                for item in missing {
                    warnings.push(Warning::UnknownRecipeItem {
                        recipe: name.clone(),
                        item,
                    });
                }
            }
        }

        Ok(index)
    }
}

/// Pushes `recipe` unless it is already the last entry for `item`.
///
/// A shaped recipe can name the same tag in two cells; the browser should list
/// it once. Recipes are visited in id order, so a duplicate is always adjacent.
fn push_once(map: &mut MultiMap<ItemId, RecipeId>, item: ItemId, recipe: RecipeId) {
    let bucket = map.entry(item).or_default();
    if bucket.last() != Some(&recipe) {
        bucket.push(recipe);
    }
}

#[allow(clippy::unwrap_used)]
#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use slotted_model::{ItemId, Namespaced};

    use crate::defs::{
        Ingredient, ItemDef, ItemResult, RecipeDef, RecipeTypeDef, TagDef, TagEntry,
    };
    use crate::registry::{Registries, RegistryError, Warning};

    fn id(s: &str) -> Namespaced {
        Namespaced::parse(s).unwrap()
    }

    fn tag(name: &str, values: &[&str]) -> TagDef {
        TagDef {
            name: id(name),
            values: values
                .iter()
                .map(|raw| {
                    raw.strip_prefix('#')
                        .map_or_else(|| TagEntry::Item(id(raw)), |rest| TagEntry::Tag(id(rest)))
                })
                .collect(),
            replace: false,
        }
    }

    /// Two planks, an oak-only tag, a planks tag inheriting it, a crafting
    /// type, and a chest recipe that takes `#test:planks`.
    fn fixture() -> Registries {
        let mut registries = Registries::new();
        for name in ["test:oak", "test:birch", "test:chest"] {
            registries.add_item(ItemDef::new(id(name))).unwrap();
        }
        registries.add_tag(tag("test:oak_planks", &["test:oak"]));
        registries.add_tag(tag("test:planks", &["#test:oak_planks", "test:birch"]));
        registries
            .add_recipe_type(RecipeTypeDef::new(id("test:crafting")))
            .unwrap();
        registries
            .add_recipe(RecipeDef::shapeless(
                id("test:chest"),
                id("test:crafting"),
                vec![Ingredient::Tag(id("test:planks"))],
                ItemResult::one(id("test:chest")),
            ))
            .unwrap();
        registries
    }

    #[test]
    fn tag_inheritance_flattens_transitively() {
        let (frozen, warnings) = fixture().freeze().unwrap();
        assert!(warnings.is_empty(), "{warnings:?}");
        let oak = frozen.item_id(&id("test:oak")).unwrap();
        let birch = frozen.item_id(&id("test:birch")).unwrap();

        assert_eq!(
            frozen.items_with(&id("test:planks")),
            &[oak, birch].into_iter().collect()
        );
        assert_eq!(
            frozen.items_with(&id("test:oak_planks")),
            &[oak].into_iter().collect()
        );
        assert!(frozen.tag_index.has_tag(oak, &id("test:oak_planks")));
        assert!(!frozen.tag_index.has_tag(birch, &id("test:oak_planks")));
        assert_eq!(frozen.tag_index.tags_of(birch).len(), 1);
        assert_eq!(frozen.tag_index.len(), 2);
        assert!(!frozen.tag_index.is_empty());
        assert_eq!(frozen.tag_index.iter().count(), 2);
        assert_eq!(frozen.tag_names().count(), 2);
    }

    #[test]
    fn an_item_can_join_a_tag_from_its_own_side() {
        let mut registries = Registries::new();
        let mut stone = ItemDef::new(id("test:stone"));
        stone.tags.push(id("test:blocks"));
        registries.add_item(stone).unwrap();
        let (frozen, warnings) = registries.freeze().unwrap();
        assert!(warnings.is_empty());
        assert!(
            frozen.tag_index.has_tag(ItemId(0), &id("test:blocks")),
            "a tag with no tag file still exists"
        );
    }

    #[test]
    fn an_unknown_tag_member_is_a_warning_not_an_error() {
        let mut registries = Registries::new();
        registries.add_tag(tag("test:planks", &["absent:oak", "#absent:tag"]));
        let (frozen, warnings) = registries.freeze().unwrap();
        assert_eq!(
            warnings,
            vec![
                Warning::UnknownTagMember {
                    tag: id("test:planks"),
                    item: id("absent:oak"),
                },
                Warning::UnknownParentTag {
                    tag: id("test:planks"),
                    parent: id("absent:tag"),
                },
            ]
        );
        assert!(frozen.items_with(&id("test:planks")).is_empty());
        assert!(frozen.items_with(&id("test:nothing")).is_empty());
    }

    #[test]
    fn a_tag_cycle_is_an_error_that_names_the_loop() {
        let mut registries = Registries::new();
        registries.add_tag(tag("test:a", &["#test:b"]));
        registries.add_tag(tag("test:b", &["#test:a"]));
        let err = registries.freeze().unwrap_err();
        let RegistryError::TagCycle(path) = &err else {
            panic!("expected a cycle, got {err}");
        };
        assert_eq!(path.first(), path.last());
        assert!(err.to_string().contains("test:a -> test:b"), "{err}");
    }

    #[test]
    fn a_tag_may_be_inherited_twice_without_looping() {
        let mut registries = Registries::new();
        registries.add_item(ItemDef::new(id("test:oak"))).unwrap();
        registries.add_tag(tag("test:base", &["test:oak"]));
        registries.add_tag(tag("test:left", &["#test:base"]));
        registries.add_tag(tag("test:right", &["#test:base"]));
        registries.add_tag(tag("test:top", &["#test:left", "#test:right"]));
        let (frozen, _) = registries.freeze().unwrap();
        assert_eq!(frozen.items_with(&id("test:top")).len(), 1);
    }

    #[test]
    fn recipes_are_indexed_by_output_and_by_use_through_tags() {
        let (frozen, _) = fixture().freeze().unwrap();
        let oak = frozen.item_id(&id("test:oak")).unwrap();
        let birch = frozen.item_id(&id("test:birch")).unwrap();
        let chest = frozen.item_id(&id("test:chest")).unwrap();
        let recipe = frozen.recipes.id_of(&id("test:chest")).unwrap();

        assert_eq!(frozen.recipe_index.for_output(chest), [recipe]);
        assert_eq!(frozen.recipe_index.uses(oak), [recipe]);
        assert_eq!(
            frozen.recipe_index.uses(birch),
            [recipe],
            "a tag ingredient reaches every member"
        );
        assert!(frozen.recipe_index.uses(chest).is_empty());
        assert_eq!(frozen.recipe_index.output_count(), 1);

        let crafting = frozen.recipe_types.id_of(&id("test:crafting")).unwrap();
        assert_eq!(frozen.recipe_index.by_type(crafting), [recipe]);
    }

    #[test]
    fn a_shaped_recipe_lists_a_repeated_ingredient_once() {
        let mut registries = Registries::new();
        registries.add_item(ItemDef::new(id("test:oak"))).unwrap();
        registries.add_item(ItemDef::new(id("test:chest"))).unwrap();
        registries
            .add_recipe_type(RecipeTypeDef::new(id("test:crafting")))
            .unwrap();
        let mut recipe = RecipeDef::shapeless(
            id("test:chest"),
            id("test:crafting"),
            vec![],
            ItemResult::one(id("test:chest")),
        );
        recipe.shape = Some(vec!["PP".to_owned(), "PP".to_owned()]);
        recipe.key.insert('P', Ingredient::Item(id("test:oak")));
        recipe
            .ingredients
            .push(Ingredient::AnyOf(vec![Ingredient::Item(id("test:oak"))]));
        registries.add_recipe(recipe).unwrap();

        let (frozen, _) = registries.freeze().unwrap();
        let oak = frozen.item_id(&id("test:oak")).unwrap();
        assert_eq!(frozen.recipe_index.uses(oak).len(), 1);
    }

    #[test]
    fn an_unknown_recipe_type_is_fatal() {
        let mut registries = Registries::new();
        registries
            .add_recipe(RecipeDef::shapeless(
                id("test:chest"),
                id("test:absent"),
                vec![],
                ItemResult::one(id("test:chest")),
            ))
            .unwrap();
        let err = registries.freeze().unwrap_err();
        assert_eq!(
            err,
            RegistryError::UnknownRecipeType {
                recipe: id("test:chest"),
                recipe_type: id("test:absent"),
            }
        );
    }

    #[test]
    fn an_unknown_recipe_item_is_a_warning() {
        let mut registries = Registries::new();
        registries
            .add_recipe_type(RecipeTypeDef::new(id("test:crafting")))
            .unwrap();
        registries
            .add_recipe(RecipeDef::shapeless(
                id("test:chest"),
                id("test:crafting"),
                vec![Ingredient::Item(id("absent:plank"))],
                ItemResult::one(id("absent:chest")),
            ))
            .unwrap();
        let (_, warnings) = registries.freeze().unwrap();
        assert_eq!(warnings.len(), 2, "{warnings:?}");
        assert!(
            warnings
                .iter()
                .all(|warning| matches!(warning, Warning::UnknownRecipeItem { .. }))
        );
    }

    #[test]
    fn a_replacing_tag_discards_what_came_before() {
        let mut registries = Registries::new();
        registries.add_item(ItemDef::new(id("test:oak"))).unwrap();
        registries.add_item(ItemDef::new(id("test:birch"))).unwrap();
        registries.add_tag(tag("test:planks", &["test:oak"]));
        registries.add_tag(tag("test:planks", &["test:birch"]));
        assert_eq!(
            registries
                .tags
                .get(&id("test:planks"))
                .unwrap()
                .values
                .len(),
            2,
            "a second tag def merges by default"
        );

        let mut replacing = tag("test:planks", &["test:birch"]);
        replacing.replace = true;
        registries.add_tag(replacing);
        let (frozen, _) = registries.freeze().unwrap();
        assert_eq!(frozen.items_with(&id("test:planks")).len(), 1);
    }
}
