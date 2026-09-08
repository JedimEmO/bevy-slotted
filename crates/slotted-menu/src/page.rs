//! Text pages (menus M2 contract 3.5).

use bevy::prelude::*;
use slotted_ui::{LocArgs, LocKey};

/// What a page shows.
#[derive(Debug, Clone, PartialEq)]
pub struct PageSpec {
    /// The heading.
    pub title: LocKey,
    /// The body, rich text, with `args`.
    pub body: LocKey,
    /// Arguments.
    pub args: LocArgs,
}

impl PageSpec {
    /// A page.
    pub fn new(title: impl Into<String>, body: impl Into<String>) -> Self {
        Self {
            title: LocKey(title.into()),
            body: LocKey(body.into()),
            args: LocArgs::new(),
        }
    }
}

/// Pushes a page.
pub fn open_page(commands: &mut Commands, spec: PageSpec) {
    // M2-IMPL: B
    let _ = (commands, spec);
}
