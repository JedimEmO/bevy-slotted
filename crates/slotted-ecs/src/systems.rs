//! The prediction loop: interpret, queue, predict, submit, reconcile.

use std::collections::VecDeque;

use bevy::prelude::*;
use slotted_model::{
    Actor, AuthorityEvent, ClickAction, Delta, DragKind, DragStage, ItemId, ItemStack, LookupCtx,
    MenuDef, MenuId, Namespaced, SlotIx,
};

use crate::authority::{Authority, PendingRoundTrips};
use crate::events::{MenuAction, PropertyChanged, SlotChanged, SlotClicked, SlotSync};
use crate::lookup::Registries;
use crate::menu::{Carried, Dropped, Favorite, Inventory, MenuProperty, OpenMenu, SlotEntities};

/// Actions collected by observers during a frame, applied in order by
/// `SlottedEcsSet::Predict`. Observers never touch inventories directly, so
/// the model sees one deterministic sequence per frame.
#[derive(Resource, Debug, Default)]
pub struct ActionQueue(pub VecDeque<MenuAction>);

/// A predicted action waiting to be handed to the [`Authority`].
///
/// `SlottedEcsSet::Predict` fills this; `Submit` drains it in the same frame.
#[derive(Debug, Clone, PartialEq)]
pub struct Submission {
    /// The menu entity the action ran on.
    pub menu: Entity,
    /// That menu's id with the authority.
    pub id: MenuId,
    /// What the player did.
    pub action: ClickAction,
    /// What the local prediction made of it.
    pub delta: Delta,
}

/// Deltas predicted this frame, in application order.
#[derive(Resource, Debug, Default)]
pub struct PendingSubmissions(pub Vec<Submission>);

/// The [`LookupCtx`] used when no [`Registries`] resource has been inserted:
/// every item stacks to one and carries no tags.
#[derive(Debug, Clone, Copy, Default)]
pub struct EmptyLookup;

impl LookupCtx for EmptyLookup {
    fn max_stack(&self, _id: ItemId) -> u32 {
        1
    }

    fn has_tag(&self, _id: ItemId, _tag: &Namespaced) -> bool {
        false
    }
}

/// Turns raw [`SlotClicked`] gestures into [`ClickAction`]s.
///
/// Owns the double-click window (virtual time) and the current drag paint
/// state. The mapping is vanilla's: left/right = `Pickup`, shift+left =
/// `QuickMove`, middle = `Clone`, a second left click on the same slot within
/// `double_click_window` = `PickupAll`.
#[derive(Resource, Debug)]
pub struct ClickInterpreter {
    /// Two clicks closer than this on the same slot become `PickupAll`.
    pub double_click_window: std::time::Duration,
    /// Last left click: slot and virtual time.
    pub last_click: Option<(Entity, SlotIx, std::time::Duration)>,
    /// Kind of the drag currently being painted, if any. Set by
    /// [`begin_drag`](Self::begin_drag) and cleared by
    /// [`end_drag`](Self::end_drag).
    pub drag: Option<DragKind>,
}

impl Default for ClickInterpreter {
    fn default() -> Self {
        Self {
            double_click_window: std::time::Duration::from_millis(250),
            last_click: None,
            drag: None,
        }
    }
}

impl ClickInterpreter {
    /// Pure mapping from a gesture to an action, without the double-click
    /// state. `slot` is the clicked slot's index.
    pub fn interpret(&self, slot: SlotIx, click: &SlotClicked) -> ClickAction {
        use slotted_model::Button;
        match (click.button, click.modifiers.shift) {
            (Button::Left, true) => ClickAction::QuickMove { slot },
            (Button::Middle, _) => ClickAction::Clone { slot },
            (button, _) => ClickAction::Pickup { slot, button },
        }
    }

    /// The full mapping, including the double-click window. `now` is
    /// `Time<Virtual>::elapsed()`; `entity` is the clicked slot entity.
    ///
    /// A second left click on the same slot entity within
    /// [`double_click_window`](Self::double_click_window) becomes
    /// `PickupAll`, exactly once: the stored click is consumed.
    pub fn interpret_with_time(
        &mut self,
        entity: Entity,
        slot: SlotIx,
        click: &SlotClicked,
        now: std::time::Duration,
    ) -> ClickAction {
        use slotted_model::Button;
        let plain_left = click.button == Button::Left && !click.modifiers.shift;
        if plain_left {
            let doubled = self.last_click.is_some_and(|(e, s, t)| {
                e == entity && s == slot && now.saturating_sub(t) <= self.double_click_window
            });
            if doubled {
                self.last_click = None;
                return ClickAction::PickupAll {
                    slot,
                    reverse: false,
                };
            }
            self.last_click = Some((entity, slot, now));
        } else {
            self.last_click = None;
        }
        self.interpret(slot, click)
    }

