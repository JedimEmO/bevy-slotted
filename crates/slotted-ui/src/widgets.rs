//! Built-in widgets. Each is a [`Widget`] registered under a `slotted:` kind
//! by the plugin. The mapping from node to components is in the contract.
//!
//! Two paths reach the same code. A typed [`UiNodeDef`] variant is dispatched
//! by [`SpawnCtx::spawn_child`] straight to one of the `spawn_*` functions
//! here; a `Custom` node is looked up in the [`WidgetRegistry`] by kind and
//! its `params` deserialised into the same arguments. That is what lets
//! `slotted:hotbar` be written either way.

pub mod bar;
pub mod icon_button;
pub mod side_tab;
pub mod tank;
pub mod viewport;
pub mod virtual_grid;

use bevy::input_focus::tab_navigation::TabIndex;
use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use bevy::ui::auto_directional_navigation::AutoDirectionalNavigation;
use serde::{Deserialize, Serialize};
use slotted_ecs::{Carried, MenuAction, Modifiers, SlotRef};
use slotted_model::{Button as ModelButton, ClickAction, InventoryRef, SlotIx, ToolbarAction};
use slotted_registry::Value;
use slotted_theme::{Role, Themed, roles};

use crate::def::{IconDef, Layout, LayoutDirection, LocKey, Tags, TextRole, UiNodeDef, WidgetKind};
use crate::input::{
    on_slot_drag_end, on_slot_drag_enter, on_slot_drag_start, on_slot_press, on_slot_release,
};
use crate::item::{ItemView, spawn_item_view_children};
use crate::screen::{SpawnCtx, Widget, WidgetRegistry};
use crate::semantic::{AnchorNode, LocText, SemanticLabel, SemanticRole, WidgetNode};
use crate::tooltip::on_slot_over;

/// Edge length of a slot in logical pixels. Not a theme token in Phase 2.
pub const SLOT_SIZE: f32 = 44.0;
/// Border width every themed node reserves, so a `BorderColor` is visible.
pub const BORDER_WIDTH: f32 = 1.0;

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
    /// The hotbar row: a 9x1 slot grid with hotbar tags.
    pub fn hotbar() -> WidgetKind {
        WidgetKind(ns("slotted:hotbar"))
    }
    /// A tooltip body. Used internally by the tooltip system.
    pub fn tooltip() -> WidgetKind {
        WidgetKind(ns("slotted:tooltip"))
    }
    /// A fluid tank (Phase 6).
    pub fn tank() -> WidgetKind {
        WidgetKind(ns("slotted:tank"))
    }
    /// A bar (Phase 6).
    pub fn bar() -> WidgetKind {
        WidgetKind(ns("slotted:bar"))
    }
    /// A progress arrow (Phase 6).
    pub fn progress() -> WidgetKind {
        WidgetKind(ns("slotted:progress"))
    }
    /// A side tab (Phase 6).
    pub fn side_tab() -> WidgetKind {
        WidgetKind(ns("slotted:side_tab"))
    }
    /// A cycling icon button (Phase 6).
    pub fn icon_button() -> WidgetKind {
        WidgetKind(ns("slotted:icon_button"))
    }
    /// A virtual grid (Phase 6).
    pub fn virtual_grid() -> WidgetKind {
        WidgetKind(ns("slotted:virtual_grid"))
    }
    /// A 3D viewport (Phase 6).
    pub fn viewport() -> WidgetKind {
        WidgetKind(ns("slotted:viewport"))
    }

    /// Every built-in kind.
    pub fn all() -> [WidgetKind; 15] {
        [
            panel(),
            text(),
            slot(),
            slot_grid(),
            button(),
            action_rail(),
            hotbar(),
            tooltip(),
            tank(),
            bar(),
            progress(),
            side_tab(),
            icon_button(),
            virtual_grid(),
            viewport(),
        ]
    }
}

// ---------------------------------------------------------------------------
// Icons
// ---------------------------------------------------------------------------

/// What an [`IconDef`] needs to become an `ImageNode`, as a `SystemParam`.
///
/// An `image` icon is an asset path; an `item` icon goes through the same
/// [`Icons`](slotted_icons::Icons) port the slot renderer uses, so a game that
/// swapped the icon source gets its own artwork here too.
#[derive(bevy::ecs::system::SystemParam)]
pub struct IconImages<'w> {
    assets: Option<Res<'w, AssetServer>>,
    icons: Option<Res<'w, slotted_icons::Icons>>,
    registries: Option<Res<'w, slotted_ecs::Registries>>,
}

impl IconImages<'_> {
    /// The `ImageNode` for `icon`.
    pub fn image(&self, icon: &IconDef) -> ImageNode {
        build_icon(
            self.assets.as_deref(),
            self.icons.as_deref(),
            self.registries.as_deref(),
            icon,
        )
    }
}

/// [`IconImages::image`] from a `&World`, for the spawn path.
pub fn icon_image(world: &World, icon: &IconDef) -> ImageNode {
    build_icon(
        world.get_resource::<AssetServer>(),
        world.get_resource::<slotted_icons::Icons>(),
        world.get_resource::<slotted_ecs::Registries>(),
        icon,
    )
}

