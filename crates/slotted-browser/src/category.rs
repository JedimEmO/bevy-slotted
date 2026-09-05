//! Recipe categories: how a recipe of some type is laid out as slots with
//! roles. The browser owns hit-testing, cycling, focus highlighting and
//! bookmarking; a category only places things. Contract section 3. Package A.

use bevy::math::{Rect, Vec2};
use serde::{Deserialize, Serialize};
use slotted_model::Namespaced;
use slotted_registry::FrozenRegistries;
use slotted_registry::defs::{Ingredient as RegistryIngredient, RecipeDef};
use slotted_registry::registry::RecipeId;
use slotted_ui::widgets::SLOT_SIZE;
use slotted_ui::{IconDef, LocKey, UiNodeDef};

use crate::ingredient::{Ingredient, IngredientTypes};

/// Identifies a [`RecipeCategory`]; the default categories reuse the recipe
/// type's id.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CategoryId(pub Namespaced);

impl CategoryId {
    /// Parses `ns:path`. Panics on a malformed id.
    pub fn new(id: &str) -> Self {
        Self(Namespaced::parse(id).unwrap_or_else(|e| panic!("bad CategoryId {id:?}: {e}")))
    }
}

/// A recipe the browser can show. Phase 3: registry recipes only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RecipeRef(pub RecipeId);

/// What a slot in a recipe layout is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SlotRole {
    /// Consumed; indexed as "uses"; transferred into the grid.
    Input,
    /// Produced; indexed as "recipes for".
    Output,
    /// Required but not consumed (a workstation); indexed as "uses".
    Catalyst,
    /// Drawn only; not indexed and not transferred.
    RenderOnly,
}

impl SlotRole {
    /// The `role=` tag value the panel writes on the node.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Input => "input",
            Self::Output => "output",
            Self::Catalyst => "catalyst",
            Self::RenderOnly => "render_only",
        }
    }
}

/// Index of a slot within a [`RecipeLayout`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RecipeSlotIx(pub u16);

/// One position in a recipe layout.
#[derive(Debug, Clone, PartialEq)]
pub struct RecipeSlot {
    /// What it is for.
    pub role: SlotRole,
    /// Top-left corner in px inside the category's `size`.
    pub pos: Vec2,
    /// Concrete alternatives (tags already expanded); the panel cycles them.
    pub alternatives: Vec<Ingredient>,
    /// Stack count drawn on the slot.
    pub count: u32,
}

/// A laid-out recipe.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RecipeLayout {
    /// Slots in emission order; [`RecipeSlotIx`] indexes this.
    pub slots: Vec<RecipeSlot>,
    /// Extra nodes the category wants drawn (labels, bars), spawned after the slots.
    pub extras: Vec<UiNodeDef>,
    /// Where the progress arrow goes, if any.
    pub arrow: Option<Rect>,
}

impl RecipeLayout {
    /// Slots with `role`.
    pub fn with_role(&self, role: SlotRole) -> impl Iterator<Item = (RecipeSlotIx, &RecipeSlot)> {
        self.slots
            .iter()
            .enumerate()
            .filter(move |(_, s)| s.role == role)
            .map(|(i, s)| (RecipeSlotIx(narrow(i)), s))
    }
}

fn narrow(i: usize) -> u16 {
    u16::try_from(i).unwrap_or(u16::MAX)
}

/// Collects what a [`RecipeCategory::layout`] emits.
#[derive(Debug, Default)]
pub struct LayoutBuilder {
    layout: RecipeLayout,
}

impl LayoutBuilder {
    /// An empty builder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a slot and returns its index.
    pub fn slot(
        &mut self,
        role: SlotRole,
        pos: Vec2,
        alternatives: Vec<Ingredient>,
        count: u32,
    ) -> RecipeSlotIx {
        self.layout.slots.push(RecipeSlot {
            role,
            pos,
            alternatives,
            count,
        });
        RecipeSlotIx(narrow(self.layout.slots.len() - 1))
    }

    /// Adds a custom node.
    pub fn extra(&mut self, node: UiNodeDef) {
        self.layout.extras.push(node);
    }

    /// Places the progress arrow.
    pub fn arrow(&mut self, rect: Rect) {
        self.layout.arrow = Some(rect);
    }

    /// The finished layout.
    pub fn finish(self) -> RecipeLayout {
        self.layout
    }
}

/// What a category sees when laying out one recipe.
#[derive(Clone, Copy)]
pub struct RecipeView<'a> {
    /// Which recipe.
    pub id: RecipeRef,
    /// Its registry definition.
    pub def: &'a RecipeDef,
    /// For tag expansion and item lookups.
    pub registries: &'a FrozenRegistries,
    /// For matching and naming.
    pub types: &'a IngredientTypes,
    /// The ingredient the player searched from, if any; categories may
    /// reorder alternatives so it shows first.
    pub focus: Option<&'a Ingredient>,
}

