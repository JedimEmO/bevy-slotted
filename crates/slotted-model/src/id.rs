//! Identifiers.
//!
//! Two kinds live here and they are not interchangeable.
//!
//! [`Namespaced`] is the *authoring* id: the `copper_chest:sorter` string a
//! mod writes in RON or Lua. It is validated once at construction, cheap to
//! clone and stable across runs, so it is what crosses the script boundary and
//! what goes into save files.
//!
//! [`ItemId`] and [`ComponentId`] are the *runtime* ids: dense `u32` handles
//! assigned when a registry freezes. They are `Copy`, compare in one
//! instruction and index straight into a `Vec`, which is what makes the click
//! state machine and the dirty-mask diffing cheap. They are only meaningful
//! next to the registry that minted them, so they never appear in a save file
//! without the registry's string map beside them.

use core::fmt;
use core::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Why a string is not a valid [`Namespaced`] id.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NamespacedError {
    /// The string had no `:` separating namespace from path.
    #[error("missing ':' separator in namespaced id `{0}`")]
    MissingSeparator(String),
    /// The string had more than one `:`.
    #[error("more than one ':' in namespaced id `{0}`")]
    ExtraSeparator(String),
    /// The namespace part was empty.
    #[error("empty namespace in namespaced id `{0}`")]
    EmptyNamespace(String),
    /// The path part was empty.
    #[error("empty path in namespaced id `{0}`")]
    EmptyPath(String),
    /// The namespace contained a character outside `[a-z0-9_.-]`.
    #[error("invalid character {ch:?} in namespace of `{id}`")]
    InvalidNamespaceChar {
        /// The offending character.
        ch: char,
        /// The id it appeared in.
        id: String,
    },
    /// The path contained a character outside `[a-z0-9_./-]`.
    #[error("invalid character {ch:?} in path of `{id}`")]
    InvalidPathChar {
        /// The offending character.
        ch: char,
        /// The id it appeared in.
        id: String,
    },
}

const fn is_namespace_char(ch: char) -> bool {
    matches!(ch, 'a'..='z' | '0'..='9' | '_' | '.' | '-')
}

const fn is_path_char(ch: char) -> bool {
    is_namespace_char(ch) || ch == '/'
}

/// A validated `namespace:path` identifier, such as `slotted:chest` or
/// `copper_chest:screens/sorter`.
///
/// The namespace accepts `[a-z0-9_.-]` and the path additionally accepts `/`,
/// matching the character set mods already know from Minecraft. Both halves
/// must be non-empty. Validation happens once, at construction, so every
/// `Namespaced` in the program is known-good.
///
/// The whole id is stored as one `String` with the separator index alongside
/// it, so [`Display`](fmt::Display) and serialisation are allocation-free and
/// [`namespace`](Self::namespace) and [`path`](Self::path) are slices into it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Namespaced {
    full: String,
    /// Byte index of the `:`. All characters are ASCII, so this is also the
    /// character index.
    colon: usize,
}

impl Namespaced {
    /// Builds an id from its two halves, validating both.
    ///
    /// ```
    /// # use slotted_model::Namespaced;
    /// let id = Namespaced::new("copper_chest", "screens/sorter").unwrap();
    /// assert_eq!(id.namespace(), "copper_chest");
    /// assert_eq!(id.path(), "screens/sorter");
    /// assert_eq!(id.to_string(), "copper_chest:screens/sorter");
    /// ```
    pub fn new(namespace: &str, path: &str) -> Result<Self, NamespacedError> {
        let full = format!("{namespace}:{path}");
        Self::validate(namespace, path, &full)?;
        Ok(Self {
            colon: namespace.len(),
            full,
        })
    }

    /// Parses a full `namespace:path` string.
    ///
    /// ```
    /// # use slotted_model::Namespaced;
    /// assert!(Namespaced::parse("slotted:chest").is_ok());
    /// assert!(Namespaced::parse("Slotted:chest").is_err());
    /// assert!(Namespaced::parse("chest").is_err());
    /// ```
    pub fn parse(s: &str) -> Result<Self, NamespacedError> {
        let (namespace, path) = s
            .split_once(':')
            .ok_or_else(|| NamespacedError::MissingSeparator(s.to_owned()))?;
        if path.contains(':') {
            return Err(NamespacedError::ExtraSeparator(s.to_owned()));
        }
        Self::validate(namespace, path, s)?;
        Ok(Self {
            colon: namespace.len(),
            full: s.to_owned(),
        })
    }

    fn validate(namespace: &str, path: &str, full: &str) -> Result<(), NamespacedError> {
        if namespace.is_empty() {
            return Err(NamespacedError::EmptyNamespace(full.to_owned()));
        }
        if path.is_empty() {
            return Err(NamespacedError::EmptyPath(full.to_owned()));
        }
        if let Some(ch) = namespace.chars().find(|c| !is_namespace_char(*c)) {
            return Err(NamespacedError::InvalidNamespaceChar {
                ch,
                id: full.to_owned(),
            });
        }
        if let Some(ch) = path.chars().find(|c| !is_path_char(*c)) {
            return Err(NamespacedError::InvalidPathChar {
                ch,
                id: full.to_owned(),
            });
        }
        Ok(())
    }

    /// The namespace half, before the `:`.
    pub fn namespace(&self) -> &str {
        &self.full[..self.colon]
    }

    /// The path half, after the `:`.
    pub fn path(&self) -> &str {
        &self.full[self.colon + 1..]
    }

