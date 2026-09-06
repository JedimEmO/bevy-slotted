//! The click state machine: vanilla's seven `ClickType`s plus toolbar actions.
//!
//! [`apply_click`] is a pure function over a [`MenuDef`], the [`Inventories`]
//! it references and a [`MenuState`]. It mutates the inventories and the state
//! in place and returns a [`Delta`] describing what changed, or a
//! [`ClickError`] having changed nothing (except for the drag state, which
//! [`ClickAction::Drag`] owns). The same function runs on the client for
//! prediction and on the authority for validation.
//!
//! The semantics follow `AbstractContainerMenu.doClick` in Java Edition
//! 1.20+; see `docs/research/research-mc-anatomy.md` section 2 for the table
//! these tests reproduce.

use serde::{Deserialize, Serialize};

pub use crate::error::ClickError;
use crate::id::ItemId;
use crate::inventory::{Inventories, InventoryRef};
use crate::menu::{DragState, LookupCtx, MenuDef, MenuState, SlotBehaviour, SlotDef, SlotIx};
use crate::stack::{ItemStack, take_from};

/// Mouse button for a plain click.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Button {
    /// Whole-stack pick up / place.
    Left,
    /// Half pick up / single place.
    Right,
    /// Reserved; vanilla ignores it in `PICKUP` mode.
    Middle,
}

/// Which distribution a drag performs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DragKind {
    /// Spread evenly, remainder stays on the cursor.
    Left,
    /// One item per painted slot.
    Right,
    /// A full stack per painted slot. Creative only.
    Middle,
}

/// Phase of a drag.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DragStage {
    /// Button pressed with a carried stack.
    Start,
    /// Cursor entered a slot.
    Add,
    /// Button released.
    End,
}

/// Inventory-management actions beyond vanilla, driven by toolbar buttons.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ToolbarAction {
    /// Sort the `Normal`, unpinned slots of one inventory by item id, merging
    /// partial stacks. Pinned (favorite) slots keep their place and content.
    Sort {
        /// Inventory to sort.
        inventory: InventoryRef,
    },
    /// Move stacks from `from` into `to` for kinds `to` already holds.
    QuickStack {
        /// Source inventory.
        from: InventoryRef,
        /// Target inventory.
        to: InventoryRef,
    },
    /// Move every unpinned stack from `from` into `to`.
    DepositAll {
        /// Source inventory.
        from: InventoryRef,
        /// Target inventory.
        to: InventoryRef,
    },
    /// Same movement as `DepositAll`, kept distinct so screens and permission
    /// rules can tell "take from container" apart from "give to container".
    LootAll {
        /// Source inventory.
        from: InventoryRef,
        /// Target inventory.
        to: InventoryRef,
    },
    /// Pin or unpin a slot.
    ToggleFavorite {
        /// Slot to toggle.
        slot: SlotIx,
    },
}

/// One player action on a menu. Mirrors the vanilla `ClickType` enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ClickAction {
    /// Plain click. `SlotIx::OUTSIDE` drops the carried stack.
    Pickup {
        /// Clicked slot.
        slot: SlotIx,
        /// Which button.
        button: Button,
    },
    /// Shift-click: move through the routing table.
    QuickMove {
        /// Clicked slot.
        slot: SlotIx,
    },
    /// Number key: swap with hotbar slot `hotbar` (0-8), or offhand (40).
    Swap {
        /// Hovered slot.
        slot: SlotIx,
        /// Hotbar index 0-8, or 40 for the offhand.
        hotbar: u8,
    },
    /// Creative middle click: copy a full stack onto the cursor.
    Clone {
        /// Clicked slot.
        slot: SlotIx,
    },
    /// Drop key: one item, or the whole stack with `all`.
    Throw {
        /// Hovered slot.
        slot: SlotIx,
        /// Ctrl held.
        all: bool,
    },
    /// Drag distribution. `slot` is required for `Add` and ignored otherwise.
    Drag {
        /// Phase.
        stage: DragStage,
        /// Distribution kind; must match across the three stages.
        kind: DragKind,
        /// Painted slot for `Add`.
        slot: Option<SlotIx>,
    },
    /// Double click: gather same-kind items onto the cursor.
    PickupAll {
        /// Clicked slot.
        slot: SlotIx,
        /// Scan slots last-to-first.
        reverse: bool,
    },
    /// A toolbar button.
    Toolbar(ToolbarAction),
    /// Cheat-give: create `count` of `item` out of nothing. Refused with
    /// [`ClickError::Permission`] unless [`Actor::can_cheat`]. The one action
    /// that is exempt from item conservation, like `Clone`.
    Give {
        /// The item to create.
        item: ItemId,
        /// How many. Capped at the item's max stack size.
        count: u32,
        /// Where it lands.
        target: GiveTarget,
    },
}

/// Where a [`ClickAction::Give`] puts the created stack.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GiveTarget {
    /// Onto the cursor: sets the carried stack, or merges into a same-kind one.
    Cursor,
    /// Into this inventory's slots, same-kind stacks first, then empty slots.
    Inventory(InventoryRef),
}

/// Who is clicking. Carries the permissions actions check.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct Actor {
    /// Creative mode: enables `Clone` and middle drags.
    pub creative: bool,
    /// May receive cheat gives: enables [`ClickAction::Give`].
    pub can_cheat: bool,
}

