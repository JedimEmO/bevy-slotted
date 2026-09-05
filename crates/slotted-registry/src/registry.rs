//! Open registries, frozen registries, and the freeze between them.
//!
//! A [`RegistryBuilder`] is what the data stage writes to: entries go in, get
//! replaced, get removed and get patched, in load order. [`freeze`] closes it
//! and hands back a [`Registry`], which has no `&mut self` method at all. That
//! is the whole immutability story: after the freeze there is no API through
//! which an entry could change, so the numeric ids stay valid for the process
//! lifetime and every browser lookup is a pure function over fixed data.
//!
//! [`freeze`]: RegistryBuilder::freeze

use std::collections::{BTreeMap, BTreeSet};

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use slotted_model::{ComponentId, ItemId, Namespaced};

use crate::defs::{
    HudLayerDef, IngredientTypeDef, ItemDef, RecipeDef, RecipeTypeDef, ScreenDef, TagDef,
    TooltipComponentDef, WidgetDef,
};
use crate::index::{RecipeIndex, TagIndex};
use crate::interner::{InternedId, Interner, RegistryId};

/// A recipe's runtime handle.
pub type RecipeId = RegistryId<RecipeDef>;
/// A recipe type's runtime handle.
pub type RecipeTypeId = RegistryId<RecipeTypeDef>;
/// A tag's runtime handle.
pub type TagId = RegistryId<TagDef>;

/// Something a mod asked a registry to do that it cannot do.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RegistryError {
    /// Two entries claimed the same id and neither asked to replace.
    #[error(
        "`{name}` is already registered in the {registry} registry; set `replace` to override it"
    )]
    Duplicate {
        /// The contested id.
        name: Namespaced,
        /// Which registry it was in.
        registry: RegistryKind,
    },
    /// A lookup or a patch named an entry that is not there.
    #[error("`{name}` is not registered in the {registry} registry")]
    Unknown {
        /// The id that was not found.
        name: Namespaced,
        /// Which registry was searched.
        registry: RegistryKind,
    },
    /// A recipe named a recipe type nobody registered. Unlike an unknown item
    /// in a tag, this is fatal: the browser has no tab to draw the recipe in
    /// and no idea how to lay its inputs out.
    #[error("recipe `{recipe}` has recipe type `{recipe_type}`, which is not registered")]
    UnknownRecipeType {
        /// The recipe holding the bad reference.
        recipe: Namespaced,
        /// The recipe type it named.
        recipe_type: Namespaced,
    },
    /// Tags inherit from each other in a loop.
    #[error("tag inheritance cycle: {}", .0.join(" -> "))]
    TagCycle(Vec<String>),
}

/// Something that is odd but not fatal, collected during a freeze.
///
/// Warnings exist because a mod pack is a moving target: a tag that lists an
/// item from a mod the player did not install is normal, and refusing to boot
/// over it would make packs unshippable.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Warning {
    /// A tag listed a member that no mod registered.
    #[error("tag `{tag}` lists `{item}`, which is not a registered item")]
    UnknownTagMember {
        /// The tag holding the reference.
        tag: Namespaced,
        /// The item it named.
        item: Namespaced,
    },
    /// A tag inherited from a tag that no mod registered.
    #[error("tag `{tag}` inherits `#{parent}`, which is not a registered tag")]
    UnknownParentTag {
        /// The tag holding the reference.
        tag: Namespaced,
        /// The tag it named.
        parent: Namespaced,
    },
    /// A recipe produced or consumed an item that no mod registered.
    #[error("recipe `{recipe}` references `{item}`, which is not a registered item")]
    UnknownRecipeItem {
        /// The recipe holding the reference.
        recipe: Namespaced,
        /// The item it named.
        item: Namespaced,
    },
}