    /// The whole `namespace:path` string.
    pub fn as_str(&self) -> &str {
        &self.full
    }
}

impl fmt::Display for Namespaced {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.full)
    }
}

impl FromStr for Namespaced {
    type Err = NamespacedError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

impl AsRef<str> for Namespaced {
    fn as_ref(&self) -> &str {
        &self.full
    }
}

impl TryFrom<String> for Namespaced {
    type Error = NamespacedError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(&value)
    }
}

impl Serialize for Namespaced {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.full)
    }
}

impl<'de> Deserialize<'de> for Namespaced {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        Self::parse(&raw).map_err(serde::de::Error::custom)
    }
}

/// A runtime item id: an index into the frozen item registry.
///
/// Assigned at freeze time and only meaningful next to the registry that
/// assigned it. Persist a [`Namespaced`] instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ItemId(pub u32);

/// A runtime id for a stack component kind, such as `slotted:damage`.
///
/// Ordered so that a `ComponentPatch` keyed on it has a canonical layout and
/// two patches can be compared by memory order rather than by content.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ComponentId(pub u32);

// `unwrap_used` is a production lint. In a test an unwrap is the assertion:
// the panic is the failure report, and a `?` or an `expect` message would only
// hide which line broke.
#[allow(clippy::unwrap_used)]
#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::{ComponentId, ItemId, Namespaced, NamespacedError};

    #[test]
    fn parses_a_simple_id() {
        let id = Namespaced::parse("slotted:chest").unwrap();
        assert_eq!(id.namespace(), "slotted");
        assert_eq!(id.path(), "chest");
        assert_eq!(id.as_str(), "slotted:chest");
    }

    #[test]
    fn path_may_contain_slashes_but_namespace_may_not() {
        assert_eq!(
            Namespaced::parse("copper_chest:screens/sorter")
                .unwrap()
                .path(),
            "screens/sorter"
        );
        assert!(matches!(
            Namespaced::parse("copper/chest:sorter"),
            Err(NamespacedError::InvalidNamespaceChar { ch: '/', .. })
        ));
    }

    #[test]
    fn accepts_the_full_character_set() {
        let id = Namespaced::parse("a-b_c.9:x-y_z.0/w").unwrap();
        assert_eq!(id.namespace(), "a-b_c.9");
        assert_eq!(id.path(), "x-y_z.0/w");
    }

    #[test]
    fn rejects_uppercase_and_spaces() {
        assert!(matches!(
            Namespaced::parse("Slotted:chest"),
            Err(NamespacedError::InvalidNamespaceChar { ch: 'S', .. })
        ));
        assert!(matches!(
            Namespaced::parse("slotted:big chest"),
            Err(NamespacedError::InvalidPathChar { ch: ' ', .. })
        ));
    }

    #[test]
    fn rejects_malformed_separators() {
        assert!(matches!(
            Namespaced::parse("chest"),
            Err(NamespacedError::MissingSeparator(_))
        ));
        assert!(matches!(
            Namespaced::parse("a:b:c"),
            Err(NamespacedError::ExtraSeparator(_))
        ));
        assert!(matches!(
            Namespaced::parse(":chest"),
            Err(NamespacedError::EmptyNamespace(_))
        ));
        assert!(matches!(
            Namespaced::parse("slotted:"),
            Err(NamespacedError::EmptyPath(_))
        ));
    }

    #[test]
    fn new_validates_the_same_way_as_parse() {
        assert_eq!(
            Namespaced::new("slotted", "chest").unwrap(),
            Namespaced::parse("slotted:chest").unwrap()
        );
        assert!(Namespaced::new("slotted", "big chest").is_err());
        assert!(Namespaced::new("", "chest").is_err());
    }

    #[test]
    fn from_str_and_display_round_trip() {
        let original = "copper_chest:screens/sorter";
        let id: Namespaced = original.parse().unwrap();
        assert_eq!(id.to_string(), original);
    }

    #[test]
    fn serialises_as_a_plain_string() {
        let id = Namespaced::parse("slotted:chest").unwrap();
        let encoded = ron::to_string(&id).unwrap();
        assert_eq!(encoded, "\"slotted:chest\"");
        let decoded: Namespaced = ron::from_str(&encoded).unwrap();
        assert_eq!(decoded, id);
    }

    #[test]
    fn deserialising_an_invalid_id_fails() {
        assert!(ron::from_str::<Namespaced>("\"NOPE\"").is_err());
    }

    proptest::proptest! {
        /// Anything built from valid halves parses back to the same value, and
        /// the two halves survive the round trip through the flat string.
        #[test]
        fn round_trips_through_string(
            namespace in "[a-z0-9_.-]{1,16}",
            path in "[a-z0-9_./-]{1,32}",
        ) {
            let id = Namespaced::new(&namespace, &path).unwrap();
            let reparsed: Namespaced = id.to_string().parse().unwrap();
            proptest::prop_assert_eq!(&reparsed, &id);
            proptest::prop_assert_eq!(reparsed.namespace(), namespace);
            proptest::prop_assert_eq!(reparsed.path(), path);
        }
    }

    #[test]
    fn runtime_ids_are_transparent() {
        assert_eq!(ron::to_string(&ItemId(7)).unwrap(), "7");
        assert_eq!(ron::from_str::<ItemId>("7").unwrap(), ItemId(7));
        assert_eq!(ron::to_string(&ComponentId(3)).unwrap(), "3");
        assert_eq!(ron::from_str::<ComponentId>("3").unwrap(), ComponentId(3));
    }
}
