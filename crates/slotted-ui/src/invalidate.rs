//! Who owns a registered definition, what a screen depends on, and the one
//! respawn implementation.
//!
//! Three registries in this crate are shared between the game and whatever
//! mods are loaded: [`Screens`], [`Injections`] and
//! [`WidgetRegistry`](crate::WidgetRegistry). Before this module a mod reload
//! could only *add* to them, so a screen a mod stopped registering stayed
//! registered forever, and an injection a mod stopped declaring stayed
//! spliced into every screen opened afterwards. [`Owner`] is what makes
//! removal possible: every entry says who put it there, and a reload
//! reconciles exactly its own set.
//!
//! The second half is invalidation. A screen is not an island: it may
//! `inherits` another, it spawns widget kinds that a template can define, and
//! injections aim at it by name or through [`ScreenKind::any`]. Respawning
//! only the screens whose own definition changed therefore missed most of the
//! ways an edit can reach a screen. [`ScreenDependencies`] indexes those three
//! relations, [`ScreenDependencies::invalidate`] turns a [`ChangeSet`] into
//! the kinds actually affected, and [`respawn_screens`] is the single
//! implementation every caller uses: the `*.screen.ron` watcher, the mod
//! reload, and a test harness driving either by hand.

use std::collections::{BTreeSet, HashMap};

use bevy::prelude::*;

use crate::def::{AnchorId, ScreenDef, ScreenKind, UiNodeDef, WidgetKind};
use crate::screen::{Injection, Injections, Screens, close_screen, spawn_screen};
use crate::semantic::ScreenRoot;
use crate::stack::{ScreenStack, push_screen_at};

/// Who registered an entry in one of the shared UI registries.
///
/// Ownership decides what a reload may take back out. A reload reconciles
/// only the entries owned by mods; a [`Owner::Game`] entry is the game's own
/// Rust registration and is never removed on its behalf.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub enum Owner {
    /// Registered from the game's own Rust code. Never removed by a reload.
    #[default]
    Game,
    /// Registered by a mod's data or control stage. The string is the mod id
    /// as the pack loader spells it.
    Mod(String),
    /// Loaded from an asset, by its path. Replaced when the file changes.
    Asset(String),
}

impl Owner {
    /// Whether a reload of the pack set may remove this entry.
    #[must_use]
    pub fn is_mod(&self) -> bool {
        matches!(self, Self::Mod(_))
    }

    /// The mod id, when this is a mod's entry.
    #[must_use]
    pub fn mod_id(&self) -> Option<&str> {
        match self {
            Self::Mod(id) => Some(id.as_str()),
            _ => None,
        }
    }
}

/// What an edit changed, as the input to
/// [`ScreenDependencies::invalidate`].
///
/// Every field is optional in the sense that an empty one contributes
/// nothing; a caller fills in the parts of the world it actually touched.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ChangeSet {
    /// Screen kinds whose definition was added or replaced.
    pub screens: BTreeSet<ScreenKind>,
    /// Screen kinds whose definition is gone. An open screen of one of these
    /// closes rather than respawning.
    pub removed_screens: BTreeSet<ScreenKind>,
    /// Widget kinds whose template was added, replaced or removed.
    pub templates: BTreeSet<WidgetKind>,
    /// Injections that were not registered before and are now.
    pub injections_added: Vec<Injection>,
    /// Injections that were registered before and are not now. These matter
    /// as much as the additions: a screen spliced with an injection that has
    /// gone away is still showing it.
    pub injections_removed: Vec<Injection>,
}

impl ChangeSet {
    /// A change set naming only these screens.
    #[must_use]
    pub fn screens(kinds: impl IntoIterator<Item = ScreenKind>) -> Self {
        Self {
            screens: kinds.into_iter().collect(),
            ..Self::default()
        }
    }

    /// Whether this names nothing at all, so invalidation has no work.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.screens.is_empty()
            && self.removed_screens.is_empty()
            && self.templates.is_empty()
            && self.injections_added.is_empty()
            && self.injections_removed.is_empty()
    }
}

/// What each registered screen depends on, built from [`Screens`].
///
/// Built fresh for each invalidation rather than kept as a resource: the
/// index is a walk over every registered screen's resolved tree, which is
/// cheap next to the respawn it is deciding, and a cached copy would be one
/// more piece of duplicated state to keep in step.
#[derive(Debug, Clone, Default)]
pub struct ScreenDependencies {
    /// Kind to every kind it inherits from, transitively.
    ancestors: HashMap<ScreenKind, BTreeSet<ScreenKind>>,
    /// Kind to every widget kind its resolved tree spawns.
    templates: HashMap<ScreenKind, BTreeSet<WidgetKind>>,
    /// Kind to every anchor id its resolved tree contains, which is what a
    /// [`ScreenKind::any`] injection matches on.
    anchors: HashMap<ScreenKind, BTreeSet<AnchorId>>,
}

