//! From `ScreenDef` to entities, and back to nothing.

use std::collections::HashMap;
use std::sync::Arc;

use bevy::ecs::system::Command;
use bevy::input_focus::tab_navigation::TabGroup;
use bevy::prelude::*;
use bevy::ui::ui_transform::UiGlobalTransform;
use slotted_registry::Value;
use slotted_theme::{ActiveTheme, Theme, Tokens};

use crate::def::{AnchorId, ScreenDef, ScreenKind, Tags, UiNodeDef, WidgetKind};
use crate::invalidate::{Owner, Reconciled};
use crate::layers::zbands;
use crate::semantic::{ScreenRoot, SemanticRole, TestId, WidgetNode};
use crate::widgets;

/// Every screen the app knows, by kind, and who registered each one.
///
/// Filled from Rust with [`Screens::register`], from `*.screen.ron` assets,
/// and from the frozen registries' `screens` payloads by the pack loader
/// ([`Screens::load_from_registry`]).
///
/// Each entry carries an [`Owner`], which is what lets a mod reload take its
/// own screens back out again -- see [`Screens::reconcile_mods`]. Entries the
/// game registered itself are never removed on a mod's behalf.
#[derive(Resource, Default, Debug, Clone)]
pub struct Screens {
    defs: HashMap<ScreenKind, Arc<ScreenDef>>,
    owners: HashMap<ScreenKind, Owner>,
    /// Game-owned definitions a mod took over, kept so removing the mod's
    /// registration restores the game's rather than leaving the kind unknown.
    shadowed: HashMap<ScreenKind, Arc<ScreenDef>>,
}

impl Screens {
    /// Register or replace, as the game's own ([`Owner::Game`]).
    pub fn register(&mut self, def: ScreenDef) -> Arc<ScreenDef> {
        self.register_owned(def, Owner::Game)
    }

    /// Register or replace, recording who registered it.
    pub fn register_owned(&mut self, def: ScreenDef, owner: Owner) -> Arc<ScreenDef> {
        let def = Arc::new(def);
        self.insert_arc(def.clone(), owner);
        def
    }

    fn insert_arc(&mut self, def: Arc<ScreenDef>, owner: Owner) {
        let kind = def.kind.clone();
        if owner.is_mod()
            && self.owners.get(&kind) == Some(&Owner::Game)
            && let Some(previous) = self.defs.get(&kind)
        {
            self.shadowed.insert(kind.clone(), previous.clone());
        }
        if !owner.is_mod() {
            self.shadowed.remove(&kind);
        }
        self.owners.insert(kind.clone(), owner);
        self.defs.insert(kind, def);
    }

    /// Lookup.
    pub fn get(&self, kind: &ScreenKind) -> Option<&Arc<ScreenDef>> {
        self.defs.get(kind)
    }

    /// Who registered `kind`.
    pub fn owner(&self, kind: &ScreenKind) -> Option<&Owner> {
        self.owners.get(kind)
    }

    /// Every registered screen.
    pub fn iter(&self) -> impl Iterator<Item = (&ScreenKind, &Arc<ScreenDef>)> {
        self.defs.iter()
    }

    /// Every registered kind.
    pub fn kinds(&self) -> impl Iterator<Item = &ScreenKind> {
        self.defs.keys()
    }

    /// How many screens are registered.
    #[must_use]
    pub fn len(&self) -> usize {
        self.defs.len()
    }

