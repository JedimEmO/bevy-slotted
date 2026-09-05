//! Interning: [`Namespaced`] authoring ids in, dense numeric handles out.
//!
//! Every registry keeps one [`Interner`]. It owns the two-way mapping between
//! the `copper_chest:sorter` string a mod wrote and the `u32` the runtime uses,
//! and it is the single place that decides what number an entry gets.
//!
//! The numeric handle type is a parameter rather than a bare `u32` so that an
//! [`ItemId`] can never be passed where a [`ComponentId`] is wanted. Handles
//! implement [`InternedId`]; [`RegistryId<T>`] covers every registry that does
//! not already have a named handle in `slotted-model`.

use core::marker::PhantomData;

use indexmap::IndexSet;
use serde::de::{SeqAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use slotted_model::{ComponentId, ItemId, Namespaced};

/// A dense numeric handle an [`Interner`] can mint.
///
/// This is deliberately not `From<u32> + Into<u32>`: [`ItemId`] and
/// [`ComponentId`] live in `slotted-model`, `u32` lives in `core`, and the
/// orphan rule puts those conversions out of reach of this crate. A local
/// trait costs one line per handle type and says exactly what is meant, which
/// is "this number indexes a registry" rather than "this is an integer".
///
/// Implement it for a new handle type with [`impl_interned_id!`](crate::impl_interned_id).
pub trait InternedId: Copy + Eq {
    /// Wraps a dense index. Only an [`Interner`] should call this.
    fn from_index(index: u32) -> Self;
    /// Unwraps the dense index.
    fn index(self) -> u32;
}

/// Implements [`InternedId`] for a newtype around a `u32`.
///
/// ```
/// # use slotted_registry::impl_interned_id;
/// #[derive(Clone, Copy, PartialEq, Eq)]
/// pub struct MachineId(pub u32);
/// impl_interned_id!(MachineId);
/// ```
#[macro_export]
macro_rules! impl_interned_id {
    ($ty:ty) => {
        impl $crate::interner::InternedId for $ty {
            fn from_index(index: u32) -> Self {
                Self(index)
            }
            fn index(self) -> u32 {
                self.0
            }
        }
    };
}

impl_interned_id!(ItemId);
impl_interned_id!(ComponentId);

/// The default handle for a registry of `T` that has no hand-written id type.
///
/// The `PhantomData<fn() -> T>` makes `RegistryId<RecipeDef>` a different type
/// from `RegistryId<ItemDef>` without borrowing `T`'s auto traits: the id stays
/// `Send`, `Sync` and `'static` whatever `T` is.
pub struct RegistryId<T: ?Sized>(u32, PhantomData<fn() -> T>);

impl<T: ?Sized> RegistryId<T> {
    /// Wraps a raw index. Only an [`Interner`] should need this.
    pub const fn new(index: u32) -> Self {
        Self(index, PhantomData)
    }

    /// The raw index.
    pub const fn get(self) -> u32 {
        self.0
    }
}

// Derived impls would demand `T: Clone`, `T: Debug` and so on, which is wrong:
// the id holds no `T`. They are written out instead.
impl<T: ?Sized> Clone for RegistryId<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T: ?Sized> Copy for RegistryId<T> {}
impl<T: ?Sized> PartialEq for RegistryId<T> {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}
impl<T: ?Sized> Eq for RegistryId<T> {}
impl<T: ?Sized> PartialOrd for RegistryId<T> {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl<T: ?Sized> Ord for RegistryId<T> {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.0.cmp(&other.0)
    }
}
impl<T: ?Sized> core::hash::Hash for RegistryId<T> {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        self.0.hash(state);
    }
}
impl<T: ?Sized> core::fmt::Debug for RegistryId<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "RegistryId({})", self.0)
    }
}
impl<T: ?Sized> Serialize for RegistryId<T> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u32(self.0)
    }
}
impl<'de, T: ?Sized> Deserialize<'de> for RegistryId<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        u32::deserialize(deserializer).map(Self::new)
    }
}
impl<T: ?Sized> InternedId for RegistryId<T> {
    fn from_index(index: u32) -> Self {
        Self::new(index)
    }
    fn index(self) -> u32 {
        self.0
    }
}

