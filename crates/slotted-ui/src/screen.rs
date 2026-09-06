//! From `ScreenDef` to entities, and back to nothing.

use std::collections::HashMap;
use std::sync::Arc;

use bevy::ecs::system::Command;
use bevy::input_focus::tab_navigation::TabGroup;
use bevy::prelude::*;
use bevy::ui::ui_transform::UiGlobalTransform;
use slotted_registry::Value;
use slotted_theme::{ActiveTheme, Theme, Tokens};

use crate::def::{AnchorId, ScreenDef, ScreenKind, UiNodeDef, WidgetKind};
use crate::layers::zbands;
use crate::semantic::{ScreenRoot, SemanticRole, TestId, WidgetNode};
use crate::widgets;

/// Every screen the app knows, by kind. Filled from Rust with
/// [`Screens::register`] and from the frozen registries' `screens` payloads
/// by the plugin at startup ([`Screens::load_from_registry`]).
#[derive(Resource, Default, Debug, Clone)]
pub struct Screens(pub HashMap<ScreenKind, Arc<ScreenDef>>);

impl Screens {
    /// Register or replace.
    pub fn register(&mut self, def: ScreenDef) -> Arc<ScreenDef> {
        let def = Arc::new(def);
        self.0.insert(def.kind.clone(), def.clone());
        def
    }

    /// Lookup.
    pub fn get(&self, kind: &ScreenKind) -> Option<&Arc<ScreenDef>> {
        self.0.get(kind)
    }

    /// Deserialises every `screens/*.ron` payload the registry kept as an
    /// untyped value. Malformed entries are logged and skipped.
    pub fn load_from_registry(&mut self, registries: &slotted_registry::FrozenRegistries) {
        for (_, name, raw) in registries.screens.iter() {
            match ScreenDef::from_value(raw.payload.clone()) {
                Ok(def) => {
                    self.register(def);
                }
                Err(e) => tracing::warn!(%name, %e, "screen payload is not a ScreenDef"),
            }
        }
    }

    /// `def` with its `inherits` chain flattened: the ancestor's tree with
    /// this screen's anchors kept. Cycles and unknown ancestors fall back to
    /// `def` itself.
    // Phase 2 screens do not inherit; see docs/design/phase2-notes-B.md item 11
    // for what has to be decided before this can be implemented.
    pub fn resolve(&self, def: &ScreenDef) -> ScreenDef {
        if def.inherits.is_some() {
            tracing::warn!(kind = ?def.kind, "screen inheritance is not implemented yet");
        }
        def.clone()
    }
}

/// A node another mod or crate adds to a screen it does not own.
#[derive(Debug, Clone, PartialEq)]
pub struct Injection {
    /// Which screen.
    pub target: ScreenKind,
    /// Where in it.
    pub anchor: AnchorId,
    /// What to add.
    pub node: UiNodeDef,
    /// Mark the spawned node as an [`crate::ExclusionZone`].
    pub exclusion: bool,
}

/// All registered injections. Consulted by [`spawn_screen`]; changing it
/// affects screens opened afterwards.
#[derive(Resource, Default, Debug, Clone)]
pub struct Injections(pub Vec<Injection>);

impl ScreenKind {
    /// The wildcard injection target: an `Injection` aimed at it lands on
    /// every screen that has the anchor (Phase 6). `slotted.inject("slotted:any", ..)`.
    pub fn any() -> Self {
        Self::new("slotted:any")
    }
}

impl Injections {
    /// Injections for one screen and anchor, in registration order. An
    /// injection targeting [`ScreenKind::any`] matches every screen.
    pub fn at<'a>(
        &'a self,
        target: &'a ScreenKind,
        anchor: &'a AnchorId,
    ) -> impl Iterator<Item = &'a Injection> + 'a {
        let any = ScreenKind::any();
        self.0
            .iter()
            .filter(move |i| (&i.target == target || i.target == any) && &i.anchor == anchor)
    }
}

/// What a widget sees while spawning.
pub struct SpawnCtx<'w> {
    /// Direct world access; the spawn runs as a `Command`.
    pub world: &'w mut World,
    /// Root entity of the screen being spawned.
    pub screen: Entity,
    /// Which screen.
    pub kind: ScreenKind,
    /// The `OpenMenu` entity, if the screen drives one.
    pub menu: Option<Entity>,
    /// The entity the new node must become a child of.
    pub parent: Entity,
}