/// Which of the standard registries an id belongs to.
///
/// Doubles as the directory name the data stage reads entries from, so
/// `RegistryKind::RecipeTypes` is `data/<modid>/recipe_types/*.ron`.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum RegistryKind {
    /// [`ItemDef`].
    #[default]
    Items,
    /// [`TagDef`].
    Tags,
    /// [`RecipeTypeDef`].
    RecipeTypes,
    /// [`RecipeDef`].
    Recipes,
    /// [`ScreenDef`].
    Screens,
    /// [`WidgetDef`].
    Widgets,
    /// [`TooltipComponentDef`].
    TooltipComponents,
    /// [`HudLayerDef`].
    HudLayers,
    /// [`IngredientTypeDef`].
    IngredientTypes,
}

impl RegistryKind {
    /// Every kind, in the order the data stage loads them. Entries a later
    /// kind refers to are loaded first, so a recipe can be validated against
    /// its recipe type in one pass.
    pub const ALL: [Self; 9] = [
        Self::Items,
        Self::Tags,
        Self::RecipeTypes,
        Self::Recipes,
        Self::Screens,
        Self::Widgets,
        Self::TooltipComponents,
        Self::HudLayers,
        Self::IngredientTypes,
    ];

    /// The directory name under `data/<modid>/`.
    pub const fn dir(self) -> &'static str {
        match self {
            Self::Items => "items",
            Self::Tags => "tags",
            Self::RecipeTypes => "recipe_types",
            Self::Recipes => "recipes",
            Self::Screens => "screens",
            Self::Widgets => "widgets",
            Self::TooltipComponents => "tooltip_components",
            Self::HudLayers => "hud_layers",
            Self::IngredientTypes => "ingredient_types",
        }
    }

    /// The kind a directory name refers to, if any.
    pub fn from_dir(dir: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.dir() == dir)
    }
}

impl core::fmt::Display for RegistryKind {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.dir())
    }
}

/// An open registry: what the data stage writes into.
///
/// Ids handed out by [`insert`](Self::insert) are *provisional*. They match the
/// frozen ids as long as nothing is removed, and [`remove`](Self::remove)
/// shifts everything after the hole down by one. Only [`freeze`](Self::freeze)
/// mints ids that are safe to keep.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RegistryBuilder<T, K = RegistryId<T>> {
    /// Only ever used to name the registry in an error message.
    kind: RegistryKind,
    entries: IndexMap<Namespaced, T>,
    #[serde(skip)]
    handle: core::marker::PhantomData<fn() -> K>,
}

impl<T, K> Default for RegistryBuilder<T, K> {
    fn default() -> Self {
        Self::with_kind(RegistryKind::Items)
    }
}

impl<T, K> RegistryBuilder<T, K> {
    /// An empty builder that names `kind` in its error messages.
    pub fn with_kind(kind: RegistryKind) -> Self {
        Self {
            kind,
            entries: IndexMap::new(),
            handle: core::marker::PhantomData,
        }
    }

    /// Which registry this builder fills.
    pub const fn kind(&self) -> RegistryKind {
        self.kind
    }
}

impl<T, K: InternedId> RegistryBuilder<T, K> {
    /// An empty builder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers `value` under `name`.
    ///
    /// # Errors
    ///
    /// [`RegistryError::Duplicate`] if `name` is taken. Use
    /// [`replace`](Self::replace) to override deliberately.
    pub fn insert(&mut self, name: Namespaced, value: T) -> Result<K, RegistryError> {
        if self.entries.contains_key(&name) {
            return Err(RegistryError::Duplicate {
                name,
                registry: self.kind,
            });
        }
        Ok(self.replace(name, value))
    }

    /// Registers `value` under `name`, overwriting any existing entry and
    /// keeping its position, and returns the provisional id.
    pub fn replace(&mut self, name: Namespaced, value: T) -> K {
        let (index, _) = self.entries.insert_full(name, value);
        K::from_index(u32::try_from(index).expect("a registry holds at most u32::MAX entries"))
    }

    /// Removes an entry, returning it. Ids after it shift down by one.
    pub fn remove(&mut self, name: &Namespaced) -> Option<T> {
        self.entries.shift_remove(name)
    }

