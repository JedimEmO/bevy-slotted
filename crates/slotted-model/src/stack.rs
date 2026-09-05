//! Item stacks and component patches.
//!
//! An [`ItemStack`] is `{id, count, patch}`. Two stacks are the *same kind*,
//! and therefore mergeable, when they have the same [`ItemId`] and a
//! structurally equal [`ComponentPatch`]; this mirrors vanilla's
//! `isSameItemSameComponents`. The patch is an ordered map so equality is a
//! single ordered walk and serialised output is canonical.
//!
//! A slot holds an `Option<ItemStack>`; the empty state is `None` and a
//! zero-count stack is never stored. [`take_from`] and [`normalise`] keep that
//! invariant for callers that operate on slots directly.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::id::{ComponentId, ItemId};
pub use crate::value::Value;

/// The per-stack component overrides, keyed by [`ComponentId`].
///
/// Empty for the overwhelming majority of stacks. Kept ordered so two patches
/// compare with one linear walk and serialise identically regardless of
/// insertion order.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ComponentPatch(BTreeMap<ComponentId, Value>);

impl ComponentPatch {
    /// An empty patch.
    pub fn new() -> Self {
        Self::default()
    }

    /// `true` when no component is overridden.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Number of overridden components.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// The value for `id`, if overridden.
    pub fn get(&self, id: ComponentId) -> Option<&Value> {
        self.0.get(&id)
    }

    /// Sets `id` to `value`, returning the previous value.
    pub fn insert(&mut self, id: ComponentId, value: impl Into<Value>) -> Option<Value> {
        self.0.insert(id, value.into())
    }

    /// Removes the override for `id`, returning it.
    pub fn remove(&mut self, id: ComponentId) -> Option<Value> {
        self.0.remove(&id)
    }

    /// Iterates the overrides in `ComponentId` order.
    pub fn iter(&self) -> impl Iterator<Item = (ComponentId, &Value)> {
        self.0.iter().map(|(k, v)| (*k, v))
    }
}

impl FromIterator<(ComponentId, Value)> for ComponentPatch {
    fn from_iter<T: IntoIterator<Item = (ComponentId, Value)>>(iter: T) -> Self {
        Self(iter.into_iter().collect())
    }
}

/// A stack of `count` items of kind `id` with component overrides `patch`.
///
/// `count` is always at least one while the stack sits in a slot; a stack that
/// reaches zero is replaced by `None`. Methods that can drain a stack say so.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ItemStack {
    /// The item kind.
    pub id: ItemId,
    /// How many items. Never zero when stored in a slot.
    pub count: u32,
    /// Component overrides. Part of the stack's kind for merging purposes.
    #[serde(default, skip_serializing_if = "ComponentPatch::is_empty")]
    pub patch: ComponentPatch,
}

impl ItemStack {
    /// A stack of `count` plain items with no component patch.
    pub fn new(id: ItemId, count: u32) -> Self {
        Self {
            id,
            count,
            patch: ComponentPatch::new(),
        }
    }

    /// Builder: attach a component patch.
    #[must_use]
    pub fn with_patch(mut self, patch: ComponentPatch) -> Self {
        self.patch = patch;
        self
    }

    /// Builder: same kind, different count.
    #[must_use]
    pub fn with_count(mut self, count: u32) -> Self {
        self.count = count;
        self
    }

    /// `true` when the count is zero.
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// Same item id and structurally equal patch. The merge test.
    pub fn same_kind(&self, other: &Self) -> bool {
        self.id == other.id && self.patch == other.patch
    }

    /// Removes up to `n` items from this stack and returns them as a new
    /// stack of the same kind, or `None` when `n` is zero.
    ///
    /// This stack may be left with a count of zero; a caller holding it in a
    /// slot should use [`take_from`] instead, which keeps the slot normalised.
    pub fn split(&mut self, n: u32) -> Option<Self> {
        let n = n.min(self.count);
        if n == 0 {
            return None;
        }
        self.count -= n;
        Some(Self {
            id: self.id,
            count: n,
            patch: self.patch.clone(),
        })
    }

