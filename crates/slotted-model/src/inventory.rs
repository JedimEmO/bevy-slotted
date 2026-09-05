//! Inventories: fixed-size slot arrays with a dirty mask.
//!
//! An [`Inventory`] is a flat array of `Option<ItemStack>` plus two bitsets:
//! `changed`, which the sync layer reads and clears to send deltas, and
//! `favorites`, which the toolbar actions honour by pinning slots in place.
//!
//! A menu never talks to an inventory directly. It addresses one of the
//! [`Inventories`] a player has open through an [`InventoryRef`], an opaque
//! handle whose meaning (container, player main, hotbar...) is decided by the
//! [`MenuDef`] that uses it, not by this module.

use serde::{Deserialize, Serialize};

use crate::menu::MenuDef;
use crate::stack::{ItemStack, normalise};

/// A bitset over slot indices.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DirtyMask {
    words: Vec<u64>,
    len: usize,
}

impl DirtyMask {
    /// A cleared mask over `len` slots.
    pub fn new(len: usize) -> Self {
        Self {
            words: vec![0; len.div_ceil(64)],
            len,
        }
    }

    /// Number of slots covered.
    pub fn len(&self) -> usize {
        self.len
    }

    /// `true` when the mask covers no slots.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Reads bit `i`. Out-of-range indices read as `false`.
    pub fn get(&self, i: usize) -> bool {
        i < self.len && self.words[i / 64] & (1 << (i % 64)) != 0
    }

    /// Sets bit `i`. Out-of-range indices are ignored.
    pub fn set(&mut self, i: usize) {
        if i < self.len {
            self.words[i / 64] |= 1 << (i % 64);
        }
    }

    /// Clears bit `i`. Out-of-range indices are ignored.
    pub fn clear(&mut self, i: usize) {
        if i < self.len {
            self.words[i / 64] &= !(1 << (i % 64));
        }
    }

    /// Sets or clears bit `i`.
    pub fn put(&mut self, i: usize, on: bool) {
        if on {
            self.set(i);
        } else {
            self.clear(i);
        }
    }

    /// Sets every bit.
    pub fn set_all(&mut self) {
        for (w, word) in self.words.iter_mut().enumerate() {
            let bits = self.len.saturating_sub(w * 64).min(64);
            *word = if bits == 64 {
                u64::MAX
            } else {
                (1u64 << bits) - 1
            };
        }
    }

    /// Clears every bit.
    pub fn clear_all(&mut self) {
        self.words.fill(0);
    }

    /// `true` when at least one bit is set.
    pub fn any(&self) -> bool {
        self.words.iter().any(|w| *w != 0)
    }

    /// Number of set bits.
    pub fn count(&self) -> usize {
        self.words.iter().map(|w| w.count_ones() as usize).sum()
    }

    /// Iterates the set indices in ascending order.
    pub fn iter(&self) -> impl Iterator<Item = usize> + '_ {
        self.words.iter().enumerate().flat_map(|(w, word)| {
            let mut bits = *word;
            std::iter::from_fn(move || {
                if bits == 0 {
                    return None;
                }
                let bit = bits.trailing_zeros() as usize;
                bits &= bits - 1;
                Some(w * 64 + bit)
            })
        })
    }
}

/// A fixed-size array of slots with change tracking.
///
/// Every mutation through this type marks the slot in `changed`; the sync
/// layer calls [`clear_changed`](Self::clear_changed) after it has sent the
/// delta. Slots never hold a zero-count stack.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Inventory {
    slots: Box<[Option<ItemStack>]>,
    changed: DirtyMask,
    favorites: DirtyMask,
}

impl Inventory {
    /// An empty inventory of `len` slots.
    pub fn new(len: usize) -> Self {
        Self {
            slots: vec![None; len].into_boxed_slice(),
            changed: DirtyMask::new(len),
            favorites: DirtyMask::new(len),
        }
    }

    /// An inventory with the given contents. Zero-count stacks become `None`.
    /// Nothing is marked changed.
    pub fn from_slots(slots: impl IntoIterator<Item = Option<ItemStack>>) -> Self {
        let mut slots: Vec<_> = slots.into_iter().collect();
        for slot in &mut slots {
            normalise(slot);
        }
        let len = slots.len();
        Self {
            slots: slots.into_boxed_slice(),
            changed: DirtyMask::new(len),
            favorites: DirtyMask::new(len),
        }
    }

    /// Number of slots.
    pub fn len(&self) -> usize {
        self.slots.len()
    }