    /// The entry registered under `name`.
    pub fn get(&self, name: &Namespaced) -> Option<&T> {
        self.entries.get(name)
    }

    /// The entry registered under `name`, mutably. Open registries are
    /// mutable; this method is what [`Registry`] deliberately does not have.
    pub fn get_mut(&mut self, name: &Namespaced) -> Option<&mut T> {
        self.entries.get_mut(name)
    }

    /// Runs `edit` against an existing entry.
    ///
    /// This is the typed counterpart to [`crate::patch`]: a Rust mod that
    /// wants to raise another mod's stack size calls this, a data mod ships a
    /// patch file, and both end up mutating the same builder.
    ///
    /// # Errors
    ///
    /// [`RegistryError::Unknown`] if nothing is registered under `name`.
    pub fn patch<R>(
        &mut self,
        name: &Namespaced,
        edit: impl FnOnce(&mut T) -> R,
    ) -> Result<R, RegistryError> {
        self.entries
            .get_mut(name)
            .map(edit)
            .ok_or_else(|| RegistryError::Unknown {
                name: name.clone(),
                registry: self.kind,
            })
    }

    /// Whether `name` is registered.
    pub fn contains(&self, name: &Namespaced) -> bool {
        self.entries.contains_key(name)
    }

    /// How many entries are registered.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether nothing is registered.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Every `(name, value)` pair in insertion order.
    pub fn iter(&self) -> impl ExactSizeIterator<Item = (&Namespaced, &T)> {
        self.entries.iter()
    }

    /// Every value, mutably, in insertion order.
    pub fn values_mut(&mut self) -> impl ExactSizeIterator<Item = &mut T> {
        self.entries.values_mut()
    }

    /// Closes the registry. Ids are assigned in insertion order and are final.
    pub fn freeze(self) -> Registry<T, K> {
        let mut interner = Interner::new();
        let mut values = Vec::with_capacity(self.entries.len());
        for (name, value) in self.entries {
            interner.intern(&name);
            values.push(value);
        }
        Registry {
            kind: self.kind,
            interner,
            values,
            handle: core::marker::PhantomData,
        }
    }
}

impl<T, K: InternedId> FromIterator<(Namespaced, T)> for RegistryBuilder<T, K> {
    fn from_iter<I: IntoIterator<Item = (Namespaced, T)>>(iter: I) -> Self {
        Self {
            kind: RegistryKind::Items,
            entries: iter.into_iter().collect(),
            handle: core::marker::PhantomData,
        }
    }
}

/// A closed registry: dense ids, immutable entries, pure lookups.
///
/// There is no method taking `&mut self`, and the fields are private, so an
/// entry cannot change after the freeze. Ids are indices into the value list,
/// which is why [`get`](Self::get) is a bounds check and a pointer offset.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Registry<T, K = RegistryId<T>> {
    kind: RegistryKind,
    interner: Interner<K>,
    values: Vec<T>,
    #[serde(skip)]
    handle: core::marker::PhantomData<fn() -> K>,
}

impl<T, K> Default for Registry<T, K> {
    fn default() -> Self {
        Self {
            kind: RegistryKind::Items,
            interner: Interner::default(),
            values: Vec::new(),
            handle: core::marker::PhantomData,
        }
    }
}

impl<T, K: InternedId> Registry<T, K> {
    /// The entry behind `id`, or `None` if the id is out of range.
    pub fn get(&self, id: K) -> Option<&T> {
        self.values.get(id.index() as usize)
    }

    /// The entry registered under `name`.
    pub fn get_by_name(&self, name: &Namespaced) -> Option<&T> {
        self.id_of(name).and_then(|id| self.get(id))
    }

    /// The id registered under `name`.
    pub fn id_of(&self, name: &Namespaced) -> Option<K> {
        self.interner.get(name)
    }

    /// The name behind `id`.
    pub fn name_of(&self, id: K) -> Option<&Namespaced> {
        self.interner.name(id)
    }

