//! Ingredients: the typed, comparable key the browser lists, searches,
//! bookmarks and transfers. Items, fluids, tags and info pages all flow
//! through the same shape. Contract section 1. Package A.

use std::collections::BTreeMap;
use std::sync::Arc;

use bevy::prelude::Resource;
use serde::{Deserialize, Serialize};
use slotted_icons::{IconRef, IconSource};
use slotted_model::{ItemId, ItemStack, Namespaced};
use slotted_registry::FrozenRegistries;

/// Identifies an [`IngredientType`]. The built-ins are in [`types`].
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct IngredientTypeId(pub Namespaced);

impl IngredientTypeId {
    /// Parses `ns:path`. Panics on a malformed id, which is a programming error.
    pub fn new(id: &str) -> Self {
        Self(Namespaced::parse(id).unwrap_or_else(|e| panic!("bad IngredientTypeId {id:?}: {e}")))
    }
}

/// The built-in ingredient type ids.
pub mod types {
    use super::IngredientTypeId;

    /// `slotted:item`.
    pub fn item() -> IngredientTypeId {
        IngredientTypeId::new("slotted:item")
    }
    /// `slotted:fluid` (placeholder in Phase 3).
    pub fn fluid() -> IngredientTypeId {
        IngredientTypeId::new("slotted:fluid")
    }
    /// `slotted:tag`.
    pub fn tag() -> IngredientTypeId {
        IngredientTypeId::new("slotted:tag")
    }
    /// `slotted:info`.
    pub fn info() -> IngredientTypeId {
        IngredientTypeId::new("slotted:info")
    }
}

/// The output of a [`SubtypeInterpreter`]: two stacks with equal keys are one
/// browser entry. [`SubtypeKey::none()`] is the key of an item without subtypes.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SubtypeKey(pub String);

impl SubtypeKey {
    /// The empty key.
    pub fn none() -> Self {
        Self(String::new())
    }

    /// A key from text.
    pub fn new(key: impl Into<String>) -> Self {
        Self(key.into())
    }

    /// Whether this is [`SubtypeKey::none()`].
    pub fn is_none(&self) -> bool {
        self.0.is_empty()
    }
}

impl Default for SubtypeKey {
    fn default() -> Self {
        Self::none()
    }
}

/// The payload of an [`Ingredient`], one variant per built-in type.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum IngredientValue {
    /// A registered item by dense id.
    Item(ItemId),
    /// A fluid by name (placeholder).
    Fluid(Namespaced),
    /// A tag by name; matches any member.
    Tag(Namespaced),
    /// An info page by id; matches nothing.
    Info(Namespaced),
}

/// What the browser lists: a typed value plus its subtype key.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Ingredient {
    /// Which [`IngredientType`] interprets this.
    pub ty: IngredientTypeId,
    /// The payload.
    pub value: IngredientValue,
    /// Subtype key; [`SubtypeKey::none()`] for most entries.
    #[serde(default, skip_serializing_if = "SubtypeKey::is_none")]
    pub subtype: SubtypeKey,
}

impl Ingredient {
    /// An item ingredient without subtype.
    pub fn item(id: ItemId) -> Self {
        Self::item_with(id, SubtypeKey::none())
    }

    /// An item ingredient with a subtype key.
    pub fn item_with(id: ItemId, subtype: SubtypeKey) -> Self {
        Self {
            ty: types::item(),
            value: IngredientValue::Item(id),
            subtype,
        }
    }

    /// A tag ingredient.
    pub fn tag(name: Namespaced) -> Self {
        Self {
            ty: types::tag(),
            value: IngredientValue::Tag(name),
            subtype: SubtypeKey::none(),
        }
    }

    /// An info page ingredient.
    pub fn info(id: Namespaced) -> Self {
        Self {
            ty: types::info(),
            value: IngredientValue::Info(id),
            subtype: SubtypeKey::none(),
        }
    }

    /// The item id if this is an item ingredient.
    pub fn item_id(&self) -> Option<ItemId> {
        match self.value {
            IngredientValue::Item(id) => Some(id),
            _ => None,
        }
    }

    /// The same ingredient with the subtype cleared, for wildcard lookups.
    #[must_use]
    pub fn without_subtype(&self) -> Self {
        Self {
            subtype: SubtypeKey::none(),
            ..self.clone()
        }
    }
}

