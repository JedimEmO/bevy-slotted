//! Typed, serialisable bookmarks. Contract section 5. Package A.

use bevy::prelude::Resource;
use serde::{Deserialize, Serialize};

use crate::category::RecipeRef;
use crate::ingredient::Ingredient;

/// One bookmark. Actions (REI-style favourites) arrive in Phase 6.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Bookmark {
    /// An ingredient.
    Item(Ingredient),
    /// A whole recipe.
    Recipe(RecipeRef),
}

/// The player's bookmarks, in strip order.
#[derive(Resource, Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Bookmarks {
    /// Bookmarks in display order.
    pub entries: Vec<Bookmark>,
}

impl Bookmarks {
    /// Adds the bookmark if absent, removes it if present. Returns whether it
    /// is present afterwards.
    pub fn toggle(&mut self, bookmark: Bookmark) -> bool {
        if let Some(i) = self.entries.iter().position(|b| *b == bookmark) {
            self.entries.remove(i);
            false
        } else {
            self.entries.push(bookmark);
            true
        }
    }

    /// Whether the bookmark is present.
    pub fn contains(&self, bookmark: &Bookmark) -> bool {
        self.entries.contains(bookmark)
    }

    /// Moves the bookmark at `from` to `to` (drag to rearrange).
    pub fn move_to(&mut self, from: usize, to: usize) {
        if from < self.entries.len() && to < self.entries.len() {
            let b = self.entries.remove(from);
            self.entries.insert(to, b);
        }
    }

    /// Number of bookmarks.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether there are none.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}