impl RecipeView<'_> {
    /// Expands a registry ingredient into concrete browser ingredients:
    /// items stay, tags become their members, `AnyOf` flattens. The focus,
    /// if among them, is moved to the front.
    pub fn expand(&self, ingredient: &RegistryIngredient) -> Vec<Ingredient> {
        let mut out = Vec::new();
        ingredient.walk(&mut |i| match i {
            RegistryIngredient::Item(name) => {
                if let Some(id) = self.registries.item_id(name) {
                    out.push(Ingredient::item(id));
                }
            }
            RegistryIngredient::Tag(tag) => {
                out.extend(
                    self.registries
                        .items_with(tag)
                        .iter()
                        .map(|id| Ingredient::item(*id)),
                );
            }
            RegistryIngredient::AnyOf(_) => {}
        });
        out.dedup();
        if let Some(focus) = self.focus
            && let Some(i) = out
                .iter()
                .position(|o| o.without_subtype() == focus.without_subtype())
        {
            out.swap(0, i);
        }
        out
    }

    /// The recipe's output as a browser ingredient.
    pub fn output(&self) -> Option<Ingredient> {
        self.registries
            .item_id(&self.def.result.item)
            .map(Ingredient::item)
    }
}

/// How recipes of some type are drawn. Registered in
/// [`BrowserPhase::Categories`](crate::BrowserPhase::Categories).
pub trait RecipeCategory: Send + Sync {
    /// Tab identity.
    fn id(&self) -> CategoryId;
    /// Tab title localisation key.
    fn title_key(&self) -> LocKey;
    /// Tab icon.
    fn icon(&self) -> IconDef;
    /// The layout's size in px, independent of the panel.
    fn size(&self) -> Vec2;
    /// The registry recipe types this category lays out.
    fn recipe_types(&self) -> Vec<Namespaced>;
    /// Emits slots and extras for one recipe.
    fn layout(&self, recipe: &RecipeView<'_>, out: &mut LayoutBuilder);
    /// Workstations shown beside the tabs.
    fn catalysts(&self) -> Vec<Ingredient> {
        Vec::new()
    }
}

/// Gap between slots in a layout, px.
pub const SLOT_GAP: f32 = 4.0;
/// Width of the arrow region between inputs and output, px.
pub const ARROW_WIDTH: f32 = 32.0;

/// Shaped and shapeless grid recipes: a `cols x rows` input grid, an arrow,
/// one output.
#[derive(Debug, Clone)]
pub struct CraftingCategory {
    id: CategoryId,
    recipe_type: Namespaced,
    cols: u16,
    rows: u16,
}

impl CraftingCategory {
    /// A grid category for `recipe_type`.
    pub fn new(id: CategoryId, recipe_type: Namespaced, (cols, rows): (u16, u16)) -> Self {
        Self {
            id,
            recipe_type,
            cols: cols.max(1),
            rows: rows.max(1),
        }
    }

    /// Grid size in cells.
    pub const fn grid(&self) -> (u16, u16) {
        (self.cols, self.rows)
    }

    fn cell_pos(col: u16, row: u16) -> Vec2 {
        Vec2::new(
            f32::from(col) * (SLOT_SIZE + SLOT_GAP),
            f32::from(row) * (SLOT_SIZE + SLOT_GAP),
        )
    }
}

impl RecipeCategory for CraftingCategory {
    fn id(&self) -> CategoryId {
        self.id.clone()
    }

    fn title_key(&self) -> LocKey {
        LocKey(format!(
            "category.{}.{}",
            self.id.0.namespace(),
            self.id.0.path()
        ))
    }

    fn icon(&self) -> IconDef {
        IconDef::Image(format!("icons/category/{}.png", self.id.0.path()))
    }

    fn size(&self) -> Vec2 {
        let grid_w = f32::from(self.cols) * (SLOT_SIZE + SLOT_GAP);
        let grid_h = f32::from(self.rows) * (SLOT_SIZE + SLOT_GAP);
        Vec2::new(grid_w + ARROW_WIDTH + SLOT_SIZE + SLOT_GAP, grid_h)
    }

    fn recipe_types(&self) -> Vec<Namespaced> {
        vec![self.recipe_type.clone()]
    }