impl ScreenDependencies {
    /// Indexes every screen in `screens`.
    #[must_use]
    pub fn build(screens: &Screens) -> Self {
        let mut index = Self::default();
        for (kind, def) in screens.iter() {
            let resolved = screens.resolve(def);
            index
                .ancestors
                .insert(kind.clone(), ancestors_of(screens, def));
            let mut templates = BTreeSet::new();
            let mut anchors = BTreeSet::new();
            resolved.root.walk(&mut |node| match node {
                UiNodeDef::Anchor { id } => {
                    anchors.insert(id.clone());
                }
                UiNodeDef::Custom { kind, .. } | UiNodeDef::Button { widget: kind, .. } => {
                    templates.insert(kind.clone());
                }
                _ => {}
            });
            index.templates.insert(kind.clone(), templates);
            index.anchors.insert(kind.clone(), anchors);
        }
        index
    }

    /// Every kind `change` reaches, in a stable order.
    ///
    /// A kind is affected when its own definition changed or went away, when
    /// any kind it inherits from did, when one of the widget kinds its tree
    /// spawns has a changed template, or when an injection that was added or
    /// removed targets it -- by name, or through [`ScreenKind::any`] and an
    /// anchor the screen actually has.
    ///
    /// Kinds in [`ChangeSet::removed_screens`] are returned even though the
    /// index no longer knows them, so the caller can close what is open.
    #[must_use]
    pub fn invalidate(&self, change: &ChangeSet) -> Vec<ScreenKind> {
        let mut out: BTreeSet<ScreenKind> = change.removed_screens.clone();
        let gone_or_changed = |kind: &ScreenKind| {
            change.screens.contains(kind) || change.removed_screens.contains(kind)
        };
        for kind in self.ancestors.keys() {
            if gone_or_changed(kind) {
                out.insert(kind.clone());
                continue;
            }
            if self.ancestors[kind].iter().any(gone_or_changed) {
                out.insert(kind.clone());
                continue;
            }
            if self.templates[kind]
                .iter()
                .any(|widget| change.templates.contains(widget))
            {
                out.insert(kind.clone());
                continue;
            }
            let touched = change
                .injections_added
                .iter()
                .chain(change.injections_removed.iter());
            if touched.into_iter().any(|inj| self.targets(kind, inj)) {
                out.insert(kind.clone());
            }
        }
        out.into_iter().collect()
    }

    /// Whether `injection` lands on `kind`.
    fn targets(&self, kind: &ScreenKind, injection: &Injection) -> bool {
        if &injection.target == kind {
            return true;
        }
        // The wildcard reaches every ordinary screen that has the anchor. It
        // used not to match anything during invalidation, which is how an
        // `slotted:any` injection could be added and change nothing on screen
        // until the player closed and re-opened by hand.
        injection.target == ScreenKind::any()
            && self
                .anchors
                .get(kind)
                .is_some_and(|anchors| anchors.contains(&injection.anchor))
    }
}

/// Every kind `def` inherits from, transitively, stopping at a cycle or a
/// missing ancestor rather than looping.
fn ancestors_of(screens: &Screens, def: &ScreenDef) -> BTreeSet<ScreenKind> {
    let mut out = BTreeSet::new();
    let mut next = def.inherits.clone();
    while let Some(kind) = next.take() {
        if !out.insert(kind.clone()) {
            break;
        }
        if out.len() > crate::screen::MAX_INHERIT_DEPTH {
            break;
        }
        next = screens
            .get(&kind)
            .and_then(|parent| parent.inherits.clone());
    }
    out
}

/// A screen an invalidation closed because nothing defines its kind any more.
///
/// Written by [`respawn_screens`]. A mod that stops registering a screen the
/// player has open cannot leave it on screen -- the tree would keep drawing
/// from a definition no registry holds -- so it closes, and this message is
/// the report of that. The pack loader turns it into a mod log entry.
#[derive(Message, Debug, Clone, PartialEq, Eq)]
pub struct ScreenDropped {
    /// The screen root that was despawned.
    pub entity: Entity,
    /// The kind it was.
    pub kind: ScreenKind,
}

