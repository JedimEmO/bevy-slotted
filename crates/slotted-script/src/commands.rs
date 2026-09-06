//! What a script asks the host to do. Contract section 1.2.

use serde::{Deserialize, Serialize};
use slotted_model::{ClickAction, MenuId, Value};

use crate::Stage;

/// Severity of a [`ScriptCommand::Log`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LogLevel {
    /// Noise.
    Trace,
    /// Developer detail.
    Debug,
    /// Normal.
    Info,
    /// Something is off.
    Warn,
    /// Something failed.
    Error,
}

/// Which stacks a static tooltip part applies to. Both empty means every
/// stack.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct TooltipFilter {
    /// Exact item ids.
    #[serde(default)]
    pub items: Vec<String>,
    /// Tag ids, without `#`.
    #[serde(default)]
    pub tags: Vec<String>,
}

/// Which tooltip tier a part shows in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TierFilter {
    /// Both.
    #[default]
    Any,
    /// Only the default tooltip.
    Compact,
    /// Only with shift held.
    Expanded,
}

/// One command table returned by `__slotted_dispatch`. Every `Value` field is
/// the untagged form (section 1.3); the host turns it into a typed def.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ScriptCommand {
    /// Add or replace an `ItemDef`.
    RegisterItem {
        /// `namespace:path`.
        id: String,
        /// The def, with `name` filled in by the prelude.
        #[serde(with = "crate::value::untagged")]
        def: Value,
    },
    /// Add to or replace a `TagDef`.
    RegisterTag {
        /// `namespace:path`.
        id: String,
        /// The def.
        #[serde(with = "crate::value::untagged")]
        def: Value,
    },
    /// Add or replace a `RecipeTypeDef`.
    RegisterRecipeType {
        /// `namespace:path`.
        id: String,
        /// The def.
        #[serde(with = "crate::value::untagged")]
        def: Value,
    },
    /// Add or replace a `RecipeDef`.
    RegisterRecipe {
        /// `namespace:path`.
        id: String,
        /// The def.
        #[serde(with = "crate::value::untagged")]
        def: Value,
    },
    /// Add or replace a screen; `def` is a `slotted_ui::ScreenDef` tree.
    RegisterScreen {
        /// `namespace:path`.
        id: String,
        /// The tree.
        #[serde(with = "crate::value::untagged")]
        def: Value,
    },
    /// Add or replace a widget template; `def` is a `UiNodeDef` with an
    /// `anchor` named `children`.
    RegisterWidget {
        /// `namespace:path`.
        id: String,
        /// The template.
        #[serde(with = "crate::value::untagged")]
        def: Value,
    },
    /// Splice a node under an anchor of a screen kind.
    Inject {
        /// Target screen kind.
        screen: String,
        /// Anchor id.
        anchor: String,
        /// A `UiNodeDef`.
        #[serde(with = "crate::value::untagged")]
        node: Value,
        /// Publish the node as an exclusion zone.
        #[serde(default)]
        exclusion: bool,
    },
    /// At the data stage: a static tooltip part. In reply to `TooltipBuild`:
    /// nodes to append (only `nodes` is read).
    AddTooltipPart {
        /// Optional stable id, for replacement on reload.
        #[serde(default)]
        id: Option<String>,
        /// Which stacks.
        #[serde(default)]
        when: TooltipFilter,
        /// Which tier.
        #[serde(default)]
        tier: TierFilter,
        /// `UiNodeDef`s.
        #[serde(default)]
        nodes: Vec<UntaggedValue>,
    },
    /// `ToolbarAction::Sort` on `inventory`.
    Sort {
        /// The menu.
        menu: MenuId,
        /// Inventory index within the menu.
        inventory: u16,
    },
    /// `ToolbarAction::QuickStack`.
    QuickStack {
        /// The menu.
        menu: MenuId,
        /// Source inventory index.
        from: u16,
        /// Target inventory index.
        to: u16,
    },
    /// Move a stack between two slots as a click plan.
    Move {
        /// The menu.
        menu: MenuId,
        /// Source slot.
        from: u16,
        /// Target slot.
        to: u16,
    },
    /// `ToolbarAction::ToggleFavorite`.
    ToggleFavorite {
        /// The menu.
        menu: MenuId,
        /// The slot.
        slot: u16,
    },
    /// Any `ClickAction`, verbatim. The escape hatch.
    Click {
        /// The menu.
        menu: MenuId,
        /// The action.
        action: ClickAction,
    },
    /// Register a fluid (Phase 6). `def` is a `slotted_ui::FluidDef` payload.
    RegisterFluid {
        /// `namespace:path`.
        id: String,
        /// The def.
        #[serde(with = "crate::value::untagged")]
        def: Value,
    },
    /// Register a HUD layer (Phase 6). `def` is a `slotted_ui::HudLayerPayload`.
    RegisterHudLayer {
        /// Layer id, `mymod:mana`.
        id: String,
        /// The def.
        #[serde(with = "crate::value::untagged")]
        def: Value,
    },
    /// Replace parts of a HUD layer (Phase 6 contract 2.1): `value` is a map
    /// with optional `tree`, `anchor`, `offset`, `scale`, `visible`. An
    /// unknown layer is created on top.
    SetHud {
        /// Layer id.
        layer: String,
        /// Payload.
        #[serde(with = "crate::value::untagged")]
        value: Value,
    },
    /// Write one value into a node of a HUD layer by `test_id` (Phase 6).
    /// `Str` sets text, `Bool` visibility, a number or `{value, max}` a fill.
    HudUpdate {
        /// Layer id.
        layer: String,
        /// The node's `test_id`.
        path: String,
        /// What to write.
        #[serde(with = "crate::value::untagged")]
        value: Value,
    },
    /// Test stage reply to `TestList`.
    TestList {
        /// Registered test names, in order.
        names: Vec<String>,
    },
    /// Test stage: the body yielded an action for the harness.
    TestStep {
        /// The action.
        op: crate::testing::TestOp,
    },
    /// Test stage: the body finished.
    TestDone {
        /// Test name.
        name: String,
        /// Whether every expectation held.
        passed: bool,
        /// The failure message.
        #[serde(default)]
        message: Option<String>,
    },
    /// A console line.
    Log {
        /// Severity.
        level: LogLevel,
        /// Text.
        message: String,
    },
    /// The prelude flagged a deprecated call.
    Deprecated {
        /// Which function.
        call: String,
        /// API version that deprecated it.
        since: u32,
        /// What to use instead.
        hint: String,
    },
    /// Emitted by the prelude at `control_start`: which events the script
    /// handles.
    Subscribe {
        /// Event names.
        events: Vec<String>,
    },
}

