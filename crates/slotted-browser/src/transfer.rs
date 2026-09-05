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

impl TransferHandler for SimpleTransfer {
    fn plan(
        &self,
        ctx: &TransferCtx<'_>,
        layout: &RecipeLayout,
        _max: bool,
    ) -> Result<TransferPlan, TransferError> {
        // PHASE3-IMPL: A — grouping by source, returning remainders,
        // `GridNotEmpty`, `max`. The skeleton finds one source per input and
        // refuses when any input has none, which is the dry-run contract.
        if ctx.state.carried.is_some() {
            return Err(TransferError::new(
                TransferErrorKind::CursorNotEmpty,
                "empty your cursor first",
            ));
        }
        let sources = self.source_slots(ctx.def);
        let mut missing = Vec::new();
        let mut actions = Vec::new();
        for (ix, slot) in layout.with_role(SlotRole::Input) {
            let found = sources.iter().copied().find(|s| {
                let sd = &ctx.def.slots[s.index()];
                ctx.inventories[sd.source]
                    .get(usize::from(sd.index))
                    .is_some_and(|stack| {
                        slot.alternatives
                            .iter()
                            .any(|alt| ctx.types.matches(alt, stack, &ctx.ctx))
                    })
            });
            match (found, ctx.def.slot_of(self.grid, self.cell_of(slot.pos))) {
                (Some(src), Some(cell)) => {
                    actions.push(ClickAction::Pickup {
                        slot: src,
                        button: Button::Left,
                    });
                    actions.push(ClickAction::Pickup {
                        slot: cell,
                        button: Button::Right,
                    });
                    actions.push(ClickAction::Pickup {
                        slot: src,
                        button: Button::Left,
                    });
                }
                _ => missing.push(ix),
            }
        }
        if missing.is_empty() {
            Ok(TransferPlan { actions })
        } else {
            Err(TransferError {
                kind: TransferErrorKind::MissingIngredients,
                missing,
                message: "missing ingredients".to_owned(),
            })
        }
    }
}