    /// Starts a drag paint of `kind`. Returns the `Drag { Start }` action.
    pub fn begin_drag(&mut self, kind: DragKind) -> ClickAction {
        self.drag = Some(kind);
        self.last_click = None;
        ClickAction::Drag {
            stage: DragStage::Start,
            kind,
            slot: None,
        }
    }

    /// Paints `slot`. `None` when no drag is in progress.
    pub fn paint(&self, slot: SlotIx) -> Option<ClickAction> {
        self.drag.map(|kind| ClickAction::Drag {
            stage: DragStage::Add,
            kind,
            slot: Some(slot),
        })
    }

    /// Ends the drag. `None` when no drag is in progress.
    pub fn end_drag(&mut self) -> Option<ClickAction> {
        self.drag.take().map(|kind| ClickAction::Drag {
            stage: DragStage::End,
            kind,
            slot: None,
        })
    }

    /// The number-key action: swap the hovered `slot` with hotbar index
    /// `hotbar` (0-8), or 40 for the offhand.
    pub const fn swap(slot: SlotIx, hotbar: u8) -> ClickAction {
        ClickAction::Swap { slot, hotbar }
    }
}

/// Observer: [`SlotClicked`] on a `SlotRef` entity becomes a [`MenuAction`]
/// on its menu.
pub fn interpret_slot_click(
    click: On<SlotClicked>,
    slots: Query<&SlotRef>,
    time: Res<Time<Virtual>>,
    mut interpreter: ResMut<ClickInterpreter>,
    mut commands: Commands,
) {
    let event = click.event();
    let Ok(slot_ref) = slots.get(event.entity) else {
        tracing::warn!(
            entity = ?event.entity,
            "SlotClicked on an entity without a SlotRef"
        );
        return;
    };
    let action =
        interpreter.interpret_with_time(event.entity, slot_ref.slot, event, time.elapsed());
    commands.trigger(MenuAction {
        entity: slot_ref.menu,
        action,
    });
}

/// Observer: [`MenuAction`] is pushed onto the [`ActionQueue`].
pub fn enqueue_menu_action(action: On<MenuAction>, mut queue: ResMut<ActionQueue>) {
    queue.0.push_back(*action.event());
}

/// `SlottedEcsSet::Input`: nothing yet beyond what observers collected. Kept
/// as a system so games can order against the set.
pub fn gather_input() {}

/// `SlottedEcsSet::Input`: clears the per-slot dirty masks set during the
/// previous frame.
///
/// The contract does not pin a clearing point, so this crate picks the one
/// that gives every downstream set a full frame to read them: masks written
/// during frame N survive `SlottedUiSet::Render` and the theme sets of that
/// same frame and are cleared at the top of frame N+1, before this frame's
/// prediction runs. See `docs/design/phase2-notes-A.md`.
pub fn clear_dirty_masks(mut inventories: Query<&mut Inventory>) {
    for mut inventory in &mut inventories {
        if inventory.changed().any() {
            inventory.bypass_change_detection().0.clear_changed();
        }
    }
}

/// Contents of every slot of `def`, in slot order, read out of `inventories`
/// through `entities`.
fn read_slots(
    def: &MenuDef,
    entities: &[Entity],
    inventories: &Query<&mut Inventory>,
) -> Vec<Option<ItemStack>> {
    def.slots
        .iter()
        .map(|slot| {
            entities
                .get(slot.source.index())
                .and_then(|e| inventories.get(*e).ok())
                .and_then(|inv| inv.get(usize::from(slot.index)))
                .cloned()
        })
        .collect()
}