fn build_icon(
    assets: Option<&AssetServer>,
    icons: Option<&slotted_icons::Icons>,
    registries: Option<&slotted_ecs::Registries>,
    icon: &IconDef,
) -> ImageNode {
    match icon {
        IconDef::Image(path) => match assets {
            Some(assets) => ImageNode::new(assets.load(path)),
            // No `AssetServer` (a bare unit-test world): the node still
            // exists, it simply has nothing to draw.
            None => ImageNode::default(),
        },
        IconDef::Item(name) => {
            let icon = registries
                .and_then(|r| r.0.items.id_of(name))
                .zip(icons)
                .map(|(id, icons)| icons.icon(&slotted_model::ItemStack::new(id, 1)));
            let mut node = ImageNode::default();
            match icon {
                Some(slotted_icons::IconRef::Atlas {
                    image,
                    layout,
                    index,
                }) => {
                    node.image = image;
                    node.texture_atlas = Some(TextureAtlas { layout, index });
                }
                Some(slotted_icons::IconRef::Solid(color)) => node.color = color,
                // An unknown item is the placeholder colour, the same signal
                // the atlas bake gives an item with no artwork.
                _ => node.color = slotted_icons::placeholder_color(name),
            }
            node
        }
    }
}

// ---------------------------------------------------------------------------
// Panel
// ---------------------------------------------------------------------------

/// `Node` for a [`Layout`], with gap and padding resolved through the theme's
/// spacing scale.
pub fn layout_node(layout: &Layout, spacing_sm: f32) -> Node {
    Node {
        display: Display::Flex,
        flex_direction: match layout.direction {
            LayoutDirection::Row => FlexDirection::Row,
            LayoutDirection::Column => FlexDirection::Column,
        },
        row_gap: Val::Px(layout.gap * spacing_sm),
        column_gap: Val::Px(layout.gap * spacing_sm),
        padding: UiRect::all(Val::Px(layout.padding * spacing_sm)),
        border: UiRect::all(Val::Px(BORDER_WIDTH)),
        width: layout.width.map_or(Val::Auto, Val::Px),
        height: layout.height.map_or(Val::Auto, Val::Px),
        align_items: if layout.center {
            AlignItems::Center
        } else {
            AlignItems::FlexStart
        },
        ..default()
    }
}

/// Spawns a themed container and its children.
pub fn spawn_panel(
    ctx: &mut SpawnCtx<'_>,
    role: &Role,
    layout: &Layout,
    children: &[UiNodeDef],
) -> Entity {
    let spacing = ctx.tokens().spacing.sm;
    let entity = ctx.spawn_node((
        layout_node(layout, spacing),
        Themed(role.clone()),
        SemanticRole::Panel,
        WidgetNode(kinds::panel()),
    ));
    ctx.spawn_children(entity, children);
    entity
}

// ---------------------------------------------------------------------------
// Text
// ---------------------------------------------------------------------------

/// Spawns a text label. Phase 2 has no locale table, so the key is the text;
/// the font is Bevy's default handle.
pub fn spawn_text(ctx: &mut SpawnCtx<'_>, key: &LocKey, style: TextRole) -> Entity {
    ctx.spawn_node((
        Node::default(),
        Text::new(key.0.clone()),
        Themed(style.role()),
        SemanticRole::Text,
        SemanticLabel(key.0.clone()),
        WidgetNode(kinds::text()),
        LocText(key.clone()),
    ))
}

// ---------------------------------------------------------------------------
// Slot
// ---------------------------------------------------------------------------

/// Spawns one slot: the full bundle from the contract's mapping table, its
/// icon, count and durability children, and the pointer observers that turn
/// gestures into `SlotClicked` and `MenuAction`.
///
/// `tab_index` orders keyboard navigation; grids pass the cell number.
pub fn spawn_slot(ctx: &mut SpawnCtx<'_>, slot: SlotIx, tags: &Tags, tab_index: i32) -> Entity {
    let radius = ctx.tokens().radii.sm;
    let entity = ctx.spawn_node((
        Node {
            width: Val::Px(SLOT_SIZE),
            height: Val::Px(SLOT_SIZE),
            border: UiRect::all(Val::Px(BORDER_WIDTH)),
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            position_type: PositionType::Relative,
            overflow: Overflow::visible(),
            border_radius: BorderRadius::all(Val::Px(radius)),
            ..default()
        },
        Themed(roles::SLOT),
        SemanticRole::Slot,
        SemanticLabel::default(),
        ItemView::default(),
        bevy::ui_widgets::Button,
        Hovered::default(),
        TabIndex(tab_index),
        AutoDirectionalNavigation::default(),
        Pickable::default(),
        WidgetNode(kinds::slot()),
        tags.clone(),
    ));
    if let Some(menu) = ctx.menu {
        ctx.world.entity_mut(entity).insert(SlotRef { menu, slot });
    }
    spawn_item_view_children(ctx.world, entity);
    let mut e = ctx.world.entity_mut(entity);
    e.observe(on_slot_press);
    e.observe(on_slot_release);
    e.observe(on_slot_over);
    e.observe(on_slot_drag_start);
    e.observe(on_slot_drag_enter);
    e.observe(on_slot_drag_end);
    entity
}

// ---------------------------------------------------------------------------
// Slot grid
// ---------------------------------------------------------------------------

