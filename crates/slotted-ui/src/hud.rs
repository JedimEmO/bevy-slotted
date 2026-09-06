//! HUD layers: an ordered, named stack of UI roots anchored to the window.
//! Phase 6 contract section 2.1.
//!
//! Built-in ids mirror what modders expect to insert above, below or replace
//! (`crosshair`, `hotbar`, `health`...). Most built-ins are empty
//! placeholders: they exist so `insert_above("health", ..)` has a target.

use std::borrow::Cow;
use std::collections::{BTreeMap, HashMap};

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::def::UiNodeDef;

/// A layer name: `hotbar`, `mymod:mana`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct HudLayerId(pub Cow<'static, str>);

impl HudLayerId {
    /// A static id.
    pub const fn new_static(s: &'static str) -> Self {
        Self(Cow::Borrowed(s))
    }

    /// A runtime id.
    pub fn new(s: impl Into<String>) -> Self {
        Self(Cow::Owned(s.into()))
    }

    /// The name.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for HudLayerId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// The built-in layer ids and their default order, bottom to top.
pub mod builtin {
    use super::HudLayerId;

    /// The crosshair; hidden while a screen is open.
    pub const CROSSHAIR: HudLayerId = HudLayerId::new_static("crosshair");
    /// The hotbar, a `slotted:hotbar` bound to [`super::HudMenu`].
    pub const HOTBAR: HudLayerId = HudLayerId::new_static("hotbar");
    /// Placeholder.
    pub const HEALTH: HudLayerId = HudLayerId::new_static("health");
    /// Placeholder.
    pub const HUNGER: HudLayerId = HudLayerId::new_static("hunger");
    /// Placeholder.
    pub const AIR: HudLayerId = HudLayerId::new_static("air");
    /// Placeholder.
    pub const EXPERIENCE: HudLayerId = HudLayerId::new_static("experience");
    /// Placeholder.
    pub const BOSS_BAR: HudLayerId = HudLayerId::new_static("boss_bar");
    /// Placeholder.
    pub const CHAT: HudLayerId = HudLayerId::new_static("chat");
    /// Placeholder.
    pub const DEBUG: HudLayerId = HudLayerId::new_static("debug");

    /// Default order, bottom to top.
    pub const ORDER: [HudLayerId; 9] = [
        CROSSHAIR, HOTBAR, HEALTH, HUNGER, AIR, EXPERIENCE, BOSS_BAR, CHAT, DEBUG,
    ];
}

/// Where on the window a layer hangs. Lowercase strings in data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NineAnchor {
    /// Top left corner.
    TopLeft,
    /// Top centre.
    Top,
    /// Top right corner.
    TopRight,
    /// Left centre.
    Left,
    /// Window centre.
    #[default]
    Center,
    /// Right centre.
    Right,
    /// Bottom left corner.
    BottomLeft,
    /// Bottom centre.
    Bottom,
    /// Bottom right corner.
    BottomRight,
}

/// Anchor, offset and scale of one layer. Also a component on the anchor
/// wrapper, where the editor rewrites it.
#[derive(Component, Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct HudAnchor {
    /// Which of the nine points.
    #[serde(default)]
    pub anchor: NineAnchor,
    /// Logical-pixel offset from the anchor point.
    #[serde(default)]
    pub offset: Vec2,
    /// `UiTransform` scale of the layer's content.
    #[serde(default = "one")]
    pub scale: f32,
}

fn one() -> f32 {
    1.0
}

impl Default for HudAnchor {
    fn default() -> Self {
        Self {
            anchor: NineAnchor::Center,
            offset: Vec2::ZERO,
            scale: 1.0,
        }
    }
}

impl HudAnchor {
    /// The absolute `Node` placing the anchor wrapper for a window of
    /// `window` logical pixels.
    pub fn node(&self, window: Vec2) -> Node {
        // PHASE6-IMPL: B. left/right/top/bottom from anchor + offset; the
        // wrapper is `PositionType::Absolute` and sized to its content.
        let _ = window;
        Node {
            position_type: PositionType::Absolute,
            ..default()
        }
    }
}

/// One layer as data.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HudLayerDef {
    /// Its id.
    pub id: HudLayerId,
    /// Placement.
    #[serde(default)]
    pub anchor: HudAnchor,
    /// The tree spawned under the anchor wrapper.
    pub tree: UiNodeDef,
    /// Shown at all.
    #[serde(default = "yes")]
    pub visible: bool,
    /// Hidden while any `ScreenRoot` exists (the crosshair).
    #[serde(default)]
    pub hide_with_screen: bool,
}

