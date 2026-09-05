//! The search grammar and evaluator. Contract section 4. Package A.
//!
//! Grammar: whitespace splits terms (AND); `|` between terms is OR and binds
//! tighter than AND; `-` at term start negates; `"quoted phrase"` keeps
//! spaces; `\` escapes the next char; a leading prefix char selects a field
//! (`@` mod, `#` tag, `$` tooltip, `&` id, `%` category), each with a
//! [`PrefixMode`].

use std::collections::BTreeMap;
use std::sync::Arc;

use bevy::prelude::Resource;
use serde::{Deserialize, Serialize};

use crate::index::{Bitset, BrowserIndex, EntryId};

/// Which index a term searches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Field {
    /// Display name (no prefix).
    Name,
    /// `@`.
    Mod,
    /// `#`.
    Tag,
    /// `$`.
    Tooltip,
    /// `&`.
    Id,
    /// `%`.
    Category,
}

impl Field {
    /// The prefix character, or `None` for [`Field::Name`].
    pub const fn prefix(self) -> Option<char> {
        match self {
            Self::Name => None,
            Self::Mod => Some('@'),
            Self::Tag => Some('#'),
            Self::Tooltip => Some('$'),
            Self::Id => Some('&'),
            Self::Category => Some('%'),
        }
    }

    /// The field for a prefix character.
    pub const fn from_prefix(c: char) -> Option<Self> {
        match c {
            '@' => Some(Self::Mod),
            '#' => Some(Self::Tag),
            '$' => Some(Self::Tooltip),
            '&' => Some(Self::Id),
            '%' => Some(Self::Category),
            _ => None,
        }
    }

    /// Every prefixed field.
    pub const PREFIXED: [Self; 5] = [
        Self::Mod,
        Self::Tag,
        Self::Tooltip,
        Self::Id,
        Self::Category,
    ];
}

/// How a prefixed field takes part in a search.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrefixMode {
    /// The prefix works, and unprefixed terms also search this field.
    Enabled,
    /// The prefix works; unprefixed terms do not touch this field.
    #[default]
    RequirePrefix,
    /// The prefix is literal text.
    Disabled,
}

/// Per-field modes plus the sort order. Defaults follow JEI: `$` Enabled,
/// `@ # %` `RequirePrefix`, `&` `Disabled`.
#[derive(Resource, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchConfig {
    /// Mode per prefixed field.
    pub modes: BTreeMap<Field, PrefixMode>,
    /// Sort stages applied in order.
    pub sort: Vec<SortStage>,
}

impl Default for SearchConfig {
    fn default() -> Self {
        let mut modes = BTreeMap::new();
        modes.insert(Field::Tooltip, PrefixMode::Enabled);
        modes.insert(Field::Mod, PrefixMode::RequirePrefix);
        modes.insert(Field::Tag, PrefixMode::RequirePrefix);
        modes.insert(Field::Category, PrefixMode::RequirePrefix);
        modes.insert(Field::Id, PrefixMode::Disabled);
        Self {
            modes,
            sort: vec![SortStage::IngredientType, SortStage::Alphabetical],
        }
    }
}

impl SearchConfig {
    /// The mode of a field; `Name` is always `Enabled`.
    pub fn mode(&self, field: Field) -> PrefixMode {
        if field == Field::Name {
            return PrefixMode::Enabled;
        }
        self.modes.get(&field).copied().unwrap_or_default()
    }
}

impl PartialOrd for Field {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Field {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        (*self as u8).cmp(&(*other as u8))
    }
}

/// One search term after tokenising.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    /// Which field; `Name` also searches every `Enabled` field.
    pub field: Field,
    /// The text to match, unquoted and unescaped.
    pub text: String,
    /// `-` prefix.
    pub negate: bool,
    /// Terms joined by `|` share a group; groups are intersected.
    pub or_group: u32,
}

/// One character of a raw term with the flag that says whether it arrived
/// quoted or escaped. Only unquoted, unescaped leading characters are read as
/// the `-` negation or a field prefix, so `"-iron"` and `\@demo` are text.
#[derive(Debug, Clone, Copy)]
struct RawChar {
    ch: char,
    literal: bool,
}