/// Spawns a `cols x rows` grid of slots for `first..first + cols*rows`,
/// row-major. Every cell carries the grid's tags.
pub fn spawn_slot_grid(
    ctx: &mut SpawnCtx<'_>,
    inventory: InventoryRef,
    cols: u16,
    rows: u16,
    first: u16,
    tags: &Tags,
    role: SemanticRole,
) -> Entity {
    let gap = ctx.tokens().spacing.sm;
    let mut tags = tags.clone();
    // The region tag defaults to the inventory index, so a locator can always
    // name a grid even when the screen author set no tags at all.
    if tags.get("region").is_none() {
        tags.0
            .insert("region".to_owned(), inventory.index().to_string());
    }
    let entity = ctx.spawn_node((
        Node {
            display: Display::Grid,
            grid_template_columns: vec![RepeatedGridTrack::px(cols, SLOT_SIZE)],
            grid_template_rows: vec![RepeatedGridTrack::px(rows, SLOT_SIZE)],
            row_gap: Val::Px(gap),
            column_gap: Val::Px(gap),
            ..default()
        },
        role,
        WidgetNode(kinds::slot_grid()),
        tags.clone(),
    ));
    let parent = std::mem::replace(&mut ctx.parent, entity);
    for i in 0..u32::from(cols) * u32::from(rows) {
        #[allow(clippy::cast_possible_truncation)]
        let slot = SlotIx(first.saturating_add(i as u16));
        #[allow(clippy::cast_possible_wrap)]
        spawn_slot(ctx, slot, &tags, i as i32);
    }
    ctx.parent = parent;
    entity
}

// ---------------------------------------------------------------------------
// Button
// ---------------------------------------------------------------------------

/// Spawns a themed button carrying `widget` as its behaviour kind.
pub fn spawn_button(ctx: &mut SpawnCtx<'_>, widget: &WidgetKind, label: Option<&str>) -> Entity {
    let tokens = ctx.tokens();
    let entity = ctx.spawn_node((
        Node {
            padding: UiRect::axes(Val::Px(tokens.spacing.md), Val::Px(tokens.spacing.sm)),
            border: UiRect::all(Val::Px(BORDER_WIDTH)),
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            border_radius: BorderRadius::all(Val::Px(tokens.radii.md)),
            ..default()
        },
        Themed(roles::BUTTON),
        SemanticRole::Button,
        SemanticLabel(label.unwrap_or(widget.0.path()).to_owned()),
        bevy::ui_widgets::Button,
        Hovered::default(),
        TabIndex(0),
        Pickable::default(),
        WidgetNode(widget.clone()),
    ));
    if let Some(label) = label {
        ctx.world.spawn((
            Node::default(),
            Text::new(label.to_owned()),
            Themed(roles::TEXT),
            Pickable::IGNORE,
            ChildOf(entity),
        ));
    }
    entity
}

// ---------------------------------------------------------------------------
// Anchor
// ---------------------------------------------------------------------------

/// Spawns a zero-size anchor node and splices the registered injections for
/// this screen and anchor under it, in registration order.
pub fn spawn_anchor(ctx: &mut SpawnCtx<'_>, id: &crate::def::AnchorId) -> Entity {
    let entity = ctx.spawn_node((
        Node::default(),
        AnchorNode(id.clone()),
        SemanticRole::Anchor,
    ));
    let injections: Vec<(UiNodeDef, bool)> = ctx
        .world
        .get_resource::<crate::screen::Injections>()
        .map(|i| {
            i.at(&ctx.kind, id)
                .map(|inj| (inj.node.clone(), inj.exclusion))
                .collect()
        })
        .unwrap_or_default();
    let parent = std::mem::replace(&mut ctx.parent, entity);
    for (node, exclusion) in &injections {
        let child = ctx.spawn_child(node);
        if *exclusion {
            ctx.world
                .entity_mut(child)
                .insert(crate::layers::ExclusionZone);
        }
    }
    ctx.parent = parent;
    entity
}

// ---------------------------------------------------------------------------
// Action rail
// ---------------------------------------------------------------------------

/// The toolbar action a rail button fires. On each rail button entity.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct RailAction(pub ToolbarAction);

/// Parameters of `slotted:action_rail`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActionRailParams {
    /// Which actions, in order. Unknown names are logged and skipped.
    #[serde(default = "default_actions")]
    pub actions: Vec<String>,
    /// The container side of every move.
    #[serde(default = "first_inventory")]
    pub container: InventoryRef,
    /// The player side of every move.
    #[serde(default = "second_inventory")]
    pub player: InventoryRef,
}

impl Default for ActionRailParams {
    fn default() -> Self {
        Self {
            actions: default_actions(),
            container: first_inventory(),
            player: second_inventory(),
        }
    }
}

fn default_actions() -> Vec<String> {
    vec![
        "sort".to_owned(),
        "quick_stack".to_owned(),
        "deposit_all".to_owned(),
        "loot_all".to_owned(),
    ]
}

impl ActionRailParams {
    /// Resolves one action name against the configured inventories.
    pub fn action(&self, name: &str) -> Option<ToolbarAction> {
        let (container, player) = (self.container, self.player);
        match name {
            "sort" => Some(ToolbarAction::Sort {
                inventory: container,
            }),
            "quick_stack" => Some(ToolbarAction::QuickStack {
                from: player,
                to: container,
            }),
            "deposit_all" => Some(ToolbarAction::DepositAll {
                from: player,
                to: container,
            }),
            "loot_all" => Some(ToolbarAction::LootAll {
                from: container,
                to: player,
            }),
            _ => None,
        }
    }
}