fn yes() -> bool {
    true
}

/// Where a registry or script layer wants to sit.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HudPlacement {
    /// On top of everything registered so far.
    #[default]
    Top,
    /// Directly above the named layer.
    Above(HudLayerId),
    /// Directly below the named layer.
    Below(HudLayerId),
    /// In the named layer's slot, replacing it.
    Replace(HudLayerId),
}

/// Why a placement failed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum HudError {
    /// The reference layer does not exist.
    #[error("no HUD layer `{0}`")]
    UnknownLayer(HudLayerId),
}

/// The ordered layer stack. Changing it re-syncs the roots next frame.
#[derive(Resource, Default, Debug, Clone)]
pub struct HudLayers {
    order: Vec<HudLayerId>,
    defs: HashMap<HudLayerId, HudLayerDef>,
    roots: HashMap<HudLayerId, Entity>,
}

impl HudLayers {
    /// Appends on top, or replaces in place when the id exists.
    pub fn register(&mut self, def: HudLayerDef) {
        // PHASE6-IMPL: B.
        let _ = def;
    }

    /// Inserts directly above `id`.
    pub fn insert_above(&mut self, id: &HudLayerId, def: HudLayerDef) -> Result<(), HudError> {
        // PHASE6-IMPL: B.
        let _ = def;
        Err(HudError::UnknownLayer(id.clone()))
    }

    /// Inserts directly below `id`.
    pub fn insert_below(&mut self, id: &HudLayerId, def: HudLayerDef) -> Result<(), HudError> {
        // PHASE6-IMPL: B.
        let _ = def;
        Err(HudError::UnknownLayer(id.clone()))
    }

    /// Replaces `id`'s def, keeping its position; the new def keeps `id`.
    pub fn replace(&mut self, id: &HudLayerId, def: HudLayerDef) -> Result<(), HudError> {
        // PHASE6-IMPL: B.
        let _ = def;
        Err(HudError::UnknownLayer(id.clone()))
    }

    /// Removes a layer.
    pub fn remove(&mut self, id: &HudLayerId) -> Option<HudLayerDef> {
        // PHASE6-IMPL: B.
        let _ = id;
        None
    }

    /// Applies a placement.
    pub fn place(&mut self, placement: &HudPlacement, def: HudLayerDef) -> Result<(), HudError> {
        match placement {
            HudPlacement::Top => {
                self.register(def);
                Ok(())
            }
            HudPlacement::Above(id) => self.insert_above(id, def),
            HudPlacement::Below(id) => self.insert_below(id, def),
            HudPlacement::Replace(id) => self.replace(id, def),
        }
    }

    /// Shows or hides a layer.
    pub fn set_visible(&mut self, id: &HudLayerId, visible: bool) {
        if let Some(def) = self.defs.get_mut(id) {
            def.visible = visible;
        }
    }

    /// Bottom to top.
    pub fn order(&self) -> &[HudLayerId] {
        &self.order
    }

    /// A layer's def.
    pub fn get(&self, id: &HudLayerId) -> Option<&HudLayerDef> {
        self.defs.get(id)
    }

    /// A layer's spawned root, once synced.
    pub fn root(&self, id: &HudLayerId) -> Option<Entity> {
        self.roots.get(id).copied()
    }

    /// Records a spawned root. Called by [`sync_hud_layers`].
    pub fn set_root(&mut self, id: HudLayerId, root: Option<Entity>) {
        match root {
            Some(e) => {
                self.roots.insert(id, e);
            }
            None => {
                self.roots.remove(&id);
            }
        }
    }

    /// Adds every `hud_layers` registry payload (`HudLayerPayload` shape).
    pub fn load_from_registry(&mut self, registries: &slotted_registry::FrozenRegistries) {
        // PHASE6-IMPL: B. Deserialise `HudLayerPayload` per entry, `place`.
        let _ = registries;
    }
}

/// The registry / script shape of a layer: a def plus a placement.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HudLayerPayload {
    /// Placement.
    #[serde(default)]
    pub anchor: HudAnchor,
    /// Insert above this layer.
    #[serde(default)]
    pub above: Option<HudLayerId>,
    /// Insert below this layer.
    #[serde(default)]
    pub below: Option<HudLayerId>,
    /// Replace this layer.
    #[serde(default)]
    pub replace: Option<HudLayerId>,
    /// Shown.
    #[serde(default = "yes")]
    pub visible: bool,
    /// Hidden while a screen is open.
    #[serde(default)]
    pub hide_with_screen: bool,
    /// The tree.
    pub tree: UiNodeDef,
}

