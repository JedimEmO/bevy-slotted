//! Recipe transfer: planning the clicks that move ingredients from the
//! player's inventories into a screen's crafting grid. A plan is a sequence
//! of `ClickAction`s triggered as `MenuAction`s, so prediction, authority and
//! conservation are untouched. Contract section 5. Package A.

use std::collections::BTreeMap;
use std::sync::Arc;

use bevy::prelude::*;
use slotted_model::{
    Actor, Button, ClickAction, Inventories, InventoryRef, LookupCtx, MenuDef, MenuState, SlotIx,
};
use slotted_ui::ScreenKind;
use slotted_ui::widgets::SLOT_SIZE;

use crate::category::{CategoryId, RecipeLayout, RecipeSlotIx, SlotRole};
use crate::ingredient::{IngredientCtx, IngredientTypes};

/// What a handler sees: the open menu's definition and current contents.
pub struct TransferCtx<'a> {
    /// The menu definition.
    pub def: &'a MenuDef,
    /// Current inventories, assembled from the menu's entities.
    pub inventories: &'a Inventories,
    /// Carried stack and state id.
    pub state: &'a MenuState,
    /// Who is clicking.
    pub actor: Actor,
    /// Stack sizes and tags.
    pub lookup: &'a dyn LookupCtx,
    /// For `matches`.
    pub types: &'a IngredientTypes,
    /// For `matches`.
    pub ctx: IngredientCtx<'a>,
}

/// Why a transfer cannot happen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TransferErrorKind {
    /// No handler for this screen and category.
    NoHandler,
    /// The carried stack must be empty first.
    CursorNotEmpty,
    /// Some inputs have no matching stack; see `missing`.
    MissingIngredients,
    /// A grid cell holds something the recipe does not want there.
    GridNotEmpty,
}

/// A refused transfer, with the slots to paint red.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{message}")]
pub struct TransferError {
    /// Which kind.
    pub kind: TransferErrorKind,
    /// Recipe slots without a source.
    pub missing: Vec<RecipeSlotIx>,
    /// Player-facing text.
    pub message: String,
}

impl TransferError {
    /// An error of `kind` with no missing slots.
    pub fn new(kind: TransferErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            missing: Vec::new(),
            message: message.into(),
        }
    }
}

/// The clicks to perform, in order, in one frame.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TransferPlan {
    /// Ordinary click actions on the open menu.
    pub actions: Vec<ClickAction>,
}

/// Plans a transfer for one screen and category.
pub trait TransferHandler: Send + Sync {
    /// Builds the click sequence, or explains why it cannot. `max` fills as
    /// many sets as fit.
    fn plan(
        &self,
        ctx: &TransferCtx<'_>,
        layout: &RecipeLayout,
        max: bool,
    ) -> Result<TransferPlan, TransferError>;

    /// The `+` button's state: plan and discard.
    fn dry_run(&self, ctx: &TransferCtx<'_>, layout: &RecipeLayout) -> Result<(), TransferError> {
        self.plan(ctx, layout, false).map(|_| ())
    }
}

/// Handlers keyed by screen kind and category; `None` is the screen's
/// fallback for every category.
#[derive(Resource, Default, Clone)]
pub struct TransferHandlers {
    map: BTreeMap<(ScreenKind, Option<CategoryId>), Arc<dyn TransferHandler>>,
}

impl TransferHandlers {
    /// Registers a handler.
    pub fn register(
        &mut self,
        screen: ScreenKind,
        category: Option<CategoryId>,
        handler: Arc<dyn TransferHandler>,
    ) {
        self.map.insert((screen, category), handler);
    }

    /// The handler for a screen and category, falling back to the screen's
    /// `None` entry.
    pub fn get(
        &self,
        screen: &ScreenKind,
        category: &CategoryId,
    ) -> Option<&Arc<dyn TransferHandler>> {
        self.map
            .get(&(screen.clone(), Some(category.clone())))
            .or_else(|| self.map.get(&(screen.clone(), None)))
    }

    /// Removes one registration.
    pub fn remove(&mut self, screen: &ScreenKind, category: Option<&CategoryId>) {
        self.map.remove(&(screen.clone(), category.cloned()));
    }