    /// Whether nothing is registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.defs.is_empty()
    }

    /// Unregisters `kind`, whoever owns it. Returns what was there.
    ///
    /// This does not touch anything already on screen; run the removal
    /// through [`crate::invalidate_and_respawn`] to close it.
    pub fn remove(&mut self, kind: &ScreenKind) -> Option<Arc<ScreenDef>> {
        self.owners.remove(kind);
        self.shadowed.remove(kind);
        self.defs.remove(kind)
    }

    /// Replaces exactly the mod-owned set with `defs`, leaving every
    /// [`Owner::Game`] and [`Owner::Asset`] entry alone.
    ///
    /// This is the reload path. A kind a mod registered last time and does not
    /// register now is removed -- or, when a game registration was shadowed by
    /// it, restored to the game's definition. The result names the kinds whose
    /// definition actually moved and the kinds that went away, which is what a
    /// [`ChangeSet`](crate::ChangeSet) wants.
    pub fn reconcile_mods(&mut self, defs: Vec<(Owner, ScreenDef)>) -> Reconciled<ScreenKind> {
        let previous: HashMap<ScreenKind, Arc<ScreenDef>> = self
            .owners
            .iter()
            .filter(|(_, owner)| owner.is_mod())
            .filter_map(|(kind, _)| Some((kind.clone(), self.defs.get(kind)?.clone())))
            .collect();
        let mut changed = Vec::new();
        let mut seen: Vec<ScreenKind> = Vec::new();
        for (owner, def) in defs {
            let kind = def.kind.clone();
            let differs = self.defs.get(&kind).is_none_or(|old| **old != def);
            self.insert_arc(Arc::new(def), owner);
            seen.push(kind.clone());
            if differs {
                changed.push(kind);
            }
        }
        let mut removed = Vec::new();
        for kind in previous.keys() {
            if seen.contains(kind) {
                continue;
            }
            if let Some(game) = self.shadowed.remove(kind) {
                self.owners.insert(kind.clone(), Owner::Game);
                self.defs.insert(kind.clone(), game);
                changed.push(kind.clone());
            } else {
                self.owners.remove(kind);
                self.defs.remove(kind);
                removed.push(kind.clone());
            }
        }
        changed.sort_by_key(|kind| kind.0.to_string());
        removed.sort_by_key(|kind| kind.0.to_string());
        Reconciled { changed, removed }
    }

    /// Deserialises every `screens/*.ron` payload the registry kept as an
    /// untyped value, as the owning mod's. Malformed entries are logged and
    /// skipped, and the mod-owned set is reconciled, so a screen a mod stopped
    /// shipping is unregistered rather than left behind.
    pub fn load_from_registry(
        &mut self,
        registries: &slotted_registry::FrozenRegistries,
    ) -> Reconciled<ScreenKind> {
        // The frozen entry does not record which mod wrote it, but a
        // registry name is namespaced and the namespace *is* the mod id in
        // every case a mod is allowed to register under (the data stage warns
        // about the others), so that is the owner.
        let mut defs = Vec::new();
        for (_, name, raw) in registries.screens.iter() {
            match ScreenDef::from_value(raw.payload.clone()) {
                Ok(def) => defs.push((Owner::Mod(name.namespace().to_owned()), def)),
                Err(e) => tracing::warn!(%name, %e, "screen payload is not a ScreenDef"),
            }
        }
        self.reconcile_mods(defs)
    }

    /// `def` with its `inherits` chain flattened: the ancestor's tree with
    /// this screen's overrides merged onto it.
    ///
    /// The merge identity is [`UiNodeDef::id`] -- a node's `test_id` tag, or
    /// an anchor's id. Walking the child's tree:
    ///
    /// * a node whose id names a node anywhere in the ancestor's tree
    ///   *replaces* that node, subtree and all (this is how a child fills an
    ///   ancestor's `anchor`);
    /// * a node with an unseen id, or with no id at all, is *appended* under
    ///   the ancestor node its own parent corresponds to;
    /// * the child's root always wins on shape (role, layout, tags), and its
    ///   children merge onto the ancestor root's children;
    /// * ids listed in the child's [`ScreenDef::remove`] are deleted from the
    ///   merged tree afterwards.
    ///
    /// `kind` is always the child's. `listring` is the child's when it is
    /// non-empty, otherwise the ancestor's.
    ///
    /// A cycle, an ancestor no one registered, or a chain longer than
    /// [`MAX_INHERIT_DEPTH`] logs one error naming the kinds and falls back to
    /// `def` itself, so a broken data file costs a plain screen rather than a
    /// panic.
    pub fn resolve(&self, def: &ScreenDef) -> ScreenDef {
        if def.inherits.is_none() {
            return def.clone();
        }
        // Leaf first; the fold below walks it back down.
        let mut chain: Vec<ScreenDef> = vec![def.clone()];
        let mut seen: Vec<ScreenKind> = vec![def.kind.clone()];
        let mut next = def.inherits.clone();
        while let Some(kind) = next.take() {
            if seen.contains(&kind) {
                tracing::error!(
                    screen = %def.kind.0,
                    ancestor = %kind.0,
                    chain = %kinds(&seen),
                    "screen inheritance cycle; using the screen's own tree"
                );
                return def.clone();
            }
            let Some(parent) = self.get(&kind) else {
                tracing::error!(
                    screen = %def.kind.0,
                    ancestor = %kind.0,
                    chain = %kinds(&seen),
                    "screen inherits a kind no one registered; using the screen's own tree"
                );
                return def.clone();
            };
            seen.push(kind);
            chain.push((**parent).clone());
            if chain.len() > MAX_INHERIT_DEPTH {
                tracing::error!(
                    screen = %def.kind.0,
                    limit = MAX_INHERIT_DEPTH,
                    chain = %kinds(&seen),
                    "screen inheritance is deeper than the limit; using the screen's own tree"
                );
                return def.clone();
            }
            next.clone_from(&parent.inherits);
        }
        let mut resolved = chain.pop().expect("the chain holds at least `def`");
        while let Some(child) = chain.pop() {
            resolved = merge_screens(&resolved, &child);
        }
        resolved
    }
}