    /// `true` when there are no slots at all (not "all slots empty").
    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }

    /// All slots in order.
    pub fn slots(&self) -> &[Option<ItemStack>] {
        &self.slots
    }

    /// The stack in slot `i`, or `None` when the slot is empty or out of range.
    pub fn get(&self, i: usize) -> Option<&ItemStack> {
        self.slots.get(i).and_then(Option::as_ref)
    }

    /// Replaces slot `i` with `stack` and marks it changed. Returns the old
    /// content. A zero-count `stack` is stored as `None`.
    ///
    /// # Panics
    /// When `i` is out of range.
    pub fn replace(&mut self, i: usize, stack: Option<ItemStack>) -> Option<ItemStack> {
        let mut stack = stack;
        normalise(&mut stack);
        self.changed.set(i);
        std::mem::replace(&mut self.slots[i], stack)
    }

    /// Sets slot `i` and marks it changed.
    ///
    /// # Panics
    /// When `i` is out of range.
    pub fn set(&mut self, i: usize, stack: Option<ItemStack>) {
        self.replace(i, stack);
    }

    /// Empties slot `i`, returning its content, and marks it changed.
    ///
    /// # Panics
    /// When `i` is out of range.
    pub fn take(&mut self, i: usize) -> Option<ItemStack> {
        self.replace(i, None)
    }

    /// Slots changed since the last [`clear_changed`](Self::clear_changed).
    pub fn changed(&self) -> &DirtyMask {
        &self.changed
    }

    /// Forgets all recorded changes.
    pub fn clear_changed(&mut self) {
        self.changed.clear_all();
    }

    /// Marks every slot changed, forcing a full resend.
    pub fn mark_all_changed(&mut self) {
        self.changed.set_all();
    }

    /// Slots the player has pinned. Toolbar actions leave them alone.
    pub fn favorites(&self) -> &DirtyMask {
        &self.favorites
    }

    /// `true` when slot `i` is pinned.
    pub fn is_favorite(&self, i: usize) -> bool {
        self.favorites.get(i)
    }

    /// Pins or unpins slot `i` and marks it changed so the view redraws.
    pub fn set_favorite(&mut self, i: usize, on: bool) {
        self.favorites.put(i, on);
        self.changed.set(i);
    }

    /// Index of the first empty slot.
    pub fn first_empty(&self) -> Option<usize> {
        self.slots.iter().position(Option::is_none)
    }

    /// Index of the first slot holding the same kind as `kind` with fewer
    /// than `max` items.
    pub fn find_mergeable(&self, kind: &ItemStack, max: u32) -> Option<usize> {
        self.slots.iter().position(|s| {
            s.as_ref()
                .is_some_and(|s| s.same_kind(kind) && s.count < max)
        })
    }

    /// Total items of the same kind as `kind` (id and patch).
    pub fn count_of(&self, kind: &ItemStack) -> u64 {
        self.slots
            .iter()
            .flatten()
            .filter(|s| s.same_kind(kind))
            .map(|s| u64::from(s.count))
            .sum()
    }
}

/// An opaque handle to one of the [`Inventories`] a menu can see.
///
/// Which inventory a handle names is a convention of the menu definition.
/// The standard builders on [`MenuDef`] document theirs as associated
/// constants (`MenuDef::CONTAINER`, `MenuDef::PLAYER_MAIN`, ...).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct InventoryRef(u16);

impl InventoryRef {
    /// Handle for position `index` in an [`Inventories`] list.
    pub const fn new(index: u16) -> Self {
        Self(index)
    }

    /// The position this handle addresses.
    pub const fn index(self) -> usize {
        self.0 as usize
    }
}

/// The ordered set of inventories a menu addresses by [`InventoryRef`].
///
/// A chest menu sees three: the chest, the player's main inventory and the
/// hotbar. The ECS layer assembles this list when a menu opens and takes it
/// apart when the menu closes.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Inventories {
    list: Vec<Inventory>,
}

impl Inventories {
    /// An empty list.
    pub fn new() -> Self {
        Self::default()
    }

    /// Empty inventories sized to fit every slot `def` references.
    ///
    /// Inventory `k` gets `1 + max index` slots over the definition's slots
    /// with source `k`, or zero slots if none reference it.
    pub fn for_menu(def: &MenuDef) -> Self {
        Self {
            list: def
                .inventory_sizes()
                .into_iter()
                .map(Inventory::new)
                .collect(),
        }
    }

    /// Appends an inventory and returns its handle.
    ///
    /// # Panics
    /// When more than `u16::MAX` inventories are added.
    pub fn push(&mut self, inventory: Inventory) -> InventoryRef {
        let handle = InventoryRef(u16::try_from(self.list.len()).expect("too many inventories"));
        self.list.push(inventory);
        handle
    }

    /// Number of inventories.
    pub fn len(&self) -> usize {
        self.list.len()
    }

    /// `true` when there are no inventories.
    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    /// The inventory behind `handle`.
    pub fn get(&self, handle: InventoryRef) -> Option<&Inventory> {
        self.list.get(handle.index())
    }