/// `SlottedEcsSet::Predict`: drain [`ActionQueue`], assemble `Inventories`
/// from the menu's inventory entities, run `apply_click`, write changed
/// inventories back, update `Carried`, write `SlotSync` messages and trigger
/// `SlotChanged` on registered slot entities, then record the delta for
/// `Submit`.
// Bevy systems declare their world access as parameters; splitting this one
// would only move the same access into a `SystemParam` struct.
#[allow(clippy::too_many_arguments)]
pub fn predict(
    mut queue: ResMut<ActionQueue>,
    mut pending: ResMut<PendingSubmissions>,
    mut dropped: ResMut<Dropped>,
    mut menus: Query<(&mut OpenMenu, &mut Carried, &SlotEntities)>,
    mut inventories: Query<&mut Inventory>,
    registries: Option<Res<Registries>>,
    mut sync: MessageWriter<SlotSync>,
    mut commands: Commands,
) {
    if queue.0.is_empty() {
        return;
    }
    let borrowed = registries.as_ref().map(|r| r.lookup());
    let lookup: &dyn LookupCtx = match borrowed.as_ref() {
        Some(l) => l,
        None => &EmptyLookup,
    };

    let actions: Vec<MenuAction> = queue.0.drain(..).collect();
    for MenuAction {
        entity: menu_entity,
        action,
    } in actions
    {
        let Ok((mut menu, mut carried, slot_entities)) = menus.get_mut(menu_entity) else {
            tracing::warn!(
                ?menu_entity,
                ?action,
                "MenuAction on an entity without OpenMenu"
            );
            continue;
        };

        let def = menu.def.clone();
        let actor: Actor = menu.actor;
        let id = menu.id;
        let entities = menu.inventories.clone();

        let Some(mut invs) = gather_inventories(&entities, &inventories) else {
            tracing::warn!(
                ?menu_entity,
                "OpenMenu names an entity without an Inventory component"
            );
            continue;
        };

        let outcome =
            slotted_model::apply_click(&def, &mut invs, &mut menu.state, action, &actor, lookup);
        let delta = match outcome {
            Ok(delta) => delta,
            Err(error) => {
                tracing::debug!(?menu_entity, ?action, %error, "action refused");
                continue;
            }
        };

        write_back(&def, &delta, &entities, &invs, &mut inventories);
        carried.set_if_neq(Carried(menu.state.carried.clone()));
        dropped.record(menu_entity, &delta.dropped);
        emit_slots(
            menu_entity,
            slot_entities,
            delta.slots.iter().cloned(),
            &mut sync,
            &mut commands,
        );

        pending.0.push(Submission {
            menu: menu_entity,
            id,
            action,
            delta,
        });
    }
}

/// Clones the component inventories named by `entities` into a model
/// `Inventories`, preserving their dirty masks so a second action in the same
/// frame accumulates onto the first.
fn gather_inventories(
    entities: &[Entity],
    inventories: &Query<&mut Inventory>,
) -> Option<slotted_model::Inventories> {
    let mut out = slotted_model::Inventories::new();
    for entity in entities {
        out.push(inventories.get(*entity).ok()?.0.clone());
    }
    Some(out)
}

/// Writes the inventories the delta touched back onto their entities.
fn write_back(
    def: &MenuDef,
    delta: &Delta,
    entities: &[Entity],
    invs: &slotted_model::Inventories,
    inventories: &mut Query<&mut Inventory>,
) {
    let mut touched: Vec<usize> = delta
        .slots
        .iter()
        .filter_map(|(ix, _)| def.slot(*ix).map(|s| s.source.index()))
        .collect();
    touched.sort_unstable();
    touched.dedup();
    for index in touched {
        let (Some(entity), Some(source)) = (
            entities.get(index),
            invs.get(slotted_model::InventoryRef::new(narrow(index))),
        ) else {
            continue;
        };
        if let Ok(mut component) = inventories.get_mut(*entity) {
            component.0 = source.clone();
        }
    }
}

/// Writes a `SlotSync` for every changed slot and triggers `SlotChanged` on
/// the ones that have a UI entity.
fn emit_slots(
    menu: Entity,
    slot_entities: &SlotEntities,
    slots: impl Iterator<Item = (SlotIx, Option<ItemStack>)>,
    sync: &mut MessageWriter<SlotSync>,
    commands: &mut Commands,
) {
    for (slot, stack) in slots {
        sync.write(SlotSync {
            menu,
            slot,
            stack: stack.clone(),
        });
        if let Some(entity) = slot_entities.0.get(&slot) {
            commands.trigger(SlotChanged {
                entity: *entity,
                menu,
                slot,
                stack,
            });
        }
    }
}

fn narrow(index: usize) -> u16 {
    u16::try_from(index).unwrap_or(u16::MAX)
}