impl Actor {
    /// A survival player.
    pub const SURVIVAL: Self = Self {
        creative: false,
        can_cheat: false,
    };
    /// A creative player who may also cheat.
    pub const CREATIVE: Self = Self {
        creative: true,
        can_cheat: true,
    };
}

/// What a successful action changed.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Delta {
    /// Changed slots with their new content, in ascending slot order.
    pub slots: Vec<(SlotIx, Option<ItemStack>)>,
    /// The carried stack after the action.
    pub carried: Option<ItemStack>,
    /// Stacks that left the menu and should spawn in the world.
    pub dropped: Vec<ItemStack>,
    /// The menu's state id after the action.
    pub state_id: u32,
    /// Set when items were taken out of an `Output` slot, so the owner can
    /// consume inputs and refill it. For a quick-move this crate moves one
    /// stack; vanilla loops until the result slot stops refilling, which the
    /// owner can reproduce by re-issuing the action.
    pub taken_from_output: Option<SlotIx>,
}

/// Applies one action. See the [module docs](self).
///
/// On `Ok`, `state.state_id` has been incremented if anything changed.
/// `Drag { Start | Add }` change no items and leave the id alone, and a
/// painted slot the cursor cannot fill is skipped rather than refused, so an
/// `Add` over an `Output`, `Locked` or full slot returns an empty [`Delta`].
///
/// On `Err`, nothing has changed except `state.drag`, which
/// [`ClickAction::Drag`] owns: `InvalidDragSequence` clears it, and so does
/// every `Drag { stage: End }`, which ends the drag whatever it returns. A
/// drag that painted no slots at all ends with an empty `Delta`.
///
/// In debug builds the total item count per kind over all inventories, the
/// carried stack and the dropped stacks is asserted unchanged, except for
/// `Clone` and middle drags (creative duplication) and for `Ghost` / `Filter`
/// slots, which hold no real items.
pub fn apply_click(
    def: &MenuDef,
    inv: &mut Inventories,
    state: &mut MenuState,
    action: ClickAction,
    actor: &Actor,
    ctx: &dyn LookupCtx,
) -> Result<Delta, ClickError> {
    check_layout(def, inv)?;

    if state.drag.is_some() && !matches!(action, ClickAction::Drag { .. }) {
        state.drag = None;
        return Err(ClickError::InvalidDragSequence);
    }

    #[cfg(debug_assertions)]
    let before = conservation::tally(def, inv, state.carried.as_ref());
    #[cfg(debug_assertions)]
    let duplicates = matches!(
        action,
        ClickAction::Clone { .. }
            | ClickAction::Give { .. }
            | ClickAction::Drag {
                kind: DragKind::Middle,
                ..
            }
    );

    let mut op = Op {
        def,
        inv,
        ctx,
        touched: Vec::new(),
        dropped: Vec::new(),
        taken_from_output: None,
    };

    let mutated = match action {
        ClickAction::Pickup { slot, button } => op.pickup(state, slot, button)?,
        ClickAction::QuickMove { slot } => op.quick_move(state, slot)?,
        ClickAction::Swap { slot, hotbar } => op.swap(state, slot, hotbar)?,
        ClickAction::Clone { slot } => op.clone_stack(state, slot, *actor)?,
        ClickAction::Throw { slot, all } => op.throw(state, slot, all)?,
        ClickAction::Drag { stage, kind, slot } => op.drag(state, stage, kind, slot, *actor)?,
        ClickAction::PickupAll { slot, reverse } => op.pickup_all(state, slot, reverse)?,
        ClickAction::Toolbar(action) => op.toolbar(action)?,
        ClickAction::Give {
            item,
            count,
            target,
        } => op.give(state, item, count, target, *actor)?,
    };

    if mutated {
        state.state_id = state.state_id.wrapping_add(1);
    }

    let Op {
        mut touched,
        dropped,
        taken_from_output,
        ..
    } = op;

    #[cfg(debug_assertions)]
    if !duplicates {
        let mut after = conservation::tally(def, inv, state.carried.as_ref());
        for d in &dropped {
            conservation::add(&mut after, d);
        }
        assert!(
            conservation::same(&before, &after),
            "item conservation violated: before {before:?}, after {after:?}"
        );
    }

    touched.sort_unstable();
    touched.dedup();
    let slots = touched
        .into_iter()
        .map(|ix| (ix, slot_content(def, inv, ix).cloned()))
        .collect();

    Ok(Delta {
        slots,
        carried: state.carried.clone(),
        dropped,
        state_id: state.state_id,
        taken_from_output,
    })
}

fn check_layout(def: &MenuDef, inv: &Inventories) -> Result<(), ClickError> {
    let ok = def.slots.iter().all(|s| {
        inv.get(s.source)
            .is_some_and(|i| usize::from(s.index) < i.len())
    });
    let hotbar_ok = def
        .hotbar
        .iter()
        .chain(def.offhand.iter())
        .all(|s| s.index() < def.slots.len());
    if ok && hotbar_ok {
        Ok(())
    } else {
        Err(ClickError::MenuMismatch)
    }
}

// ---------------------------------------------------------------------------
// Drag preview
// ---------------------------------------------------------------------------

