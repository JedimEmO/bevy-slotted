//! Namespaced registries for slotted, and the data stage that fills them.
//!
//! Registries are open during the data stage and frozen before the control
//! stage runs. Freezing interns every [`Namespaced`](slotted_model::Namespaced)
//! id into a dense numeric handle and hands back an immutable registry, so all
//! later lookups are pure functions over fixed data. See `docs/PLAN.md`
//! section 4.2.
//!
//! # The shape of a load
//!
//! 1. Read every `mod.toml` into a [`ModManifest`] and
//!    sort them with [`resolve_load_order`].
//! 2. Hand that order to a [`DataStage`] and point it at an
//!    [`AssetSource`].
//! 3. The stage reads every mod's entry files, then runs three patch rounds
//!    over them ([`Round`]), then deserialises the results into
//!    typed defs and freezes.
//! 4. What comes back is a [`FrozenRegistries`]:
//!    dense ids, resolved tags, and a recipe index.
//!
//! Steps 1 and 2 need IO, so this crate does neither. It owns the port and the
//! pure logic; `slotted-packs` will own the adapter that layers mods and
//! resource packs behind the same trait.
//!
//! # Building a registry set in code
//!
//! Nothing here requires files. A game with no mod support at all can register
//! everything in Rust and freeze:
//!
//! ```
//! use slotted_registry::defs::{Ingredient, ItemDef, ItemResult, RecipeDef, RecipeTypeDef, TagDef, TagEntry};
//! use slotted_registry::registry::Registries;
//! use slotted_model::Namespaced;
//!
//! let id = |s: &str| Namespaced::parse(s).expect("a valid id");
//!
//! let mut registries = Registries::new();
//! registries.add_item(ItemDef::new(id("demo:oak_plank")))?;
//! registries.add_item(ItemDef::new(id("demo:birch_plank")))?;
//! let mut chest = ItemDef::new(id("demo:chest"));
//! chest.max_stack_size = 16;
//! registries.add_item(chest)?;
//!
//! // Both planks go in one tag, so the recipe can accept either.
//! registries.add_tag(TagDef {
//!     name: id("demo:planks"),
//!     values: vec![
//!         TagEntry::Item(id("demo:oak_plank")),
//!         TagEntry::Item(id("demo:birch_plank")),
//!     ],
//!     replace: false,
//! });
//!
//! registries.add_recipe_type(RecipeTypeDef::new(id("demo:crafting")))?;
//! registries.add_recipe(RecipeDef::shapeless(
//!     id("demo:chest"),
//!     id("demo:crafting"),
//!     vec![Ingredient::Tag(id("demo:planks"))],
//!     ItemResult::one(id("demo:chest")),
//! ))?;
//!
//! let (frozen, warnings) = registries.freeze()?;
//! assert!(warnings.is_empty());
//!
//! // Look a recipe up by what it produces.
//! let chest_id = frozen.item_id(&id("demo:chest")).expect("registered above");
//! let [recipe] = frozen.recipe_index.for_output(chest_id) else {
//!     panic!("exactly one recipe makes a chest");
//! };
//! assert_eq!(frozen.recipes.name_of(*recipe), Some(&id("demo:chest")));
//!
//! // And by what it consumes, through the tag.
//! let birch = frozen.item_id(&id("demo:birch_plank")).expect("registered above");
//! assert_eq!(frozen.recipe_index.uses(birch), [*recipe]);
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

pub mod defs;
pub mod index;
pub mod interner;
pub mod loader;
pub mod manifest;
pub mod patch;
pub mod registry;
pub mod ron_value;

/// The untyped value a data file parses to before it becomes a typed def.
///
/// This is RON's own [`Value`](ron::Value). Patches run against it, which is
/// why every def type has to survive a round trip through it; see
/// [`defs`] for what that rules out.
pub type Value = ron::Value;

pub use defs::{
    FluidDef, HudLayerDef, Ingredient, IngredientTypeDef, ItemDef, ItemResult, Rarity, RecipeDef,
    RecipeTypeDef, ScreenDef, TagDef, TagEntry, TooltipComponentDef, WidgetDef,
};
pub use index::{RecipeIndex, TagIndex};
pub use interner::{InternedId, Interner, RegistryId};
pub use loader::{
    AssetSource, DataStage, InMemorySource, LoadError, LoadReport, Loaded, SourceError,
};
pub use manifest::{
    Dependency, DependencyKind, LoadOrder, LoadOrderError, ManifestError, ModId, ModManifest,
    resolve_load_order,
};
pub use patch::{Patch, PatchError, PatchList, PatchOp, Round};
pub use registry::{
    FrozenRegistries, Registries, Registry, RegistryBuilder, RegistryError, RegistryKind, Warning,
};
pub use ron_value::{ConvertError, from_model, to_model};

#[cfg(feature = "std-fs")]
pub use loader::DirSource;
