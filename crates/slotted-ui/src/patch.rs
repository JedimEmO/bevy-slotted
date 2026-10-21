//! Keyed patching of an open screen (client-app review, F1, second half).
//!
//! `replace_screen` despawns a whole tree and spawns it again for any change
//! of a view: every node repainted, every text re-shaped, layout rebuilt,
//! focus re-resolved. [`patch_screen`] diffs the definition a screen was
//! built from against a new one and touches only what moved:
//!
//! * a node whose definition is equal is not touched;
//! * a text or rich text whose key or arguments moved is rewritten in place;
//! * the children of a panel, card or scroll are matched by `test_id` (nodes
//!   without one by kind and order), so a list that grew, shrank or reordered
//!   spawns and despawns only the rows that did;
//! * any other node that differs (a button, a slot grid, a custom widget) is
//!   respawned where it stood, since only its own widget knows its shape.
//!
//! Focus stays on the focused node's id where that id survives. The screen's
//! kind, presentation and list ring are fixed at spawn, so a patch that
//! changes one is a replace.

use std::collections::{HashMap, HashSet, VecDeque};
use std::mem::{Discriminant, discriminant};
use std::sync::Arc;

use bevy::ecs::system::Command;
use bevy::input_focus::{FocusCause, InputFocus};
use bevy::prelude::*;

use crate::def::{ScreenDef, ScreenKind, TextOpts, UiNodeDef};
use crate::screen::{Rec, ScreenTrace, Screens, SpawnCtx, spawn_traced};
use crate::semantic::{LocText, ScreenRoot, SemanticLabel, TestId};
use crate::stack::{ReplaceScreen, ScreenStack, restore_focus};
use crate::widgets::text::RichText;

/// Patches the screen at `root` to `def`, or replaces the top screen with it
/// when the two cannot share a tree (another kind, presentation or list
/// ring, or a root that was not spawned from a definition).
pub fn patch_screen(commands: &mut Commands, root: Entity, def: Arc<ScreenDef>) {
    commands.queue(PatchScreen { root, def });
}

/// The command behind [`patch_screen`].
pub struct PatchScreen {
    /// The screen root to patch.
    pub root: Entity,
    /// What it should show.
    pub def: Arc<ScreenDef>,
}

impl Command for PatchScreen {
    type Out = ();

    fn apply(self, world: &mut World) {
        if try_patch(world, self.root, &self.def) {
            return;
        }
        let menu = world
            .resource::<ScreenStack>()
            .entry(self.root)
            .and_then(|e| e.menu);
        let root = world.spawn_empty().id();
        ReplaceScreen {
            root,
            def: self.def,
            menu,
        }
        .apply(world);
    }
}

/// How many entities the last patch of each kind spawned and despawned.
/// Public for tests and probes: a patch to an equal definition reads 0, 0.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PatchStats {
    /// Nodes spawned (subtree roots, as the patcher counts them).
    pub spawned: u32,
    /// Nodes despawned (subtree roots).
    pub despawned: u32,
    /// Texts rewritten in place.
    pub rewritten: u32,
}

fn try_patch(world: &mut World, root: Entity, def: &ScreenDef) -> bool {
    let resolved = world
        .get_resource::<Screens>()
        .map_or_else(|| def.clone(), |s| s.resolve(def));
    let Some(trace) = world.get::<ScreenTrace>(root) else {
        return false;
    };
    let old = trace.def.clone();
    if old.kind != resolved.kind
        || old.presentation != resolved.presentation
        || old.listring != resolved.listring
    {
        return false;
    }
    let Some(menu) = world.get::<ScreenRoot>(root).map(|r| r.menu) else {
        return false;
    };
    let Some(trace) = world.entity_mut(root).take::<ScreenTrace>() else {
        return false;
    };

    let focus = focus_anchor(world, root);
    // A screen that moves its own focus (a press that leads on to the next
    // control) says so by its `initial_focus`, which a replace always
    // obeyed; a patch obeys it the same, and a screen that names none
    // keeps the focus where the player put it.
    let hint_now = resolved.initial_focus.clone();
    let mut patcher = Patcher {
        world,
        root,
        kind: resolved.kind.clone(),
        menu,
        stats: PatchStats::default(),
    };
    let node = patcher.patch_node(&old.root, &resolved.root, trace.node, root, false);
    let stats = patcher.stats;
    world.entity_mut(root).insert((
        ScreenTrace {
            def: Arc::new(resolved),
            node,
        },
        stats,
    ));
    let asked = hint_now.clone();
    world
        .entity_mut(root)
        .insert(crate::semantic::ScreenFocusHint(hint_now.clone()));
    refocus(world, root, focus, asked, hint_now);
    true
}

