//! The client and the server against each other over a [`Loopback`], on a
//! link that is by turns perfect, slow, shuffled and lossy.
#![allow(clippy::unwrap_used)]

use pretty_assertions::assert_eq;
use slotted_model::{
    Actor, Authority, AuthorityEvent, Button, ClickAction, DragKind, DragStage, Inventories,
    Inventory, InventoryId, ItemId, ItemStack, LookupCtx, MenuDef, MenuId, MenuSnapshot, MenuState,
    Namespaced, SlotIx, ToolbarAction, ValidationLevel, apply_click, slot_view,
};
use slotted_net::{ClientEnd, Conditions, Loopback, MenuServer, Outcome, PeerId, RemoteAuthority};

// --------------------------------------------------------------- the world

const STONE: ItemId = ItemId(1);
const EGG: ItemId = ItemId(2);

struct Items;

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
fn stack(id: ItemId, count: u32) -> Option<ItemStack> {
    Some(ItemStack::new(id, count))
}

/// A chest of four slots over four player slots, small enough to read: menu
/// slots `0..4` are the container and `4..8` the player, and the number keys
/// address the player slots.
fn def() -> MenuDef {
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

/// The chest as it starts: 64 stone in cell 0, 10 eggs in cell 1.
fn starting_container() -> Inventory {
    Inventory::from_slots([stack(STONE, 64), stack(EGG, 10), None, None])
}

/// One player's own four slots: 5 stone in cell 2.
fn starting_player() -> Inventory {
    Inventory::from_slots([None, None, stack(STONE, 5), None])
}

fn starting_inventories() -> Inventories {
    [starting_container(), starting_player()]
        .into_iter()
        .collect()
}

// -------------------------------------------------------------- the client

/// A client: its own copy of the menu, its authority, and a note of every
/// slot the server made it change.
struct Client {
    def: MenuDef,
    inventories: Inventories,
    state: MenuState,
    authority: RemoteAuthority<ClientEnd>,
    /// Slots this client had to correct because the server said so.
    corrections: Vec<(SlotIx, Option<ItemStack>)>,
    peer: PeerId,
    /// This client's own session on the server. Two players at one chest have
    /// two of these, which is the difference the store made.
    menu: MenuId,
}

impl Client {
    fn new(link: &Loopback, menu: MenuId, peer: PeerId, end: ClientEnd) -> Self {
        let _ = link;
        Self {
            menu,
            def: def(),
            inventories: starting_inventories(),
            state: MenuState::new(&def()),
            authority: RemoteAuthority::new(end).retry_after(2),
            corrections: Vec::new(),
            peer,
        }
    }

    /// Predicts `action` locally and submits it. Returns whether the local
    /// model accepted it at all.
    fn click(&mut self, action: ClickAction) -> bool {
        let outcome = apply_click(
            &self.def,
            &mut self.inventories,
            &mut self.state,
            action,
            &Actor::SURVIVAL,
            &Items,
        );
        let Ok(delta) = outcome else {
            return false;
        };
        self.authority.submit(self.menu, action, &delta).unwrap();
        true
    }

    /// Drains the authority and applies what it says. Returns the events, so
    /// a test can assert on the shape of the answer as well as the result.
    fn reconcile(&mut self) -> Vec<AuthorityEvent> {
        let events = self.authority.poll();
        for event in &events {
            match event {
                AuthorityEvent::Ack { .. } | AuthorityEvent::Property { .. } => {}
                AuthorityEvent::Slot { slot, stack, .. } => {
                    self.write(*slot, stack.clone());
                }
                AuthorityEvent::Resync { snapshot, .. } => self.overwrite(snapshot),
            }
        }
        events
    }

    /// Writes one authoritative slot, recording it as a correction when it
    /// really was one.
    fn write(&mut self, slot: SlotIx, stack: Option<ItemStack>) {
        if self.slot(slot) == stack {
            return;
        }
        self.corrections.push((slot, stack.clone()));
        let Some(sd) = self.def.slot(slot).cloned() else {
            return;
        };
        if sd.behaviour.is_ghost() {
            self.state.set_hint(slot, stack);
        } else {
            self.inventories[sd.source].set(usize::from(sd.index), stack);
        }
    }

    /// Replaces everything with the server's picture, noting only the slots
    /// that actually differed. This is what `slotted_ecs::apply_resync` does
    /// with a snapshot, and the reason a resync does not repaint a screen.
    fn overwrite(&mut self, snapshot: &MenuSnapshot) {
        let before: Vec<Option<ItemStack>> = self.all_slots();
        self.inventories = snapshot.inventories.clone();
        self.state = snapshot.state.clone();
        for (index, old) in before.into_iter().enumerate() {
            let ix = SlotIx(u16::try_from(index).unwrap());
            let new = self.slot(ix);
            if old != new {
                self.corrections.push((ix, new));
            }
        }
    }

    fn slot(&self, slot: SlotIx) -> Option<ItemStack> {
        slot_view(&self.def, &self.inventories, &self.state, slot).cloned()
    }

    fn all_slots(&self) -> Vec<Option<ItemStack>> {
        (0..self.def.slots.len())
            .map(|i| self.slot(SlotIx(u16::try_from(i).unwrap())))
            .collect()
    }

    /// Total items of every kind, cursor included: the number a resync must
    /// not invent or lose.
    fn tally(&self) -> Vec<(ItemId, u64)> {
        tally(&self.inventories, self.state.carried.as_ref())
    }
}

fn tally(inventories: &Inventories, carried: Option<&ItemStack>) -> Vec<(ItemId, u64)> {
    let mut out: std::collections::BTreeMap<u32, u64> = std::collections::BTreeMap::new();
    for (_, inventory) in inventories.iter() {
        for stack in inventory.slots().iter().flatten() {
            *out.entry(stack.id.0).or_default() += u64::from(stack.count);
        }
    }
    if let Some(carried) = carried {
        *out.entry(carried.id.0).or_default() += u64::from(carried.count);
    }
    out.into_iter().map(|(id, n)| (ItemId(id), n)).collect()
}

// -------------------------------------------------------------- the server

struct World {
    link: Loopback,
    server: MenuServer,
    end: slotted_net::ServerEnd,
    /// The one chest every session binds.
    container: InventoryId,
    next_menu: u32,
}

impl World {
    fn new(seed: u64) -> Self {
        let link = Loopback::new(seed);
        let end = link.server();
        let mut server = MenuServer::new();
        let container = server.add_shared(starting_container());
        Self {
            link,
            server,
            end,
            container,
            next_menu: 1,
        }
    }

    /// A new player: their own link end, their own player inventory, their
    /// own session, and the shared chest.
    fn join(&mut self) -> Client {
        let end = self.link.add_client();
        let peer = end.peer();
        let player = self.server.add_private(peer, starting_player());
        let menu = MenuId(self.next_menu);
        self.next_menu += 1;
        self.server
            .open(
                menu,
                peer,
                def(),
                vec![self.container, player],
                Actor::SURVIVAL,
            )
            .unwrap();
        Client::new(&self.link, menu, peer, end)
    }

    fn pump(&mut self) -> Vec<Outcome> {
        self.server.pump(&self.end, &Items)
    }

    /// One round: the link delivers, the server answers, the link delivers
    /// again, and every client reconciles.
    fn settle(&mut self, clients: &mut [&mut Client]) -> Vec<Outcome> {
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

    /// What one session's slot holds on the server.
    fn slot(&self, menu: MenuId, slot: u16) -> Option<ItemStack> {
        self.server.slot(menu, SlotIx(slot))
    }

    /// The items one session can see: the chest plus that player's own
    /// inventory, which is what the matching client's tally covers.
    fn tally(&self, menu: MenuId) -> Vec<(ItemId, u64)> {
        let snapshot = self.server.snapshot(menu).unwrap();
        tally(&snapshot.inventories, snapshot.state.carried.as_ref())
    }
}

fn left(slot: u16) -> ClickAction {
    ClickAction::Pickup {
        slot: SlotIx(slot),
        button: Button::Left,
    }
}

// ------------------------------------------------------------------- tests

#[test]
fn the_happy_path_costs_one_ack_and_nothing_else() {
    let mut world = World::new(1);
    let mut client = world.join();

    assert!(client.click(left(0)));
    assert!(client.click(left(4)));
    let outcomes = world.settle(&mut [&mut client]);

    assert_eq!(outcomes, vec![Outcome::Acked, Outcome::Acked]);
    assert!(
        client.corrections.is_empty(),
        "a prediction the server agrees with corrects nothing: {:?}",
        client.corrections
    );
    assert_eq!(client.slot(SlotIx(0)), None);
    assert_eq!(world.slot(client.menu, 0), None);
    assert_eq!(world.slot(client.menu, 4), stack(STONE, 64));
    assert_eq!(client.state.state_id, 2);
    assert_eq!(world.server.state_id(client.menu), Some(2));
    assert_eq!(client.authority.in_flight(), 0);
}

#[test]
fn a_disagreement_resyncs_the_client_and_touches_only_what_differs() {
    let mut world = World::new(2);
    let mut client = world.join();

    // The server's chest slot 0 is locked; the client's copy is not, so the
    // client happily predicts a move the server will refuse. This is the
    // shape of every desync: two definitions that drifted apart.
    let mut locked = def();
    locked.slots[0].behaviour = slotted_model::SlotBehaviour::Locked;
    let bindings = world.server.bindings_of(client.menu).unwrap().to_vec();
    world.server.close(client.menu);
    world
        .server
        .open(client.menu, client.peer, locked, bindings, Actor::SURVIVAL)
        .unwrap();

    assert!(client.click(left(0)), "the client predicts the pickup");
    assert_eq!(client.slot(SlotIx(0)), None, "and shows it immediately");

    let outcomes = world.settle(&mut [&mut client]);

    assert_eq!(
        outcomes,
        vec![Outcome::Refused(slotted_model::ClickError::SlotLocked)]
    );
    assert_eq!(
        client.corrections,
        vec![(SlotIx(0), stack(STONE, 64))],
        "one slot moved, so one slot is repainted"
    );
    assert_eq!(client.state.carried, None, "and the cursor is emptied");
    assert_eq!(client.slot(SlotIx(0)), stack(STONE, 64));
    assert_eq!(client.tally(), world.tally(client.menu));
}

#[test]
fn acks_that_arrive_out_of_order_still_settle_every_click() {
    let mut world = World::new(4242);
    let mut client = world.join();

    // The clicks go out in order; the answers come back shuffled.
    for slot in [0, 4, 1, 5] {
        client.click(left(slot));
        world.link.tick();
        world.pump();
    }
    world.link.set_conditions(Conditions::reordering(6));
    world.link.advance(10);

    let events = client.reconcile();
    let acked: Vec<u32> = events
        .iter()
        .filter_map(|e| match e {
            AuthorityEvent::Ack { state_id, .. } => Some(*state_id),
            _ => None,
        })
        .collect();
    assert_eq!(acked.len(), 4, "every click is answered exactly once");
    let mut sorted = acked.clone();
    sorted.sort_unstable();
    assert_eq!(sorted, vec![1, 2, 3, 4]);
    assert_eq!(
        client.authority.in_flight(),
        0,
        "and nothing is left waiting"
    );
    assert!(
        client.corrections.is_empty(),
        "reordering the acks changes no slot: {:?}",
        client.corrections
    );
}

#[test]
fn a_dropped_click_is_retried_and_applied_exactly_once() {
    let mut world = World::new(5);
    let mut client = world.join();

    // Nothing crosses the link.
    world.link.set_conditions(Conditions::lossy(100));
    assert!(client.click(left(0)));
    world.link.tick();
    assert!(world.pump().is_empty(), "the server never saw it");
    assert_eq!(client.authority.in_flight(), 1);

    // The link comes back. The client resends after its retry window and the
    // server applies the click for the first time.
    world.link.set_conditions(Conditions::PERFECT);
    let outcomes = world.settle(&mut [&mut client]);
    assert!(client.authority.retransmits() >= 1, "the click was resent");
    assert_eq!(outcomes, vec![Outcome::Acked]);
    assert_eq!(world.slot(client.menu, 0), None);
    assert_eq!(world.server.state_id(client.menu), Some(1));

    // A second copy of the same click, arriving late, must not move anything
    // again: the sequence number says it is the one already applied.
    world.link.set_conditions(Conditions::PERFECT);
    let before = world.server.snapshot(client.menu).unwrap();
    for _ in 0..4 {
        world.settle(&mut [&mut client]);
    }
    assert_eq!(world.server.snapshot(client.menu).unwrap(), before);
    assert_eq!(client.tally(), world.tally(client.menu));
}

#[test]
fn two_clients_on_one_container_see_each_others_changes() {
    let mut world = World::new(6);
    let mut a = world.join();
    let mut b = world.join();

    assert!(a.click(left(0)), "a picks the stone up");
    world.settle(&mut [&mut a, &mut b]);

    assert_eq!(world.slot(a.menu, 0), None);
    assert_eq!(
        b.corrections,
        vec![(SlotIx(0), None)],
        "b was told about the one slot that changed"
    );
    assert_eq!(b.slot(SlotIx(0)), None, "and its copy agrees");
    assert!(
        a.corrections.is_empty(),
        "a predicted it, so a was only acked"
    );

    // Now b puts its own stack where a took one from.
    b.corrections.clear();
    assert!(b.click(left(6)), "b picks up its hotbar stone");
    assert!(b.click(left(0)), "and drops it in the chest");
    world.settle(&mut [&mut a, &mut b]);

    assert_eq!(world.slot(a.menu, 0), stack(STONE, 5));
    assert_eq!(a.slot(SlotIx(0)), stack(STONE, 5), "a sees b's stack");
    assert!(
        a.corrections.contains(&(SlotIx(0), stack(STONE, 5))),
        "and was told about it slot by slot: {:?}",
        a.corrections
    );
}

#[test]
fn conservation_holds_on_both_sides_across_a_thousand_lossy_actions() {
    let mut world = World::new(20_260_906);
    let mut client = world.join();
    world.link.set_conditions(Conditions::lossy(5));

    let expected = world.tally(client.menu);
    let mut rng: u64 = 0x9E37_79B9_7F4A_7C15;
    let mut random = move || {
        rng ^= rng >> 12;
        rng ^= rng << 25;
        rng ^= rng >> 27;
        rng.wrapping_mul(0x2545_F491_4F6C_DD1D)
    };

    for step in 0..1000_u32 {
        let slot = u16::try_from(random() % 8).unwrap();
        let action = match random() % 8 {
            0 => ClickAction::QuickMove { slot: SlotIx(slot) },
            1 => ClickAction::Pickup {
                slot: SlotIx(slot),
                button: Button::Right,
            },
            2 => ClickAction::Throw {
                slot: SlotIx(slot),
                all: random() % 2 == 0,
            },
            3 => ClickAction::PickupAll {
                slot: SlotIx(slot),
                reverse: false,
            },
            4 => ClickAction::Swap {
                slot: SlotIx(slot),
                hotbar: u8::try_from(random() % 4).unwrap(),
            },
            5 => ClickAction::Toolbar(ToolbarAction::QuickStack {
                from: MenuDef::PLAYER_MAIN,
                to: MenuDef::CONTAINER,
            }),
            6 => ClickAction::Drag {
                stage: match random() % 3 {
                    0 => DragStage::Start,
                    1 => DragStage::Add,
                    _ => DragStage::End,
                },
                kind: DragKind::Left,
                slot: Some(SlotIx(slot)),
            },
            _ => left(slot),
        };
        client.click(action);
        if step % 3 == 0 {
            world.settle(&mut [&mut client]);
        }
    }
    // Let everything still in flight finish, on a link that works again.
    world.link.set_conditions(Conditions::PERFECT);
    for _ in 0..40 {
        world.settle(&mut [&mut client]);
    }

    assert!(
        world.link.dropped() > 0,
        "the link really did lose messages"
    );
    // `Throw` moves items out of the menu and into the world, so the tally
    // only has to be conserved *up to* what was thrown; both sides must
    // nonetheless agree, and neither may invent an item.
    let server_total: u64 = world.tally(client.menu).iter().map(|(_, n)| n).sum();
    let expected_total: u64 = expected.iter().map(|(_, n)| n).sum();
    assert!(
        server_total <= expected_total,
        "the server minted items: {server_total} from {expected_total}"
    );
    assert_eq!(
        client.tally(),
        world.tally(client.menu),
        "client and server ended on the same items"
    );
    assert_eq!(
        client.all_slots(),
        (0..8)
            .map(|i| world.slot(client.menu, i))
            .collect::<Vec<_>>(),
        "and on the same slots"
    );
    assert_eq!(client.authority.in_flight(), 0);
}

#[test]
fn the_server_validates_in_release_builds_too() {
    // The level is not a debug assertion: a `MenuServer` runs `Always`
    // whatever the build profile, which is the whole point of the setting.
    assert!(ValidationLevel::Always.checks(left(0)));
    let mut world = World::new(7);
    let mut client = world.join();
    // A cheat give from a survival actor: refused by the server's own copy of
    // the rules, not by anything the client chose to send.
    assert!(!client.click(ClickAction::Give {
        item: STONE,
        count: 64,
        target: slotted_model::GiveTarget::Cursor,
    }));
    assert_eq!(
        world.tally(client.menu),
        tally(&starting_inventories(), None)
    );
}