/// The English label of a built-in rail action, or the id itself for one the
/// library does not name.
///
/// The rail draws these unless a locale defines `slotted.rail.<action>`, so a
/// game that ships no `.ftl` at all still gets readable buttons instead of the
/// action ids, and a game that ships one overrides them without touching Rust.
pub fn default_rail_label(action: &str) -> &str {
    match action {
        "sort" => "Sort",
        "quick_stack" => "Quick stack",
        "deposit_all" => "Deposit all",
        "loot_all" => "Loot all",
        other => other,
    }
}

/// The localisation key of a rail action's label.
pub fn rail_label_key(action: &str) -> LocKey {
    LocKey(format!("slotted.rail.{action}"))
}

/// What one rail button draws: the locale's word for the action, else the
/// library's English default.
fn rail_label(world: &World, action: &str) -> String {
    world
        .get_resource::<crate::loc::Localization>()
        .and_then(|loc| loc.resolve(&rail_label_key(action)))
        .unwrap_or_else(|| default_rail_label(action).to_owned())
}

/// Spawns the action rail: one button per action, each tagged `action=<name>`
/// and carrying a [`RailAction`] whose `Activate` observer triggers
/// `MenuAction(ClickAction::Toolbar(..))`.
///
/// A button's `Tags` keep the raw action id, so a locator names `sort`
/// whatever language the label is in.
pub fn spawn_action_rail(ctx: &mut SpawnCtx<'_>, params: &ActionRailParams) -> Entity {
    let tokens = ctx.tokens();
    let entity = ctx.spawn_node((
        Node {
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(tokens.spacing.sm),
            padding: UiRect::all(Val::Px(tokens.spacing.sm)),
            border: UiRect::all(Val::Px(BORDER_WIDTH)),
            ..default()
        },
        Themed(roles::RAIL),
        SemanticRole::Rail,
        WidgetNode(kinds::action_rail()),
    ));
    let parent = std::mem::replace(&mut ctx.parent, entity);
    for name in &params.actions {
        let Some(action) = params.action(name) else {
            tracing::warn!(%name, "unknown action rail action");
            continue;
        };
        let label = rail_label(ctx.world, name);
        let button = spawn_button(ctx, &kinds::button(), Some(&label));
        ctx.world
            .entity_mut(button)
            .insert((RailAction(action), Tags::new().with("action", name)));
        ctx.world.entity_mut(button).observe(on_rail_activate);
    }
    ctx.parent = parent;
    entity
}

/// Observer on rail buttons: `Activate` becomes a `MenuAction` on the menu the
/// screen drives.
pub fn on_rail_activate(
    activate: On<bevy::ui_widgets::Activate>,
    rail: Query<&RailAction>,
    screens: Query<&crate::semantic::ScreenRoot>,
    parents: Query<&ChildOf>,
    mut commands: Commands,
) {
    let entity = activate.entity;
    let Ok(action) = rail.get(entity) else {
        return;
    };
    let Some(menu) = menu_of(entity, &parents, &screens) else {
        tracing::warn!(?entity, "rail button is not inside a screen with a menu");
        return;
    };
    commands.trigger(MenuAction {
        entity: menu,
        action: ClickAction::Toolbar(action.0),
    });
}

/// Walks up to the `ScreenRoot` and returns the menu it drives.
pub fn menu_of(
    entity: Entity,
    parents: &Query<&ChildOf>,
    screens: &Query<&crate::semantic::ScreenRoot>,
) -> Option<Entity> {
    let mut current = entity;
    loop {
        if let Ok(root) = screens.get(current) {
            return root.menu;
        }
        current = parents.get(current).ok()?.parent();
    }
}

// ---------------------------------------------------------------------------
// Hotbar
// ---------------------------------------------------------------------------

/// Parameters of `slotted:hotbar`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HotbarParams {
    /// First slot index of the row.
    #[serde(default)]
    pub first: u16,
    /// Which inventory the row draws from.
    #[serde(default = "hotbar_inventory")]
    pub inventory: InventoryRef,
}

const fn hotbar_inventory() -> InventoryRef {
    InventoryRef::new(2)
}

impl Default for HotbarParams {
    fn default() -> Self {
        Self {
            first: 0,
            inventory: hotbar_inventory(),
        }
    }
}

/// Spawns the hotbar: a 9x1 slot grid whose cells are tagged `region=hotbar`.
pub fn spawn_hotbar(ctx: &mut SpawnCtx<'_>, params: &HotbarParams, tags: &Tags) -> Entity {
    let mut tags = tags.clone();
    tags.0.insert("region".to_owned(), "hotbar".to_owned());
    spawn_slot_grid(
        ctx,
        params.inventory,
        9,
        1,
        params.first,
        &tags,
        SemanticRole::Hotbar,
    )
}

// ---------------------------------------------------------------------------
// Tooltip
// ---------------------------------------------------------------------------

