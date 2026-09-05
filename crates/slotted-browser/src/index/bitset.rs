//! A dense bitset over [`EntryId`](super::EntryId)s. Package A.

/// A fixed-capacity bitset; `and`, `or` and `and_not` are word-wise.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct Bitset {
    words: Vec<u64>,
    len: usize,
}

impl Bitset {
    /// A bitset of `len` cleared bits.
    pub fn new(len: usize) -> Self {
        Self {
            words: vec![0; len.div_ceil(64)],
            len,
        }
    }

    /// A bitset of `len` set bits.
    pub fn full(len: usize) -> Self {
        let mut set = Self::new(len);
        for i in 0..len {
            set.insert(i);
        }
        set
    }

    /// Number of addressable bits.
    pub const fn len(&self) -> usize {
        self.len
    }

    /// Whether no bit is set.
    pub fn is_empty(&self) -> bool {
        self.words.iter().all(|w| *w == 0)
    }

    /// Grows the set to address at least `len` bits. Existing bits keep
    /// their positions and the new ones start cleared; shrinking is a no-op.
    pub fn grow(&mut self, len: usize) {
        if len <= self.len {
            return;
        }
        self.words.resize(len.div_ceil(64), 0);
        self.len = len;
    }

    /// Sets bit `i`; out of range is ignored.
    pub fn insert(&mut self, i: usize) {
        if i < self.len {
            self.words[i / 64] |= 1 << (i % 64);
        }
    }

    /// Clears bit `i`.
    pub fn remove(&mut self, i: usize) {
        if i < self.len {
            self.words[i / 64] &= !(1 << (i % 64));
        }
    }

    /// Whether bit `i` is set.
    pub fn contains(&self, i: usize) -> bool {
        i < self.len && self.words[i / 64] & (1 << (i % 64)) != 0
    }

    /// Number of set bits.
    pub fn count(&self) -> usize {
        self.words.iter().map(|w| w.count_ones() as usize).sum()
    }

    /// `self &= other`.
    pub fn and(&mut self, other: &Self) {
        for (a, b) in self.words.iter_mut().zip(&other.words) {
            *a &= *b;
        }
        for a in self.words.iter_mut().skip(other.words.len()) {
            *a = 0;
        }
    }

    /// `self |= other`.
    pub fn or(&mut self, other: &Self) {
        for (a, b) in self.words.iter_mut().zip(&other.words) {
            *a |= *b;
        }
    }

    /// `self &= !other`.
    pub fn and_not(&mut self, other: &Self) {
        for (a, b) in self.words.iter_mut().zip(&other.words) {
            *a &= !*b;
        }
    }

    /// Set bits in ascending order.
    pub fn iter(&self) -> impl Iterator<Item = usize> + '_ {
        self.words.iter().enumerate().flat_map(|(wi, w)| {
            let mut w = *w;
            std::iter::from_fn(move || {
                if w == 0 {
                    return None;
                }
                let bit = w.trailing_zeros() as usize;
                w &= w - 1;
                Some(wi * 64 + bit)
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_ops_and_iteration() {
        let mut a = Bitset::new(130);
        a.insert(0);
        a.insert(64);
        a.insert(129);
        let mut b = Bitset::new(130);
        b.insert(64);
        b.insert(1);
        let mut c = a.clone();
        c.and(&b);
        assert_eq!(c.iter().collect::<Vec<_>>(), vec![64]);
        let mut d = a.clone();
        d.and_not(&b);
        assert_eq!(d.iter().collect::<Vec<_>>(), vec![0, 129]);
        a.or(&b);
        assert_eq!(a.count(), 4);
        assert_eq!(Bitset::full(70).count(), 70);
    }
}