    /// The inventory behind `handle`, mutably.
    pub fn get_mut(&mut self, handle: InventoryRef) -> Option<&mut Inventory> {
        self.list.get_mut(handle.index())
    }

    /// All inventories with their handles, in order.
    pub fn iter(&self) -> impl Iterator<Item = (InventoryRef, &Inventory)> {
        self.list.iter().enumerate().map(|(i, inv)| {
            (
                InventoryRef(u16::try_from(i).expect("too many inventories")),
                inv,
            )
        })
    }

    /// All inventories mutably, in order.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut Inventory> {
        self.list.iter_mut()
    }

    /// Total items of the same kind as `kind` across every inventory.
    pub fn count_of(&self, kind: &ItemStack) -> u64 {
        self.list.iter().map(|inv| inv.count_of(kind)).sum()
    }
}

impl std::ops::Index<InventoryRef> for Inventories {
    type Output = Inventory;

    fn index(&self, handle: InventoryRef) -> &Inventory {
        &self.list[handle.index()]
    }
}

impl std::ops::IndexMut<InventoryRef> for Inventories {
    fn index_mut(&mut self, handle: InventoryRef) -> &mut Inventory {
        &mut self.list[handle.index()]
    }
}

impl FromIterator<Inventory> for Inventories {
    fn from_iter<T: IntoIterator<Item = Inventory>>(iter: T) -> Self {
        Self {
            list: iter.into_iter().collect(),
        }
    }
}

#[allow(clippy::unwrap_used)]
#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::{DirtyMask, Inventories, Inventory, InventoryRef};
    use crate::id::ItemId;
    use crate::stack::ItemStack;

    const STONE: ItemId = ItemId(1);
    const EGG: ItemId = ItemId(2);

    #[test]
    fn dirty_mask_spans_words() {
        let mut m = DirtyMask::new(130);
        assert_eq!(m.len(), 130);
        m.set(0);
        m.set(63);
        m.set(64);
        m.set(129);
        m.set(500);
        assert_eq!(m.iter().collect::<Vec<_>>(), vec![0, 63, 64, 129]);
        assert_eq!(m.count(), 4);
        assert!(m.get(64) && !m.get(65) && !m.get(500));
        m.clear(64);
        assert!(!m.get(64));
        m.set_all();
        assert_eq!(m.count(), 130);
        m.clear_all();
        assert!(!m.any());
    }

    #[test]
    fn set_all_on_word_multiple() {
        let mut m = DirtyMask::new(64);
        m.set_all();
        assert_eq!(m.count(), 64);
    }

    #[test]
    fn mutations_mark_changed_and_normalise() {
        let mut inv = Inventory::new(3);
        inv.set(1, Some(ItemStack::new(STONE, 5)));
        inv.set(2, Some(ItemStack::new(STONE, 0)));
        assert_eq!(inv.get(1).unwrap().count, 5);
        assert!(inv.get(2).is_none());
        assert_eq!(inv.changed().iter().collect::<Vec<_>>(), vec![1, 2]);
        inv.clear_changed();
        assert_eq!(inv.take(1).unwrap().count, 5);
        assert!(inv.changed().get(1));
    }

    #[test]
    fn search_helpers() {
        let inv = Inventory::from_slots([
            Some(ItemStack::new(STONE, 64)),
            None,
            Some(ItemStack::new(STONE, 3)),
            Some(ItemStack::new(EGG, 3)),
        ]);
        assert_eq!(inv.first_empty(), Some(1));
        assert_eq!(inv.find_mergeable(&ItemStack::new(STONE, 1), 64), Some(2));
        assert_eq!(inv.find_mergeable(&ItemStack::new(EGG, 1), 16), Some(3));
        assert_eq!(inv.find_mergeable(&ItemStack::new(ItemId(9), 1), 64), None);
        assert_eq!(inv.count_of(&ItemStack::new(STONE, 1)), 67);
    }

    #[test]
    fn inventories_index_by_handle() {
        let mut invs = Inventories::new();
        let a = invs.push(Inventory::new(2));
        let b = invs.push(Inventory::new(5));
        assert_eq!((a, b), (InventoryRef::new(0), InventoryRef::new(1)));
        assert_eq!(invs[b].len(), 5);
        assert!(invs.get(InventoryRef::new(2)).is_none());
        invs[a].set(0, Some(ItemStack::new(EGG, 4)));
        assert_eq!(invs.count_of(&ItemStack::new(EGG, 1)), 4);
    }

    #[test]
    fn round_trips_through_ron() {
        let mut inv = Inventory::new(70);
        inv.set(69, Some(ItemStack::new(STONE, 2)));
        inv.set_favorite(3, true);
        let back: Inventory = ron::from_str(&ron::to_string(&inv).unwrap()).unwrap();
        assert_eq!(back, inv);
    }
}
