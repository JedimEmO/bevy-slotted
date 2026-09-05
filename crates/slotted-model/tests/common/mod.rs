//! Shared fixtures for the integration tests.
#![allow(dead_code, clippy::unwrap_used)]

use slotted_model::{
    Actor, ClickAction, ClickError, Delta, Inventories, InventoryRef, ItemId, ItemStack, LookupCtx,
    MenuDef, MenuState, Namespaced, SlotIx,
};

/// Stacks to 64.
pub const STONE: ItemId = ItemId(1);
/// Stacks to 16.
pub const EGG: ItemId = ItemId(2);
/// Stacks to 1.
pub const SWORD: ItemId = ItemId(3);
/// Stacks to 1, tagged `slotted:armor/head`.
pub const HELMET: ItemId = ItemId(4);
/// Stacks to 64, a second stackable kind.
pub const DIRT: ItemId = ItemId(5);

pub struct Items;

impl LookupCtx for Items {
    fn max_stack(&self, id: ItemId) -> u32 {
        match id {
            STONE | DIRT => 64,
            EGG => 16,
            _ => 1,
        }
    }

    fn has_tag(&self, id: ItemId, tag: &Namespaced) -> bool {
        id == HELMET && tag.as_str() == "slotted:armor/head"
    }
}

pub struct Fixture {
    pub def: MenuDef,
    pub inv: Inventories,
    pub state: MenuState,
}

impl Fixture {
    pub fn new(def: MenuDef) -> Self {
        let inv = Inventories::for_menu(&def);
        let state = MenuState::new(&def);
        Self { def, inv, state }
    }

    pub fn chest() -> Self {
        Self::new(MenuDef::chest(3))
    }

    pub fn player() -> Self {
        Self::new(MenuDef::player())
    }

    /// Puts `stack` at the inventory position behind menu slot `slot`.
    pub fn put(&mut self, slot: u16, stack: ItemStack) {
        let sd = self.def.slot(SlotIx(slot)).unwrap().clone();
        self.inv[sd.source].set(usize::from(sd.index), Some(stack));
    }

    pub fn put_in(&mut self, inventory: InventoryRef, index: usize, stack: ItemStack) {
        self.inv[inventory].set(index, Some(stack));
    }

    pub fn carry(&mut self, stack: ItemStack) {
        self.state.carried = Some(stack);
    }

    /// Content of menu slot `slot`.
    pub fn at(&self, slot: u16) -> Option<&ItemStack> {
        let sd = self.def.slot(SlotIx(slot)).unwrap();
        self.inv[sd.source].get(usize::from(sd.index))
    }

    /// `(id, count)` of menu slot `slot`, or `None`.
    pub fn at_kind(&self, slot: u16) -> Option<(ItemId, u32)> {
        self.at(slot).map(|s| (s.id, s.count))
    }

    pub fn carried(&self) -> Option<(ItemId, u32)> {
        self.state.carried.as_ref().map(|s| (s.id, s.count))
    }

    pub fn click(&mut self, action: ClickAction) -> Result<Delta, ClickError> {
        self.click_as(action, Actor::SURVIVAL)
    }

    pub fn click_as(&mut self, action: ClickAction, actor: Actor) -> Result<Delta, ClickError> {
        slotted_model::apply_click(
            &self.def,
            &mut self.inv,
            &mut self.state,
            action,
            &actor,
            &Items,
        )
    }

    pub fn ok(&mut self, action: ClickAction) -> Delta {
        self.click(action).unwrap()
    }

    /// Total items of `id` everywhere, including the cursor.
    pub fn total(&self, id: ItemId) -> u64 {
        let kind = ItemStack::new(id, 1);
        self.inv.count_of(&kind)
            + self
                .state
                .carried
                .as_ref()
                .filter(|c| c.same_kind(&kind))
                .map_or(0, |c| u64::from(c.count))
    }
}

pub fn stack(id: ItemId, count: u32) -> ItemStack {
    ItemStack::new(id, count)
}

pub fn left(slot: u16) -> ClickAction {
    ClickAction::Pickup {
        slot: SlotIx(slot),
        button: slotted_model::Button::Left,
    }
}

pub fn right(slot: u16) -> ClickAction {
    ClickAction::Pickup {
        slot: SlotIx(slot),
        button: slotted_model::Button::Right,
    }
}

pub fn shift(slot: u16) -> ClickAction {
    ClickAction::QuickMove { slot: SlotIx(slot) }
}