/// The focused node, when it is inside `root`, and the nearest id at or above
/// it: what to look for again if the node itself is respawned.
struct FocusAnchor {
    entity: Entity,
    id: Option<String>,
}

fn focus_anchor(world: &World, root: Entity) -> Option<FocusAnchor> {
    let focused = world.get_resource::<InputFocus>()?.get()?;
    let mut at = focused;
    let mut id = None;
    loop {
        if id.is_none() {
            id = world.get::<TestId>(at).map(|t| t.0.clone());
        }
        if at == root {
            return Some(FocusAnchor {
                entity: focused,
                id,
            });
        }
        at = world.get::<ChildOf>(at)?.parent();
    }
}

fn under(world: &World, mut at: Entity, root: Entity) -> bool {
    loop {
        if at == root {
            return true;
        }
        match world.get::<ChildOf>(at) {
            Some(parent) => at = parent.parent(),
            None => return false,
        }
    }
}

fn focus_named(world: &mut World, root: Entity, id: &str) -> bool {
    let named = world
        .get_resource::<crate::semantic::TestIdIndex>()
        .and_then(|index| {
            index
                .entities(id)
                .iter()
                .copied()
                .find(|e| under(world, *e, root))
        })
        .and_then(|node| first_focusable(world, node));
    let Some(target) = named else {
        return false;
    };
    if let Some(mut focus) = world.get_resource_mut::<InputFocus>() {
        focus.set(target, FocusCause::Navigated);
    }
    if let Some(mut screen) = world.get_mut::<ScreenRoot>(root) {
        screen.initial_focus = Some(target);
    }
    let mut stack = world.resource_mut::<ScreenStack>();
    if let Some(entry) = stack.entries_mut().iter_mut().find(|e| e.root == root) {
        entry.focus = Some(target);
    }
    true
}

/// Puts the focus where the patched screen wants it. `asked` is a new
/// `initial_focus`, which wins; else the focused node stays, or, when it
/// was respawned, its id is found again; else the screen's own
/// `initial_focus` (`hint`), else the stack's restore.
fn refocus(
    world: &mut World,
    root: Entity,
    anchor: Option<FocusAnchor>,
    asked: Option<String>,
    hint: Option<String>,
) {
    if let Some(id) = asked
        && focus_named(world, root, &id)
    {
        return;
    }
    let Some(anchor) = anchor else {
        return;
    };
    if world.get_entity(anchor.entity).is_ok() {
        return;
    }
    if let Some(id) = anchor.id
        && focus_named(world, root, &id)
    {
        return;
    }
    if let Some(id) = hint
        && focus_named(world, root, &id)
    {
        return;
    }
    restore_focus(world);
}

/// A child's identity among its siblings: its id and kind, and which of that
/// pair it is when several repeat.
type Key = (Option<String>, Discriminant<UiNodeDef>, usize);

fn keys(defs: &[UiNodeDef]) -> Vec<Key> {
    let mut seen: HashMap<(Option<&str>, Discriminant<UiNodeDef>), usize> = HashMap::new();
    defs.iter()
        .map(|def| {
            let id = def.id();
            let kind = discriminant(def);
            let nth = seen.entry((id, kind)).or_insert(0);
            let key = (id.map(str::to_owned), kind, *nth);
            *nth += 1;
            key
        })
        .collect()
}

/// Whether two nodes are the same in everything but their children, for the
/// containers whose children are patched one by one; any other node is
/// equal only when it is equal.
fn same_shell(old: &UiNodeDef, new: &UiNodeDef) -> bool {
    use UiNodeDef::{Card, Panel, Scroll};
    match (old, new) {
        (
            Panel {
                role: ra,
                layout: la,
                tags: ta,
                ..
            },
            Panel {
                role: rb,
                layout: lb,
                tags: tb,
                ..
            },
        ) => ra == rb && la == lb && ta == tb,
        (
            Card {
                role: ra,
                layout: la,
                disabled: da,
                tags: ta,
                ..
            },
            Card {
                role: rb,
                layout: lb,
                disabled: db,
                tags: tb,
                ..
            },
        ) => ra == rb && la == lb && da == db && ta == tb,
        (
            Scroll {
                layout: la,
                scrollbar: sa,
                tags: ta,
                ..
            },
            Scroll {
                layout: lb,
                scrollbar: sb,
                tags: tb,
                ..
            },
        ) => la == lb && sa == sb && ta == tb,
        _ => false,
    }
}

