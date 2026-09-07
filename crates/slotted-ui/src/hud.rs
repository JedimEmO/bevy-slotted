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

pub use crate::def::NineAnchor;

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
    ///
    /// The wrapper is sized to its content: a left-ish anchor sets `left`, a
    /// right-ish one sets `right`, and a centred axis sets `left` (or `top`)
    /// to the window's midpoint. Centring the *content* on that midpoint is
    /// the job of [`transform`](Self::transform), which shifts the wrapper by
    /// half its own size; the two are always applied together.
    pub fn node(&self, window: Vec2) -> Node {
        let mut node = Node {
            position_type: PositionType::Absolute,
            ..default()
        };
        match self.anchor {
            NineAnchor::TopLeft | NineAnchor::Left | NineAnchor::BottomLeft => {
                node.left = px(self.offset.x);
            }
            NineAnchor::Top | NineAnchor::Center | NineAnchor::Bottom => {
                node.left = px(window.x * 0.5 + self.offset.x);
            }
            NineAnchor::TopRight | NineAnchor::Right | NineAnchor::BottomRight => {
                node.right = px(-self.offset.x);
            }
        }
        match self.anchor {
            NineAnchor::TopLeft | NineAnchor::Top | NineAnchor::TopRight => {
                node.top = px(self.offset.y);
            }
            NineAnchor::Left | NineAnchor::Center | NineAnchor::Right => {
                node.top = px(window.y * 0.5 + self.offset.y);
            }
            NineAnchor::BottomLeft | NineAnchor::Bottom | NineAnchor::BottomRight => {
                node.bottom = px(-self.offset.y);
            }
        }
        node
    }

    /// The wrapper's `UiTransform`: the layer's [`scale`](Self::scale), plus
    /// the half-size shift that centres a centred axis on its midpoint.
    /// A `UiTransform` translation is added after the scale, so the two do
    /// not interact.
    pub fn transform(&self) -> bevy::ui::ui_transform::UiTransform {
        use bevy::ui::ui_transform::{UiTransform, Val2};
        let x = match self.anchor {
            NineAnchor::Top | NineAnchor::Center | NineAnchor::Bottom => percent(-50),
            _ => px(0),
        };
        let y = match self.anchor {
            NineAnchor::Left | NineAnchor::Center | NineAnchor::Right => percent(-50),
            _ => px(0),
        };
        UiTransform {
            translation: Val2::new(x, y),
            scale: Vec2::splat(self.scale),
            ..UiTransform::IDENTITY
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

impl HudLayerDef {
    /// A layer that draws nothing: a zero-size `hud.panel` at the window
    /// centre. The seven built-in placeholders are these, and it is what
    /// `set_hud` starts from when it names a layer nobody registered.
    pub fn empty(id: HudLayerId) -> Self {
        Self {
            id,
            anchor: HudAnchor::default(),
            tree: UiNodeDef::Panel {
                role: slotted_theme::roles::HUD_PANEL,
                layout: crate::def::Layout {
                    width: Some(crate::def::Length::Px(0.0)),
                    height: Some(crate::def::Length::Px(0.0)),
                    ..crate::def::Layout::default()
                },
                children: Vec::new(),
                tags: crate::def::Tags::new(),
            },
            visible: true,
            hide_with_screen: false,
        }
    }
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
    /// The layer's position in [`order`](Self::order).
    pub fn position(&self, id: &HudLayerId) -> Option<usize> {
        self.order.iter().position(|other| other == id)
    }

    /// Appends on top, or replaces in place when the id exists.
    pub fn register(&mut self, def: HudLayerDef) {
        if self.position(&def.id).is_none() {
            self.order.push(def.id.clone());
        }
        self.defs.insert(def.id.clone(), def);
    }

    /// Inserts directly above `id`.
    pub fn insert_above(&mut self, id: &HudLayerId, def: HudLayerDef) -> Result<(), HudError> {
        let at = self.slot_of(id)? + 1;
        self.insert_at(at, def);
        Ok(())
    }

    /// Inserts directly below `id`.
    pub fn insert_below(&mut self, id: &HudLayerId, def: HudLayerDef) -> Result<(), HudError> {
        let at = self.slot_of(id)?;
        self.insert_at(at, def);
        Ok(())
    }

    /// Replaces `id`'s def, keeping its position; the new def keeps `id`.
    pub fn replace(&mut self, id: &HudLayerId, mut def: HudLayerDef) -> Result<(), HudError> {
        self.slot_of(id)?;
        def.id = id.clone();
        self.defs.insert(id.clone(), def);
        Ok(())
    }

    /// Removes a layer.
    pub fn remove(&mut self, id: &HudLayerId) -> Option<HudLayerDef> {
        let at = self.position(id)?;
        self.order.remove(at);
        self.defs.remove(id)
    }

    /// `id`'s slot, or [`HudError::UnknownLayer`].
    fn slot_of(&self, id: &HudLayerId) -> Result<usize, HudError> {
        self.position(id)
            .ok_or_else(|| HudError::UnknownLayer(id.clone()))
    }

    /// Puts `def` at `at`, first taking it out of the order if it is already
    /// somewhere else. Re-placing a layer moves it; it never duplicates.
    fn insert_at(&mut self, mut at: usize, def: HudLayerDef) {
        if let Some(existing) = self.position(&def.id) {
            self.order.remove(existing);
            if existing < at {
                at -= 1;
            }
        }
        self.order.insert(at.min(self.order.len()), def.id.clone());
        self.defs.insert(def.id.clone(), def);
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
    ///
    /// A malformed payload, or one placed relative to a layer that does not
    /// exist, is logged and skipped: one bad mod must not cost the player
    /// their whole HUD.
    pub fn load_from_registry(&mut self, registries: &slotted_registry::FrozenRegistries) {
        for (_, name, raw) in registries.hud_layers.iter() {
            let id = HudLayerId::new(name.to_string());
            match HudLayerPayload::from_value(raw.payload.clone()) {
                Ok(payload) => {
                    let (def, placement) = payload.into_def(id);
                    if let Err(e) = self.place(&placement, def) {
                        tracing::warn!(%name, %e, "HUD layer placement");
                    }
                }
                Err(e) => tracing::warn!(%name, %e, "hud layer payload is not a HudLayerPayload"),
            }
        }
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
    /// Types one registry payload, the way `ScreenDef::from_value` does.
    ///
    /// # Errors
    ///
    /// The first field that did not fit, as a `ron::Error::Message`.
    pub fn from_value(value: slotted_registry::Value) -> Result<Self, ron::Error> {
        let message = ron::Error::Message;
        let untyped = slotted_registry::to_model(&value).map_err(|e| message(e.to_string()))?;
        slotted_model::from_value(untyped).map_err(|e| message(e.to_string()))
    }

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
///
/// Seven of the nine are zero-size `hud.panel`s. They draw nothing: they are
/// the anchors a mod means by `insert_above("health", ..)`, and a game that
/// wants a real health bar replaces one.
pub fn register_builtin_layers(layers: &mut HudLayers, hotbar: &HudHotbar) {
    for id in builtin::ORDER {
        let def = match &id {
            id if *id == builtin::CROSSHAIR => HudLayerDef {
                id: id.clone(),
                anchor: HudAnchor::default(),
                tree: UiNodeDef::Panel {
                    role: slotted_theme::roles::HUD_CROSSHAIR,
                    layout: crate::def::Layout {
                        width: Some(crate::def::Length::Px(CROSSHAIR_SIZE)),
                        height: Some(crate::def::Length::Px(CROSSHAIR_SIZE)),
                        ..crate::def::Layout::default()
                    },
                    children: Vec::new(),
                    tags: crate::def::Tags::new(),
                },
                visible: true,
                hide_with_screen: true,
            },
            id if *id == builtin::HOTBAR => HudLayerDef {
                id: id.clone(),
                anchor: HudAnchor {
                    anchor: NineAnchor::Bottom,
                    offset: HOTBAR_OFFSET,
                    scale: 1.0,
                },
                tree: UiNodeDef::Custom {
                    kind: crate::widgets::kinds::hotbar(),
                    params: slotted_registry::from_model(&slotted_model::Value::Map(
                        [(
                            "first".to_owned(),
                            slotted_model::Value::Int(i64::from(hotbar.first)),
                        )]
                        .into_iter()
                        .collect(),
                    )),
                    children: Vec::new(),
                    tags: crate::def::Tags::new(),
                },
                visible: true,
                // A screen draws the player's hotbar row itself, so the HUD
                // one has to stand down while a screen is open or the player
                // sees two hotbars.
                hide_with_screen: true,
            },
            id => HudLayerDef::empty(id.clone()),
        };
        layers.register(def);
    }
}

/// Edge length of the built-in crosshair, in logical pixels.
pub const CROSSHAIR_SIZE: f32 = 12.0;

/// How far above the window's bottom edge the built-in hotbar sits.
pub const HOTBAR_OFFSET: Vec2 = Vec2::new(0.0, -24.0);

/// What a spawned layer root was built from. Comparing it against what the
/// registry now says is how [`sync_hud_layers`] decides to respawn: the tree,
/// the z position and the menu are all baked into entities, so a change to any
/// of them means the root is stale. `visible` and the anchor are *not* here:
/// showing and hiding is [`hud_screen_visibility`]'s job and moving is
/// [`reanchor_hud_layers`]'s. Neither may cost a respawn -- a layer that
/// respawned on every drag event could not be dragged at all.
#[derive(Component, Debug, Clone, PartialEq)]
struct HudSpec {
    tree: UiNodeDef,
    position: usize,
    menu: Option<Entity>,
    hide_with_screen: bool,
}

/// The built-in hotbar's `first` slot, kept in step with [`HudHotbar`].
///
/// The def is registered before the game has said which slot its hotbar row
/// starts at, and a player inventory menu puts that row a long way in. Rather
/// than freeze the plugin's default into the layer, the sync reads the
/// resource every time; changing it respawns the layer.
fn hotbar_first(id: &HudLayerId, tree: UiNodeDef, first: u16) -> UiNodeDef {
    if *id != builtin::HOTBAR {
        return tree;
    }
    let UiNodeDef::Custom {
        kind,
        params,
        children,
        tags,
    } = tree
    else {
        return tree;
    };
    let mut map = match slotted_registry::to_model(&params) {
        Ok(slotted_model::Value::Map(map)) => map,
        _ => BTreeMap::new(),
    };
    map.insert(
        "first".to_owned(),
        slotted_model::Value::Int(i64::from(first)),
    );
    UiNodeDef::Custom {
        kind,
        params: slotted_registry::from_model(&slotted_model::Value::Map(map)),
        children,
        tags,
    }
}

/// The window a HUD root is laid out against, in logical pixels.
fn window_size(world: &mut World) -> Vec2 {
    world
        .query_filtered::<&Window, With<bevy::window::PrimaryWindow>>()
        .single(world)
        .map_or(Vec2::new(1280.0, 720.0), |w| {
            Vec2::new(w.width(), w.height())
        })
}

/// `SlottedUiSet::Render`: spawns roots for layers without one, despawns
/// roots whose def changed or vanished, respects `HudLayout` and `HudMenu`.
pub fn sync_hud_layers(world: &mut World) {
    if !world
        .get_resource::<crate::plugin::SlottedUiConfig>()
        .is_none_or(|config| config.hud.spawn)
    {
        return;
    }
    let menu = world
        .get_resource::<HudMenu>()
        .copied()
        .unwrap_or_default()
        .0;
    let hotbar = world
        .get_resource::<HudHotbar>()
        .copied()
        .unwrap_or_default();
    let overrides = world
        .get_resource::<HudLayout>()
        .cloned()
        .unwrap_or_default();
    let Some(layers) = world.get_resource::<HudLayers>() else {
        return;
    };
    // The hotbar binds to `HudMenu`; without one there is nothing to draw.
    let wanted: Vec<(HudLayerId, HudSpec, bool)> = layers
        .order()
        .iter()
        .enumerate()
        .filter(|(_, id)| **id != builtin::HOTBAR || menu.is_some())
        .filter_map(|(position, id)| {
            let def = layers.get(id)?;
            Some((
                id.clone(),
                HudSpec {
                    tree: hotbar_first(id, def.tree.clone(), hotbar.first),
                    position,
                    menu,
                    hide_with_screen: def.hide_with_screen,
                },
                def.visible,
            ))
        })
        .collect();

    let live: Vec<(Entity, HudLayerId, HudSpec)> = world
        .query::<(Entity, &HudLayerRoot, &HudSpec)>()
        .iter(world)
        .map(|(e, root, spec)| (e, root.id.clone(), spec.clone()))
        .collect();
    let mut kept: HashMap<HudLayerId, Entity> = HashMap::new();
    for (entity, id, spec) in live {
        let fresh = wanted
            .iter()
            .any(|(other, want, _)| *other == id && *want == spec);
        if fresh {
            kept.insert(id, entity);
        } else {
            world.entity_mut(entity).despawn();
        }
    }

    let window = window_size(world);
    let mut roots: Vec<(HudLayerId, Option<Entity>)> = Vec::new();
    for (id, spec, visible) in wanted {
        if let Some(entity) = kept.get(&id) {
            roots.push((id, Some(*entity)));
            continue;
        }
        let anchor = overrides.anchors.get(&id).copied().unwrap_or_else(|| {
            world
                .get_resource::<HudLayers>()
                .and_then(|layers| layers.get(&id).map(|def| def.anchor))
                .unwrap_or_default()
        });
        let root = spawn_hud_root(world, &id, &spec, anchor, visible, window);
        roots.push((id, Some(root)));
    }
    let mut recorded: Vec<HudLayerId> = Vec::new();
    if let Some(layers) = world.get_resource::<HudLayers>() {
        recorded = layers.order().to_vec();
    }
    if let Some(mut layers) = world.get_resource_mut::<HudLayers>() {
        for id in recorded {
            let root = roots
                .iter()
                .find(|(other, _)| *other == id)
                .and_then(|(_, root)| *root);
            layers.set_root(id, root);
        }
    }
}

/// One layer root: a full-window pass-through node, its anchor wrapper, and
/// the layer's tree under that.
fn spawn_hud_root(
    world: &mut World,
    id: &HudLayerId,
    spec: &HudSpec,
    anchor: HudAnchor,
    visible: bool,
    window: Vec2,
) -> Entity {
    let position = i32::try_from(spec.position).unwrap_or(0);
    let root = world
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                width: percent(100),
                height: percent(100),
                ..default()
            },
            Pickable::IGNORE,
            GlobalZIndex(crate::layers::zbands::HUD + position),
            HudLayerRoot { id: id.clone() },
            crate::semantic::SemanticRole::HudLayer,
            crate::semantic::SemanticLabel(id.to_string()),
            spec.clone(),
            if visible {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            },
        ))
        .id();
    let wrapper = world
        .spawn((
            anchor.node(window),
            anchor.transform(),
            anchor,
            HudAnchored { id: id.clone() },
            Pickable::IGNORE,
            ChildOf(root),
        ))
        .id();
    let mut ctx = crate::screen::SpawnCtx {
        world,
        screen: root,
        kind: crate::def::ScreenKind::new(HUD_SCREEN_KIND),
        menu: spec.menu,
        parent: wrapper,
    };
    ctx.spawn_child(&spec.tree);
    root
}

/// The `ScreenKind` a HUD layer's tree is spawned under. Not a real screen:
/// nothing opens it, and no `ScreenRoot` carries it. Widgets that ask which
/// screen they are in see this.
pub const HUD_SCREEN_KIND: &str = "slotted:hud";

/// `SlottedUiSet::Render`: keeps every anchor wrapper's `Node` in step with
/// its `HudAnchor` and the window size, so a drag in the editor and a window
/// resize both land without respawning the layer.
pub fn reanchor_hud_layers(
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
    layers: Res<HudLayers>,
    layout: Res<HudLayout>,
    mut wrappers: Query<(
        &HudAnchored,
        &mut HudAnchor,
        &mut Node,
        &mut bevy::ui::ui_transform::UiTransform,
    )>,
    mut last: Local<Option<Vec2>>,
) {
    let window = windows.single().map_or(Vec2::new(1280.0, 720.0), |w| {
        Vec2::new(w.width(), w.height())
    });
    let resized = *last != Some(window);
    *last = Some(window);
    for (anchored, mut anchor, mut node, mut transform) in &mut wrappers {
        // The saved layout wins over the def: it is what the editor writes,
        // and where the player last put the layer is where it belongs.
        let wanted = layout
            .anchors
            .get(&anchored.id)
            .copied()
            .or_else(|| layers.get(&anchored.id).map(|def| def.anchor));
        if let Some(wanted) = wanted
            && *anchor != wanted
        {
            *anchor = wanted;
        }
        let wanted_node = anchor.node(window);
        if resized || *node != wanted_node {
            *node = wanted_node;
        }
        let wanted_transform = anchor.transform();
        if *transform != wanted_transform {
            *transform = wanted_transform;
        }
    }
}

/// `SlottedUiSet::Render`: hides `hide_with_screen` layers while a
/// `ScreenRoot` exists.
pub fn hud_screen_visibility(
    layers: Res<HudLayers>,
    screens: Query<(), With<crate::semantic::ScreenRoot>>,
    mut roots: Query<(&HudLayerRoot, &mut Visibility)>,
) {
    let screen_open = !screens.is_empty();
    for (root, mut visibility) in &mut roots {
        let Some(def) = layers.get(&root.id) else {
            continue;
        };
        let wanted = if def.visible && !(def.hide_with_screen && screen_open) {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if *visibility != wanted {
            *visibility = wanted;
        }
    }
}

/// `SlottedUiSet::Render`: applies [`HudUpdate`] messages.
pub fn apply_hud_updates(world: &mut World) {
    let updates: Vec<HudUpdate> = match world.get_resource_mut::<Messages<HudUpdate>>() {
        Some(mut messages) => messages.drain().collect(),
        None => return,
    };
    for update in updates {
        let Some(root) = world
            .get_resource::<HudLayers>()
            .and_then(|layers| layers.root(&update.layer))
        else {
            tracing::warn!(layer = %update.layer, "hud update for a layer that is not spawned");
            continue;
        };
        let Some(target) = find_by_test_id(world, root, &update.path) else {
            tracing::warn!(
                layer = %update.layer,
                path = %update.path,
                "hud update found no node with that test_id"
            );
            continue;
        };
        match update.value {
            HudValue::Text(text) => {
                if let Some(mut existing) = world.get_mut::<Text>(target) {
                    existing.0 = text;
                } else {
                    world.entity_mut(target).insert(Text::new(text));
                }
            }
            HudValue::Fill { value, max } => {
                world
                    .entity_mut(target)
                    .insert(crate::widgets::tank::FillValue { value, max });
            }
            HudValue::Visible(show) => {
                world.entity_mut(target).insert(if show {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                });
            }
        }
    }
}

/// The first descendant of `root` (or `root` itself) whose `TestId` is `path`.
fn find_by_test_id(world: &World, root: Entity, path: &str) -> Option<Entity> {
    if world
        .get::<crate::semantic::TestId>(root)
        .is_some_and(|id| id.0 == path)
    {
        return Some(root);
    }
    let children: Vec<Entity> = world
        .get::<Children>(root)
        .map(|c| c.iter().collect())
        .unwrap_or_default();
    children
        .into_iter()
        .find_map(|child| find_by_test_id(world, child, path))
}