/// `SlottedEcsSet::Submit`: hand each predicted delta to the `Authority`
/// resource and bump `PendingRoundTrips`.
///
/// A submission the authority refuses outright does not count as a round
/// trip; the menu's whole slot set is re-emitted instead, which is the local
/// stand-in for the resync a networked authority would send.
// Bevy systems declare their world access as parameters; splitting this one
// would only move the same access into a `SystemParam` struct.
#[allow(clippy::too_many_arguments)]
pub fn submit(
    mut pending: ResMut<PendingSubmissions>,
    mut round_trips: ResMut<PendingRoundTrips>,
    authority: Option<Res<Authority>>,
    menus: Query<(&OpenMenu, &SlotEntities)>,
    inventories: Query<&mut Inventory>,
    mut sync: MessageWriter<SlotSync>,
    mut commands: Commands,
) {
    if pending.0.is_empty() {
        return;
    }
    let Some(authority) = authority else {
        tracing::warn!("no Authority resource; dropping predicted deltas");
        pending.0.clear();
        return;
    };
    for submission in pending.0.drain(..) {
        match authority
            .0
            .submit(submission.id, submission.action, &submission.delta)
        {
            Ok(()) => round_trips.0 = round_trips.0.saturating_add(1),
            Err(error) => {
                tracing::warn!(menu = ?submission.menu, %error, "authority refused the submission");
                let Ok((menu, slot_entities)) = menus.get(submission.menu) else {
                    continue;
                };
                let contents = read_slots(&menu.def, &menu.inventories, &inventories);
                let all = contents
                    .into_iter()
                    .enumerate()
                    .map(|(i, stack)| (SlotIx(narrow(i)), stack));
                emit_slots(
                    submission.menu,
                    slot_entities,
                    all,
                    &mut sync,
                    &mut commands,
                );
            }
        }
    }
}