/// What every type method can look at.
#[derive(Clone, Copy)]
pub struct IngredientCtx<'a> {
    /// The frozen registries.
    pub registries: &'a FrozenRegistries,
    /// Registered subtype interpreters.
    pub subtypes: &'a Subtypes,
    /// The localisation port, for a definition whose display name is a key.
    pub loc: &'a slotted_ui::Localization,
}

/// Decides which component patches make a distinct browser entry.
pub trait SubtypeInterpreter: Send + Sync {
    /// The key of a concrete stack.
    fn key(&self, stack: &ItemStack, registries: &FrozenRegistries) -> SubtypeKey;
    /// The keys to expand an item into when building the entry list.
    fn variants(&self, item: ItemId, registries: &FrozenRegistries) -> Vec<SubtypeKey>;
}

/// The interpreter every item gets unless one is registered: no subtypes.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoSubtypes;

impl SubtypeInterpreter for NoSubtypes {
    fn key(&self, _stack: &ItemStack, _registries: &FrozenRegistries) -> SubtypeKey {
        SubtypeKey::none()
    }

    fn variants(&self, _item: ItemId, _registries: &FrozenRegistries) -> Vec<SubtypeKey> {
        vec![SubtypeKey::none()]
    }
}

/// Registered subtype interpreters, by item name. Filled in
/// [`BrowserPhase::Subtypes`](crate::BrowserPhase::Subtypes).
#[derive(Resource, Default, Clone)]
pub struct Subtypes {
    by_item: BTreeMap<Namespaced, Arc<dyn SubtypeInterpreter>>,
    fallback: Option<Arc<dyn SubtypeInterpreter>>,
}

impl Subtypes {
    /// Registers an interpreter for one item; replaces an earlier one.
    pub fn register(&mut self, item: Namespaced, interpreter: Arc<dyn SubtypeInterpreter>) {
        self.by_item.insert(item, interpreter);
    }

    /// The interpreter for an item, or [`NoSubtypes`].
    pub fn for_item(&self, item: &Namespaced) -> Arc<dyn SubtypeInterpreter> {
        self.by_item
            .get(item)
            .cloned()
            .or_else(|| self.fallback.clone())
            .unwrap_or_else(|| Arc::new(NoSubtypes))
    }

    /// Sets the interpreter used for items without their own registration.
    pub fn set_fallback(&mut self, interpreter: Arc<dyn SubtypeInterpreter>) {
        self.fallback = Some(interpreter);
    }

    /// Items with a registered interpreter.
    pub fn items(&self) -> impl Iterator<Item = &Namespaced> {
        self.by_item.keys()
    }

    /// Drops one registration; validation removes dangling ones.
    pub fn remove(&mut self, item: &Namespaced) -> Option<Arc<dyn SubtypeInterpreter>> {
        self.by_item.remove(item)
    }
}