/// A term as the scanner found it, before `-` and prefixes are peeled off.
#[derive(Debug, Default)]
struct RawTerm {
    chars: Vec<RawChar>,
    /// A `|` sat between this term and the previous one.
    joined: bool,
}

/// Splits a query into tokens. See the module docs for the grammar.
///
/// One pass over the characters. Whitespace ends a term, `|` ends a term and
/// marks the next one as belonging to the same OR group, `"` toggles quoting
/// and `\` makes the next character literal. Terms that end up empty (a lone
/// `-`, a stray `|`, trailing whitespace) produce no token.
pub fn tokenize(query: &str, config: &SearchConfig) -> Vec<Token> {
    let raw = scan(query);
    let mut out = Vec::new();
    let mut group = 0u32;
    let mut emitted = false;
    for term in &raw {
        if emitted && !term.joined {
            group = group.saturating_add(1);
        }
        let Some(mut token) = peel(term, config) else {
            // An empty term still separates OR groups when it was not joined.
            continue;
        };
        token.or_group = group;
        out.push(token);
        emitted = true;
    }
    out
}

/// Character scan: quotes, escapes, whitespace and `|`.
fn scan(query: &str) -> Vec<RawTerm> {
    let mut terms: Vec<RawTerm> = Vec::new();
    let mut current = RawTerm::default();
    let mut started = false;
    let mut quoted = false;
    let mut join_next = false;
    let mut chars = query.chars();
    while let Some(ch) = chars.next() {
        match ch {
            '\\' => {
                if let Some(next) = chars.next() {
                    current.chars.push(RawChar {
                        ch: next,
                        literal: true,
                    });
                    started = true;
                }
            }
            '"' => {
                quoted = !quoted;
                started = true;
            }
            '|' if !quoted => {
                end_term(&mut current, &mut started, &mut join_next, &mut terms, true);
            }
            c if c.is_whitespace() && !quoted => {
                end_term(
                    &mut current,
                    &mut started,
                    &mut join_next,
                    &mut terms,
                    false,
                );
            }
            c => {
                current.chars.push(RawChar {
                    ch: c,
                    literal: quoted,
                });
                started = true;
            }
        }
    }
    end_term(
        &mut current,
        &mut started,
        &mut join_next,
        &mut terms,
        false,
    );
    terms
}

/// Ends the term under construction, remembering whether a `|` followed.
fn end_term(
    current: &mut RawTerm,
    started: &mut bool,
    join_next: &mut bool,
    terms: &mut Vec<RawTerm>,
    join_after: bool,
) {
    if *started {
        let mut term = std::mem::take(current);
        term.joined = *join_next;
        terms.push(term);
        *started = false;
        *join_next = join_after;
    } else if join_after {
        // `a | b`: the whitespace between the terms ends nothing, so it must
        // not forget the `|` that came before it.
        *join_next = true;
    }
}

/// Peels `-` and a field prefix off a raw term. `None` when nothing is left.
fn peel(term: &RawTerm, config: &SearchConfig) -> Option<Token> {
    let mut rest = term.chars.as_slice();
    let mut negate = false;
    if let Some(first) = rest.first()
        && first.ch == '-'
        && !first.literal
    {
        negate = true;
        rest = &rest[1..];
    }
    let mut field = Field::Name;
    if let Some(first) = rest.first()
        && !first.literal
        && let Some(f) = Field::from_prefix(first.ch)
        && config.mode(f) != PrefixMode::Disabled
    {
        field = f;
        rest = &rest[1..];
    }
    let text: String = rest.iter().map(|c| c.ch).collect();
    if text.is_empty() {
        return None;
    }
    Some(Token {
        field,
        text,
        negate,
        or_group: 0,
    })
}

/// Entries matching every OR-group and no negated term, minus `hidden`.
/// An empty token list is every visible entry.
pub fn evaluate(
    index: &BrowserIndex,
    tokens: &[Token],
    config: &SearchConfig,
    hidden: &Bitset,
) -> Bitset {
    let n = index.len();
    let mut result = Bitset::full(n);
    let mut groups: BTreeMap<u32, Bitset> = BTreeMap::new();
    for token in tokens {
        let hits = field_hits(index, token, config);
        if token.negate {
            result.and_not(&hits);
        } else {
            groups
                .entry(token.or_group)
                .or_insert_with(|| Bitset::new(n))
                .or(&hits);
        }
    }
    for group in groups.values() {
        result.and(group);
    }
    result.and_not(hidden);
    result
}