/// Whether `a` and `b` differ in at most their arguments.
fn same_but_args(a: &TextOpts, b: &TextOpts) -> bool {
    a.wrap == b.wrap
        && a.align == b.align
        && a.max_lines == b.max_lines
        && a.inline == b.inline
        && a.role == b.role
}

struct Patcher<'w> {
    world: &'w mut World,
    root: Entity,
    kind: ScreenKind,
    menu: Option<Entity>,
    stats: PatchStats,
}

impl Patcher<'_> {
    fn spawn_fresh(&mut self, def: &UiNodeDef, parent: Entity, ignore: bool) -> Rec {
        self.stats.spawned += 1;
        let mut ctx = SpawnCtx {
            world: self.world,
            screen: self.root,
            kind: self.kind.clone(),
            menu: self.menu,
            parent,
        };
        let rec = spawn_traced(&mut ctx, def);
        if ignore {
            ignore_pointer(self.world, rec.entity);
        }
        rec
    }

    fn respawn(&mut self, new: &UiNodeDef, rec: &Rec, parent: Entity, ignore: bool) -> Rec {
        self.stats.despawned += 1;
        self.world.despawn(rec.entity);
        self.spawn_fresh(new, parent, ignore)
    }

    /// The node `rec` stands for, brought from `old` to `new`; the record of
    /// what stands there afterwards. `parent` is where a respawn goes.
    fn patch_node(
        &mut self,
        old: &UiNodeDef,
        new: &UiNodeDef,
        rec: Rec,
        parent: Entity,
        ignore: bool,
    ) -> Rec {
        use UiNodeDef::{Card, RichText as Rich, Scroll, Text};
        if same_shell(old, new) {
            let (old_kids, new_kids) = (old.children(), new.children());
            if rec.children.len() == old_kids.len() {
                // Where the children hang: the node itself, except a scroll's
                // viewport, which is read off a child that is there to ask.
                let hang = match old {
                    Scroll { .. } => rec
                        .children
                        .first()
                        .and_then(|c| self.world.get::<ChildOf>(c.entity))
                        .map(ChildOf::parent),
                    _ => Some(rec.entity),
                };
                if let Some(hang) = hang {
                    let ignore = ignore || matches!(old, Card { .. });
                    let entity = rec.entity;
                    let children =
                        self.patch_children(old_kids, new_kids, rec.children, hang, ignore);
                    if matches!(new, Card { .. }) {
                        self.relabel_card(entity, new_kids);
                    }
                    return Rec { entity, children };
                }
            }
            return self.respawn(new, &rec, parent, ignore);
        }
        if old == new {
            return rec;
        }
        match (old, new) {
            (
                Text {
                    key: ka,
                    style: sa,
                    opts: oa,
                    tags: ta,
                },
                Text {
                    key: kb,
                    style: sb,
                    opts: ob,
                    tags: tb,
                },
            ) if sa == sb && ta == tb && same_but_args(oa, ob) => {
                // The words are written here as well as asked for from the
                // localiser, which runs earlier in the frame: a screen
                // patched in `Update` would otherwise show the old words for
                // a frame, which a spawn never does.
                let shown = self
                    .world
                    .get_resource::<crate::loc::Localization>()
                    .and_then(|l| l.resolve_with(kb, &ob.args))
                    .unwrap_or_else(|| kb.0.clone());
                let want = LocText::with_args(kb.clone(), ob.args.clone());
                if let Some(mut loc) = self.world.get_mut::<LocText>(rec.entity)
                    && *loc != want
                {
                    *loc = want;
                }
                if let Some(mut text) = self.world.get_mut::<bevy::ui::widget::Text>(rec.entity)
                    && text.0 != shown
                {
                    text.0 = shown;
                }
                if ka != kb
                    && let Some(mut label) = self.world.get_mut::<SemanticLabel>(rec.entity)
                {
                    label.0.clone_from(&kb.0);
                }
                self.stats.rewritten += 1;
                rec
            }
            (
                Rich {
                    key: ka,
                    style: sa,
                    opts: oa,
                    tags: ta,
                },
                Rich {
                    key: kb,
                    style: sb,
                    opts: ob,
                    tags: tb,
                },
            ) if sa == sb && ta == tb && same_but_args(oa, ob) => {
                if let Some(mut rich) = self.world.get_mut::<RichText>(rec.entity) {
                    rich.key = kb.clone();
                    rich.args = ob.args.clone();
                }
                if ka != kb
                    && let Some(mut label) = self.world.get_mut::<SemanticLabel>(rec.entity)
                {
                    label.0.clone_from(&kb.0);
                }
                self.stats.rewritten += 1;
                rec
            }
            _ => self.respawn(new, &rec, parent, ignore),
        }
    }

    /// A card is called by the first text inside it.
    fn relabel_card(&mut self, card: Entity, children: &[UiNodeDef]) {
        let Some(key) = first_text_key(children) else {
            return;
        };
        if let Some(mut label) = self.world.get_mut::<SemanticLabel>(card)
            && label.0 != *key
        {
            label.0.clone_from(key);
        }
    }

    fn patch_children(
        &mut self,
        old: &[UiNodeDef],
        new: &[UiNodeDef],
        recs: Vec<Rec>,
        parent: Entity,
        ignore: bool,
    ) -> Vec<Rec> {
        let mut by_key: HashMap<Key, VecDeque<usize>> = HashMap::new();
        for (i, key) in keys(old).into_iter().enumerate() {
            by_key.entry(key).or_default().push_back(i);
        }
        let mut recs: Vec<Option<Rec>> = recs.into_iter().map(Some).collect();
        let mut out: Vec<Rec> = Vec::with_capacity(new.len());
        for (def, key) in new.iter().zip(keys(new)) {
            let at = by_key.get_mut(&key).and_then(VecDeque::pop_front);
            let rec = match at.and_then(|i| recs[i].take().map(|r| (i, r))) {
                Some((i, rec)) => self.patch_node(&old[i], def, rec, parent, ignore),
                None => self.spawn_fresh(def, parent, ignore),
            };
            out.push(rec);
        }
        for rec in recs.into_iter().flatten() {
            self.stats.despawned += 1;
            self.world.despawn(rec.entity);
        }
        self.reorder(parent, &out);
        out
    }

    /// Puts `wanted` in order in `parent`'s children, in the places the
    /// def's own children held (a widget may keep nodes of its own among
    /// them).
    fn reorder(&mut self, parent: Entity, wanted: &[Rec]) {
        let Some(children) = self.world.get::<Children>(parent) else {
            return;
        };
        let current: Vec<Entity> = children.to_vec();
        let mine: HashSet<Entity> = wanted.iter().map(|r| r.entity).collect();
        let order: Vec<Entity> = wanted.iter().map(|r| r.entity).collect();
        let held: Vec<Entity> = current
            .iter()
            .copied()
            .filter(|e| mine.contains(e))
            .collect();
        if held == order {
            return;
        }
        let mut result = Vec::with_capacity(current.len());
        let mut placed = false;
        for entity in current {
            if mine.contains(&entity) {
                if !placed {
                    result.extend(order.iter().copied());
                    placed = true;
                }
            } else {
                result.push(entity);
            }
        }
        self.world.entity_mut(parent).replace_children(&result);
    }
}

fn first_text_key(defs: &[UiNodeDef]) -> Option<&String> {
    defs.iter().find_map(|def| match def {
        UiNodeDef::Text { key, .. } => Some(&key.0),
        other => first_text_key(other.children()),
    })
}

/// A card's content is never a control: its whole subtree ignores the pointer.
fn ignore_pointer(world: &mut World, entity: Entity) {
    let mut below = vec![entity];
    while let Some(at) = below.pop() {
        if let Some(children) = world.get::<Children>(at) {
            below.extend(children.iter());
        }
        world.entity_mut(at).insert(Pickable::IGNORE);
    }
}

/// `node` if it can take focus, else its first focusable descendant.
fn first_focusable(world: &World, node: Entity) -> Option<Entity> {
    if world.get::<crate::focus_ring::Focusable>(node).is_some() {
        return Some(node);
    }
    let children = world.get::<Children>(node)?;
    children.iter().find_map(|c| first_focusable(world, c))
}
