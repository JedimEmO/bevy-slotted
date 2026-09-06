//! The shared harness for the `slotted-net` session tests.
//!
//! One world, one loopback link, one container store, and a client that
//! predicts the way `slotted-ecs` does. `sessions.rs` proves the first round
//! of review findings against it; `review_round2.rs` proves the seams between
//! them.

#![allow(dead_code, clippy::unwrap_used)]

use std::collections::BTreeMap;

use slotted_model::{
    Actor, Authority, AuthorityEvent, Button, ClickAction, Inventories, Inventory, InventoryId,
    ItemId, ItemStack, LookupCtx, MenuDef, MenuId, MenuSnapshot, MenuState, Namespaced,
    PropertyDef, PropertyId, SlotIx, apply_click, slot_view,
};
use slotted_net::{
    Access, ClientEnd, Loopback, MenuServer, Outcome, PeerId, RemoteAuthority, ServerEnd,
    ServerMessage, SessionInfo, Transport,
};

// --------------------------------------------------------------- the world

pub const STONE: ItemId = ItemId(1);
pub const EGG: ItemId = ItemId(2);
pub const BURN: PropertyId = PropertyId(7);

pub struct Items;

impl LookupCtx for Items {
    fn max_stack(&self, id: ItemId) -> u32 {
        match id {
            STONE => 64,
            EGG => 16,
            _ => 1,
        }
    }

    fn has_tag(&self, _id: ItemId, _tag: &Namespaced) -> bool {
        false
    }
}

#[allow(clippy::unnecessary_wraps)]
pub fn stack(id: ItemId, count: u32) -> Option<ItemStack> {
    Some(ItemStack::new(id, count))
}

/// Menu slots `0..4` are the chest, `4..8` the player's own four.
pub fn def() -> MenuDef {
    let mut def = MenuDef::new();
    let container = def.add_slots(MenuDef::CONTAINER, 4, slotted_model::SlotBehaviour::Normal);
    let player = def.add_slots(
        MenuDef::PLAYER_MAIN,
        4,
        slotted_model::SlotBehaviour::Normal,
    );
    def.hotbar = player.iter().collect();
    def.quick_move = slotted_model::RoutingTable::new()
        .route(container, [MenuDef::PLAYER_MAIN])
        .route(player, [MenuDef::CONTAINER]);
    def.listring = vec![MenuDef::CONTAINER, MenuDef::PLAYER_MAIN];
    def
}

/// The same two inventories, addressed the other way round: menu slots `0..4`
/// are the *player's*, `4..8` the chest, and the chest is `InventoryRef` 1.
///
/// Nothing about this is exotic; it is what a second screen over the same
/// block looks like. It exists here to prove that a slot index is a fact
/// about a session and means nothing across two of them.
pub fn mirrored_def() -> MenuDef {
    let mut def = MenuDef::new();
    def.add_slots(MenuDef::CONTAINER, 4, slotted_model::SlotBehaviour::Normal);
    def.add_slots(
        MenuDef::PLAYER_MAIN,
        4,
        slotted_model::SlotBehaviour::Normal,
    );
    def
}

pub fn furnace_def() -> MenuDef {
    let mut def = def();
    def.properties = vec![PropertyDef {
        id: BURN,
        initial: 0,
    }];
    def
}

pub fn starting_container() -> Inventory {
    Inventory::from_slots([stack(STONE, 64), stack(EGG, 10), None, None])
}

pub fn starting_player() -> Inventory {
    Inventory::from_slots([None, None, stack(STONE, 5), None])
}

pub fn tally(inventories: &Inventories, carried: Option<&ItemStack>) -> BTreeMap<u32, u64> {
    let mut out: BTreeMap<u32, u64> = BTreeMap::new();
    for (_, inventory) in inventories.iter() {
        for stack in inventory.slots().iter().flatten() {
            *out.entry(stack.id.0).or_default() += u64::from(stack.count);
        }
    }
    if let Some(carried) = carried {
        *out.entry(carried.id.0).or_default() += u64::from(carried.count);
    }
    out
}

pub fn left(slot: u16) -> ClickAction {
    ClickAction::Pickup {
        slot: SlotIx(slot),
        button: Button::Left,
    }
}

// -------------------------------------------------------------- the client