/// A two-way map between [`Namespaced`] ids and dense numeric handles.
///
/// Ids are handed out in interning order starting at zero, so a frozen
/// interner's handles are exactly the indices of its name list. That is what
/// lets a [`Registry`](crate::registry::Registry) keep its values in a plain
/// `Vec` and index it with the handle.
///
/// ```
/// # use slotted_model::{ItemId, Namespaced};
/// # use slotted_registry::interner::Interner;
/// let mut interner = Interner::<ItemId>::new();
/// let chest = Namespaced::parse("slotted:chest")?;
/// let id = interner.intern(&chest);
/// assert_eq!(id, ItemId(0));
/// assert_eq!(interner.intern(&chest), id, "interning is idempotent");
/// assert_eq!(interner.name(id), Some(&chest));
/// # Ok::<(), slotted_model::NamespacedError>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Interner<K> {
    names: IndexSet<Namespaced>,
    handle: PhantomData<fn() -> K>,
}

impl<K> Default for Interner<K> {
    fn default() -> Self {
        Self {
            names: IndexSet::new(),
            handle: PhantomData,
        }
    }
}

impl<K: InternedId> Interner<K> {
    /// An empty interner.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the handle for `name`, minting a new one the first time the
    /// interner sees it.
    ///
    /// # Panics
    ///
    /// If more than `u32::MAX` distinct names are interned.
    pub fn intern(&mut self, name: &Namespaced) -> K {
        if let Some(index) = self.names.get_index_of(name) {
            return K::from_index(narrow(index));
        }
        let (index, _) = self.names.insert_full(name.clone());
        K::from_index(narrow(index))
    }

    /// The handle for `name`, or `None` if it was never interned.
    pub fn get(&self, name: &Namespaced) -> Option<K> {
        self.names
            .get_index_of(name)
            .map(|index| K::from_index(narrow(index)))
    }

    /// The name behind `handle`, or `None` if the handle came from a different
    /// interner and is out of range here.
    pub fn name(&self, handle: K) -> Option<&Namespaced> {
        self.names.get_index(handle.index() as usize)
    }

    /// Whether `name` has a handle.
    pub fn contains(&self, name: &Namespaced) -> bool {
        self.names.contains(name)
    }

    /// How many names are interned.
    pub fn len(&self) -> usize {
        self.names.len()
    }

    /// Whether nothing is interned yet.
    pub fn is_empty(&self) -> bool {
        self.names.is_empty()
    }

    /// Every `(handle, name)` pair, in handle order.
    pub fn iter(&self) -> impl ExactSizeIterator<Item = (K, &Namespaced)> {
        self.names
            .iter()
            .enumerate()
            .map(|(index, name)| (K::from_index(narrow(index)), name))
    }

    /// Every name, in handle order.
    pub fn names(&self) -> impl ExactSizeIterator<Item = &Namespaced> {
        self.names.iter()
    }

    /// Drops every name, keeping the allocation.
    pub fn clear(&mut self) {
        self.names.clear();
    }
}

fn narrow(index: usize) -> u32 {
    u32::try_from(index).expect("a registry never holds more than u32::MAX entries")
}

impl<'a, K: InternedId> IntoIterator for &'a Interner<K> {
    type Item = (K, &'a Namespaced);
    type IntoIter = core::iter::Map<
        core::iter::Enumerate<indexmap::set::Iter<'a, Namespaced>>,
        fn((usize, &'a Namespaced)) -> (K, &'a Namespaced),
    >;

    fn into_iter(self) -> Self::IntoIter {
        fn pair<K: InternedId>((index, name): (usize, &Namespaced)) -> (K, &Namespaced) {
            (K::from_index(narrow(index)), name)
        }
        self.names.iter().enumerate().map(pair::<K>)
    }
}

impl<K: InternedId> FromIterator<Namespaced> for Interner<K> {
    fn from_iter<I: IntoIterator<Item = Namespaced>>(iter: I) -> Self {
        Self {
            names: iter.into_iter().collect(),
            handle: PhantomData,
        }
    }
}

// An interner is a name list and the handles are its indices, so serialising
// the list is enough and it keeps the wire format readable.
impl<K> Serialize for Interner<K> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_seq(&self.names)
    }
}