    /// Whether `name` is registered.
    pub fn contains(&self, name: &Namespaced) -> bool {
        self.interner.contains(name)
    }

    /// How many entries there are.
    pub fn len(&self) -> usize {
        self.values.len()
    }

    /// Whether the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// Every `(id, name, value)` triple in id order.
    pub fn iter(&self) -> impl ExactSizeIterator<Item = (K, &Namespaced, &T)> {
        self.interner
            .iter()
            .zip(&self.values)
            .map(|((id, name), value)| (id, name, value))
    }

    /// Every value in id order.
    pub fn values(&self) -> impl ExactSizeIterator<Item = &T> {
        self.values.iter()
    }

    /// The string map scripts and save files need: name to id.
    pub fn interner(&self) -> &Interner<K> {
        &self.interner
    }

    /// Which registry this is.
    pub const fn kind(&self) -> RegistryKind {
        self.kind
    }
}

/// The whole standard registry set while the data stage is open.
///
/// The fields are the typed accessors: `registries.items` is a
/// `RegistryBuilder<ItemDef, ItemId>` and nothing else will type-check against
/// it, so a screen definition cannot land in the item registry by accident.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Registries {
    /// Items, keyed by [`ItemId`] so the frozen ids are the ones the model uses.
    pub items: RegistryBuilder<ItemDef, ItemId>,
    /// Tags. Merged rather than collided; see [`Self::add_tag`].
    pub tags: RegistryBuilder<TagDef, TagId>,
    /// Recipe categories.
    pub recipe_types: RegistryBuilder<RecipeTypeDef, RecipeTypeId>,
    /// Recipes.
    pub recipes: RegistryBuilder<RecipeDef, RecipeId>,
    /// Screen kinds. Payload stays opaque until the ui crate exists.
    pub screens: RegistryBuilder<ScreenDef>,
    /// Widget kinds.
    pub widgets: RegistryBuilder<WidgetDef>,
    /// Tooltip parts.
    pub tooltip_components: RegistryBuilder<TooltipComponentDef>,
    /// HUD layers.
    pub hud_layers: RegistryBuilder<HudLayerDef>,
    /// Ingredient kinds the browser can display.
    pub ingredient_types: RegistryBuilder<IngredientTypeDef>,
}

impl Default for Registries {
    fn default() -> Self {
        Self {
            items: RegistryBuilder::with_kind(RegistryKind::Items),
            tags: RegistryBuilder::with_kind(RegistryKind::Tags),
            recipe_types: RegistryBuilder::with_kind(RegistryKind::RecipeTypes),
            recipes: RegistryBuilder::with_kind(RegistryKind::Recipes),
            screens: RegistryBuilder::with_kind(RegistryKind::Screens),
            widgets: RegistryBuilder::with_kind(RegistryKind::Widgets),
            tooltip_components: RegistryBuilder::with_kind(RegistryKind::TooltipComponents),
            hud_layers: RegistryBuilder::with_kind(RegistryKind::HudLayers),
            ingredient_types: RegistryBuilder::with_kind(RegistryKind::IngredientTypes),
        }
    }
}

impl Registries {
    /// Empty registries.
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers an item under its own [`name`](ItemDef::name).
    ///
    /// # Errors
    ///
    /// [`RegistryError::Duplicate`] if the id is taken.
    pub fn add_item(&mut self, item: ItemDef) -> Result<ItemId, RegistryError> {
        let name = item.name.clone();
        self.items.insert(name, item)
    }

    /// Registers a tag, merging into any tag already registered under the same
    /// id unless the incoming def sets [`replace`](TagDef::replace).
    ///
    /// Tags are the one registry where a second definition of the same id is
    /// normal rather than an error: that is how two mods both put their chest
    /// into `slotted:chests`.
    pub fn add_tag(&mut self, tag: TagDef) -> TagId {
        let name = tag.name.clone();
        if tag.replace || !self.tags.contains(&name) {
            return self.tags.replace(name, tag);
        }
        let existing = self
            .tags
            .get_mut(&name)
            .expect("the entry was just checked to exist");
        for value in tag.values {
            if !existing.values.contains(&value) {
                existing.values.push(value);
            }
        }
        self.tags.provisional_id_of(&name)
    }