pub struct Client {
    pub menu: MenuId,
    pub peer: PeerId,
    /// The player's own inventory in the store, so a test can reopen a
    /// session over the same pockets after closing one.
    pub player: InventoryId,
    pub def: MenuDef,
    pub inventories: Inventories,
    pub state: MenuState,
    pub authority: RemoteAuthority<ClientEnd>,
    pub resyncs: usize,
    pub properties: Vec<(PropertyId, i32)>,
}

impl Client {
    pub fn click(&mut self, action: ClickAction) -> bool {
        let Ok(delta) = apply_click(
            &self.def,
            &mut self.inventories,
            &mut self.state,
            action,
            &Actor::SURVIVAL,
            &Items,
        ) else {
            return false;
        };
        self.authority.submit(self.menu, action, &delta).unwrap();
        true
    }

    pub fn reconcile(&mut self) -> Vec<AuthorityEvent> {
        let events = self.authority.poll();
        for event in &events {
            match event {
                AuthorityEvent::Ack { .. } => {}
                AuthorityEvent::Property { id, value, .. } => self.properties.push((*id, *value)),
                AuthorityEvent::Slot { slot, stack, .. } => self.write(*slot, stack.clone()),
                AuthorityEvent::Resync { snapshot, .. } => {
                    self.resyncs += 1;
                    self.overwrite(snapshot);
                }
            }
        }
        events
    }

    pub fn write(&mut self, slot: SlotIx, stack: Option<ItemStack>) {
        let Some(sd) = self.def.slot(slot).cloned() else {
            return;
        };
        if sd.behaviour.is_ghost() {
            self.state.set_hint(slot, stack);
        } else {
            self.inventories[sd.source].set(usize::from(sd.index), stack);
        }
    }

    pub fn overwrite(&mut self, snapshot: &MenuSnapshot) {
        self.inventories = snapshot.inventories.clone();
        self.state = snapshot.state.clone();
    }

    pub fn slot(&self, slot: u16) -> Option<ItemStack> {
        slot_view(&self.def, &self.inventories, &self.state, SlotIx(slot)).cloned()
    }

    pub fn tally(&self) -> BTreeMap<u32, u64> {
        tally(&self.inventories, self.state.carried.as_ref())
    }

    /// Every menu slot, hints included. Compared instead of the inventories
    /// themselves because a client's dirty masks record its own local
    /// prediction and say nothing about whether the two ends agree.
    pub fn all_slots(&self) -> Vec<Option<ItemStack>> {
        (0..self.def.slots.len())
            .map(|i| self.slot(u16::try_from(i).unwrap()))
            .collect()
    }

    /// The raw link end, for a test that wants to lose a message or forge one.
    pub fn end(&self) -> ClientEnd {
        self.authority.transport().clone()
    }
}

// -------------------------------------------------------------- the server

pub struct World {
    pub link: Loopback,
    pub server: MenuServer,
    pub end: ServerEnd,
    pub container: InventoryId,
    pub next_menu: u32,
}

impl World {
    pub fn new(seed: u64) -> Self {
        Self::with_server(seed, MenuServer::new())
    }

    pub fn with_server(seed: u64, mut server: MenuServer) -> Self {
        let link = Loopback::new(seed);
        let end = link.server();
        let container = server.add_shared(starting_container());
        Self {
            link,
            server,
            end,
            container,
            next_menu: 1,
        }
    }

    pub fn join(&mut self) -> Client {
        self.join_with(def(), false)
    }

    /// A player with their own link end, their own private inventory and
    /// their own session over the shared container.
    pub fn join_with(&mut self, def: MenuDef, mirrored: bool) -> Client {
        let end = self.link.add_client();
        let peer = end.peer();
        let player = self.server.add_private(peer, starting_player());
        let bindings = if mirrored {
            vec![player, self.container]
        } else {
            vec![self.container, player]
        };
        let menu = MenuId(self.next_menu);
        self.next_menu += 1;
        self.server
            .open(menu, peer, def.clone(), bindings, Actor::SURVIVAL)
            .unwrap();
        // Seeded from the server, the way a game seeds a screen it has just
        // opened: the state carries the container's real property values,
        // which `MenuState::new` knows nothing about.
        let snapshot = self.server.snapshot(menu).unwrap();
        Client {
            menu,
            peer,
            player,
            state: snapshot.state,
            def,
            inventories: snapshot.inventories,
            authority: RemoteAuthority::new(end).retry_after(2),
            resyncs: 0,
            properties: Vec::new(),
        }
    }

