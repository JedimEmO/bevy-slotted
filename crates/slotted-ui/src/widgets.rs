//! Built-in widgets. Each is a [`Widget`] registered under a `slotted:` kind
//! by the plugin. The mapping from node to components is in the contract.

use bevy::prelude::*;
use slotted_registry::Value;

use crate::def::UiNodeDef;
use crate::screen::{SpawnCtx, Widget, WidgetRegistry};
use crate::semantic::{SemanticRole, WidgetNode};

/// The built-in kinds.
pub mod kinds {
    use crate::def::WidgetKind;
    use slotted_model::Namespaced;

    fn ns(s: &'static str) -> Namespaced {
        Namespaced::parse(s).expect("built-in widget kinds are well formed")
    }

    /// A themed container.
    pub fn panel() -> WidgetKind {
        WidgetKind(ns("slotted:panel"))
    }
    /// A text label.
    pub fn text() -> WidgetKind {
        WidgetKind(ns("slotted:text"))
    }
    /// A single slot.
    pub fn slot() -> WidgetKind {
        WidgetKind(ns("slotted:slot"))
    }
    /// A slot grid.
    pub fn slot_grid() -> WidgetKind {
        WidgetKind(ns("slotted:slot_grid"))
    }
    /// A button.
    pub fn button() -> WidgetKind {
        WidgetKind(ns("slotted:button"))
    }
    /// The action rail: sort, quick stack, deposit all, loot all.
    pub fn action_rail() -> WidgetKind {
        WidgetKind(ns("slotted:action_rail"))
    }
    /// The hotbar row: a 1x9 slot grid with hotbar tags.
    pub fn hotbar() -> WidgetKind {
        WidgetKind(ns("slotted:hotbar"))
    }
    /// A tooltip body. Used internally by the tooltip system.
    pub fn tooltip() -> WidgetKind {
        WidgetKind(ns("slotted:tooltip"))
    }

    /// Every built-in kind registered in Phase 2.
    pub fn all() -> [WidgetKind; 8] {
        [
            panel(),
            text(),
            slot(),
            slot_grid(),
            button(),
            action_rail(),
            hotbar(),
            tooltip(),
        ]
    }
}

macro_rules! stub_widget {
    ($(#[$doc:meta])* $name:ident, $role:expr, $kind:expr) => {
        $(#[$doc])*
        #[derive(Debug, Default, Clone, Copy)]
        pub struct $name;

        impl Widget for $name {
            // PHASE2-IMPL: agent B.
            fn spawn(&self, ctx: &mut SpawnCtx<'_>, _params: &Value, children: &[UiNodeDef]) -> Entity {
                let entity = ctx
                    .world
                    .spawn((Node::default(), $role, WidgetNode($kind), ChildOf(ctx.parent)))
                    .id();
                let parent = std::mem::replace(&mut ctx.parent, entity);
                for child in children {
                    ctx.spawn_child(child);
                }
                ctx.parent = parent;
                entity
            }
        }
    };
}

stub_widget!(
    /// `Panel`: `Node` from `Layout`, `Themed(role)`.
    PanelWidget,
    SemanticRole::Panel,
    kinds::panel()
);
stub_widget!(
    /// `Text`: `Text`, `Themed(style.role())`, `SemanticLabel(text)`.
    TextWidget,
    SemanticRole::Text,
    kinds::text()
);
stub_widget!(
    /// `Slot`: see the contract's slot bundle.
    SlotWidget,
    SemanticRole::Slot,
    kinds::slot()
);
stub_widget!(
    /// `SlotGrid`: a `Display::Grid` node of `SlotWidget`s.
    SlotGridWidget,
    SemanticRole::Grid,
    kinds::slot_grid()
);
stub_widget!(
    /// `Button`: `bevy_ui_widgets::Button`, `Themed(button)`, `Activate` observer.
    ButtonWidget,
    SemanticRole::Button,
    kinds::button()
);
stub_widget!(
    /// `slotted:action_rail`: a column of buttons that trigger `MenuAction`
    /// with `ClickAction::Toolbar`. Params: `(actions: ["sort", ...])`.
    ActionRailWidget,
    SemanticRole::Rail,
    kinds::action_rail()
);
stub_widget!(
    /// `slotted:hotbar`: a 1x9 `SlotGridWidget`. Params: `(first: u16)`.
    HotbarWidget,
    SemanticRole::Hotbar,
    kinds::hotbar()
);
stub_widget!(
    /// `slotted:tooltip`: `Themed(tooltip)` panel with a `tooltip.frame` child.
    TooltipWidget,
    SemanticRole::Tooltip,
    kinds::tooltip()
);

/// Registers every built-in under its kind.
pub fn register_builtins(registry: &mut WidgetRegistry) {
    registry.register(kinds::panel(), PanelWidget);
    registry.register(kinds::text(), TextWidget);
    registry.register(kinds::slot(), SlotWidget);
    registry.register(kinds::slot_grid(), SlotGridWidget);
    registry.register(kinds::button(), ButtonWidget);
    registry.register(kinds::action_rail(), ActionRailWidget);
    registry.register(kinds::hotbar(), HotbarWidget);
    registry.register(kinds::tooltip(), TooltipWidget);
}

/// The toolbar action a rail button fires. On each rail button entity.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct RailAction(pub slotted_model::ToolbarAction);

/// `SlottedUiSet::Render`: swap slot roles with hover, focus and carried
/// state (`slot` / `slot.hover` / `slot.focus` / `slot.carried`).
// PHASE2-IMPL: agent B.
pub fn slot_state_roles() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_kinds_are_registered() {
        let mut reg = WidgetRegistry::default();
        register_builtins(&mut reg);
        for k in kinds::all() {
            assert!(reg.get(&k).is_some(), "{k:?} missing");
        }
        assert_eq!(
            kinds::hotbar(),
            crate::def::WidgetKind::new("slotted:hotbar")
        );
    }
}