/// How many `inherits` links [`Screens::resolve`] follows before it treats the
/// chain as a data error.
pub const MAX_INHERIT_DEPTH: usize = 8;

fn kinds(chain: &[ScreenKind]) -> String {
    chain
        .iter()
        .map(|k| k.0.to_string())
        .collect::<Vec<_>>()
        .join(" -> ")
}

/// One inheritance step: `child`'s overrides onto `base`'s tree.
fn merge_screens(base: &ScreenDef, child: &ScreenDef) -> ScreenDef {
    let mut root = merge_nodes(&base.root, &child.root);
    for id in &child.remove {
        if !remove_node(&mut root, id) {
            tracing::warn!(
                screen = %child.kind.0,
                node = %id,
                "screen `remove` names a node the inherited tree does not have"
            );
        }
    }
    ScreenDef {
        kind: child.kind.clone(),
        // Flattened: the result must not be resolved a second time.
        inherits: None,
        root,
        listring: if child.listring.is_empty() {
            base.listring.clone()
        } else {
            child.listring.clone()
        },
        remove: Vec::new(),
    }
}

/// `over`'s shape with `base`'s children underneath it, `over`'s own children
/// merged in by id.
fn merge_nodes(base: &UiNodeDef, over: &UiNodeDef) -> UiNodeDef {
    let mut merged = over.clone();
    let mut children = base.children().to_vec();
    for node in over.children() {
        let replaced = node
            .id()
            .is_some_and(|id| replace_node(&mut children, id, node));
        if !replaced {
            children.push(node.clone());
        }
    }
    if let Some(slot) = merged.children_mut() {
        *slot = children;
    }
    merged
}

/// Replaces the first node in `list` (at any depth) whose id is `id`.
fn replace_node(list: &mut [UiNodeDef], id: &str, node: &UiNodeDef) -> bool {
    for existing in list.iter_mut() {
        if existing.id() == Some(id) {
            *existing = node.clone();
            return true;
        }
        if let Some(children) = existing.children_mut()
            && replace_node(children, id, node)
        {
            return true;
        }
    }
    false
}