fn field_hits(index: &BrowserIndex, token: &Token, config: &SearchConfig) -> Bitset {
    let one = |field: Field| match field {
        Field::Name => index.names.find(&token.text),
        Field::Tooltip => index.tooltips.find(&token.text),
        Field::Id => index.ids.find(&token.text),
        Field::Mod => index.mods.find(&token.text),
        Field::Tag => index.tags.find(&token.text),
        Field::Category => index.categories.find(&token.text),
    };
    let mut hits = one(token.field);
    if token.field == Field::Name {
        for field in Field::PREFIXED {
            if config.mode(field) == PrefixMode::Enabled {
                hits.or(&one(field));
            }
        }
    }
    hits
}

/// One ordering criterion; stages apply lexicographically.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SortStage {
    /// Index order, which is type registration order then type entry order.
    Registration,
    /// By display name.
    Alphabetical,
    /// By ingredient type registration order.
    IngredientType,
    /// By mod namespace.
    ModName,
    /// Rarest last.
    Rarity,
}

/// Orders a result set. Stages apply lexicographically and the [`EntryId`]
/// breaks every tie, so the order is total and stable across runs.
pub fn sort(index: &BrowserIndex, set: &Bitset, stages: &[SortStage]) -> Vec<EntryId> {
    let mut ids: Vec<EntryId> = set
        .iter()
        .map(|i| EntryId(u32::try_from(i).unwrap_or(u32::MAX)))
        .collect();
    ids.sort_by(|a, b| {
        let (ea, eb) = (&index.entries[a.index()], &index.entries[b.index()]);
        for stage in stages {
            let ord = match stage {
                SortStage::Registration => a.cmp(b),
                SortStage::Alphabetical => {
                    ea.display.to_lowercase().cmp(&eb.display.to_lowercase())
                }
                SortStage::IngredientType => index
                    .type_rank(&ea.ingredient.ty)
                    .cmp(&index.type_rank(&eb.ingredient.ty)),
                SortStage::ModName => ea.mod_ns.cmp(&eb.mod_ns),
                SortStage::Rarity => (ea.rarity as u8).cmp(&(eb.rarity as u8)),
            };
            if ord != std::cmp::Ordering::Equal {
                return ord;
            }
        }
        a.cmp(b)
    });
    ids
}

/// The last search result, keyed by what can change it.
#[derive(Resource, Debug, Default, Clone)]
pub struct SearchCache {
    key: Option<(String, u32, u32)>,
    result: Arc<Vec<EntryId>>,
}

impl SearchCache {
    /// The cached result for this key, if it is the one stored.
    pub fn get(
        &self,
        query: &str,
        hidden_version: u32,
        visibility_version: u32,
    ) -> Option<Arc<Vec<EntryId>>> {
        match &self.key {
            Some((q, h, v)) if q == query && *h == hidden_version && *v == visibility_version => {
                Some(self.result.clone())
            }
            _ => None,
        }
    }

    /// Replaces the cached result.
    pub fn put(
        &mut self,
        query: &str,
        hidden_version: u32,
        visibility_version: u32,
        result: Arc<Vec<EntryId>>,
    ) {
        self.key = Some((query.to_owned(), hidden_version, visibility_version));
        self.result = result;
    }

    /// Forgets the cached result.
    pub fn invalidate(&mut self) {
        self.key = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn negation_and_prefixes_tokenize() {
        let cfg = SearchConfig::default();
        let tokens = tokenize("iron -@demo #ingots &id", &cfg);
        assert_eq!(tokens.len(), 4);
        assert_eq!(tokens[0].field, Field::Name);
        assert!(tokens[1].negate);
        assert_eq!(tokens[1].field, Field::Mod);
        assert_eq!(tokens[2].field, Field::Tag);
        // `&` is Disabled by default, so it stays literal text.
        assert_eq!(tokens[3].field, Field::Name);
        assert_eq!(tokens[3].text, "&id");
    }
}