    /// Registered keys.
    pub fn keys(&self) -> impl Iterator<Item = &(ScreenKind, Option<CategoryId>)> {
        self.map.keys()
    }
}

/// The handler most screens need: a grid inventory fed from source
/// inventories, JEI's `IRecipeTransferInfo` / EMI's `StandardRecipeHandler`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimpleTransfer {
    /// The crafting grid inventory.
    pub grid: InventoryRef,
    /// Grid width in cells; recipe slot positions map to cells through it.
    pub grid_cols: u16,
    /// Where ingredients come from, in preference order.
    pub sources: Vec<InventoryRef>,
}

impl SimpleTransfer {
    /// The grid cell a recipe slot at `pos` (px) lands in.
    pub fn cell_of(&self, pos: Vec2) -> u16 {
        let cell = SLOT_SIZE + crate::category::SLOT_GAP;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let (col, row) = ((pos.x / cell).floor() as u16, (pos.y / cell).floor() as u16);
        row * self.grid_cols + col
    }

    /// Menu slots of the sources, in order.
    pub fn source_slots(&self, def: &MenuDef) -> Vec<SlotIx> {
        self.sources
            .iter()
            .flat_map(|inv| def.slots_of(*inv))
            .collect()
    }
}

impl SimpleTransfer {
    /// The grid slot each `Input` recipe slot maps to, refusing when a cell
    /// falls outside the menu's grid.
    fn cells(&self, ctx: &TransferCtx<'_>, layout: &RecipeLayout) -> Vec<(RecipeSlotIx, SlotIx)> {
        layout
            .with_role(SlotRole::Input)
            .filter_map(|(ix, slot)| {
                ctx.def
                    .slot_of(self.grid, self.cell_of(slot.pos))
                    .map(|cell| (ix, cell))
            })
            .collect()
    }

    /// Refuses when any grid slot holds a stack the recipe does not want in
    /// that cell, which includes every occupied cell the recipe does not use.
    fn check_grid(
        &self,
        ctx: &TransferCtx<'_>,
        layout: &RecipeLayout,
    ) -> Result<(), TransferError> {
        let wanted: BTreeMap<SlotIx, &[crate::ingredient::Ingredient]> = layout
            .with_role(SlotRole::Input)
            .filter_map(|(_, slot)| {
                ctx.def
                    .slot_of(self.grid, self.cell_of(slot.pos))
                    .map(|cell| (cell, slot.alternatives.as_slice()))
            })
            .collect();
        for cell in ctx.def.slots_of(self.grid) {
            let Some(stack) = stack_at(ctx, cell) else {
                continue;
            };
            let ok = wanted.get(&cell).is_some_and(|alternatives| {
                alternatives
                    .iter()
                    .any(|alt| ctx.types.matches(alt, stack, &ctx.ctx))
            });
            if !ok {
                return Err(TransferError::new(
                    TransferErrorKind::GridNotEmpty,
                    "clear the crafting grid first",
                ));
            }
        }
        Ok(())
    }

    /// One set: for every input cell the source slot that feeds it. The first
    /// alternative with any match wins, ties by the lowest slot, and a source
    /// slot is only offered while `remaining` says it still holds items.
    fn assign(
        ctx: &TransferCtx<'_>,
        layout: &RecipeLayout,
        cells: &[(RecipeSlotIx, SlotIx)],
        sources: &[SlotIx],
        remaining: &mut BTreeMap<SlotIx, u32>,
    ) -> Result<Vec<(SlotIx, SlotIx)>, Vec<RecipeSlotIx>> {
        let mut plan = Vec::new();
        let mut missing = Vec::new();
        for (ix, cell) in cells {
            let slot = &layout.slots[usize::from(ix.0)];
            let found = slot.alternatives.iter().find_map(|alt| {
                sources.iter().copied().find(|s| {
                    remaining.get(s).copied().unwrap_or(0) > 0
                        && stack_at(ctx, *s).is_some_and(|st| ctx.types.matches(alt, st, &ctx.ctx))
                })
            });
            match found {
                Some(src) => {
                    *remaining.entry(src).or_default() -= 1;
                    plan.push((src, *cell));
                }
                None => missing.push(*ix),
            }
        }
        if missing.is_empty() {
            Ok(plan)
        } else {
            Err(missing)
        }
    }