/// Spawns a tooltip body: a `tooltip` panel with a `tooltip.frame` child that
/// holds the composed parts.
pub fn spawn_tooltip(ctx: &mut SpawnCtx<'_>, parts: &[UiNodeDef]) -> Entity {
    let tokens = ctx.tokens();
    let entity = ctx.spawn_node((
        Node {
            position_type: PositionType::Absolute,
            flex_direction: FlexDirection::Column,
            padding: UiRect::all(Val::Px(tokens.spacing.sm)),
            border: UiRect::all(Val::Px(BORDER_WIDTH)),
            ..default()
        },
        Themed(roles::TOOLTIP),
        SemanticRole::Tooltip,
        WidgetNode(kinds::tooltip()),
        Pickable::IGNORE,
    ));
    let frame = ctx
        .world
        .spawn((
            Node {
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(tokens.spacing.xs),
                border: UiRect::all(Val::Px(BORDER_WIDTH)),
                ..default()
            },
            Themed(roles::TOOLTIP_FRAME),
            Pickable::IGNORE,
            ChildOf(entity),
        ))
        .id();
    ctx.spawn_children(frame, parts);
    entity
}

// ---------------------------------------------------------------------------
// Registry adapters
// ---------------------------------------------------------------------------

/// Reads a widget's parameters from its registry payload.
///
/// The payload goes through `slotted_model::Value` rather than
/// `ron::Value::into_rust`, which is what lets a params field be an `Option`:
/// a present value is `Some` whether it was written `Some(x)` in a `.ron` file
/// or as a bare `x` by a mod's `data.lua`. Phase 2 forbade `Option` here for
/// exactly the reason that hop could not express one.
macro_rules! params_of {
    ($params:expr, $kind:literal) => {
        match $params {
            Value::Unit => Default::default(),
            other => match slotted_registry::to_model(other)
                .map_err(|e| e.to_string())
                .and_then(|v| slotted_model::from_value(v).map_err(|e| e.to_string()))
            {
                Ok(p) => p,
                Err(e) => {
                    tracing::warn!(%e, "bad params for {}; using defaults", $kind);
                    Default::default()
                }
            },
        }
    };
}

/// `slotted:panel` through the registry.
#[derive(Debug, Default, Clone, Copy)]
pub struct PanelWidget;

/// Parameters of `slotted:panel`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PanelParams {
    /// Theme role.
    #[serde(default = "panel_role")]
    pub role: Role,
    /// Layout.
    #[serde(default)]
    pub layout: Layout,
}

impl Default for PanelParams {
    fn default() -> Self {
        Self {
            role: panel_role(),
            layout: Layout::default(),
        }
    }
}

fn panel_role() -> Role {
    roles::PANEL
}

impl Widget for PanelWidget {
    fn spawn(&self, ctx: &mut SpawnCtx<'_>, params: &Value, children: &[UiNodeDef]) -> Entity {
        let params: PanelParams = params_of!(params, "slotted:panel");
        spawn_panel(ctx, &params.role, &params.layout, children)
    }
}

/// `slotted:text` through the registry.
#[derive(Debug, Default, Clone, Copy)]
pub struct TextWidget;

/// Parameters of `slotted:text`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextParams {
    /// Localisation key.
    pub key: LocKey,
    /// Style.
    #[serde(default = "body_style")]
    pub style: TextRole,
}

fn body_style() -> TextRole {
    TextRole::Body
}

impl Default for TextParams {
    fn default() -> Self {
        Self {
            key: LocKey(String::new()),
            style: TextRole::Body,
        }
    }
}

impl Widget for TextWidget {
    fn spawn(&self, ctx: &mut SpawnCtx<'_>, params: &Value, _children: &[UiNodeDef]) -> Entity {
        let params: TextParams = params_of!(params, "slotted:text");
        spawn_text(ctx, &params.key, params.style)
    }
}

/// `slotted:slot` through the registry.
#[derive(Debug, Default, Clone, Copy)]
pub struct SlotWidget;

/// Parameters of `slotted:slot`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SlotParams {
    /// Slot index.
    #[serde(default = "first_slot")]
    pub slot: SlotIx,
}

const fn first_slot() -> SlotIx {
    SlotIx(0)
}

impl Default for SlotParams {
    fn default() -> Self {
        Self { slot: SlotIx(0) }
    }
}

impl Widget for SlotWidget {
    fn spawn(&self, ctx: &mut SpawnCtx<'_>, params: &Value, _children: &[UiNodeDef]) -> Entity {
        let params: SlotParams = params_of!(params, "slotted:slot");
        spawn_slot(ctx, params.slot, &Tags::new(), 0)
    }
}

/// `slotted:slot_grid` through the registry.
#[derive(Debug, Default, Clone, Copy)]
pub struct SlotGridWidget;

/// Parameters of `slotted:slot_grid`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SlotGridParams {
    /// Which inventory.
    #[serde(default = "first_inventory")]
    pub inventory: InventoryRef,
    /// Columns.
    #[serde(default = "nine")]
    pub cols: u16,
    /// Rows.
    #[serde(default = "one")]
    pub rows: u16,
    /// First slot index.
    #[serde(default)]
    pub first: u16,
}

const fn first_inventory() -> InventoryRef {
    InventoryRef::new(0)
}

const fn second_inventory() -> InventoryRef {
    InventoryRef::new(1)
}

const fn nine() -> u16 {
    9
}

const fn one() -> u16 {
    1
}