/// What one painted slot would hold if the drag ended now.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DragPreview {
    /// The stack the slot would hold afterwards, ghost hints included.
    pub stack: ItemStack,
    /// How many items the distribution would add to this slot. `0` for a
    /// `Ghost` or `Filter` slot, which shows a hint rather than storing items.
    pub delta: u32,
}

/// The exact per-slot result of ending the drag in `state` right now, without
/// touching anything.
///
/// Empty when no drag is running, when nothing is carried, or when the drag
/// has painted no slots. The result is the same plan
/// [`ClickAction::Drag`] with [`DragStage::End`] applies, because both go
/// through the same function: a slot missing here is a slot `End` would not
/// write.
///
/// `ctx` is the same lookup the click would run against; stack caps and slot
/// filters both need it.
pub fn preview_drag(
    def: &MenuDef,
    inv: &Inventories,
    state: &MenuState,
    ctx: &dyn LookupCtx,
) -> Vec<(SlotIx, DragPreview)> {
    let (Some(drag), Some(carried)) = (state.drag.as_ref(), state.carried.as_ref()) else {
        return Vec::new();
    };
    plan_drag(def, inv, ctx, drag, carried).slots
}

/// `true` when `slot` would take at least one item of `stack` right now.
///
/// The rule a drag paint and a plain place both follow: the slot must be one
/// that accepts placements, its filter must pass, and it must be empty, a
/// hint slot, or a same-kind stack below its cap. A screen uses it to colour
/// the carried ghost over the slot the pointer is on.
pub fn can_accept(
    def: &MenuDef,
    inv: &Inventories,
    ctx: &dyn LookupCtx,
    slot: SlotIx,
    stack: &ItemStack,
) -> bool {
    let Some(sd) = def
        .slot(slot)
        .filter(|sd| sd.behaviour != SlotBehaviour::Disabled)
    else {
        return false;
    };
    if !sd.may_place(stack, ctx) {
        return false;
    }
    if sd.behaviour.is_ghost() {
        return true;
    }
    match slot_content(def, inv, slot) {
        None => true,
        Some(existing) => existing.same_kind(stack) && existing.count < sd.cap(stack.id, ctx),
    }
}

/// A whole distribution: what each painted slot ends up holding, and what is
/// left on the cursor.
struct DragPlan {
    slots: Vec<(SlotIx, DragPreview)>,
    remaining: u32,
}

/// The one place a drag distribution is decided. [`preview_drag`] reads the
/// plan; `Op::distribute` reads it and writes it. Keeping them one function is
/// what makes the phantom preview exact rather than a second guess.
fn plan_drag(
    def: &MenuDef,
    inv: &Inventories,
    ctx: &dyn LookupCtx,
    drag: &DragState,
    carried: &ItemStack,
) -> DragPlan {
    let n = u32::try_from(drag.slots.len()).unwrap_or(u32::MAX);
    let mut plan = DragPlan {
        slots: Vec::new(),
        remaining: carried.count,
    };
    if n == 0 {
        return plan;
    }
    let item_max = ctx.max_stack(carried.id).max(1);
    let per = match drag.kind {
        DragKind::Left => carried.count / n,
        DragKind::Right => 1,
        DragKind::Middle => item_max,
    };
    for &slot in &drag.slots {
        let Some(sd) = def
            .slot(slot)
            .filter(|sd| sd.behaviour != SlotBehaviour::Disabled)
        else {
            continue;
        };
        if !sd.may_place(carried, ctx) {
            continue;
        }
        if sd.behaviour.is_ghost() {
            let ghost = carried.clone().with_count(1);
            if slot_content(def, inv, slot) != Some(&ghost) {
                plan.slots.push((
                    slot,
                    DragPreview {
                        stack: ghost,
                        delta: 0,
                    },
                ));
            }
            continue;
        }
        let existing = slot_content(def, inv, slot);
        if existing.is_some_and(|e| !e.same_kind(carried)) {
            continue;
        }
        let placed = existing.map_or(0, |e| e.count);
        let target = (per + placed).min(sd.cap(carried.id, ctx));
        let mut add = target.saturating_sub(placed);
        if drag.kind != DragKind::Middle {
            add = add.min(plan.remaining);
        }
        if add == 0 {
            continue;
        }
        plan.remaining = plan.remaining.saturating_sub(add);
        plan.slots.push((
            slot,
            DragPreview {
                stack: carried.clone().with_count(placed + add),
                delta: add,
            },
        ));
    }
    plan
}

fn slot_content<'i>(def: &MenuDef, inv: &'i Inventories, ix: SlotIx) -> Option<&'i ItemStack> {
    let s = def.slot(ix)?;
    inv.get(s.source)?.get(usize::from(s.index))
}

/// A `Normal` slot whose filter accepts `stack`: the only kind of slot bulk
/// moves may store real items in.
fn may_store(sd: &SlotDef, stack: &ItemStack, ctx: &dyn LookupCtx) -> bool {
    sd.behaviour == SlotBehaviour::Normal && sd.may_place(stack, ctx)
}

/// Working context for one action.
struct Op<'a> {
    def: &'a MenuDef,
    inv: &'a mut Inventories,
    ctx: &'a dyn LookupCtx,
    touched: Vec<SlotIx>,
    dropped: Vec<ItemStack>,
    taken_from_output: Option<SlotIx>,
}