impl<'de, K> Deserialize<'de> for Interner<K> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct NameSeq<K>(PhantomData<fn() -> K>);

        impl<'de, K> Visitor<'de> for NameSeq<K> {
            type Value = Interner<K>;

            fn expecting(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                f.write_str("a sequence of namespaced ids")
            }

            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
                let mut names = IndexSet::with_capacity(seq.size_hint().unwrap_or(0));
                while let Some(name) = seq.next_element::<Namespaced>()? {
                    if !names.insert(name.clone()) {
                        return Err(serde::de::Error::custom(format!(
                            "duplicate id `{name}` in an interner"
                        )));
                    }
                }
                Ok(Interner {
                    names,
                    handle: PhantomData,
                })
            }
        }

        deserializer.deserialize_seq(NameSeq(PhantomData))
    }
}

// `unwrap_used` is a production lint. In a test an unwrap is the assertion.
#[allow(clippy::unwrap_used)]
#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use slotted_model::{ComponentId, ItemId, Namespaced};

    use super::{Interner, RegistryId};

    fn id(s: &str) -> Namespaced {
        Namespaced::parse(s).unwrap()
    }

    #[test]
    fn interning_is_dense_and_idempotent() {
        let mut interner = Interner::<ItemId>::new();
        let chest = interner.intern(&id("slotted:chest"));
        let stone = interner.intern(&id("slotted:stone"));
        assert_eq!((chest, stone), (ItemId(0), ItemId(1)));
        assert_eq!(interner.intern(&id("slotted:chest")), chest);
        assert_eq!(interner.len(), 2);
    }

    #[test]
    fn round_trips_name_to_handle_and_back() {
        let mut interner = Interner::<ComponentId>::new();
        for name in ["slotted:damage", "slotted:lore", "copper_chest:tint"] {
            let handle = interner.intern(&id(name));
            assert_eq!(interner.name(handle).unwrap().as_str(), name);
            assert_eq!(interner.get(&id(name)), Some(handle));
            assert!(interner.contains(&id(name)));
        }
        assert_eq!(interner.get(&id("slotted:absent")), None);
        assert_eq!(interner.name(ComponentId(99)), None);
    }

    #[test]
    fn iterates_in_handle_order_not_alphabetical_order() {
        let mut interner = Interner::<ItemId>::new();
        interner.intern(&id("b:b"));
        interner.intern(&id("a:a"));
        let seen: Vec<_> = interner.iter().map(|(k, n)| (k, n.to_string())).collect();
        assert_eq!(
            seen,
            vec![(ItemId(0), "b:b".to_owned()), (ItemId(1), "a:a".to_owned())]
        );
        assert_eq!((&interner).into_iter().count(), 2);
        assert_eq!(interner.names().count(), 2);
    }

    #[test]
    fn survives_a_serde_round_trip() {
        let mut interner = Interner::<ItemId>::new();
        interner.intern(&id("slotted:chest"));
        interner.intern(&id("slotted:stone"));
        let encoded = ron::to_string(&interner).unwrap();
        assert_eq!(encoded, "[\"slotted:chest\",\"slotted:stone\"]");
        let decoded: Interner<ItemId> = ron::from_str(&encoded).unwrap();
        assert_eq!(decoded, interner);
    }

    #[test]
    fn rejects_a_duplicate_on_deserialisation() {
        let err = ron::from_str::<Interner<ItemId>>("[\"a:a\",\"a:a\"]").unwrap_err();
        assert!(err.to_string().contains("duplicate id"), "{err}");
    }

    #[test]
    fn registry_ids_are_typed_but_transparent() {
        struct Recipe;
        struct Screen;
        let recipe = RegistryId::<Recipe>::new(3);
        assert_eq!(recipe.get(), 3);
        assert_eq!(ron::to_string(&recipe).unwrap(), "3");
        assert_eq!(
            ron::from_str::<RegistryId<Screen>>("3").unwrap(),
            RegistryId::<Screen>::new(3)
        );
        assert_eq!(format!("{recipe:?}"), "RegistryId(3)");
    }

    #[test]
    fn an_empty_interner_reports_empty() {
        let mut interner = Interner::<ItemId>::new();
        assert!(interner.is_empty());
        interner.intern(&id("a:a"));
        assert!(!interner.is_empty());
        interner.clear();
        assert!(interner.is_empty());
    }

    #[test]
    fn collects_from_an_iterator_of_names() {
        let interner: Interner<ItemId> = [id("a:a"), id("b:b")].into_iter().collect();
        assert_eq!(interner.get(&id("b:b")), Some(ItemId(1)));
    }
}