impl Default for SlotGridParams {
    fn default() -> Self {
        Self {
            inventory: InventoryRef::new(0),
            cols: 9,
            rows: 1,
            first: 0,
        }
    }
}

impl Widget for SlotGridWidget {
    fn spawn(&self, ctx: &mut SpawnCtx<'_>, params: &Value, _children: &[UiNodeDef]) -> Entity {
        let p: SlotGridParams = params_of!(params, "slotted:slot_grid");
        spawn_slot_grid(
            ctx,
            p.inventory,
            p.cols,
            p.rows,
            p.first,
            &Tags::new(),
            SemanticRole::Grid,
        )
    }
}

/// `slotted:button` through the registry.
#[derive(Debug, Default, Clone, Copy)]
pub struct ButtonWidget;

/// Parameters of `slotted:button`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ButtonParams {
    /// Label text; the widget kind's path when absent.
    #[serde(default)]
    pub label: Option<String>,
}

impl Widget for ButtonWidget {
    fn spawn(&self, ctx: &mut SpawnCtx<'_>, params: &Value, _children: &[UiNodeDef]) -> Entity {
        let p: ButtonParams = params_of!(params, "slotted:button");
        spawn_button(ctx, &kinds::button(), p.label.as_deref())
    }
}

/// `slotted:action_rail` through the registry.
#[derive(Debug, Default, Clone, Copy)]
pub struct ActionRailWidget;

impl Widget for ActionRailWidget {
    fn spawn(&self, ctx: &mut SpawnCtx<'_>, params: &Value, _children: &[UiNodeDef]) -> Entity {
        let p: ActionRailParams = params_of!(params, "slotted:action_rail");
        spawn_action_rail(ctx, &p)
    }
}

/// `slotted:hotbar` through the registry.
#[derive(Debug, Default, Clone, Copy)]
pub struct HotbarWidget;

impl Widget for HotbarWidget {
    fn spawn(&self, ctx: &mut SpawnCtx<'_>, params: &Value, _children: &[UiNodeDef]) -> Entity {
        let p: HotbarParams = params_of!(params, "slotted:hotbar");
        spawn_hotbar(ctx, &p, &Tags::new())
    }
}

/// `slotted:tooltip` through the registry.
#[derive(Debug, Default, Clone, Copy)]
pub struct TooltipWidget;

impl Widget for TooltipWidget {
    fn spawn(&self, ctx: &mut SpawnCtx<'_>, _params: &Value, children: &[UiNodeDef]) -> Entity {
        spawn_tooltip(ctx, children)
    }
}

// ---------------------------------------------------------------------------
// Phase 6 widgets through the registry
// ---------------------------------------------------------------------------

/// `slotted:tank` through the registry.
#[derive(Debug, Default, Clone, Copy)]
pub struct TankWidget;

impl Widget for TankWidget {
    fn spawn(&self, ctx: &mut SpawnCtx<'_>, params: &Value, _children: &[UiNodeDef]) -> Entity {
        let p: tank::TankParams = params_of!(params, "slotted:tank");
        tank::spawn_tank(ctx, &p, &Tags::new())
    }

    fn tooltip(&self, entity: Entity, world: &World, out: &mut Vec<UiNodeDef>) {
        fill_tooltip(entity, world, out);
    }
}

/// `slotted:bar` through the registry.
#[derive(Debug, Default, Clone, Copy)]
pub struct BarWidget;

impl Widget for BarWidget {
    fn spawn(&self, ctx: &mut SpawnCtx<'_>, params: &Value, _children: &[UiNodeDef]) -> Entity {
        let p: bar::BarParams = params_of!(params, "slotted:bar");
        bar::spawn_bar(ctx, &p, bar::BarStyle::Bar, &Tags::new())
    }

    fn tooltip(&self, entity: Entity, world: &World, out: &mut Vec<UiNodeDef>) {
        fill_tooltip(entity, world, out);
    }
}

/// `slotted:progress` through the registry.
#[derive(Debug, Default, Clone, Copy)]
pub struct ProgressWidget;

impl Widget for ProgressWidget {
    fn spawn(&self, ctx: &mut SpawnCtx<'_>, params: &Value, _children: &[UiNodeDef]) -> Entity {
        let p: bar::BarParams = params_of!(params, "slotted:progress");
        bar::spawn_bar(ctx, &p, bar::BarStyle::Progress, &Tags::new())
    }

    fn tooltip(&self, entity: Entity, world: &World, out: &mut Vec<UiNodeDef>) {
        fill_tooltip(entity, world, out);
    }
}

/// `slotted:side_tab` through the registry.
#[derive(Debug, Default, Clone, Copy)]
pub struct SideTabWidget;

impl Widget for SideTabWidget {
    fn spawn(&self, ctx: &mut SpawnCtx<'_>, params: &Value, children: &[UiNodeDef]) -> Entity {
        let p: side_tab::SideTabParams = params_of!(params, "slotted:side_tab");
        side_tab::spawn_side_tab(ctx, &p, children, &Tags::new())
    }
}

/// `slotted:icon_button` through the registry.
#[derive(Debug, Default, Clone, Copy)]
pub struct IconButtonWidget;