    /// Registers a recipe type under its own name.
    ///
    /// # Errors
    ///
    /// [`RegistryError::Duplicate`] if the id is taken.
    pub fn add_recipe_type(&mut self, def: RecipeTypeDef) -> Result<RecipeTypeId, RegistryError> {
        let name = def.name.clone();
        self.recipe_types.insert(name, def)
    }

    /// Registers a recipe under its own name.
    ///
    /// # Errors
    ///
    /// [`RegistryError::Duplicate`] if the id is taken.
    pub fn add_recipe(&mut self, def: RecipeDef) -> Result<RecipeId, RegistryError> {
        let name = def.name.clone();
        self.recipes.insert(name, def)
    }

    /// Closes every registry at once and builds the tag and recipe indices.
    ///
    /// Returns the frozen set and any non-fatal [`Warning`]s. Component ids are
    /// interned here too, from every component key every item declares.
    ///
    /// # Errors
    ///
    /// [`RegistryError::UnknownRecipeType`] if a recipe names a recipe type
    /// nobody registered, or [`RegistryError::TagCycle`] if tags inherit from
    /// each other in a loop.
    pub fn freeze(self) -> Result<(FrozenRegistries, Vec<Warning>), RegistryError> {
        let mut warnings = Vec::new();

        let mut components = Interner::<ComponentId>::new();
        for (_, item) in self.items.iter() {
            for key in item.components.keys() {
                components.intern(key);
            }
        }

        let items = self.items.freeze();
        let tags = self.tags.freeze();
        let recipe_types = self.recipe_types.freeze();
        let recipes = self.recipes.freeze();

        let tag_index = TagIndex::build(&tags, &items, &mut warnings)?;
        let recipe_index =
            RecipeIndex::build(&recipes, &recipe_types, &items, &tag_index, &mut warnings)?;

        Ok((
            FrozenRegistries {
                items,
                tags,
                recipe_types,
                recipes,
                screens: self.screens.freeze(),
                widgets: self.widgets.freeze(),
                tooltip_components: self.tooltip_components.freeze(),
                hud_layers: self.hud_layers.freeze(),
                ingredient_types: self.ingredient_types.freeze(),
                components,
                tag_index,
                recipe_index,
            },
            warnings,
        ))
    }
}

impl<T, K: InternedId> RegistryBuilder<T, K> {
    /// The provisional id of an existing entry. Private helper for
    /// [`Registries::add_tag`], which needs the id of the entry it merged into.
    fn provisional_id_of(&self, name: &Namespaced) -> K {
        let index = self
            .entries
            .get_index_of(name)
            .expect("caller checked the entry exists");
        K::from_index(u32::try_from(index).expect("a registry holds at most u32::MAX entries"))
    }
}

/// The whole standard registry set after the freeze.
///
/// This is what the control stage, the browser and every script sees. Nothing
/// in it can change, so a lookup never needs a lock and an [`ItemId`] stays
/// valid until the process reloads its data.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FrozenRegistries {
    /// Items. `ItemId(n)` is the `n`-th item registered.
    pub items: Registry<ItemDef, ItemId>,
    /// Tags, as authored. The resolved view is [`tag_index`](Self::tag_index).
    pub tags: Registry<TagDef, TagId>,
    /// Recipe categories.
    pub recipe_types: Registry<RecipeTypeDef, RecipeTypeId>,
    /// Recipes, as authored. The lookups are in
    /// [`recipe_index`](Self::recipe_index).
    pub recipes: Registry<RecipeDef, RecipeId>,
    /// Screen kinds.
    pub screens: Registry<ScreenDef>,
    /// Widget kinds.
    pub widgets: Registry<WidgetDef>,
    /// Tooltip parts.
    pub tooltip_components: Registry<TooltipComponentDef>,
    /// HUD layers.
    pub hud_layers: Registry<HudLayerDef>,
    /// Ingredient kinds.
    pub ingredient_types: Registry<IngredientTypeDef>,
    /// Every component key any item declared, interned to [`ComponentId`].
    pub components: Interner<ComponentId>,
    /// Tag membership with inheritance already resolved.
    pub tag_index: TagIndex,
    /// Which recipes make an item and which consume it.
    pub recipe_index: RecipeIndex,
}