    /// Moves as many items as fit from this stack into `other`, where `other`
    /// may hold at most `max` items. Returns how many moved. Moves nothing
    /// when the two are not the same kind.
    ///
    /// This stack may be left with a count of zero.
    pub fn merge_into(&mut self, other: &mut Self, max: u32) -> u32 {
        if !self.same_kind(other) {
            return 0;
        }
        let room = max.saturating_sub(other.count);
        let moved = self.count.min(room);
        self.count -= moved;
        other.count += moved;
        moved
    }
}

/// Replaces a zero-count stack with `None`.
pub fn normalise(slot: &mut Option<ItemStack>) {
    if slot.as_ref().is_some_and(ItemStack::is_empty) {
        *slot = None;
    }
}

/// Takes up to `n` items out of `slot`, leaving it normalised.
pub fn take_from(slot: &mut Option<ItemStack>, n: u32) -> Option<ItemStack> {
    let taken = slot.as_mut()?.split(n);
    normalise(slot);
    taken
}

#[allow(clippy::unwrap_used)]
#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::{ComponentPatch, ItemStack, Value, normalise, take_from};
    use crate::id::{ComponentId, ItemId};

    const STONE: ItemId = ItemId(1);

    #[test]
    fn same_kind_needs_equal_patch() {
        let a = ItemStack::new(STONE, 3);
        let mut b = ItemStack::new(STONE, 9);
        assert!(a.same_kind(&b));
        b.patch.insert(ComponentId(0), 5_i64);
        assert!(!a.same_kind(&b));
        let mut c = ItemStack::new(STONE, 1);
        c.patch.insert(ComponentId(0), 5_i64);
        assert!(b.same_kind(&c));
    }

    #[test]
    fn split_and_merge_move_counts() {
        let mut a = ItemStack::new(STONE, 10);
        let taken = a.split(4).unwrap();
        assert_eq!((a.count, taken.count), (6, 4));
        assert!(a.split(0).is_none());
        let mut b = ItemStack::new(STONE, 60);
        assert_eq!(a.merge_into(&mut b, 64), 4);
        assert_eq!((a.count, b.count), (2, 64));
        let mut other = ItemStack::new(ItemId(2), 1);
        assert_eq!(a.merge_into(&mut other, 64), 0);
    }

    #[test]
    fn take_from_keeps_slot_normalised() {
        let mut slot = Some(ItemStack::new(STONE, 2));
        assert_eq!(take_from(&mut slot, 1).unwrap().count, 1);
        assert!(slot.is_some());
        assert_eq!(take_from(&mut slot, 5).unwrap().count, 1);
        assert!(slot.is_none());
        assert!(take_from(&mut slot, 1).is_none());
        let mut zero = Some(ItemStack::new(STONE, 0));
        normalise(&mut zero);
        assert!(zero.is_none());
    }

    #[test]
    fn patch_serialises_canonically() {
        let mut p = ComponentPatch::new();
        p.insert(ComponentId(2), "b");
        p.insert(ComponentId(1), Value::Bool(true));
        let mut q = ComponentPatch::new();
        q.insert(ComponentId(1), true);
        q.insert(ComponentId(2), "b");
        assert_eq!(ron::to_string(&p).unwrap(), ron::to_string(&q).unwrap());
        let back: ComponentPatch = ron::from_str(&ron::to_string(&p).unwrap()).unwrap();
        assert_eq!(back, p);
    }

    #[test]
    fn empty_patch_is_skipped_in_serialised_stack() {
        let s = ItemStack::new(STONE, 4);
        let encoded = ron::to_string(&s).unwrap();
        assert!(!encoded.contains("patch"));
        let back: ItemStack = ron::from_str(&encoded).unwrap();
        assert_eq!(back, s);
    }
}