/// A [`Value`] inside a `Vec`, where `#[serde(with)]` cannot reach.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct UntaggedValue(#[serde(with = "crate::value::untagged")] pub Value);

impl ScriptCommand {
    /// The `type` string.
    pub const fn name(&self) -> &'static str {
        match self {
            Self::RegisterItem { .. } => "register_item",
            Self::RegisterTag { .. } => "register_tag",
            Self::RegisterRecipeType { .. } => "register_recipe_type",
            Self::RegisterRecipe { .. } => "register_recipe",
            Self::RegisterScreen { .. } => "register_screen",
            Self::RegisterWidget { .. } => "register_widget",
            Self::Inject { .. } => "inject",
            Self::AddTooltipPart { .. } => "add_tooltip_part",
            Self::Sort { .. } => "sort",
            Self::QuickStack { .. } => "quick_stack",
            Self::Move { .. } => "move",
            Self::ToggleFavorite { .. } => "toggle_favorite",
            Self::Click { .. } => "click",
            Self::RegisterFluid { .. } => "register_fluid",
            Self::RegisterHudLayer { .. } => "register_hud_layer",
            Self::SetHud { .. } => "set_hud",
            Self::HudUpdate { .. } => "hud_update",
            Self::TestList { .. } => "test_list",
            Self::TestStep { .. } => "test_step",
            Self::TestDone { .. } => "test_done",
            Self::Log { .. } => "log",
            Self::Deprecated { .. } => "deprecated",
            Self::Subscribe { .. } => "subscribe",
        }
    }

    /// The stages this command is legal in. `None` means any stage.
    pub const fn allowed_stage(&self) -> Option<Stage> {
        match self {
            Self::RegisterItem { .. }
            | Self::RegisterTag { .. }
            | Self::RegisterRecipeType { .. }
            | Self::RegisterRecipe { .. }
            | Self::RegisterScreen { .. }
            | Self::RegisterWidget { .. }
            | Self::RegisterFluid { .. }
            | Self::RegisterHudLayer { .. }
            | Self::Inject { .. } => Some(Stage::Data),
            Self::Sort { .. }
            | Self::QuickStack { .. }
            | Self::Move { .. }
            | Self::ToggleFavorite { .. }
            | Self::Click { .. }
            | Self::SetHud { .. }
            | Self::HudUpdate { .. }
            | Self::Subscribe { .. } => Some(Stage::Control),
            Self::TestList { .. } | Self::TestStep { .. } | Self::TestDone { .. } => {
                Some(Stage::Test)
            }
            Self::AddTooltipPart { .. } | Self::Log { .. } | Self::Deprecated { .. } => None,
        }
    }
}