impl Widget for IconButtonWidget {
    fn spawn(&self, ctx: &mut SpawnCtx<'_>, params: &Value, _children: &[UiNodeDef]) -> Entity {
        let p: icon_button::IconButtonParams = params_of!(params, "slotted:icon_button");
        icon_button::spawn_icon_button(ctx, &p, &Tags::new())
    }

    fn tooltip(&self, entity: Entity, world: &World, out: &mut Vec<UiNodeDef>) {
        let Some(state) = world.get::<icon_button::IconButtonState>(entity) else {
            return;
        };
        if state.states.is_empty() {
            return;
        }
        out.push(UiNodeDef::Text {
            key: state.state().label.clone(),
            style: TextRole::Body,
            tags: Tags::new(),
        });
    }
}

/// `slotted:virtual_grid` through the registry.
#[derive(Debug, Default, Clone, Copy)]
pub struct VirtualGridWidget;

impl Widget for VirtualGridWidget {
    fn spawn(&self, ctx: &mut SpawnCtx<'_>, params: &Value, _children: &[UiNodeDef]) -> Entity {
        let p: virtual_grid::VirtualGridParams = params_of!(params, "slotted:virtual_grid");
        virtual_grid::spawn_virtual_grid(ctx, &p, &Tags::new())
    }
}

/// `slotted:viewport` through the registry.
#[derive(Debug, Default, Clone, Copy)]
pub struct ViewportWidget;

impl Widget for ViewportWidget {
    fn spawn(&self, ctx: &mut SpawnCtx<'_>, params: &Value, _children: &[UiNodeDef]) -> Entity {
        let p: viewport::ViewportParams = params_of!(params, "slotted:viewport");
        viewport::spawn_viewport(ctx, &p, &Tags::new())
    }
}

/// The one `"<value> / <max> <unit>"` line a tank, bar or progress arrow adds
/// to its tooltip. A literal, not a key: `LocText` leaves an unknown key
/// verbatim, so this reads the same with and without a locale.
fn fill_tooltip(entity: Entity, world: &World, out: &mut Vec<UiNodeDef>) {
    let Some(fill) = world.get::<tank::FillValue>(entity) else {
        return;
    };
    let unit = world
        .get::<tank::TankFluidSource>(entity)
        .map_or("", |s| s.unit.as_str());
    out.push(UiNodeDef::Text {
        key: LocKey(tank::fill_label(fill, unit)),
        style: TextRole::Body,
        tags: Tags::new(),
    });
}

/// Registers every built-in under its kind.
pub fn register_builtins(registry: &mut WidgetRegistry) {
    registry.register(kinds::tank(), TankWidget);
    registry.register(kinds::bar(), BarWidget);
    registry.register(kinds::progress(), ProgressWidget);
    registry.register(kinds::side_tab(), SideTabWidget);
    registry.register(kinds::icon_button(), IconButtonWidget);
    registry.register(kinds::virtual_grid(), VirtualGridWidget);
    registry.register(kinds::viewport(), ViewportWidget);
    registry.register(kinds::panel(), PanelWidget);
    registry.register(kinds::text(), TextWidget);
    registry.register(kinds::slot(), SlotWidget);
    registry.register(kinds::slot_grid(), SlotGridWidget);
    registry.register(kinds::button(), ButtonWidget);
    registry.register(kinds::action_rail(), ActionRailWidget);
    registry.register(kinds::hotbar(), HotbarWidget);
    registry.register(kinds::tooltip(), TooltipWidget);
}

// ---------------------------------------------------------------------------
// Slot state roles
// ---------------------------------------------------------------------------

/// `SlottedUiSet::Render`: swap slot roles with hover, focus and carried
/// state. Focus wins over hover; carried wins over both.
pub fn slot_state_roles(
    focus: Option<Res<bevy::input_focus::InputFocus>>,
    carried: Query<&Carried>,
    mut slots: Query<(Entity, &SlotRef, &Hovered, &mut Themed), With<SemanticRole>>,
) {
    let focused = focus.and_then(|f| f.get());
    for (entity, slot_ref, hovered, mut themed) in &mut slots {
        let carrying = carried.get(slot_ref.menu).is_ok_and(|c| c.0.is_some());
        let role = if hovered.get() && carrying {
            roles::SLOT_CARRIED
        } else if focused == Some(entity) {
            roles::SLOT_FOCUS
        } else if hovered.get() {
            roles::SLOT_HOVER
        } else {
            roles::SLOT
        };
        if themed.0 != role {
            themed.0 = role;
        }
    }
}

/// `SlottedUiSet::Input`: digit keys 1-9 while a slot is hovered swap that
/// slot with the matching hotbar slot.
pub fn hotbar_swap_keys(
    keys: Res<ButtonInput<KeyCode>>,
    hovered: Query<(&SlotRef, &Hovered)>,
    mut commands: Commands,
) {
    const DIGITS: [KeyCode; 9] = [
        KeyCode::Digit1,
        KeyCode::Digit2,
        KeyCode::Digit3,
        KeyCode::Digit4,
        KeyCode::Digit5,
        KeyCode::Digit6,
        KeyCode::Digit7,
        KeyCode::Digit8,
        KeyCode::Digit9,
    ];
    let Some(hotbar) = DIGITS.iter().position(|k| keys.just_pressed(*k)) else {
        return;
    };
    for (slot_ref, hovered) in &hovered {
        if !hovered.get() {
            continue;
        }
        #[allow(clippy::cast_possible_truncation)]
        commands.trigger(MenuAction {
            entity: slot_ref.menu,
            action: ClickAction::Swap {
                slot: slot_ref.slot,
                hotbar: hotbar as u8,
            },
        });
    }
}