impl FrozenRegistries {
    /// The id of an item by name, for the common case of looking one up.
    pub fn item_id(&self, name: &Namespaced) -> Option<ItemId> {
        self.items.id_of(name)
    }

    /// Every tag the registries know about, resolved, in id order.
    pub fn tag_names(&self) -> impl ExactSizeIterator<Item = &Namespaced> {
        self.tags.interner().names()
    }

    /// The set of item ids in `tag`, empty if the tag is unknown.
    pub fn items_with(&self, tag: &Namespaced) -> &BTreeSet<ItemId> {
        self.tag_index.items_with(tag)
    }
}

/// A [`BTreeMap`] used as a multimap, exposed as slices.
pub(crate) type MultiMap<K, V> = BTreeMap<K, Vec<V>>;

#[allow(clippy::unwrap_used)]
#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use slotted_model::{ItemId, Namespaced};

    use super::{Registries, RegistryBuilder, RegistryError, RegistryKind};
    use crate::defs::{ItemDef, Rarity};

    fn id(s: &str) -> Namespaced {
        Namespaced::parse(s).unwrap()
    }

    fn item(name: &str) -> ItemDef {
        ItemDef::new(id(name))
    }

    #[test]
    fn insert_hands_out_dense_ids() {
        let mut builder = RegistryBuilder::<ItemDef, ItemId>::new();
        assert_eq!(builder.insert(id("a:a"), item("a:a")).unwrap(), ItemId(0));
        assert_eq!(builder.insert(id("a:b"), item("a:b")).unwrap(), ItemId(1));
        assert_eq!(builder.len(), 2);
        assert!(!builder.is_empty());
        assert!(builder.contains(&id("a:a")));
    }

    #[test]
    fn a_duplicate_is_an_error_unless_replace_is_asked_for() {
        let mut builder = RegistryBuilder::<ItemDef, ItemId>::new();
        builder.insert(id("a:a"), item("a:a")).unwrap();
        let err = builder.insert(id("a:a"), item("a:a")).unwrap_err();
        assert_eq!(
            err,
            RegistryError::Duplicate {
                name: id("a:a"),
                registry: RegistryKind::Items,
            }
        );
        assert!(err.to_string().contains("already registered"), "{err}");

        let mut replacement = item("a:a");
        replacement.max_stack_size = 1;
        assert_eq!(builder.replace(id("a:a"), replacement), ItemId(0));
        assert_eq!(builder.get(&id("a:a")).unwrap().max_stack_size, 1);
        assert_eq!(builder.len(), 1, "replacing keeps the entry count");
    }

    #[test]
    fn remove_takes_the_entry_out() {
        let mut builder = RegistryBuilder::<ItemDef, ItemId>::new();
        builder.insert(id("a:a"), item("a:a")).unwrap();
        builder.insert(id("a:b"), item("a:b")).unwrap();
        assert_eq!(builder.remove(&id("a:a")).unwrap().name, id("a:a"));
        assert_eq!(builder.remove(&id("a:a")), None);
        assert_eq!(builder.len(), 1);
        // The provisional id shifted, which is why only `freeze` mints ids
        // that are safe to keep.
        assert_eq!(builder.freeze().id_of(&id("a:b")), Some(ItemId(0)));
    }

    #[test]
    fn get_mut_and_patch_both_edit_in_place() {
        let mut builder = RegistryBuilder::<ItemDef, ItemId>::new();
        builder.insert(id("a:a"), item("a:a")).unwrap();
        builder.get_mut(&id("a:a")).unwrap().max_stack_size = 16;
        let rarity = builder
            .patch(&id("a:a"), |def| {
                def.rarity = Rarity::Rare;
                def.rarity
            })
            .unwrap();
        assert_eq!(rarity, Rarity::Rare);
        assert_eq!(builder.get(&id("a:a")).unwrap().max_stack_size, 16);
        assert_eq!(builder.values_mut().count(), 1);

        let err = builder.patch(&id("a:missing"), |_| ()).unwrap_err();
        assert!(err.to_string().contains("not registered"), "{err}");
    }

    #[test]
    fn freezing_keeps_insertion_order_and_makes_ids_stable() {
        let mut builder = RegistryBuilder::<ItemDef, ItemId>::new();
        let provisional: Vec<_> = ["z:z", "a:a", "m:m"]
            .into_iter()
            .map(|name| builder.insert(id(name), item(name)).unwrap())
            .collect();
        // A late `replace` must not renumber anything.
        builder.replace(id("z:z"), item("z:z"));
        let registry = builder.freeze();

        for (name, expected) in ["z:z", "a:a", "m:m"].into_iter().zip(provisional) {
            assert_eq!(registry.id_of(&id(name)), Some(expected));
            assert_eq!(registry.name_of(expected), Some(&id(name)));
            assert_eq!(registry.get(expected).unwrap().name, id(name));
            assert_eq!(registry.get_by_name(&id(name)).unwrap().name, id(name));
        }
        assert_eq!(registry.get(ItemId(9)), None);
        assert_eq!(registry.id_of(&id("q:q")), None);
        assert!(registry.contains(&id("a:a")));
        assert_eq!(registry.iter().count(), 3);
        assert_eq!(registry.values().count(), 3);
        assert_eq!(registry.interner().len(), 3);
        assert!(!registry.is_empty());
    }

    #[test]
    fn registry_kinds_map_to_directories_both_ways() {
        for kind in RegistryKind::ALL {
            assert_eq!(RegistryKind::from_dir(kind.dir()), Some(kind));
            assert_eq!(kind.to_string(), kind.dir());
        }
        assert_eq!(RegistryKind::from_dir("nope"), None);
        assert_eq!(RegistryKind::default(), RegistryKind::Items);
    }

    #[test]
    fn adding_an_item_uses_its_own_name() {
        let mut registries = Registries::new();
        assert_eq!(registries.add_item(item("a:a")).unwrap(), ItemId(0));
        assert!(registries.add_item(item("a:a")).is_err());
        let (frozen, warnings) = registries.freeze().unwrap();
        assert!(warnings.is_empty());
        assert_eq!(frozen.item_id(&id("a:a")), Some(ItemId(0)));
    }

    #[test]
    fn freezing_interns_every_component_key() {
        let mut registries = Registries::new();
        let mut sword = item("a:sword");
        sword
            .components
            .insert(id("slotted:damage"), crate::Value::Unit);
        registries.add_item(sword).unwrap();
        let (frozen, _) = registries.freeze().unwrap();
        assert_eq!(frozen.components.len(), 1);
        assert!(frozen.components.contains(&id("slotted:damage")));
    }

    #[test]
    fn a_builder_survives_a_serde_round_trip() {
        let mut builder = RegistryBuilder::<ItemDef, ItemId>::new();
        builder.insert(id("a:a"), item("a:a")).unwrap();
        let text = ron::to_string(&builder).unwrap();
        let decoded: RegistryBuilder<ItemDef, ItemId> = ron::from_str(&text).unwrap();
        assert_eq!(decoded, builder);
    }

    #[test]
    fn a_builder_collects_from_pairs() {
        let builder: RegistryBuilder<ItemDef, ItemId> =
            [(id("a:a"), item("a:a"))].into_iter().collect();
        assert_eq!(builder.len(), 1);
    }
}