/// A family of ingredients: how to name, group, tag, draw and match them.
pub trait IngredientType: Send + Sync {
    /// The type id.
    fn id(&self) -> IngredientTypeId;
    /// Player-facing name; indexed under the unprefixed field.
    fn display_name(&self, ing: &Ingredient, ctx: &IngredientCtx<'_>) -> String;
    /// The `@` field: the namespace of the mod that owns the entry.
    fn mod_namespace(&self, ing: &Ingredient, ctx: &IngredientCtx<'_>) -> String;
    /// The `#` field.
    fn tags(&self, ing: &Ingredient, ctx: &IngredientCtx<'_>) -> Vec<String>;
    /// The `$` field: lines the tooltip would show.
    fn tooltip_text(&self, ing: &Ingredient, ctx: &IngredientCtx<'_>) -> Vec<String>;
    /// The icon to draw on a card or recipe slot.
    fn icon(&self, ing: &Ingredient, icons: &dyn IconSource) -> IconRef;
    /// Every entry of this type, in the order it should sort under
    /// `SortStage::Registration`.
    fn entries(&self, ctx: &IngredientCtx<'_>) -> Vec<Ingredient>;
    /// Whether a concrete stack satisfies this ingredient.
    fn matches(&self, ing: &Ingredient, stack: &ItemStack, ctx: &IngredientCtx<'_>) -> bool;
    /// A stack for cheat-give and ghost hints, if the type has one.
    fn as_stack(&self, ing: &Ingredient, count: u32) -> Option<ItemStack>;
}

/// Registered ingredient types. Filled in
/// [`BrowserPhase::IngredientTypes`](crate::BrowserPhase::IngredientTypes).
#[derive(Resource, Default, Clone)]
pub struct IngredientTypes {
    types: Vec<Arc<dyn IngredientType>>,
}

impl IngredientTypes {
    /// Registers a type; a second registration of the same id replaces the
    /// first and keeps its position.
    pub fn register(&mut self, ty: Arc<dyn IngredientType>) {
        let id = ty.id();
        match self.types.iter().position(|t| t.id() == id) {
            Some(i) => self.types[i] = ty,
            None => self.types.push(ty),
        }
    }

    /// The type by id.
    pub fn get(&self, id: &IngredientTypeId) -> Option<&Arc<dyn IngredientType>> {
        self.types.iter().find(|t| &t.id() == id)
    }

    /// Types in registration order.
    pub fn iter(&self) -> impl ExactSizeIterator<Item = &Arc<dyn IngredientType>> {
        self.types.iter()
    }

    /// Whether `stack` satisfies `ing`, through `ing`'s type.
    pub fn matches(&self, ing: &Ingredient, stack: &ItemStack, ctx: &IngredientCtx<'_>) -> bool {
        self.get(&ing.ty)
            .is_some_and(|t| t.matches(ing, stack, ctx))
    }

    /// Display name through the type; the raw value on an unknown type.
    pub fn display_name(&self, ing: &Ingredient, ctx: &IngredientCtx<'_>) -> String {
        self.get(&ing.ty)
            .map_or_else(|| format!("{:?}", ing.value), |t| t.display_name(ing, ctx))
    }

    /// The built-in four, in the order the panel sorts them.
    pub fn with_builtins() -> Self {
        let mut types = Self::default();
        types.register(Arc::new(ItemType));
        types.register(Arc::new(FluidType));
        types.register(Arc::new(TagType));
        types.register(Arc::new(InfoType::default()));
        types
    }
}

/// `slotted:item`: every registered item, expanded by subtype variants.
#[derive(Debug, Default, Clone, Copy)]
pub struct ItemType;

impl IngredientType for ItemType {
    fn id(&self) -> IngredientTypeId {
        types::item()
    }

    /// `ItemDef::display_name` is documented as "a localisation key or literal
    /// display name", and a mod writes a key. It is resolved through the port
    /// here, which is the one place a card, the search index and an ingredient
    /// tooltip all read, so they cannot disagree. A key nothing defines stays
    /// verbatim, and an item with no display name at all falls back to its id.
    fn display_name(&self, ing: &Ingredient, ctx: &IngredientCtx<'_>) -> String {
        let Some(id) = ing.item_id() else {
            return String::new();
        };
        let def = ctx.registries.items.get(id);
        def.and_then(|d| d.display_name.as_deref())
            .map(|name| ctx.loc.text_for(name))
            .or_else(|| ctx.registries.items.name_of(id).map(ToString::to_string))
            .unwrap_or_default()
    }

    fn mod_namespace(&self, ing: &Ingredient, ctx: &IngredientCtx<'_>) -> String {
        ing.item_id()
            .and_then(|id| ctx.registries.items.name_of(id))
            .map(|n| n.namespace().to_owned())
            .unwrap_or_default()
    }

    fn tags(&self, ing: &Ingredient, ctx: &IngredientCtx<'_>) -> Vec<String> {
        ing.item_id()
            .map(|id| {
                ctx.registries
                    .tag_index
                    .tags_of(id)
                    .iter()
                    .map(ToString::to_string)
                    .collect()
            })
            .unwrap_or_default()
    }

    fn tooltip_text(&self, ing: &Ingredient, ctx: &IngredientCtx<'_>) -> Vec<String> {
        // `TooltipParts` lives in the ui crate and needs a world, so the index
        // uses what the item definition itself declares: the name, the rarity
        // and the `slotted:*` component keys.
        let mut out = vec![self.display_name(ing, ctx)];
        let Some(def) = ing.item_id().and_then(|id| ctx.registries.items.get(id)) else {
            return out;
        };
        out.push(def.rarity.as_str().to_owned());
        out.extend(
            def.components
                .keys()
                .filter(|k| k.namespace() == "slotted")
                .map(ToString::to_string),
        );
        if !ing.subtype.is_none() {
            out.push(ing.subtype.0.clone());
        }
        out
    }

    fn icon(&self, ing: &Ingredient, icons: &dyn IconSource) -> IconRef {
        self.as_stack(ing, 1)
            .map_or(IconRef::Missing, |s| icons.icon(&s))
    }

    fn entries(&self, ctx: &IngredientCtx<'_>) -> Vec<Ingredient> {
        let mut out = Vec::with_capacity(ctx.registries.items.len());
        for (id, name, _) in ctx.registries.items.iter() {
            for key in ctx.subtypes.for_item(name).variants(id, ctx.registries) {
                out.push(Ingredient::item_with(id, key));
            }
        }
        out
    }

    fn matches(&self, ing: &Ingredient, stack: &ItemStack, ctx: &IngredientCtx<'_>) -> bool {
        let Some(id) = ing.item_id() else {
            return false;
        };
        if stack.id != id {
            return false;
        }
        if ing.subtype.is_none() {
            return true;
        }
        let name = ctx.registries.items.name_of(id);
        name.is_some_and(|n| ctx.subtypes.for_item(n).key(stack, ctx.registries) == ing.subtype)
    }

    fn as_stack(&self, ing: &Ingredient, count: u32) -> Option<ItemStack> {
        ing.item_id().map(|id| ItemStack::new(id, count))
    }
}

/// `slotted:fluid`: a placeholder with no entries until fluids exist.
#[derive(Debug, Default, Clone, Copy)]
pub struct FluidType;

impl IngredientType for FluidType {
    fn id(&self) -> IngredientTypeId {
        types::fluid()
    }
    fn display_name(&self, ing: &Ingredient, _ctx: &IngredientCtx<'_>) -> String {
        match &ing.value {
            IngredientValue::Fluid(n) => n.path().to_owned(),
            _ => String::new(),
        }
    }
    fn mod_namespace(&self, ing: &Ingredient, _ctx: &IngredientCtx<'_>) -> String {
        match &ing.value {
            IngredientValue::Fluid(n) => n.namespace().to_owned(),
            _ => String::new(),
        }
    }
    fn tags(&self, _ing: &Ingredient, _ctx: &IngredientCtx<'_>) -> Vec<String> {
        Vec::new()
    }
    fn tooltip_text(&self, _ing: &Ingredient, _ctx: &IngredientCtx<'_>) -> Vec<String> {
        Vec::new()
    }
    fn icon(&self, _ing: &Ingredient, _icons: &dyn IconSource) -> IconRef {
        IconRef::Missing
    }
    fn entries(&self, _ctx: &IngredientCtx<'_>) -> Vec<Ingredient> {
        Vec::new()
    }
    fn matches(&self, _ing: &Ingredient, _stack: &ItemStack, _ctx: &IngredientCtx<'_>) -> bool {
        false
    }
    fn as_stack(&self, _ing: &Ingredient, _count: u32) -> Option<ItemStack> {
        None
    }
}

/// `slotted:tag`: one entry per tag name; matches any member.
#[derive(Debug, Default, Clone, Copy)]
pub struct TagType;

impl IngredientType for TagType {
    fn id(&self) -> IngredientTypeId {
        types::tag()
    }
    fn display_name(&self, ing: &Ingredient, _ctx: &IngredientCtx<'_>) -> String {
        match &ing.value {
            IngredientValue::Tag(n) => format!("#{n}"),
            _ => String::new(),
        }
    }
    fn mod_namespace(&self, ing: &Ingredient, _ctx: &IngredientCtx<'_>) -> String {
        match &ing.value {
            IngredientValue::Tag(n) => n.namespace().to_owned(),
            _ => String::new(),
        }
    }
    fn tags(&self, ing: &Ingredient, _ctx: &IngredientCtx<'_>) -> Vec<String> {
        match &ing.value {
            IngredientValue::Tag(n) => vec![n.to_string()],
            _ => Vec::new(),
        }
    }
    fn tooltip_text(&self, ing: &Ingredient, ctx: &IngredientCtx<'_>) -> Vec<String> {
        let mut out = vec![self.display_name(ing, ctx)];
        if let IngredientValue::Tag(name) = &ing.value {
            out.extend(
                ctx.registries
                    .items_with(name)
                    .iter()
                    .filter_map(|id| ctx.registries.items.name_of(*id))
                    .map(ToString::to_string),
            );
        }
        out
    }
    /// A tag has no icon of its own. `icon` sees no registries, so the panel
    /// draws the members it cycles through instead of asking here.
    fn icon(&self, _ing: &Ingredient, _icons: &dyn IconSource) -> IconRef {
        IconRef::Missing
    }
    fn entries(&self, ctx: &IngredientCtx<'_>) -> Vec<Ingredient> {
        ctx.registries
            .tag_names()
            .map(|n| Ingredient::tag(n.clone()))
            .collect()
    }
    fn matches(&self, ing: &Ingredient, stack: &ItemStack, ctx: &IngredientCtx<'_>) -> bool {
        match &ing.value {
            IngredientValue::Tag(n) => ctx.registries.tag_index.has_tag(stack.id, n),
            _ => false,
        }
    }
    fn as_stack(&self, _ing: &Ingredient, _count: u32) -> Option<ItemStack> {
        None
    }
}

/// One `slotted:info` entry: a titled page of text in the browser.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InfoPage {
    /// Its id.
    pub id: Namespaced,
    /// Card title.
    pub title: String,
    /// Body lines, also indexed as tooltip text.
    pub body: Vec<String>,
}

/// Registered info pages.
#[derive(Resource, Default, Debug, Clone)]
pub struct InfoPages(pub Vec<InfoPage>);

/// `slotted:info`: entries from [`InfoPages`]; matches nothing.
///
/// The pages are carried by the type rather than looked up through
/// [`IngredientCtx`], which holds registries and subtypes only. The index
/// builder re-registers this type from the [`InfoPages`] resource before every
/// build, so a page registered in any phase is indexed.
#[derive(Debug, Default, Clone)]
pub struct InfoType {
    pages: Arc<Vec<InfoPage>>,
}

impl InfoType {
    /// A type over these pages.
    pub fn new(pages: impl Into<Arc<Vec<InfoPage>>>) -> Self {
        Self {
            pages: pages.into(),
        }
    }

    fn page(&self, ing: &Ingredient) -> Option<&InfoPage> {
        let IngredientValue::Info(id) = &ing.value else {
            return None;
        };
        self.pages.iter().find(|p| p.id == *id)
    }
}

impl IngredientType for InfoType {
    fn id(&self) -> IngredientTypeId {
        types::info()
    }
    fn display_name(&self, ing: &Ingredient, _ctx: &IngredientCtx<'_>) -> String {
        if let Some(page) = self.page(ing) {
            return page.title.clone();
        }
        match &ing.value {
            IngredientValue::Info(n) => n.path().to_owned(),
            _ => String::new(),
        }
    }
    fn mod_namespace(&self, ing: &Ingredient, _ctx: &IngredientCtx<'_>) -> String {
        match &ing.value {
            IngredientValue::Info(n) => n.namespace().to_owned(),
            _ => String::new(),
        }
    }
    fn tags(&self, _ing: &Ingredient, _ctx: &IngredientCtx<'_>) -> Vec<String> {
        Vec::new()
    }
    fn tooltip_text(&self, ing: &Ingredient, _ctx: &IngredientCtx<'_>) -> Vec<String> {
        self.page(ing).map(|p| p.body.clone()).unwrap_or_default()
    }
    fn icon(&self, _ing: &Ingredient, _icons: &dyn IconSource) -> IconRef {
        IconRef::Missing
    }
    fn entries(&self, _ctx: &IngredientCtx<'_>) -> Vec<Ingredient> {
        self.pages
            .iter()
            .map(|p| Ingredient::info(p.id.clone()))
            .collect()
    }
    fn matches(&self, _ing: &Ingredient, _stack: &ItemStack, _ctx: &IngredientCtx<'_>) -> bool {
        false
    }
    fn as_stack(&self, _ing: &Ingredient, _count: u32) -> Option<ItemStack> {
        None
    }
}