impl<'a> Op<'a> {
    /// The definition of a real, enabled slot.
    fn slot_def(&self, ix: SlotIx) -> Result<&'a SlotDef, ClickError> {
        match self.def.slot(ix) {
            Some(sd) if sd.behaviour != SlotBehaviour::Disabled => Ok(sd),
            _ => Err(ClickError::NoSuchSlot),
        }
    }

    fn content(&self, ix: SlotIx) -> Option<&ItemStack> {
        slot_content(self.def, self.inv, ix)
    }

    fn set(&mut self, ix: SlotIx, stack: Option<ItemStack>) {
        let sd = &self.def.slots[ix.index()];
        self.inv[sd.source].set(usize::from(sd.index), stack);
        self.touched.push(ix);
    }

    fn take(&mut self, ix: SlotIx) -> Option<ItemStack> {
        let sd = &self.def.slots[ix.index()];
        self.touched.push(ix);
        self.inv[sd.source].take(usize::from(sd.index))
    }

    /// Takes up to `n` items out of `ix`.
    fn take_n(&mut self, ix: SlotIx, n: u32) -> Option<ItemStack> {
        let mut stack = self.take(ix);
        let taken = take_from(&mut stack, n);
        self.set(ix, stack);
        taken
    }

    fn note_output(&mut self, ix: SlotIx, sd: &SlotDef) {
        if sd.behaviour == SlotBehaviour::Output {
            self.taken_from_output = Some(ix);
        }
    }

    fn cap(&self, sd: &SlotDef, stack: &ItemStack) -> u32 {
        sd.cap(stack.id, self.ctx)
    }

    fn item_max(&self, stack: &ItemStack) -> u32 {
        self.ctx.max_stack(stack.id).max(1)
    }

    /// Menu slots backed by `refs`, in menu order, optionally reversed.
    fn target_slots(
        &self,
        refs: &[InventoryRef],
        reverse: bool,
        exclude: Option<SlotIx>,
    ) -> Vec<SlotIx> {
        let mut out: Vec<SlotIx> = refs
            .iter()
            .flat_map(|r| self.def.slots_of(*r))
            .filter(|s| Some(*s) != exclude)
            .collect();
        if reverse {
            out.reverse();
        }
        out
    }

    /// Vanilla `moveItemStackTo`: merge into same-kind stacks first, then
    /// fill empty slots, both in target order. Returns whether anything moved.
    /// `stack` may be left empty.
    fn move_into(&mut self, stack: &mut ItemStack, targets: &[SlotIx]) -> bool {
        let mut moved = false;
        if self.item_max(stack) > 1 {
            for &t in targets {
                if stack.is_empty() {
                    break;
                }
                let sd = &self.def.slots[t.index()];
                if !may_store(sd, stack, self.ctx) {
                    continue;
                }
                let cap = self.cap(sd, stack);
                let Some(existing) = self.content(t) else {
                    continue;
                };
                if !existing.same_kind(stack) || existing.count >= cap {
                    continue;
                }
                let mut merged = existing.clone();
                if stack.merge_into(&mut merged, cap) > 0 {
                    self.set(t, Some(merged));
                    moved = true;
                }
            }
        }
        for &t in targets {
            if stack.is_empty() {
                break;
            }
            let sd = &self.def.slots[t.index()];
            if self.content(t).is_some() || !may_store(sd, stack, self.ctx) {
                continue;
            }
            let cap = self.cap(sd, stack);
            if let Some(part) = stack.split(cap) {
                self.set(t, Some(part));
                moved = true;
            }
        }
        moved
    }

    // ----- PICKUP ---------------------------------------------------------

    fn pickup(
        &mut self,
        state: &mut MenuState,
        slot: SlotIx,
        button: Button,
    ) -> Result<bool, ClickError> {
        let left = match button {
            Button::Left => true,
            Button::Right => false,
            Button::Middle => return Err(ClickError::NotAllowed),
        };

        if slot.is_outside() {
            let Some(carried) = state.carried.as_ref() else {
                return Err(ClickError::NothingToDo);
            };
            let n = if left { carried.count } else { 1 };
            let dropped = take_from(&mut state.carried, n).expect("carried is non-empty");
            self.dropped.push(dropped);
            return Ok(true);
        }

        let sd = self.slot_def(slot)?;
        if sd.behaviour.is_ghost() {
            return self.ghost_click(state, slot, sd);
        }
        if sd.behaviour == SlotBehaviour::Locked {
            return Err(ClickError::SlotLocked);
        }

        match (self.content(slot).cloned(), state.carried.clone()) {
            (None, None) => Err(ClickError::NothingToDo),

            // Place all (left) or one (right).
            (None, Some(carried)) => {
                if !sd.may_place(&carried, self.ctx) {
                    return Err(ClickError::NotAllowed);
                }
                let n = if left { carried.count } else { 1 };
                let n = n.min(self.cap(sd, &carried));
                let placed = take_from(&mut state.carried, n).expect("carried is non-empty");
                self.set(slot, Some(placed));
                Ok(true)
            }

            // Pick up all (left) or ceil(half) (right).
            (Some(present), None) => {
                let n = if left {
                    present.count
                } else {
                    present.count.div_ceil(2)
                };
                state.carried = self.take_n(slot, n);
                self.note_output(slot, sd);
                Ok(true)
            }

            (Some(present), Some(carried)) => {
                if sd.may_place(&carried, self.ctx) {
                    if present.same_kind(&carried) {
                        // Merge some of the carried stack into the slot.
                        let cap = self.cap(sd, &carried);
                        let want = if left { carried.count } else { 1 };
                        let moved = want.min(cap.saturating_sub(present.count));
                        if moved == 0 {
                            return Err(ClickError::NothingToDo);
                        }
                        take_from(&mut state.carried, moved);
                        let total = present.count + moved;
                        self.set(slot, Some(present.with_count(total)));
                        Ok(true)
                    } else if carried.count <= self.cap(sd, &carried) {
                        // Swap.
                        let carried = state.carried.take();
                        state.carried = self.take(slot);
                        self.set(slot, carried);
                        Ok(true)
                    } else {
                        Err(ClickError::NothingToDo)
                    }
                } else if present.same_kind(&carried) {
                    // Cannot place (an output slot) but the same kind: pull
                    // from the slot onto the cursor. This is how repeated
                    // clicks on a crafting result accumulate.
                    let room = self.item_max(&carried).saturating_sub(carried.count);
                    let n = present.count.min(room);
                    if n == 0 {
                        return Err(ClickError::NothingToDo);
                    }
                    self.take_n(slot, n);
                    if let Some(c) = state.carried.as_mut() {
                        c.count += n;
                    }
                    self.note_output(slot, sd);
                    Ok(true)
                } else {
                    Err(ClickError::NotAllowed)
                }
            }
        }
    }

    /// Ghost and Filter slots: an empty hand clears, a full hand sets the
    /// slot to one of the carried kind without consuming it.
    fn ghost_click(
        &mut self,
        state: &mut MenuState,
        slot: SlotIx,
        sd: &SlotDef,
    ) -> Result<bool, ClickError> {
        match state.carried.as_ref() {
            None => {
                if self.content(slot).is_none() {
                    return Err(ClickError::NothingToDo);
                }
                self.set(slot, None);
                Ok(true)
            }
            Some(carried) => {
                if !sd.may_place(carried, self.ctx) {
                    return Err(ClickError::NotAllowed);
                }
                let ghost = carried.clone().with_count(1);
                if self.content(slot) == Some(&ghost) {
                    return Err(ClickError::NothingToDo);
                }
                self.set(slot, Some(ghost));
                Ok(true)
            }
        }
    }

    // ----- QUICK_MOVE -----------------------------------------------------

    fn quick_move(&mut self, _state: &mut MenuState, slot: SlotIx) -> Result<bool, ClickError> {
        let sd = self.slot_def(slot)?;
        if sd.behaviour.is_ghost() {
            if self.content(slot).is_none() {
                return Err(ClickError::NothingToDo);
            }
            self.set(slot, None);
            return Ok(true);
        }
        if !sd.may_pickup() {
            return Err(ClickError::SlotLocked);
        }
        let Some(mut moving) = self.content(slot).cloned() else {
            return Err(ClickError::NothingToDo);
        };
        let targets = self.quick_move_targets(slot, sd);
        if targets.is_empty() || !self.move_into(&mut moving, &targets) {
            return Err(ClickError::NothingToDo);
        }
        self.set(slot, Some(moving));
        self.note_output(slot, sd);
        Ok(true)
    }

    /// Routing-table targets for `slot`, falling back to the next inventory
    /// in the listring.
    fn quick_move_targets(&self, slot: SlotIx, sd: &SlotDef) -> Vec<SlotIx> {
        if let Some(rule) = self.def.quick_move.resolve_rule(slot) {
            return self.target_slots(&rule.to, rule.reverse, Some(slot));
        }
        let ring = &self.def.listring;
        let Some(pos) = ring.iter().position(|r| *r == sd.source) else {
            return Vec::new();
        };
        let next = ring[(pos + 1) % ring.len()];
        if next == sd.source {
            return Vec::new();
        }
        self.target_slots(&[next], false, Some(slot))
    }

    // ----- SWAP -----------------------------------------------------------

    fn swap(
        &mut self,
        _state: &mut MenuState,
        slot: SlotIx,
        hotbar: u8,
    ) -> Result<bool, ClickError> {
        let target = match hotbar {
            0..=8 => self.def.hotbar.get(usize::from(hotbar)).copied(),
            40 => self.def.offhand,
            _ => None,
        }
        .ok_or(ClickError::NoSuchSlot)?;
        let sd = self.slot_def(slot)?;
        let td = self.slot_def(target)?;
        if slot == target {
            return Err(ClickError::NothingToDo);
        }
        if sd.behaviour.is_ghost() {
            return Err(ClickError::NotAllowed);
        }
        if sd.behaviour == SlotBehaviour::Locked || td.behaviour != SlotBehaviour::Normal {
            return Err(ClickError::SlotLocked);
        }

        let in_hotbar = self.content(target).cloned();
        let in_slot = self.content(slot).cloned();
        match (in_hotbar, in_slot) {
            (None, None) => Err(ClickError::NothingToDo),

            // Move the slot's stack to the empty hotbar slot.
            (None, Some(present)) => {
                if !td.may_place(&present, self.ctx) || present.count > self.cap(td, &present) {
                    return Err(ClickError::NothingToDo);
                }
                self.take(slot);
                self.set(target, Some(present));
                self.note_output(slot, sd);
                Ok(true)
            }

            // Move the hotbar stack into the empty slot, up to the slot cap.
            (Some(mut held), None) => {
                if !sd.may_place(&held, self.ctx) {
                    return Err(ClickError::NotAllowed);
                }
                let cap = self.cap(sd, &held);
                let part = held.split(cap);
                self.set(target, Some(held));
                self.set(slot, part);
                Ok(true)
            }

            // Exchange. If the hotbar stack exceeds the slot cap, or the
            // hovered slot's stack does not fit the hotbar slot's own filter
            // and cap, only `cap` moves in and the displaced stack goes
            // wherever it fits in the player's inventories, or is dropped.
            (Some(mut held), Some(present)) => {
                if !sd.may_place(&held, self.ctx) {
                    return Err(ClickError::NotAllowed);
                }
                let cap = self.cap(sd, &held);
                let fits_back =
                    td.may_place(&present, self.ctx) && present.count <= self.cap(td, &present);
                if held.count > cap || !fits_back {
                    let part = held.split(cap);
                    self.set(target, Some(held));
                    self.set(slot, part);
                    let mut displaced = present;
                    let mut refs = vec![td.source];
                    refs.extend_from_slice(self.def.quick_move.resolve(target));
                    let homes = self.target_slots(&refs, false, Some(slot));
                    self.move_into(&mut displaced, &homes);
                    if !displaced.is_empty() {
                        self.dropped.push(displaced);
                    }
                } else {
                    self.set(target, Some(present));
                    self.set(slot, Some(held));
                }
                self.note_output(slot, sd);
                Ok(true)
            }
        }
    }

    // ----- CLONE ----------------------------------------------------------

    fn clone_stack(
        &mut self,
        state: &mut MenuState,
        slot: SlotIx,
        actor: Actor,
    ) -> Result<bool, ClickError> {
        if !actor.creative {
            return Err(ClickError::Permission);
        }
        if state.carried.is_some() {
            return Err(ClickError::NothingToDo);
        }
        self.slot_def(slot)?;
        let Some(present) = self.content(slot).cloned() else {
            return Err(ClickError::NothingToDo);
        };
        let max = self.item_max(&present);
        state.carried = Some(present.with_count(max));
        Ok(true)
    }

    // ----- GIVE -----------------------------------------------------------

    fn give(
        &mut self,
        state: &mut MenuState,
        item: ItemId,
        count: u32,
        target: GiveTarget,
        actor: Actor,
    ) -> Result<bool, ClickError> {
        if !actor.can_cheat {
            return Err(ClickError::Permission);
        }
        if count == 0 {
            return Err(ClickError::NothingToDo);
        }
        let mut stack = ItemStack::new(item, count);
        let max = self.item_max(&stack);
        stack.count = stack.count.min(max);
        match target {
            GiveTarget::Cursor => match state.carried.as_mut() {
                None => {
                    state.carried = Some(stack);
                    Ok(true)
                }
                Some(carried) if carried.same_kind(&stack) => {
                    if carried.count >= max {
                        return Err(ClickError::NothingToDo);
                    }
                    carried.count = (carried.count + stack.count).min(max);
                    Ok(true)
                }
                Some(_) => Err(ClickError::NotAllowed),
            },
            GiveTarget::Inventory(inventory) => {
                let targets = self.target_slots(&[inventory], false, None);
                if targets.is_empty() {
                    return Err(ClickError::NoSuchSlot);
                }
                if self.move_into(&mut stack, &targets) {
                    Ok(true)
                } else {
                    Err(ClickError::NothingToDo)
                }
            }
        }
    }

    // ----- THROW ----------------------------------------------------------

    fn throw(
        &mut self,
        state: &mut MenuState,
        slot: SlotIx,
        all: bool,
    ) -> Result<bool, ClickError> {
        let sd = self.slot_def(slot)?;
        if state.carried.is_some() {
            return Err(ClickError::NothingToDo);
        }
        if sd.behaviour.is_ghost() {
            if self.content(slot).is_none() {
                return Err(ClickError::NothingToDo);
            }
            self.set(slot, None);
            return Ok(true);
        }
        if !sd.may_pickup() {
            return Err(ClickError::SlotLocked);
        }
        let Some(present) = self.content(slot) else {
            return Err(ClickError::NothingToDo);
        };
        let n = if all { present.count } else { 1 };
        let taken = self.take_n(slot, n).expect("slot is non-empty");
        self.dropped.push(taken);
        self.note_output(slot, sd);
        Ok(true)
    }

    // ----- QUICK_CRAFT (drag) ---------------------------------------------

    fn drag(
        &mut self,
        state: &mut MenuState,
        phase: DragStage,
        kind: DragKind,
        slot: Option<SlotIx>,
        actor: Actor,
    ) -> Result<bool, ClickError> {
        match phase {
            DragStage::Start => {
                state.drag = None;
                if state.carried.is_none() {
                    return Err(ClickError::InvalidDragSequence);
                }
                if kind == DragKind::Middle && !actor.creative {
                    return Err(ClickError::Permission);
                }
                state.drag = Some(DragState {
                    kind,
                    slots: Vec::new(),
                });
                Ok(false)
            }
            DragStage::Add => {
                let (Some(drag), Some(carried), Some(slot)) =
                    (state.drag.as_ref(), state.carried.as_ref(), slot)
                else {
                    state.drag = None;
                    return Err(ClickError::InvalidDragSequence);
                };
                if drag.kind != kind {
                    state.drag = None;
                    return Err(ClickError::InvalidDragSequence);
                }
                let Some(sd) = self.def.slot(slot) else {
                    return Err(ClickError::NoSuchSlot);
                };
                if drag.slots.contains(&slot) {
                    return Ok(false);
                }
                let painted = u32::try_from(drag.slots.len()).unwrap_or(u32::MAX);
                if !self.can_paint(sd, slot, carried, kind, painted) {
                    // Vanilla drags the cursor across whatever is in the way
                    // and paints only the slots that can take an item, so a
                    // slot that cannot is skipped rather than refused.
                    return Ok(false);
                }
                if let Some(drag) = state.drag.as_mut() {
                    drag.slots.push(slot);
                }
                Ok(false)
            }
            DragStage::End => {
                let Some(drag) = state.drag.take() else {
                    return Err(ClickError::InvalidDragSequence);
                };
                if drag.kind != kind {
                    return Err(ClickError::InvalidDragSequence);
                }
                let Some(carried) = state.carried.clone() else {
                    return Err(ClickError::InvalidDragSequence);
                };
                self.distribute(state, &drag, &carried)
            }
        }
    }

    /// Vanilla `canItemQuickReplace && mayPlace && (middle || count > painted)`.
    fn can_paint(
        &self,
        sd: &SlotDef,
        slot: SlotIx,
        carried: &ItemStack,
        kind: DragKind,
        painted: u32,
    ) -> bool {
        if !sd.may_place(carried, self.ctx) {
            return false;
        }
        if sd.behaviour.is_ghost() {
            return true;
        }
        let fits = match self.content(slot) {
            None => true,
            Some(e) => e.same_kind(carried) && e.count < self.cap(sd, carried),
        };
        fits && (kind == DragKind::Middle || carried.count > painted)
    }

    fn distribute(
        &mut self,
        state: &mut MenuState,
        drag: &DragState,
        carried: &ItemStack,
    ) -> Result<bool, ClickError> {
        if drag.slots.is_empty() {
            // A drag that painted nothing: the drag is over and nothing moved.
            return Ok(false);
        }
        let plan = plan_drag(self.def, self.inv, self.ctx, drag, carried);
        if plan.slots.is_empty() {
            return Err(ClickError::NothingToDo);
        }
        for (slot, preview) in plan.slots {
            self.set(slot, Some(preview.stack));
        }
        state.carried = (plan.remaining > 0).then(|| carried.clone().with_count(plan.remaining));
        Ok(true)
    }

    // ----- PICKUP_ALL -----------------------------------------------------

    fn pickup_all(
        &mut self,
        state: &mut MenuState,
        slot: SlotIx,
        reverse: bool,
    ) -> Result<bool, ClickError> {
        let sd = self.slot_def(slot)?;
        let Some(carried) = state.carried.clone() else {
            return Err(ClickError::NothingToDo);
        };
        // Vanilla only gathers when the double-clicked slot is empty or
        // cannot be picked from; otherwise the second click was a pickup.
        if self.content(slot).is_some() && sd.may_pickup() {
            return Err(ClickError::NothingToDo);
        }
        let max = self.item_max(&carried);
        let mut count = carried.count;
        let mut order: Vec<SlotIx> = (0..self.def.slots.len())
            .map(|i| SlotIx(u16::try_from(i).expect("slot count fits u16")))
            .collect();
        if reverse {
            order.reverse();
        }
        // Pass 0 takes from partial stacks, pass 1 from full ones.
        for pass in 0..2 {
            for &s in &order {
                if count >= max {
                    break;
                }
                let sd = &self.def.slots[s.index()];
                if sd.behaviour != SlotBehaviour::Normal {
                    continue;
                }
                let Some(present) = self.content(s) else {
                    continue;
                };
                if !present.same_kind(&carried) || (pass == 0 && present.count >= max) {
                    continue;
                }
                let n = present.count.min(max - count);
                self.take_n(s, n);
                count += n;
            }
        }
        if count == carried.count {
            return Err(ClickError::NothingToDo);
        }
        state.carried = Some(carried.with_count(count));
        Ok(true)
    }

    // ----- Toolbar --------------------------------------------------------

    fn toolbar(&mut self, action: ToolbarAction) -> Result<bool, ClickError> {
        match action {
            ToolbarAction::Sort { inventory } => self.sort(inventory),
            ToolbarAction::QuickStack { from, to } => self.bulk_move(from, to, true),
            ToolbarAction::DepositAll { from, to } | ToolbarAction::LootAll { from, to } => {
                self.bulk_move(from, to, false)
            }
            ToolbarAction::ToggleFavorite { slot } => {
                let sd = self.slot_def(slot)?;
                if sd.behaviour != SlotBehaviour::Normal {
                    return Err(ClickError::NotAllowed);
                }
                let inv = &mut self.inv[sd.source];
                let i = usize::from(sd.index);
                inv.set_favorite(i, !inv.is_favorite(i));
                self.touched.push(slot);
                Ok(true)
            }
        }
    }

    /// Plain (`Normal`, uncapped, unfiltered), unpinned slots of `inventory`.
    fn plain_slots(&self, inventory: InventoryRef) -> Vec<SlotIx> {
        let inv = &self.inv[inventory];
        self.def
            .slots_of(inventory)
            .filter(|s| {
                let sd = &self.def.slots[s.index()];
                sd.behaviour == SlotBehaviour::Normal
                    && sd.max_stack.is_none()
                    && sd.accepts.is_none()
                    && !inv.is_favorite(usize::from(sd.index))
            })
            .collect()
    }

    fn sort(&mut self, inventory: InventoryRef) -> Result<bool, ClickError> {
        self.inv.get(inventory).ok_or(ClickError::NoSuchSlot)?;
        let slots = self.plain_slots(inventory);
        if slots.len() < 2 {
            return Err(ClickError::NothingToDo);
        }
        // Group by kind, first-seen order, then stable sort by id.
        let mut kinds: Vec<(ItemStack, u64)> = Vec::new();
        for &s in &slots {
            if let Some(present) = self.content(s) {
                match kinds.iter_mut().find(|(k, _)| k.same_kind(present)) {
                    Some((_, total)) => *total += u64::from(present.count),
                    None => kinds.push((present.clone(), u64::from(present.count))),
                }
            }
        }
        kinds.sort_by_key(|(k, _)| k.id);
        let mut layout: Vec<Option<ItemStack>> = Vec::with_capacity(slots.len());
        for (kind, mut total) in kinds {
            let max = u64::from(self.item_max(&kind));
            while total > 0 {
                let n = total.min(max);
                total -= n;
                layout.push(Some(
                    kind.clone()
                        .with_count(u32::try_from(n).expect("stack fits u32")),
                ));
            }
        }
        layout.resize(slots.len(), None);
        let mut changed = false;
        for (&s, new) in slots.iter().zip(layout) {
            if self.content(s) != new.as_ref() {
                self.set(s, new);
                changed = true;
            }
        }
        if changed {
            Ok(true)
        } else {
            Err(ClickError::NothingToDo)
        }
    }

    /// Moves unpinned stacks from `from` into `to`. With `only_present`, only
    /// kinds `to` already holds move (quick stack).
    fn bulk_move(
        &mut self,
        from: InventoryRef,
        to: InventoryRef,
        only_present: bool,
    ) -> Result<bool, ClickError> {
        if from == to {
            return Err(ClickError::NotAllowed);
        }
        self.inv.get(from).ok_or(ClickError::NoSuchSlot)?;
        self.inv.get(to).ok_or(ClickError::NoSuchSlot)?;
        let targets = self.target_slots(&[to], false, None);
        let present: Vec<ItemStack> = targets
            .iter()
            .filter(|t| self.def.slots[t.index()].behaviour == SlotBehaviour::Normal)
            .filter_map(|t| self.content(*t).cloned())
            .collect();
        let mut moved = false;
        for s in self.plain_or_filtered_sources(from) {
            let Some(mut stack) = self.content(s).cloned() else {
                continue;
            };
            if only_present && !present.iter().any(|p| p.same_kind(&stack)) {
                continue;
            }
            if self.move_into(&mut stack, &targets) {
                self.set(s, Some(stack));
                moved = true;
            }
        }
        if moved {
            Ok(true)
        } else {
            Err(ClickError::NothingToDo)
        }
    }

    /// `Normal`, unpinned source slots of `inventory` (filters and caps are
    /// fine on a source).
    fn plain_or_filtered_sources(&self, inventory: InventoryRef) -> Vec<SlotIx> {
        let inv = &self.inv[inventory];
        self.def
            .slots_of(inventory)
            .filter(|s| {
                let sd = &self.def.slots[s.index()];
                sd.behaviour == SlotBehaviour::Normal && !inv.is_favorite(usize::from(sd.index))
            })
            .collect()
    }
}