impl SpawnCtx<'_> {
    /// The theme's token table, or the defaults when no theme has loaded.
    /// Widgets read spacing and radii from here; colours are never their
    /// business.
    pub fn tokens(&self) -> Tokens {
        let handle = self
            .world
            .get_resource::<ActiveTheme>()
            .map(|a| a.0.clone());
        handle
            .and_then(|h| {
                self.world
                    .get_resource::<Assets<Theme>>()
                    .and_then(|assets| assets.get(&h).map(|t| t.tokens.clone()))
            })
            .unwrap_or_default()
    }

    /// Spawns one entity as a child of `self.parent`. The way every widget
    /// makes its root.
    pub fn spawn_node(&mut self, bundle: impl Bundle) -> Entity {
        let parent = self.parent;
        self.world.spawn((bundle, ChildOf(parent))).id()
    }

    /// Spawns `defs` under `parent`, restoring `self.parent` afterwards.
    pub fn spawn_children(&mut self, parent: Entity, defs: &[UiNodeDef]) {
        let previous = std::mem::replace(&mut self.parent, parent);
        for def in defs {
            self.spawn_child(def);
        }
        self.parent = previous;
    }

    /// Spawns `def` under `self.parent` and returns its root entity. The one
    /// dispatch point: typed variants go to their built-in widget, `Custom`
    /// goes through the [`WidgetRegistry`].
    #[allow(clippy::too_many_lines)]
    pub fn spawn_child(&mut self, def: &UiNodeDef) -> Entity {
        let entity = match def {
            UiNodeDef::Panel {
                role,
                layout,
                children,
                ..
            } => widgets::spawn_panel(self, role, layout, children),
            UiNodeDef::Text { key, style, .. } => widgets::spawn_text(self, key, *style),
            UiNodeDef::Slot { slot, tags } => widgets::spawn_slot(self, *slot, tags, 0),
            UiNodeDef::SlotGrid {
                inventory,
                cols,
                rows,
                first,
                tags,
            } => widgets::spawn_slot_grid(
                self,
                *inventory,
                *cols,
                *rows,
                *first,
                tags,
                SemanticRole::Grid,
            ),
            UiNodeDef::Button { widget, .. } => widgets::spawn_button(self, widget, None),
            UiNodeDef::Anchor { id } => widgets::spawn_anchor(self, id),
            UiNodeDef::Custom {
                kind,
                params,
                children,
                ..
            } => self.spawn_custom(kind, params, children),
            UiNodeDef::Tank {
                property,
                capacity,
                orientation,
                fluid,
                fluid_property,
                unit,
                tags,
            } => widgets::tank::spawn_tank(
                self,
                &widgets::tank::TankParams {
                    property: *property,
                    capacity: *capacity,
                    orientation: *orientation,
                    fluid: fluid.clone(),
                    fluid_property: *fluid_property,
                    unit: unit.clone(),
                },
                tags,
            ),
            UiNodeDef::Bar {
                property,
                max,
                direction,
                text,
                tags,
            } => widgets::bar::spawn_bar(
                self,
                &widgets::bar::BarParams {
                    property: *property,
                    max: *max,
                    direction: *direction,
                    text: *text,
                },
                widgets::bar::BarStyle::Bar,
                tags,
            ),
            UiNodeDef::Progress {
                property,
                max,
                direction,
                tags,
            } => widgets::bar::spawn_bar(
                self,
                &widgets::bar::BarParams {
                    property: *property,
                    max: *max,
                    direction: *direction,
                    text: false,
                },
                widgets::bar::BarStyle::Progress,
                tags,
            ),
            UiNodeDef::SideTab {
                icon,
                side,
                label,
                open,
                children,
                tags,
            } => widgets::side_tab::spawn_side_tab(
                self,
                &widgets::side_tab::SideTabParams {
                    icon: icon.clone(),
                    side: *side,
                    label: label.clone(),
                    open: *open,
                },
                children,
                tags,
            ),
            UiNodeDef::IconButton {
                states,
                property,
                tags,
            } => widgets::icon_button::spawn_icon_button(
                self,
                &widgets::icon_button::IconButtonParams {
                    states: states.clone(),
                    property: *property,
                },
                tags,
            ),
            UiNodeDef::VirtualGrid {
                source,
                cols,
                rows,
                tags,
            } => widgets::virtual_grid::spawn_virtual_grid(
                self,
                &widgets::virtual_grid::VirtualGridParams {
                    source: source.clone(),
                    cols: *cols,
                    rows: *rows,
                },
                tags,
            ),
            UiNodeDef::Viewport {
                subject,
                size,
                tags,
            } => widgets::viewport::spawn_viewport(
                self,
                &widgets::viewport::ViewportParams {
                    subject: subject.clone(),
                    size: *size,
                },
                tags,
            ),
        };
        if let Some(tags) = def.tags() {
            if !tags.0.is_empty() {
                self.world.entity_mut(entity).insert(tags.clone());
            }
            if let Some(id) = tags.get(crate::def::Tags::TEST_ID) {
                self.world.entity_mut(entity).insert(TestId::new(id));
            }
        }
        entity
    }

    fn spawn_custom(
        &mut self,
        kind: &WidgetKind,
        params: &Value,
        children: &[UiNodeDef],
    ) -> Entity {
        let widget = self
            .world
            .get_resource::<WidgetRegistry>()
            .and_then(|r| r.get(kind).cloned());
        if let Some(widget) = widget {
            let entity = widget.spawn(self, params, children);
            self.world
                .entity_mut(entity)
                .insert(WidgetNode(kind.clone()));
            entity
        } else {
            tracing::warn!(?kind, "no widget registered for kind");
            self.spawn_node((
                Node::default(),
                SemanticRole::Custom(kind.0.to_string()),
                WidgetNode(kind.clone()),
            ))
        }
    }
}

