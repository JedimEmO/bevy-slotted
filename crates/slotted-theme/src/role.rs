//! Semantic roles: the keys of a theme's `roles` map.

use std::borrow::Cow;
use std::fmt;

use serde::{Deserialize, Serialize};

/// A semantic role a node plays, such as `slot.hover`.
///
/// Dotted names are a convention, not a hierarchy the theme resolves: a theme
/// that lacks `slot.hover` falls back to `slot` only because
/// [`Theme::material`](crate::Theme::material) strips one segment at a time.
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Role(pub Cow<'static, str>);

impl Role {
    /// A role from a static name. Use the constants in [`roles`] where one exists.
    pub const fn new_static(name: &'static str) -> Self {
        Self(Cow::Borrowed(name))
    }

    /// A role from a runtime name (a mod's custom role).
    pub fn new(name: impl Into<String>) -> Self {
        Self(Cow::Owned(name.into()))
    }

    /// The name.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The role with the last dotted segment removed, or `None` for a root
    /// role. `slot.hover` -> `slot`.
    pub fn parent(&self) -> Option<Self> {
        let s = self.as_str();
        s.rfind('.').map(|i| Self::new(&s[..i]))
    }
}

impl fmt::Debug for Role {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Role({})", self.0)
    }
}

impl fmt::Display for Role {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<&'static str> for Role {
    fn from(s: &'static str) -> Self {
        Self::new_static(s)
    }
}

/// Well-known roles. Every shipped theme defines all of these; a theme that
/// omits one is reported by [`Theme::missing_roles`](crate::Theme::missing_roles).
pub mod roles {
    use super::Role;

    /// A screen panel.
    pub const PANEL: Role = Role::new_static("panel");
    /// Panel title text.
    pub const PANEL_TITLE: Role = Role::new_static("panel.title");
    /// A slot at rest.
    pub const SLOT: Role = Role::new_static("slot");
    /// A hovered slot.
    pub const SLOT_HOVER: Role = Role::new_static("slot.hover");
    /// A keyboard-focused slot.
    pub const SLOT_FOCUS: Role = Role::new_static("slot.focus");
    /// A slot while a stack is carried over it (drop target).
    pub const SLOT_CARRIED: Role = Role::new_static("slot.carried");
    /// Tooltip body.
    pub const TOOLTIP: Role = Role::new_static("tooltip");
    /// Tooltip frame decorator.
    pub const TOOLTIP_FRAME: Role = Role::new_static("tooltip.frame");
    /// A button at rest.
    pub const BUTTON: Role = Role::new_static("button");
    /// The primary button of a screen.
    pub const BUTTON_PRIMARY: Role = Role::new_static("button.primary");
    /// A hovered button.
    pub const BUTTON_HOVER: Role = Role::new_static("button.hover");
    /// Body text.
    pub const TEXT: Role = Role::new_static("text");
    /// De-emphasised text.
    pub const TEXT_MUTED: Role = Role::new_static("text.muted");
    /// The stack count in a slot corner.
    pub const COUNT: Role = Role::new_static("count");
    /// The action rail strip beside a grid.
    pub const RAIL: Role = Role::new_static("rail");
    /// The stack following the pointer. Not in [`ALL`]: a theme that omits it
    /// falls back to nothing, and the carried node is drawn by its item view.
    pub const CARRIED: Role = Role::new_static("carried");

    /// Every well-known role, for completeness checks.
    pub const ALL: [Role; 15] = [
        PANEL,
        PANEL_TITLE,
        SLOT,
        SLOT_HOVER,
        SLOT_FOCUS,
        SLOT_CARRIED,
        TOOLTIP,
        TOOLTIP_FRAME,
        BUTTON,
        BUTTON_PRIMARY,
        BUTTON_HOVER,
        TEXT,
        TEXT_MUTED,
        COUNT,
        RAIL,
    ];
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parent_strips_one_segment() {
        assert_eq!(roles::SLOT_HOVER.parent(), Some(roles::SLOT));
        assert_eq!(roles::SLOT.parent(), None);
    }
}