/// Deletes every node under `root` whose id is `id`. The root itself is never
/// removed: a screen with no tree is not a screen.
fn remove_node(root: &mut UiNodeDef, id: &str) -> bool {
    let Some(children) = root.children_mut() else {
        return false;
    };
    let before = children.len();
    children.retain(|c| c.id() != Some(id));
    let mut found = children.len() != before;
    for child in children.iter_mut() {
        found |= remove_node(child, id);
    }
    found
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
    /// Who registered it. A reload replaces exactly the mod-owned
    /// injections; see [`crate::reconcile_mod_injections`].
    pub owner: Owner,
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

/// The theme's token table as seen from a `&World`, or the defaults when no
/// theme has loaded.
///
/// The `&World` form of [`SpawnCtx::tokens`], for the exclusive systems that
/// spawn or resize nodes outside a spawn context.
pub fn active_tokens(world: &World) -> Tokens {
    world
        .get_resource::<ActiveTheme>()
        .map(|a| a.0.clone())
        .and_then(|h| {
            world
                .get_resource::<Assets<Theme>>()
                .and_then(|assets| assets.get(&h).map(|t| t.tokens.clone()))
        })
        .unwrap_or_default()
}

impl SpawnCtx<'_> {
    /// The theme's token table, or the defaults when no theme has loaded.
    /// Widgets read spacing and radii from here; colours are never their
    /// business.
    pub fn tokens(&self) -> Tokens {
        active_tokens(self.world)
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
                // Merged, not replaced: a widget may already have written
                // tags of its own (a slot grid's `region`, an icon button's
                // `state`), and the def's tags are an addition to those, not
                // a replacement for them. Same key: the def wins.
                let mut merged = self.world.get::<Tags>(entity).cloned().unwrap_or_default();
                merged.0.extend(tags.0.clone());
                self.world.entity_mut(entity).insert(merged);
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

/// Kind to implementation, and who registered each. Built-ins are registered
/// by the plugin as [`Owner::Game`].
///
/// A mod's `RegisterWidget` puts a template here under [`Owner::Mod`]; the
/// reload path replaces exactly that set, so a template a mod stopped
/// shipping stops resolving instead of outliving it.
#[derive(Resource, Default, Clone)]
pub struct WidgetRegistry {
    widgets: HashMap<WidgetKind, Arc<dyn Widget>>,
    owners: HashMap<WidgetKind, Owner>,
}

impl WidgetRegistry {
    /// Register or replace, as the game's own.
    pub fn register(&mut self, kind: WidgetKind, widget: impl Widget + 'static) {
        self.register_owned(kind, widget, Owner::Game);
    }

    /// Register or replace, recording who registered it.
    pub fn register_owned(
        &mut self,
        kind: WidgetKind,
        widget: impl Widget + 'static,
        owner: Owner,
    ) {
        self.owners.insert(kind.clone(), owner);
        self.widgets.insert(kind, Arc::new(widget));
    }

    /// Lookup.
    pub fn get(&self, kind: &WidgetKind) -> Option<&Arc<dyn Widget>> {
        self.widgets.get(kind)
    }

    /// Who registered `kind`.
    pub fn owner(&self, kind: &WidgetKind) -> Option<&Owner> {
        self.owners.get(kind)
    }

    /// Every registered kind.
    pub fn kinds(&self) -> impl Iterator<Item = &WidgetKind> {
        self.widgets.keys()
    }

    /// Unregisters `kind`.
    pub fn remove(&mut self, kind: &WidgetKind) {
        self.owners.remove(kind);
        self.widgets.remove(kind);
    }

    /// Replaces exactly the mod-owned templates with `next`, leaving the
    /// built-ins and the game's own registrations alone. The result names the
    /// kinds a screen using them has to be respawned for.
    pub fn reconcile_mods(
        &mut self,
        next: Vec<(WidgetKind, Arc<dyn Widget>, Owner)>,
    ) -> Reconciled<WidgetKind> {
        let before: Vec<WidgetKind> = self
            .owners
            .iter()
            .filter(|(_, owner)| owner.is_mod())
            .map(|(kind, _)| kind.clone())
            .collect();
        let mut changed = Vec::new();
        for (kind, widget, owner) in next {
            self.owners.insert(kind.clone(), owner);
            self.widgets.insert(kind.clone(), widget);
            changed.push(kind);
        }
        let mut removed = Vec::new();
        for kind in before {
            if changed.contains(&kind) {
                continue;
            }
            self.owners.remove(&kind);
            self.widgets.remove(&kind);
            removed.push(kind);
        }
        changed.sort_by_key(|kind| kind.0.to_string());
        removed.sort_by_key(|kind| kind.0.to_string());
        Reconciled { changed, removed }
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
    // Deduplicated by `contains` rather than by `Vec::dedup`, which only
    // collapses *adjacent* equal entries: two mods aiming at the same missing
    // anchor with a third injection registered between them would otherwise be
    // recorded twice and logged twice. Registration order is kept, because it
    // is the order the author will read the log in.
    let mut missing: Vec<AnchorId> = Vec::new();
    for anchor in wanted {
        if !present.contains(&anchor) && !missing.contains(&anchor) {
            missing.push(anchor);
        }
    }
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

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::def::{LocKey, TextRole};
    use pretty_assertions::assert_eq;
    use slotted_theme::roles;

    /// A text node whose `test_id` is its own text, so the merged tree reads
    /// as a list of ids.
    fn node(id: &str) -> UiNodeDef {
        UiNodeDef::Text {
            key: LocKey(id.to_owned()),
            style: TextRole::Body,
            tags: Tags::new().with(Tags::TEST_ID, id),
        }
    }

    fn panel(id: &str, children: Vec<UiNodeDef>) -> UiNodeDef {
        UiNodeDef::Panel {
            role: roles::PANEL,
            layout: crate::def::Layout::default(),
            children,
            tags: Tags::new().with(Tags::TEST_ID, id),
        }
    }

    fn screen(kind: &str, inherits: Option<&str>, root: UiNodeDef) -> ScreenDef {
        ScreenDef {
            kind: ScreenKind::new(kind),
            inherits: inherits.map(ScreenKind::new),
            root,
            listring: vec![],
            remove: vec![],
        }
    }

    /// Ids of a tree, depth first, so an assertion reads as a shape.
    fn ids(def: &ScreenDef) -> Vec<String> {
        let mut out = Vec::new();
        def.root.walk(&mut |n| {
            if let Some(id) = n.id() {
                out.push(id.to_owned());
            }
        });
        out
    }

    fn registered(defs: Vec<ScreenDef>) -> Screens {
        let mut screens = Screens::default();
        for def in defs {
            screens.register(def);
        }
        screens
    }

    #[test]
    fn a_three_deep_chain_accumulates_every_ancestors_children() {
        let screens = registered(vec![
            screen("demo:base", None, panel("root", vec![node("from_base")])),
            screen(
                "demo:middle",
                Some("demo:base"),
                panel("root", vec![node("from_middle")]),
            ),
            screen(
                "demo:leaf",
                Some("demo:middle"),
                panel("root", vec![node("from_leaf")]),
            ),
        ]);
        let leaf = screens.get(&ScreenKind::new("demo:leaf")).unwrap();
        let resolved = screens.resolve(leaf);

        assert_eq!(resolved.kind, ScreenKind::new("demo:leaf"));
        assert_eq!(resolved.inherits, None);
        assert_eq!(
            ids(&resolved),
            ["root", "from_base", "from_middle", "from_leaf"]
        );
    }

    #[test]
    fn a_child_node_replaces_the_ancestor_node_of_the_same_id_in_place() {
        let screens = registered(vec![
            screen(
                "demo:base",
                None,
                panel(
                    "root",
                    vec![
                        node("before"),
                        panel(
                            "box",
                            vec![UiNodeDef::Anchor {
                                id: AnchorId::new("rail"),
                            }],
                        ),
                        node("after"),
                    ],
                ),
            ),
            screen(
                "demo:leaf",
                Some("demo:base"),
                panel("root", vec![panel("rail", vec![node("rail_button")])]),
            ),
        ]);
        let leaf = screens.get(&ScreenKind::new("demo:leaf")).unwrap();
        let resolved = screens.resolve(leaf);

        // The anchor kept its place two levels down; nothing was appended.
        assert_eq!(
            ids(&resolved),
            ["root", "before", "box", "rail", "rail_button", "after"]
        );
    }

    #[test]
    fn remove_deletes_an_inherited_node_at_any_depth() {
        let mut leaf = screen("demo:leaf", Some("demo:base"), panel("root", vec![]));
        leaf.remove = vec!["unwanted".to_owned()];
        let screens = registered(vec![
            screen(
                "demo:base",
                None,
                panel(
                    "root",
                    vec![panel("box", vec![node("unwanted"), node("kept")])],
                ),
            ),
            leaf,
        ]);
        let leaf = screens.get(&ScreenKind::new("demo:leaf")).unwrap();
        let resolved = screens.resolve(leaf);

        assert_eq!(ids(&resolved), ["root", "box", "kept"]);
    }

    #[test]
    fn a_cycle_falls_back_to_the_screens_own_tree() {
        let screens = registered(vec![
            screen("demo:a", Some("demo:b"), panel("a_root", vec![node("a")])),
            screen("demo:b", Some("demo:a"), panel("b_root", vec![node("b")])),
        ]);
        let a = screens.get(&ScreenKind::new("demo:a")).unwrap();
        let resolved = screens.resolve(a);

        assert_eq!(resolved, **a, "a cycle is a data error, not a panic");
    }

    #[test]
    fn an_unknown_ancestor_falls_back_to_the_screens_own_tree() {
        let screens = registered(vec![screen(
            "demo:leaf",
            Some("demo:missing"),
            panel("root", vec![node("own")]),
        )]);
        let leaf = screens.get(&ScreenKind::new("demo:leaf")).unwrap();

        assert_eq!(screens.resolve(leaf), **leaf);
    }

    #[test]
    fn the_child_wins_on_kind_and_on_a_non_empty_listring() {
        let mut base = screen("demo:base", None, panel("root", vec![]));
        base.listring = vec![
            slotted_model::InventoryRef::new(0),
            slotted_model::InventoryRef::new(1),
        ];
        let leaf = screen("demo:leaf", Some("demo:base"), panel("root", vec![]));
        let mut overriding = screen("demo:other", Some("demo:base"), panel("root", vec![]));
        overriding.listring = vec![slotted_model::InventoryRef::new(2)];
        let screens = registered(vec![base, leaf, overriding]);

        let leaf = screens.resolve(screens.get(&ScreenKind::new("demo:leaf")).unwrap());
        assert_eq!(
            leaf.listring,
            vec![
                slotted_model::InventoryRef::new(0),
                slotted_model::InventoryRef::new(1)
            ]
        );

        let other = screens.resolve(screens.get(&ScreenKind::new("demo:other")).unwrap());
        assert_eq!(other.kind, ScreenKind::new("demo:other"));
        assert_eq!(other.listring, vec![slotted_model::InventoryRef::new(2)]);
    }
}