/// Closes and re-opens every open screen whose kind is in `kinds`, on the same
/// menu entity, so its slots re-seed without an inventory write.
///
/// Contract 2.6 step 4. This is the only respawn implementation in the
/// workspace: [`crate::screen_asset::apply_screen_assets`] calls it after a
/// `*.screen.ron` changed on disk and `slotted-packs` calls it after a mod
/// reload, both through [`invalidate_and_respawn`]. [`Screens`] must already
/// hold the new definitions.
///
/// A kind [`Screens`] no longer knows cannot respawn: that screen is closed
/// and a [`ScreenDropped`] written.
pub fn respawn_screens(world: &mut World, kinds: &[ScreenKind]) {
    if kinds.is_empty() {
        return;
    }
    let roots: Vec<(Entity, ScreenKind, Option<Entity>)> = world
        .query::<(Entity, &ScreenRoot)>()
        .iter(world)
        .filter(|(_, root)| kinds.contains(&root.kind))
        .map(|(entity, root)| (entity, root.kind.clone(), root.menu))
        .collect();
    if roots.is_empty() {
        return;
    }
    let screens = world.resource::<Screens>().clone();
    for (root, kind, menu) in roots {
        if let Some(def) = screens.get(&kind).cloned() {
            // A stacked root goes back in at the same position, so a hot
            // reload of an open screen does not drop it out of the stack
            // and `Back` keeps closing it. The close is the same for both:
            // `on_screen_closed` removes the entry.
            let stacked = world
                .get_resource::<ScreenStack>()
                .and_then(|stack| stack.entries().iter().position(|entry| entry.root == root));
            let mut commands = world.commands();
            close_screen(&mut commands, root);
            world.flush();
            if let Some(index) = stacked {
                push_screen_at(world, index, def, menu);
            } else {
                let mut commands = world.commands();
                spawn_screen(&mut commands, def, menu);
                world.flush();
            }
        } else {
            tracing::warn!(
                screen = %kind.0,
                "nothing registers this screen any more; closing the open one"
            );
            let mut commands = world.commands();
            close_screen(&mut commands, root);
            world.flush();
            world.write_message(ScreenDropped {
                entity: root,
                kind: kind.clone(),
            });
        }
    }
}

/// Indexes [`Screens`], asks it what `change` reaches, and respawns that.
///
/// The one entry point a caller with a change set wants. Returns the kinds it
/// acted on, which is what a test asserts against.
pub fn invalidate_and_respawn(world: &mut World, change: &ChangeSet) -> Vec<ScreenKind> {
    if change.is_empty() {
        return Vec::new();
    }
    let affected = {
        let screens = world.resource::<Screens>();
        ScreenDependencies::build(screens).invalidate(change)
    };
    respawn_screens(world, &affected);
    affected
}

/// Convenience for a caller that already knows the exact kinds, used by the
/// asset watcher and by tests: builds the change set and runs invalidation, so
/// an edit to a base screen still reaches the screens derived from it.
pub fn invalidate_screens(world: &mut World, changed: &[ScreenKind]) -> Vec<ScreenKind> {
    invalidate_and_respawn(world, &ChangeSet::screens(changed.iter().cloned()))
}

/// The difference a reconcile made to a registry, as the input to a
/// [`ChangeSet`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Reconciled<T> {
    /// Entries added or replaced by a different value.
    pub changed: Vec<T>,
    /// Entries the owner stopped registering.
    pub removed: Vec<T>,
}

impl<T> Reconciled<T> {
    /// Whether nothing moved.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.changed.is_empty() && self.removed.is_empty()
    }
}

/// Removes every mod-owned injection and installs `next` in its place,
/// reporting what actually moved.
///
/// Split out of [`Injections`] so the diff lives beside the invalidation that
/// consumes it.
pub fn reconcile_mod_injections(
    injections: &mut Injections,
    next: Vec<Injection>,
) -> Reconciled<Injection> {
    let mut before: Vec<Injection> = Vec::new();
    injections.0.retain(|existing| {
        if existing.owner.is_mod() {
            before.push(existing.clone());
            false
        } else {
            true
        }
    });
    injections.0.extend(next.iter().cloned());
    Reconciled {
        changed: next
            .iter()
            .filter(|inj| !before.contains(inj))
            .cloned()
            .collect(),
        removed: before
            .into_iter()
            .filter(|inj| !next.contains(inj))
            .collect(),
    }
}