/// A widget implementation, registered under a [`WidgetKind`].
pub trait Widget: Send + Sync {
    /// Spawns the widget's root under `ctx.parent` and returns it. The
    /// root must carry `Node`, `SemanticRole` and `WidgetNode(kind)`.
    fn spawn(&self, ctx: &mut SpawnCtx<'_>, params: &Value, children: &[UiNodeDef]) -> Entity;

    /// Extra tooltip parts for this widget, if any.
    fn tooltip(&self, _entity: Entity, _world: &World, _out: &mut Vec<UiNodeDef>) {}
}

/// Kind to implementation. Built-ins are registered by the plugin.
#[derive(Resource, Default, Clone)]
pub struct WidgetRegistry(pub HashMap<WidgetKind, Arc<dyn Widget>>);

impl WidgetRegistry {
    /// Register or replace.
    pub fn register(&mut self, kind: WidgetKind, widget: impl Widget + 'static) {
        self.0.insert(kind, Arc::new(widget));
    }

    /// Lookup.
    pub fn get(&self, kind: &WidgetKind) -> Option<&Arc<dyn Widget>> {
        self.0.get(kind)
    }
}

/// Anchors an [`Injection`] named that the spawned screen does not have, on
/// the screen root.
///
/// An injection is a mod reaching into a screen it does not own, so a typo in
/// the anchor id is the failure mode to expect. Dropping it silently leaves
/// the author with a screen that is simply missing their node; this records
/// what was dropped and the spawn logs it once.
#[derive(Component, Debug, Clone, Default, PartialEq, Eq)]
pub struct UnmatchedInjections(pub Vec<AnchorId>);

/// The screen root exists with its full tree. Layout has not run yet.
#[derive(EntityEvent, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScreenSpawned {
    /// The root.
    pub entity: Entity,
}

/// First layout after spawn is done; `rect` is the root panel in logical
/// pixels. Triggered from `PostUpdate` after `UiSystems::Layout`.
#[derive(EntityEvent, Debug, Clone, Copy, PartialEq)]
pub struct ScreenLayout {
    /// The root.
    pub entity: Entity,
    /// Panel rect.
    pub rect: Rect,
}

/// The screen is about to be despawned.
#[derive(EntityEvent, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScreenClosed {
    /// The root.
    pub entity: Entity,
}

/// The command [`spawn_screen`] queues. Reads [`Screens`], [`Injections`]
/// and [`WidgetRegistry`] from the world when it runs.
pub struct SpawnScreen {
    /// Pre-reserved root entity.
    pub root: Entity,
    /// The screen.
    pub def: Arc<ScreenDef>,
    /// The `OpenMenu` entity to bind slots to.
    pub menu: Option<Entity>,
}

impl Command for SpawnScreen {
    type Out = ();