/// Reads the modifier keys a gesture was made with.
pub fn modifiers_from(keys: &ButtonInput<KeyCode>) -> Modifiers {
    Modifiers {
        shift: keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight),
        ctrl: keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight),
        alt: keys.pressed(KeyCode::AltLeft) || keys.pressed(KeyCode::AltRight),
    }
}

/// Translates a picking button to the model's.
pub fn model_button(button: bevy::picking::pointer::PointerButton) -> ModelButton {
    use bevy::picking::pointer::PointerButton;
    match button {
        PointerButton::Primary => ModelButton::Left,
        PointerButton::Secondary => ModelButton::Right,
        PointerButton::Middle => ModelButton::Middle,
    }
}

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

    #[test]
    fn rail_actions_resolve_against_the_two_inventories() {
        let p = ActionRailParams::default();
        assert_eq!(
            p.action("sort"),
            Some(ToolbarAction::Sort {
                inventory: InventoryRef::new(0)
            })
        );
        assert_eq!(
            p.action("quick_stack"),
            Some(ToolbarAction::QuickStack {
                from: InventoryRef::new(1),
                to: InventoryRef::new(0)
            })
        );
        assert_eq!(
            p.action("loot_all"),
            Some(ToolbarAction::LootAll {
                from: InventoryRef::new(0),
                to: InventoryRef::new(1)
            })
        );
        assert_eq!(p.action("nonsense"), None);
    }

    #[test]
    fn layout_multiplies_gap_and_padding_by_the_spacing_step() {
        let node = layout_node(
            &Layout {
                direction: LayoutDirection::Row,
                gap: 2.0,
                padding: 1.0,
                width: Some(320.0),
                height: None,
                center: true,
            },
            6.0,
        );
        assert_eq!(node.flex_direction, FlexDirection::Row);
        assert_eq!(node.row_gap, Val::Px(12.0));
        assert_eq!(node.padding.left, Val::Px(6.0));
        assert_eq!(node.width, Val::Px(320.0));
        assert_eq!(node.height, Val::Auto);
        assert_eq!(node.align_items, AlignItems::Center);
    }

    /// The conversion `params_of!` performs, so these tests exercise the path
    /// a widget actually reads its parameters through.
    fn read_params<T: serde::de::DeserializeOwned>(value: &Value) -> Result<T, String> {
        slotted_registry::to_model(value)
            .map_err(|e| e.to_string())
            .and_then(|v| slotted_model::from_value(v).map_err(|e| e.to_string()))
    }

    /// Params travel through the registry as an untyped `Value` and are read
    /// back through `slotted_model::from_value`.
    #[test]
    fn params_survive_the_untyped_value_round_trip() {
        let value: Value = ron::from_str("(first: 54, inventory: 2)").expect("parses");
        assert_eq!(
            read_params::<HotbarParams>(&value).expect("deserialises"),
            HotbarParams {
                first: 54,
                inventory: InventoryRef::new(2),
            }
        );
    }

    #[test]
    fn hotbar_params_parse_from_a_ron_value() {
        let value: Value = ron::from_str("(first: 54)").expect("value parses");
        assert_eq!(
            read_params::<HotbarParams>(&value).expect("params deserialise"),
            HotbarParams {
                first: 54,
                inventory: InventoryRef::new(2),
            }
        );
    }

    /// Phase 2 forbade an `Option` params field because `ron::Value::into_rust`
    /// accepted only a `Some(x)` wrapper the untyped payload had already lost,
    /// and a Lua table could not write one either. Phase 4 reads params through
    /// `slotted_model::from_value`, where a present value is `Some`, an absent
    /// key is `None` and both spellings of a data file agree.
    #[test]
    fn a_params_field_may_now_be_an_option() {
        #[derive(Debug, Default, PartialEq, Serialize, Deserialize)]
        struct Optional {
            #[serde(default)]
            label: Option<String>,
        }

        let bare: Value = ron::from_str(r#"(label: "Sort")"#).expect("parses");
        let wrapped: Value = ron::from_str(r#"(label: Some("Sort"))"#).expect("parses");
        // A wholly empty payload is `Value::Unit`, which `params_of!` answers
        // with the type's `Default`; this is a payload that has other keys and
        // simply left `label` out.
        let absent: Value = ron::from_str("(unrelated: 1)").expect("parses");
        let explicit_none: Value = ron::from_str("(label: None)").expect("parses");

        assert_eq!(
            read_params::<Optional>(&bare).expect("a bare value is Some"),
            Optional {
                label: Some("Sort".to_owned())
            }
        );
        assert_eq!(
            read_params::<Optional>(&wrapped).expect("a RON Some is Some"),
            Optional {
                label: Some("Sort".to_owned())
            }
        );
        assert_eq!(
            read_params::<Optional>(&absent).expect("an absent key is None"),
            Optional { label: None }
        );
        assert_eq!(
            read_params::<Optional>(&explicit_none).expect("a RON None is None"),
            Optional { label: None }
        );
    }
}