impl HudLayerPayload {
    /// Splits into the def and its placement.
    pub fn into_def(self, id: HudLayerId) -> (HudLayerDef, HudPlacement) {
        let placement = if let Some(above) = self.above {
            HudPlacement::Above(above)
        } else if let Some(below) = self.below {
            HudPlacement::Below(below)
        } else if let Some(replace) = self.replace {
            HudPlacement::Replace(replace)
        } else {
            HudPlacement::Top
        };
        (
            HudLayerDef {
                id,
                anchor: self.anchor,
                tree: self.tree,
                visible: self.visible,
                hide_with_screen: self.hide_with_screen,
            },
            placement,
        )
    }
}

/// The `OpenMenu` HUD slot widgets bind to (the player's inventory menu).
/// `None` skips the `hotbar` layer.
#[derive(Resource, Default, Debug, Clone, Copy, PartialEq, Eq)]
pub struct HudMenu(pub Option<Entity>);

/// Which slot the built-in `hotbar` layer starts at in [`HudMenu`]'s menu.
#[derive(Resource, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct HudHotbar {
    /// First `SlotIx` of the hotbar row.
    pub first: u16,
}

/// HUD options on `SlottedUiConfig`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HudConfig {
    /// Spawn HUD layer roots at all.
    pub spawn: bool,
    /// Register the nine built-in layers.
    pub builtins: bool,
}

impl Default for HudConfig {
    fn default() -> Self {
        Self {
            spawn: true,
            builtins: true,
        }
    }
}

/// On a layer's full-window root. `SemanticRole::HudLayer`.
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub struct HudLayerRoot {
    /// Which layer.
    pub id: HudLayerId,
}

/// On the anchor wrapper under a root; the node the editor drags.
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub struct HudAnchored {
    /// Which layer.
    pub id: HudLayerId,
}

/// The anchors the position editor has written, by layer. Serde; persisted
/// by [`crate::hud_editor`] as RON. Overrides a def's anchor at spawn.
#[derive(Resource, Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HudLayout {
    /// Overrides by layer id.
    pub anchors: BTreeMap<HudLayerId, HudAnchor>,
}

/// A value pushed into a HUD layer by path (`HudUpdate`).
#[derive(Debug, Clone, PartialEq)]
pub enum HudValue {
    /// Replace a `Text` node's text.
    Text(String),
    /// Write a `FillValue`.
    Fill {
        /// Current.
        value: f32,
        /// Maximum.
        max: f32,
    },
    /// Show or hide the node.
    Visible(bool),
}

/// Write one value into a node of a layer. `path` is the node's `TestId`.
#[derive(Message, Debug, Clone, PartialEq)]
pub struct HudUpdate {
    /// Which layer.
    pub layer: HudLayerId,
    /// The target node's `test_id`.
    pub path: String,
    /// What to write.
    pub value: HudValue,
}

/// Registers the nine built-in defs (contract 2.1). Called by the plugin when
/// `HudConfig::builtins`.
pub fn register_builtin_layers(layers: &mut HudLayers, hotbar: &HudHotbar) {
    // PHASE6-IMPL: B. crosshair (12 px hud.crosshair panel, hide_with_screen),
    // hotbar (Custom slotted:hotbar first: hotbar.first, Bottom, (0, -24)),
    // seven zero-size placeholder panels.
    let _ = (layers, hotbar);
}

/// `SlottedUiSet::Render`: spawns roots for layers without one, despawns
/// roots whose def changed or vanished, respects `HudLayout` and `HudMenu`.
pub fn sync_hud_layers(world: &mut World) {
    // PHASE6-IMPL: B. Exclusive: trees spawn through `SpawnCtx`.
    let _ = world;
}

/// `SlottedUiSet::Render`: hides `hide_with_screen` layers while a
/// `ScreenRoot` exists.
pub fn hud_screen_visibility(
    layers: Res<HudLayers>,
    screens: Query<(), With<crate::semantic::ScreenRoot>>,
    mut roots: Query<(&HudLayerRoot, &mut Visibility)>,
) {
    // PHASE6-IMPL: B.
    let _ = (&layers, &screens, &mut roots);
}

/// `SlottedUiSet::Render`: applies [`HudUpdate`] messages.
pub fn apply_hud_updates(world: &mut World) {
    // PHASE6-IMPL: B. Resolve `path` as a `TestId` under the layer root;
    // write `Text`, `FillValue` or `Visibility`; warn on a miss.
    let _ = world;
}