    fn apply(self, world: &mut World) {
        let resolved = world
            .get_resource::<Screens>()
            .map_or_else(|| (*self.def).clone(), |s| s.resolve(&self.def));
        world.entity_mut(self.root).insert((
            Node {
                position_type: PositionType::Absolute,
                width: percent(100),
                height: percent(100),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            GlobalZIndex(zbands::SCREEN),
            Pickable::IGNORE,
            TabGroup::new(0),
            ScreenRoot {
                kind: resolved.kind.clone(),
                menu: self.menu,
            },
            SemanticRole::Screen,
        ));
        let mut ctx = SpawnCtx {
            world,
            screen: self.root,
            kind: resolved.kind.clone(),
            menu: self.menu,
            parent: self.root,
        };
        ctx.spawn_child(&resolved.root);
        report_unmatched_injections(world, self.root, &resolved.kind);
        world.trigger(ScreenSpawned { entity: self.root });
    }
}

/// Records, on the screen root, every injection for this screen whose anchor
/// the spawned tree does not contain.
fn report_unmatched_injections(world: &mut World, root: Entity, kind: &ScreenKind) {
    let wanted: Vec<AnchorId> = world
        .get_resource::<Injections>()
        .map(|i| {
            i.0.iter()
                .filter(|inj| &inj.target == kind || inj.target == ScreenKind::any())
                .map(|inj| inj.anchor.clone())
                .collect()
        })
        .unwrap_or_default();
    if wanted.is_empty() {
        return;
    }
    let mut present: Vec<AnchorId> = Vec::new();
    collect_anchors(world, root, &mut present);
    let mut missing: Vec<AnchorId> = wanted
        .into_iter()
        .filter(|anchor| !present.contains(anchor))
        .collect();
    missing.dedup();
    if missing.is_empty() {
        return;
    }
    for anchor in &missing {
        tracing::warn!(
            screen = %kind.0,
            anchor = %anchor.0,
            "injection targets an anchor this screen does not have"
        );
    }
    world.entity_mut(root).insert(UnmatchedInjections(missing));
}

fn collect_anchors(world: &World, entity: Entity, out: &mut Vec<AnchorId>) {
    if let Some(anchor) = world.get::<crate::semantic::AnchorNode>(entity) {
        out.push(anchor.0.clone());
    }
    let children: Vec<Entity> = world
        .get::<Children>(entity)
        .map(|c| c.iter().collect())
        .unwrap_or_default();
    for child in children {
        collect_anchors(world, child, out);
    }
}

/// Marks a screen root whose [`ScreenLayout`] has already been triggered.
#[derive(Component, Debug, Default, Clone, Copy)]
pub struct ScreenLaidOut;

/// `SlottedUiSet::Layout`: triggers [`ScreenLayout`] once per screen root,
/// the first frame its panel has a size. The rect is the root's first laid-out
/// child, not the full-window centring root.
pub fn emit_screen_layout(
    roots: Query<(Entity, &Children), (With<ScreenRoot>, Without<ScreenLaidOut>)>,
    nodes: Query<(&ComputedNode, &UiGlobalTransform)>,
    mut commands: Commands,
) {
    for (root, children) in &roots {
        let Some((node, transform)) = children.iter().find_map(|c| nodes.get(c).ok()) else {
            continue;
        };
        let size = node.size() * node.inverse_scale_factor();
        if size.x <= 0.0 || size.y <= 0.0 {
            continue;
        }
        let center = transform.translation * node.inverse_scale_factor();
        commands.entity(root).insert(ScreenLaidOut);
        commands.trigger(ScreenLayout {
            entity: root,
            rect: Rect::from_center_size(center, size),
        });
    }
}

/// Spawns `def` and returns its root entity immediately; the tree exists
/// once commands apply. `menu` binds `Slot` and `SlotGrid` nodes through
/// `SlotRef`; pass `None` for menu-less screens (a settings page).
pub fn spawn_screen(commands: &mut Commands, def: Arc<ScreenDef>, menu: Option<Entity>) -> Entity {
    let root = commands.spawn_empty().id();
    commands.queue(SpawnScreen { root, def, menu });
    root
}

/// Triggers [`ScreenClosed`] and despawns the tree. Does not close the menu;
/// call `slotted_ecs::close_menu` for that.
pub fn close_screen(commands: &mut Commands, root: Entity) {
    commands.trigger(ScreenClosed { entity: root });
    commands.entity(root).despawn();
}
