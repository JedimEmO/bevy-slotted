//! Substring index: a sorted suffix array over the lowercased concatenation
//! of every entry's text, so any substring query is two binary searches.
//! Build `O(n log n)` comparisons over `n` total characters (comparisons are
//! bounded by the longest text); query `O(m log n + k)` for a query of `m`
//! chars and `k` hits before dedupe. Package A.

use super::bitset::Bitset;

/// One field's substring index over `entries` texts.
#[derive(Debug, Clone, Default)]
pub struct SubstringIndex {
    /// Lowercased texts joined with `\0`; entry `i` occupies `ranges[i]`.
    text: Vec<char>,
    /// Suffix start positions sorted by the suffix they begin.
    suffixes: Vec<u32>,
    /// `owner[i]` is the entry id of the text containing char `i`.
    owner: Vec<u32>,
    entries: usize,
}

impl SubstringIndex {
    /// Builds from `(entry id, text)` pairs; an entry may contribute several
    /// texts (tooltip lines).
    pub fn build(entries: usize, texts: impl IntoIterator<Item = (u32, String)>) -> Self {
        let mut text: Vec<char> = Vec::new();
        let mut owner: Vec<u32> = Vec::new();
        for (id, t) in texts {
            for ch in t.chars().flat_map(char::to_lowercase) {
                text.push(ch);
                owner.push(id);
            }
            text.push('\0');
            owner.push(id);
        }
        let mut suffixes: Vec<u32> = (0..text.len())
            .filter(|i| text[*i] != '\0')
            .map(|i| u32::try_from(i).unwrap_or(u32::MAX))
            .collect();
        suffixes.sort_unstable_by(|a, b| {
            let sa = suffix(&text, *a as usize);
            let sb = suffix(&text, *b as usize);
            sa.cmp(sb)
        });
        Self {
            text,
            suffixes,
            owner,
            entries,
        }
    }

    /// Entries whose text contains `needle` (case-insensitive). An empty
    /// needle matches every entry that has any text.
    pub fn find(&self, needle: &str) -> Bitset {
        let needle: Vec<char> = needle.chars().flat_map(char::to_lowercase).collect();
        let mut out = Bitset::new(self.entries);
        let lo = self
            .suffixes
            .partition_point(|s| starts_before(suffix(&self.text, *s as usize), &needle));
        for s in &self.suffixes[lo..] {
            let suf = suffix(&self.text, *s as usize);
            if !suf.starts_with(&needle) {
                break;
            }
            out.insert(self.owner[*s as usize] as usize);
        }
        out
    }

    /// Number of indexed characters.
    pub fn len(&self) -> usize {
        self.text.len()
    }

    /// Whether nothing was indexed.
    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }
}

/// The suffix starting at `i`, ending at the next `\0` or the end.
fn suffix(text: &[char], i: usize) -> &[char] {
    let end = text[i..]
        .iter()
        .position(|c| *c == '\0')
        .map_or(text.len(), |p| i + p);
    &text[i..end]
}

/// Whether `s` sorts strictly before every string with prefix `needle`.
fn starts_before(s: &[char], needle: &[char]) -> bool {
    let n = needle.len().min(s.len());
    match s[..n].cmp(&needle[..n]) {
        std::cmp::Ordering::Less => true,
        std::cmp::Ordering::Greater => false,
        std::cmp::Ordering::Equal => s.len() < needle.len(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn substrings_hit_the_right_entries() {
        let idx = SubstringIndex::build(
            3,
            [
                (0, "Cobblestone".to_owned()),
                (1, "Oak Planks".to_owned()),
                (2, "Stone Bricks".to_owned()),
            ],
        );
        assert_eq!(idx.find("stone").iter().collect::<Vec<_>>(), vec![0, 2]);
        assert_eq!(idx.find("PLANK").iter().collect::<Vec<_>>(), vec![1]);
        assert_eq!(idx.find("k p").iter().collect::<Vec<_>>(), vec![1]);
        assert!(idx.find("zzz").is_empty());
        assert_eq!(idx.find("").count(), 3);
    }
}