    pub fn pump(&mut self) -> Vec<Outcome> {
        self.server.pump(&self.end, &Items)
    }

    pub fn settle(&mut self, clients: &mut [&mut Client]) -> Vec<Outcome> {
        let mut outcomes = Vec::new();
        for _ in 0..8 {
            self.link.tick();
            outcomes.extend(self.pump());
            self.link.tick();
            for client in clients.iter_mut() {
                client.reconcile();
            }
        }
        outcomes
    }

    /// Every menu slot of one session as the server sees it.
    pub fn all_slots(&self, menu: MenuId) -> Vec<Option<ItemStack>> {
        let count = self.server.snapshot(menu).map_or(0, |_| 8);
        (0..count)
            .map(|i| self.server.slot(menu, SlotIx(u16::try_from(i).unwrap())))
            .collect()
    }

    /// Opens a fresh session for a client that closed its own, over the same
    /// container and the same private pockets: what reopening a chest does.
    ///
    /// The client's local copy is reseeded from the server, the way a game
    /// does when a screen opens.
    pub fn reopen(&mut self, client: &mut Client) {
        let menu = MenuId(self.next_menu);
        self.next_menu += 1;
        self.server
            .open(
                menu,
                client.peer,
                client.def.clone(),
                vec![self.container, client.player],
                Actor::SURVIVAL,
            )
            .unwrap();
        let snapshot = self.server.snapshot(menu).unwrap();
        client.menu = menu;
        client.inventories = snapshot.inventories;
        client.state = snapshot.state;
    }

    /// Every item in the store, by kind. What the world holds, ignoring who
    /// is looking at it.
    pub fn store_tally(&self) -> BTreeMap<u32, u64> {
        let mut out: BTreeMap<u32, u64> = BTreeMap::new();
        for (_, container) in self.server.store().iter() {
            for stack in container.inventory.slots().iter().flatten() {
                *out.entry(stack.id.0).or_default() += u64::from(stack.count);
            }
        }
        out
    }

    /// Everything in the store plus whatever every open session is carrying:
    /// the total the server is responsible for, which a close must not lower.
    pub fn world_tally(&self) -> BTreeMap<u32, u64> {
        let mut out = self.store_tally();
        for menu in self.open_menus() {
            if let Some(carried) = self
                .server
                .snapshot(menu)
                .and_then(|snapshot| snapshot.state.carried)
            {
                *out.entry(carried.id.0).or_default() += u64::from(carried.count);
            }
        }
        out
    }

    /// Every session the server still holds, ascending.
    pub fn open_menus(&self) -> Vec<MenuId> {
        (1..self.next_menu)
            .map(MenuId)
            .filter(|menu| self.server.owner_of(*menu).is_some())
            .collect()
    }

    pub fn container_cell(&self, cell: usize) -> Option<ItemStack> {
        self.server
            .store()
            .inventory(self.container)
            .and_then(|inventory| inventory.get(cell))
            .cloned()
    }
}

/// A policy that grants access and then takes it away again: the player
/// walked off, the block was broken, the trade was cancelled.
#[derive(Debug)]
pub struct Revocable(pub std::sync::atomic::AtomicBool);

impl Access for Revocable {
    fn may_act(&self, _peer: PeerId, _session: SessionInfo<'_>) -> bool {
        self.0.load(std::sync::atomic::Ordering::Relaxed)
    }
}

/// The same policy behind a handle, so the test can flip it while the server
/// holds it.
#[derive(Debug)]
pub struct Shared(pub std::sync::Arc<Revocable>);

impl Access for Shared {
    fn may_act(&self, peer: PeerId, session: SessionInfo<'_>) -> bool {
        self.0.may_act(peer, session)
    }
}

/// Drains and discards everything waiting for a client: what a link that
/// loses a burst does, without changing the conditions for anybody else.
pub fn lose(end: &ClientEnd) -> Vec<ServerMessage> {
    end.poll().into_iter().map(|(_, m)| m).collect()
}

/// Ticks the client's clock until it retransmits, without letting it see
/// anything the server has sent in the meantime.
pub fn force_retransmit(world: &mut World, client: &Client) {
    for _ in 0..4 {
        world.link.tick();
        lose(&client.end());
        client.authority.poll();
    }
}