#[cfg(debug_assertions)]
mod conservation {
    use super::{Inventories, ItemStack, MenuDef};

    pub type Tally = Vec<(ItemStack, u64)>;

    pub fn add(tally: &mut Tally, stack: &ItemStack) {
        match tally.iter_mut().find(|(k, _)| k.same_kind(stack)) {
            Some((_, n)) => *n += u64::from(stack.count),
            None => tally.push((stack.clone().with_count(1), u64::from(stack.count))),
        }
    }

    /// Items per kind over every non-ghost slot plus the carried stack.
    pub fn tally(def: &MenuDef, inv: &Inventories, carried: Option<&ItemStack>) -> Tally {
        let ghost: Vec<_> = def
            .slots
            .iter()
            .filter(|s| s.behaviour.is_ghost())
            .map(|s| (s.source, usize::from(s.index)))
            .collect();
        let mut out = Tally::new();
        for (handle, inventory) in inv.iter() {
            for (i, slot) in inventory.slots().iter().enumerate() {
                if ghost.contains(&(handle, i)) {
                    continue;
                }
                if let Some(s) = slot {
                    add(&mut out, s);
                }
            }
        }
        if let Some(c) = carried {
            add(&mut out, c);
        }
        out
    }

    pub fn same(a: &Tally, b: &Tally) -> bool {
        let covered = |x: &Tally, y: &Tally| {
            x.iter()
                .all(|(k, n)| *n == 0 || y.iter().any(|(k2, n2)| k2.same_kind(k) && n2 == n))
        };
        covered(a, b) && covered(b, a)
    }
}