    fn layout(&self, recipe: &RecipeView<'_>, out: &mut LayoutBuilder) {
        if recipe.def.is_shaped() {
            self.layout_shaped(recipe, out);
        } else {
            self.layout_shapeless(recipe, out);
        }
        let grid_w = f32::from(self.cols) * (SLOT_SIZE + SLOT_GAP);
        let mid_y = (self.size().y - SLOT_SIZE) * 0.5;
        out.arrow(Rect::new(
            grid_w,
            mid_y,
            grid_w + ARROW_WIDTH,
            mid_y + SLOT_SIZE,
        ));
        let output = recipe.output().into_iter().collect();
        out.slot(
            SlotRole::Output,
            Vec2::new(grid_w + ARROW_WIDTH, mid_y),
            output,
            recipe.def.result.count,
        );
    }
}

impl CraftingCategory {
    /// Rows of the `shape`, each character resolved through `key`; a space, or
    /// a character the key does not name, leaves the cell empty. Rows and
    /// columns past the grid are dropped.
    fn layout_shaped(&self, recipe: &RecipeView<'_>, out: &mut LayoutBuilder) {
        let shape = recipe.def.shape.as_ref().expect("caller checked is_shaped");
        for (row, line) in shape.iter().enumerate().take(usize::from(self.rows)) {
            for (col, ch) in line.chars().enumerate().take(usize::from(self.cols)) {
                let Some(ingredient) = recipe.def.key.get(&ch) else {
                    continue;
                };
                let (col, row) = (narrow16(col), narrow16(row));
                out.slot(
                    SlotRole::Input,
                    Self::cell_pos(col, row),
                    recipe.expand(ingredient),
                    1,
                );
            }
        }
    }

    /// Shapeless inputs fill the grid row-major.
    fn layout_shapeless(&self, recipe: &RecipeView<'_>, out: &mut LayoutBuilder) {
        let mut cells = (0..self.rows).flat_map(|r| (0..self.cols).map(move |c| (c, r)));
        for ingredient in &recipe.def.ingredients {
            let Some((c, r)) = cells.next() else { break };
            out.slot(
                SlotRole::Input,
                Self::cell_pos(c, r),
                recipe.expand(ingredient),
                1,
            );
        }
    }
}

fn narrow16(i: usize) -> u16 {
    u16::try_from(i).unwrap_or(u16::MAX)
}

/// One input, an arrow, one output; an optional render-only fuel slot from
/// `RecipeDef::extra.fuel`. The furnace shape.
#[derive(Debug, Clone)]
pub struct ProcessingCategory {
    id: CategoryId,
    recipe_type: Namespaced,
}

impl ProcessingCategory {
    /// A one-input category for `recipe_type`.
    pub fn new(id: CategoryId, recipe_type: Namespaced) -> Self {
        Self { id, recipe_type }
    }
}

impl RecipeCategory for ProcessingCategory {
    fn id(&self) -> CategoryId {
        self.id.clone()
    }

    fn title_key(&self) -> LocKey {
        LocKey(format!(
            "category.{}.{}",
            self.id.0.namespace(),
            self.id.0.path()
        ))
    }

    fn icon(&self) -> IconDef {
        IconDef::Image(format!("icons/category/{}.png", self.id.0.path()))
    }

    fn size(&self) -> Vec2 {
        Vec2::new(
            SLOT_SIZE + SLOT_GAP + ARROW_WIDTH + SLOT_SIZE,
            SLOT_SIZE * 2.0 + SLOT_GAP,
        )
    }

    fn recipe_types(&self) -> Vec<Namespaced> {
        vec![self.recipe_type.clone()]
    }

    fn layout(&self, recipe: &RecipeView<'_>, out: &mut LayoutBuilder) {
        let input = recipe
            .def
            .all_ingredients()
            .next()
            .map(|i| recipe.expand(i))
            .unwrap_or_default();
        out.slot(SlotRole::Input, Vec2::ZERO, input, 1);
        if let Some(fuel) = fuel_ingredient(recipe) {
            out.slot(
                SlotRole::RenderOnly,
                Vec2::new(0.0, SLOT_SIZE + SLOT_GAP),
                fuel,
                1,
            );
        }
        let x = SLOT_SIZE + SLOT_GAP;
        out.arrow(Rect::new(x, 0.0, x + ARROW_WIDTH, SLOT_SIZE));
        out.slot(
            SlotRole::Output,
            Vec2::new(x + ARROW_WIDTH, 0.0),
            recipe.output().into_iter().collect(),
            recipe.def.result.count,
        );
    }
}

/// `RecipeDef::extra` is opaque to the registry. A processing recipe may name
/// its fuel there as `(fuel: "ns:item")` or `(fuel: "#ns:tag")`; anything else
/// means the category draws no fuel slot.
fn fuel_ingredient(recipe: &RecipeView<'_>) -> Option<Vec<Ingredient>> {
    let slotted_registry::Value::Map(map) = &recipe.def.extra else {
        return None;
    };
    let key = slotted_registry::Value::String("fuel".to_owned());
    let slotted_registry::Value::String(text) = map.get(&key)? else {
        return None;
    };
    let ingredient = match text.strip_prefix('#') {
        Some(tag) => RegistryIngredient::Tag(Namespaced::parse(tag).ok()?),
        None => RegistryIngredient::Item(Namespaced::parse(text).ok()?),
    };
    let expanded = recipe.expand(&ingredient);
    (!expanded.is_empty()).then_some(expanded)
}