    /// How many sets the smallest input cell can hold.
    fn cell_cap(ctx: &TransferCtx<'_>, plan: &[(SlotIx, SlotIx)]) -> u32 {
        plan.iter()
            .filter_map(|(src, cell)| {
                let stack = stack_at(ctx, *src)?;
                let def = ctx.def.slot(*cell)?;
                Some(def.cap(stack.id, ctx.lookup))
            })
            .min()
            .unwrap_or(1)
            .max(1)
    }
}

/// The stack in a menu slot, if any.
fn stack_at<'a>(ctx: &'a TransferCtx<'_>, slot: SlotIx) -> Option<&'a slotted_model::ItemStack> {
    let def = ctx.def.slot(slot)?;
    ctx.inventories.get(def.source)?.get(usize::from(def.index))
}

impl TransferHandler for SimpleTransfer {
    /// Refuses a full cursor or a dirty grid, assigns one source slot per
    /// input, then emits the clicks grouped by source: pick the source stack
    /// up, right-click one item into each cell it feeds, and put the remainder
    /// back into the source slot, which the pickup left empty. Nothing is
    /// returned when the source ran out, because the cursor is empty then.
    fn plan(
        &self,
        ctx: &TransferCtx<'_>,
        layout: &RecipeLayout,
        max: bool,
    ) -> Result<TransferPlan, TransferError> {
        if ctx.state.carried.is_some() {
            return Err(TransferError::new(
                TransferErrorKind::CursorNotEmpty,
                "empty your cursor first",
            ));
        }
        self.check_grid(ctx, layout)?;
        let cells = self.cells(ctx, layout);
        let inputs = layout.with_role(SlotRole::Input).count();
        if cells.len() != inputs {
            return Err(TransferError {
                kind: TransferErrorKind::MissingIngredients,
                missing: layout
                    .with_role(SlotRole::Input)
                    .filter(|(_, s)| ctx.def.slot_of(self.grid, self.cell_of(s.pos)).is_none())
                    .map(|(ix, _)| ix)
                    .collect(),
                message: "the recipe does not fit this grid".to_owned(),
            });
        }
        let sources = self.source_slots(ctx.def);
        let mut remaining: BTreeMap<SlotIx, u32> = sources
            .iter()
            .map(|s| (*s, stack_at(ctx, *s).map_or(0, |st| st.count)))
            .collect();

        let first =
            Self::assign(ctx, layout, &cells, &sources, &mut remaining).map_err(|missing| {
                TransferError {
                    kind: TransferErrorKind::MissingIngredients,
                    missing,
                    message: "missing ingredients".to_owned(),
                }
            })?;
        let cap = Self::cell_cap(ctx, &first);
        let mut rounds = vec![first];
        if max {
            while u32::try_from(rounds.len()).unwrap_or(u32::MAX) < cap {
                match Self::assign(ctx, layout, &cells, &sources, &mut remaining) {
                    Ok(round) => rounds.push(round),
                    Err(_) => break,
                }
            }
        }

        let mut actions = Vec::new();
        let mut left: BTreeMap<SlotIx, u32> = sources
            .iter()
            .map(|s| (*s, stack_at(ctx, *s).map_or(0, |st| st.count)))
            .collect();
        for round in rounds {
            let mut by_source: BTreeMap<SlotIx, Vec<SlotIx>> = BTreeMap::new();
            let mut order: Vec<SlotIx> = Vec::new();
            for (src, cell) in round {
                if !by_source.contains_key(&src) {
                    order.push(src);
                }
                by_source.entry(src).or_default().push(cell);
            }
            for src in order {
                let placed = u32::try_from(by_source[&src].len()).unwrap_or(u32::MAX);
                let held = left.entry(src).or_default();
                let returns = *held > placed;
                *held = held.saturating_sub(placed);
                actions.push(ClickAction::Pickup {
                    slot: src,
                    button: Button::Left,
                });
                for cell in &by_source[&src] {
                    actions.push(ClickAction::Pickup {
                        slot: *cell,
                        button: Button::Right,
                    });
                }
                if returns {
                    actions.push(ClickAction::Pickup {
                        slot: src,
                        button: Button::Left,
                    });
                }
            }
        }
        Ok(TransferPlan { actions })
    }
}