/// `SlottedEcsSet::Reconcile`: poll the authority. `Ack` decrements
/// `PendingRoundTrips`; `Resync` overwrites the menu state and inventories
/// and re-emits the slots that changed; `Property` updates the
/// `MenuProperty` child and triggers `PropertyChanged`.
// Bevy systems declare their world access as parameters; splitting this one
// would only move the same access into a `SystemParam` struct.
#[allow(clippy::too_many_arguments)]
pub fn reconcile(
    authority: Option<Res<Authority>>,
    mut round_trips: ResMut<PendingRoundTrips>,
    mut menus: Query<(Entity, &mut OpenMenu, &mut Carried, &SlotEntities)>,
    mut inventories: Query<&mut Inventory>,
    mut properties: Query<(Entity, &mut MenuProperty, &ChildOf)>,
    mut sync: MessageWriter<SlotSync>,
    mut commands: Commands,
) {
    let Some(authority) = authority else {
        return;
    };
    let events = authority.0.poll();
    if events.is_empty() {
        return;
    }
    let by_id: Vec<(MenuId, Entity)> = menus.iter().map(|(e, m, _, _)| (m.id, e)).collect();
    let entity_of = |id: MenuId| by_id.iter().find(|(m, _)| *m == id).map(|(_, e)| *e);

    for event in events {
        match event {
            AuthorityEvent::Ack { menu, .. } => {
                if entity_of(menu).is_none() {
                    tracing::debug!(?menu, "ack for a menu that is no longer open");
                }
                round_trips.0 = round_trips.0.saturating_sub(1);
            }
            AuthorityEvent::Resync { menu, snapshot } => {
                round_trips.0 = round_trips.0.saturating_sub(1);
                let Some(entity) = entity_of(menu) else {
                    tracing::debug!(?menu, "resync for a menu that is no longer open");
                    continue;
                };
                apply_resync(
                    entity,
                    &snapshot,
                    &mut menus,
                    &mut inventories,
                    &mut sync,
                    &mut commands,
                );
            }
            AuthorityEvent::Property { menu, id, value } => {
                let Some(entity) = entity_of(menu) else {
                    continue;
                };
                if let Ok((_, mut open, _, _)) = menus.get_mut(entity)
                    && let Some(position) = open.def.properties.iter().position(|p| p.id == id)
                    && let Some(slot) = open.state.properties.get_mut(position)
                {
                    *slot = value;
                }
                for (child, mut property, child_of) in &mut properties {
                    if child_of.parent() == entity && property.id == id {
                        property.value = value;
                        commands.trigger(PropertyChanged {
                            entity: child,
                            menu: entity,
                            id,
                            value,
                        });
                    }
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn apply_resync(
    entity: Entity,
    snapshot: &slotted_model::MenuSnapshot,
    menus: &mut Query<(Entity, &mut OpenMenu, &mut Carried, &SlotEntities)>,
    inventories: &mut Query<&mut Inventory>,
    sync: &mut MessageWriter<SlotSync>,
    commands: &mut Commands,
) {
    let Ok((_, mut menu, mut carried, slot_entities)) = menus.get_mut(entity) else {
        return;
    };
    let def = menu.def.clone();
    let entities = menu.inventories.clone();
    let before = read_slots(&def, &entities, inventories);

    for (index, target) in entities.iter().enumerate() {
        let Some(source) = snapshot
            .inventories
            .get(slotted_model::InventoryRef::new(narrow(index)))
        else {
            continue;
        };
        let Ok(mut component) = inventories.get_mut(*target) else {
            continue;
        };
        if component.len() == source.len() {
            // Overwrite slot by slot so only the differences are marked dirty.
            for i in 0..source.len() {
                if component.get(i) != source.get(i) {
                    component.set(i, source.get(i).cloned());
                }
                if component.is_favorite(i) != source.is_favorite(i) {
                    component.set_favorite(i, source.is_favorite(i));
                }
            }
        } else {
            component.0 = source.clone();
            component.0.mark_all_changed();
        }
    }

    menu.state = snapshot.state.clone();
    carried.set_if_neq(Carried(menu.state.carried.clone()));

    let after = read_slots(&def, &entities, inventories);
    let changed = before
        .into_iter()
        .zip(after)
        .enumerate()
        .filter(|(_, (old, new))| old != new)
        .map(|(i, (_, new))| (SlotIx(narrow(i)), new))
        .collect::<Vec<_>>();
    emit_slots(entity, slot_entities, changed.into_iter(), sync, commands);
}

/// `SlottedEcsSet::Reconcile`: mirrors `Inventory::is_favorite` onto the slot
/// entities as the [`Favorite`] marker.
pub fn mirror_favorites(
    slots: Query<(Entity, &SlotRef, Has<Favorite>)>,
    menus: Query<&OpenMenu>,
    inventories: Query<&Inventory>,
    mut commands: Commands,
) {
    for (entity, slot_ref, marked) in &slots {
        let Ok(menu) = menus.get(slot_ref.menu) else {
            continue;
        };
        let favorite = menu
            .def
            .slot(slot_ref.slot)
            .and_then(|slot| {
                let source = menu.inventories.get(slot.source.index())?;
                let inventory = inventories.get(*source).ok()?;
                Some(inventory.is_favorite(usize::from(slot.index)))
            })
            .unwrap_or(false);
        if favorite && !marked {
            commands.entity(entity).insert(Favorite);
        } else if !favorite && marked {
            commands.entity(entity).remove::<Favorite>();
        }
    }
}

/// Keeps [`SlotEntities`] on each menu in step with `Added<SlotRef>`, and
/// seeds every newly registered slot with the model's current contents.
///
/// A slot entity spawned after its menu was opened has never seen a
/// [`SlotChanged`], so without this seeding a freshly spawned screen would
/// render empty until the player's first click. The initial trigger is the
/// slot's whole state, exactly as a prediction or a resync would deliver it.
pub fn register_slot_refs(
    added: Query<(Entity, &SlotRef), Added<SlotRef>>,
    mut slot_entities: Query<&mut crate::SlotEntities>,
    menus: Query<&OpenMenu>,
    inventories: Query<&Inventory>,
    mut sync: MessageWriter<SlotSync>,
    mut commands: Commands,
) {
    for (entity, slot_ref) in &added {
        let Ok(mut slots) = slot_entities.get_mut(slot_ref.menu) else {
            tracing::warn!(
                ?entity,
                ?slot_ref,
                "SlotRef points at an entity without OpenMenu"
            );
            continue;
        };
        slots.0.insert(slot_ref.slot, entity);

        let stack = menus.get(slot_ref.menu).ok().and_then(|menu| {
            let slot = menu.def.slot(slot_ref.slot)?;
            let source = menu.inventories.get(slot.source.index())?;
            inventories
                .get(*source)
                .ok()?
                .get(usize::from(slot.index))
                .cloned()
        });
        sync.write(SlotSync {
            menu: slot_ref.menu,
            slot: slot_ref.slot,
            stack: stack.clone(),
        });
        commands.trigger(SlotChanged {
            entity,
            menu: slot_ref.menu,
            slot: slot_ref.slot,
            stack,
        });
    }
}

use crate::SlotRef;
