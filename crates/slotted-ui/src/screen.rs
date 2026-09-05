//! From `ScreenDef` to entities, and back to nothing.

use std::collections::HashMap;
use std::sync::Arc;

use bevy::ecs::system::Command;
use bevy::prelude::*;
use slotted_registry::Value;

use crate::def::{AnchorId, ScreenDef, ScreenKind, UiNodeDef, WidgetKind};
use crate::layers::zbands;
use crate::semantic::{ScreenRoot, SemanticRole, TestId};

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
    // PHASE2-IMPL: agent B. Phase 2 screens do not inherit; return a clone.
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

impl Injections {
    /// Injections for one screen and anchor, in registration order.
    pub fn at<'a>(
        &'a self,
        target: &'a ScreenKind,
        anchor: &'a AnchorId,
    ) -> impl Iterator<Item = &'a Injection> + 'a {
        self.0
            .iter()
            .filter(move |i| &i.target == target && &i.anchor == anchor)
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
    /// Spawns `def` under `self.parent` through the widget registry and
    /// returns its root entity. The way widgets spawn their children.
    // PHASE2-IMPL: agent B. Dispatch on `UiNodeDef`; `Custom` goes through
    // `WidgetRegistry`. Apply the mapping table in the contract.
    pub fn spawn_child(&mut self, def: &UiNodeDef) -> Entity {
        let entity = self
            .world
            .spawn((Node::default(), ChildOf(self.parent)))
            .id();
        if let Some(tags) = def.tags() {
            self.world.entity_mut(entity).insert(tags.clone());
            if let Some(id) = tags.get(crate::def::Tags::TEST_ID) {
                self.world.entity_mut(entity).insert(TestId::new(id));
            }
        }
        tracing::warn!(
            ?def,
            "spawn_child is not implemented yet; spawned a bare node"
        );
        entity
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

    // PHASE2-IMPL: agent B. Resolve inherits through `Screens`, splice
    // `Injections` at anchors, then walk the tree with `SpawnCtx::spawn_child`.
    fn apply(self, world: &mut World) {
        let resolved = world
            .get_resource::<Screens>()
            .map_or_else(|| (*self.def).clone(), |s| s.resolve(&self.def));
        world.entity_mut(self.root).insert((
            Node {
                width: percent(100),
                height: percent(100),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            GlobalZIndex(zbands::SCREEN),
            Pickable::IGNORE,
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
        world.trigger(ScreenSpawned { entity: self.root });
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
