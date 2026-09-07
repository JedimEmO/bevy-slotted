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

    /// A fluid tank body (Phase 6).
    pub const TANK: Role = Role::new_static("tank");
    /// The tank's fill child; used when no fluid is resolved.
    pub const TANK_FILL: Role = Role::new_static("tank.fill");
    /// A bar body.
    pub const BAR: Role = Role::new_static("bar");
    /// The bar's fill child.
    pub const BAR_FILL: Role = Role::new_static("bar.fill");
    /// The optional `value / max` text on a bar.
    pub const BAR_TEXT: Role = Role::new_static("bar.text");
    /// A progress arrow body.
    pub const PROGRESS: Role = Role::new_static("progress");
    /// A progress arrow's fill; a `Gradient` here is the optional gradient.
    pub const PROGRESS_FILL: Role = Role::new_static("progress.fill");
    /// A closed side tab.
    pub const TAB_SIDE: Role = Role::new_static("tab.side");
    /// An open side tab.
    pub const TAB_SIDE_OPEN: Role = Role::new_static("tab.side.open");
    /// The side tab's header button.
    pub const TAB_SIDE_HEADER: Role = Role::new_static("tab.side.header");
    /// The side tab's content panel.
    pub const TAB_SIDE_CONTENT: Role = Role::new_static("tab.side.content");
    /// The column of side tabs beside a panel.
    pub const TAB_RAIL: Role = Role::new_static("tab.rail");
    /// A cycling icon button at rest.
    pub const ICON_BUTTON: Role = Role::new_static("icon_button");
    /// A hovered icon button.
    pub const ICON_BUTTON_HOVER: Role = Role::new_static("icon_button.hover");
    /// A virtual grid body.
    pub const VIRTUAL_GRID: Role = Role::new_static("virtual_grid");
    /// The virtual grid's scrollbar track.
    pub const VIRTUAL_GRID_SCROLLBAR: Role = Role::new_static("virtual_grid.scrollbar");
    /// The virtual grid's scrollbar thumb.
    pub const VIRTUAL_GRID_THUMB: Role = Role::new_static("virtual_grid.thumb");
    /// A 3D viewport frame.
    pub const VIEWPORT: Role = Role::new_static("viewport");
    /// A HUD layer's panel.
    pub const HUD_PANEL: Role = Role::new_static("hud.panel");
    /// The built-in crosshair.
    pub const HUD_CROSSHAIR: Role = Role::new_static("hud.crosshair");
    /// The outline drawn around a HUD layer in edit mode.
    pub const HUD_EDIT_FRAME: Role = Role::new_static("hud.edit.frame");
    /// The focus ring that follows `InputFocus` off the mouse (menus
    /// contract 2.3). A border and no fill.
    pub const FOCUS_RING: Role = Role::new_static("focus.ring");
    /// The full-window scrim under a modal screen (menus contract 3.2).
    pub const SCRIM: Role = Role::new_static("scrim");

    /// Every well-known role, for completeness checks.
    pub const ALL: [Role; 38] = [
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
        TANK,
        TANK_FILL,
        BAR,
        BAR_FILL,
        BAR_TEXT,
        PROGRESS,
        PROGRESS_FILL,
        TAB_SIDE,
        TAB_SIDE_OPEN,
        TAB_SIDE_HEADER,
        TAB_SIDE_CONTENT,
        TAB_RAIL,
        ICON_BUTTON,
        ICON_BUTTON_HOVER,
        VIRTUAL_GRID,
        VIRTUAL_GRID_SCROLLBAR,
        VIRTUAL_GRID_THUMB,
        VIEWPORT,
        HUD_PANEL,
        HUD_CROSSHAIR,
        HUD_EDIT_FRAME,
        FOCUS_RING,
        SCRIM,
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
