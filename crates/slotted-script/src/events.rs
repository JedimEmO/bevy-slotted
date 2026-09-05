//! What the host tells a script. Contract section 1.2.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use slotted_model::{MenuId, Value};

/// A stack as a script sees it: names, not interned ids.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StackInfo {
    /// `namespace:path` of the item.
    pub item: String,
    /// How many.
    pub count: u32,
    /// The stack's component patch, as an untagged value map.
    #[serde(default = "empty_map", with = "crate::value::untagged")]
    pub components: Value,
}

fn empty_map() -> Value {
    Value::Map(BTreeMap::new())
}

/// Pointer button, lowercase in Lua.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Button {
    /// Primary.
    Left,
    /// Secondary.
    Right,
    /// Wheel.
    Middle,
}

/// Modifier keys held during a click.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct Modifiers {
    /// Shift.
    #[serde(default)]
    pub shift: bool,
    /// Control.
    #[serde(default)]
    pub ctrl: bool,
    /// Alt.
    #[serde(default)]
    pub alt: bool,
}

/// Tooltip detail level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tier {
    /// The default tooltip.
    Compact,
    /// Shift held.
    Expanded,
}

/// Which direction a recipe lookup goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LookupMode {
    /// What makes the item.
    Recipes,
    /// What the item makes.
    Uses,
}

/// One event delivered to `__slotted_dispatch`. The `type` field is the
/// `snake_case` variant name, which is also the name a script passes to
/// `slotted.on`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ScriptEvent {
    /// Once, right after a data chunk executed; the reply carries the
    /// registrations.
    DataStage {
        /// [`crate::API_VERSION`].
        api_version: u32,
    },
    /// Once, right after a control chunk executed; the reply starts with
    /// `Subscribe`.
    ControlStart {
        /// [`crate::API_VERSION`].
        api_version: u32,
        /// Every loaded mod, in load order.
        mods: Vec<String>,
    },
    /// A completed pointer click on a slot.
    SlotClick {
        /// The menu the slot belongs to.
        menu: MenuId,
        /// Screen kind, `namespace:path`.
        screen: String,
        /// Menu-wide slot index.
        slot: u16,
        /// Which button.
        button: Button,
        /// Held modifiers.
        modifiers: Modifiers,
        /// What was in the slot when clicked.
        stack: Option<StackInfo>,
    },
    /// A button or custom widget was activated.
    WidgetActivate {
        /// The menu, if the screen has one.
        menu: Option<MenuId>,
        /// Screen kind.
        screen: String,
        /// Widget kind, `namespace:path`.
        widget: String,
        /// The node's tags.
        tags: BTreeMap<String, String>,
    },
    /// A tooltip is being composed for `stack`.
    TooltipBuild {
        /// The stack under the pointer.
        stack: StackInfo,
        /// Requested tier.
        tier: Tier,
    },
    /// The browser opened a recipe page.
    RecipeLookup {
        /// The focused item.
        item: String,
        /// Recipes or uses.
        mode: LookupMode,
    },
    /// A screen's tree exists.
    ScreenOpened {
        /// The menu, if any.
        menu: Option<MenuId>,
        /// Screen kind.
        screen: String,
    },
    /// A screen is about to be despawned.
    ScreenClosed {
        /// The menu, if any.
        menu: Option<MenuId>,
        /// Screen kind.
        screen: String,
    },
    /// Periodic HUD tick, only when the host enables it.
    HudTick {
        /// Virtual time since start.
        elapsed_ms: u64,
    },
    /// The browser search text changed.
    SearchChanged {
        /// The new text.
        text: String,
    },
}

impl ScriptEvent {
    /// The `type` string, which is also the `slotted.on` name.
    pub const fn name(&self) -> &'static str {
        match self {
            Self::DataStage { .. } => "data_stage",
            Self::ControlStart { .. } => "control_start",
            Self::SlotClick { .. } => "slot_click",
            Self::WidgetActivate { .. } => "widget_activate",
            Self::TooltipBuild { .. } => "tooltip_build",
            Self::RecipeLookup { .. } => "recipe_lookup",
            Self::ScreenOpened { .. } => "screen_opened",
            Self::ScreenClosed { .. } => "screen_closed",
            Self::HudTick { .. } => "hud_tick",
            Self::SearchChanged { .. } => "search_changed",
        }
    }
}
