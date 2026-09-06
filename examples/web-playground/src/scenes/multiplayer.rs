//! Scene 7: two clients and a server over a lossy loopback link.
//!
//! `docs/design/showcase-contract.md` section 3.6. All three run in this tab,
//! in the one Bevy world, with no workers and no sockets: a
//! [`MenuServer`] over a container store, a [`Loopback`] with adjustable
//! latency and loss, and two [`RemoteAuthority`] clients over it. Two
//! `demo:chest` screens sit side by side at half width, bound to one shared
//! chest container and to a private player inventory each, so a click on the
//! left is a click the right one has to be told about.
//!
//! The one thing that needed inventing is the routing.
//! [`slotted_ecs::Authority`](slotted::ecs::Authority) is a single resource,
//! and here there are two clients; [`RoutingAuthority`] is the adapter that
//! makes that work, dispatching each submission to the client that owns the
//! menu it names. It is a scene's business, not a library's: a game has one
//! player and one connection.

use std::collections::HashMap;
use std::sync::Arc;

use bevy::prelude::*;
use slotted::prelude::*;
use slotted_model::{
    Actor, Authority as AuthorityPort, AuthorityError, AuthorityEvent, ClickAction, Delta, MenuId,
    ResyncRequest, ValidationLevel,
};
use slotted_net::{
    ClientEnd, Conditions, Loopback, MenuServer, Outcome, PeerId, RemoteAuthority, ServerEnd,
};
use slotted_registry::FrozenRegistries;

use crate::bus::Bus;
use crate::scenes;
use crate::showcase::SceneHandler;
use crate::snapshot;

/// One of the two clients' screen roots, which share the canvas.
#[derive(Component)]
pub struct ClientScreen;

/// Shrinks each client's screen until it fits the half of the canvas it has.
///
/// `PostUpdate`, after the layout has measured: the root is a column of half
/// the canvas and the chest panel under it is whatever nine slots and an
/// action rail come to, which is wider. Scaling the root by the ratio is what
/// makes "side by side" true rather than "one on top of the other". A panel
/// that already fits is left alone, so a wide window shows both at full size.
///
/// The 0.96 is a gutter: two panels that exactly meet in the middle read as
/// one panel with a seam.
pub fn fit_client_screens(
    mut roots: Query<(&ComputedNode, &Children, &mut UiTransform), With<ClientScreen>>,
    panels: Query<&ComputedNode>,
) {
    for (root, children, mut transform) in &mut roots {
        let available = root.size().x * 0.96;
        let widest = children
            .iter()
            .filter_map(|child| panels.get(child).ok())
            .map(|node| node.size().x)
            .fold(0.0_f32, f32::max);
        if widest <= 0.0 || available <= 0.0 {
            continue;
        }
        let scale = (available / widest).min(1.0);
        if (transform.scale.x - scale).abs() > 0.005 {
            transform.scale = Vec2::splat(scale);
        }
    }
}

/// The two players. `PeerId(0)` is the server, so the clients start at one.
const ALICE: PeerId = PeerId(1);
const BOB: PeerId = PeerId(2);

/// Frames per second the loopback's tick is worth, for turning the page's
/// milliseconds into link ticks.
const FPS: u32 = 60;

/// Scene 7.
pub struct MultiplayerScene;

impl SceneHandler for MultiplayerScene {
    fn enter(&self, world: &mut World) {
        let bus = world.resource::<Bus>().clone();
        let Some(registries) = scenes::registries(world) else {
            bus.log(
                "error",
                "showcase",
                "multiplayer needs the mods to load first",
            );
            return;
        };
        match build(world, &registries) {
            Ok(link) => {
                bus.log(
                    "net",
                    "net",
                    "server up: one shared chest, two clients over a loopback",
                );
                world.insert_resource(link);
            }
            Err(message) => bus.log("error", "showcase", message),
        }
    }

    fn leave(&self, world: &mut World) {
        scenes::teardown(world);
        world.remove_resource::<NetLink>();
        // Back to single player: the prediction is the truth again, and the
        // next scene's clicks must not queue up behind a server that is gone.
        world.insert_resource(slotted::ecs::Authority::local());
    }
}

// ---------------------------------------------------------------------------
// The link
// ---------------------------------------------------------------------------

/// The server, the link and the two sessions, for as long as the scene is up.
#[derive(Resource)]
pub struct NetLink {
    /// The authoritative side.
    pub server: MenuServer,
    /// The link both clients cross.
    pub link: Loopback,
    /// The server's end of it.
    pub end: ServerEnd,
    /// Which peer owns which session, for the message log.
    pub sessions: Vec<(MenuId, PeerId)>,
    /// Messages the server has answered, for the counter the page shows.
    pub answered: u64,
}

impl NetLink {
    /// Applies the page's latency and loss sliders.
    ///
    /// Latency arrives in milliseconds because that is what a person
    /// understands; the link counts ticks, and a tick here is a frame, so the
    /// two are one division apart. A round trip costs twice what the slider
    /// says, which is also what a person expects of a ping.
    pub fn set_conditions(&self, latency_ms: u32, drop_percent: u8) {
        self.link.set_conditions(Conditions {
            latency: (latency_ms * FPS).div_ceil(1000),
            jitter: 0,
            drop_percent: drop_percent.min(100),
        });
    }
}

/// The server's own state, for a snapshot that survives a restart.
///
/// The contract (section 4) asks for the server's container rather than a
/// client's, and the reason is that the two are allowed to disagree: a client
/// applies a click straight away and the server may say no. A snapshot taken
/// from a client is therefore a snapshot of a guess, and restoring it into a
/// freshly built pair would promote that guess to the truth.
///
/// `None` when the sessions and the server disagree about what is bound, which
/// can only happen if a session was closed underneath this; a snapshot is then
/// better skipped than written half-right.
#[must_use]
pub fn capture(link: &NetLink, registries: &FrozenRegistries) -> Option<snapshot::NetSnapshot> {
    let store = link.server.store();
    let mut chest: Option<snapshot::InventorySnapshot> = None;
    let mut players = Vec::with_capacity(link.sessions.len());
    let mut carried = Vec::with_capacity(link.sessions.len());

    for (menu, _peer) in &link.sessions {
        let bindings = link.server.bindings_of(*menu)?;
        // The shared chest first, then that peer's private inventories: the
        // order `build` binds them in, which is the order they are restored.
        let (shared, private) = bindings.split_first()?;
        if chest.is_none() {
            chest = Some(snapshot::capture_inventory(
                registries,
                store.inventory(*shared)?,
            ));
        }
        let mut pockets = Vec::with_capacity(private.len());
        for id in private {
            pockets.push(snapshot::capture_inventory(
                registries,
                store.inventory(*id)?,
            ));
        }
        players.push(pockets);
        // And whatever is on that peer's cursor, which is in no container at
        // all and would otherwise be the one thing a restart could lose.
        carried.push(
            link.server
                .snapshot(*menu)
                .and_then(|state| state.state.carried)
                .and_then(|stack| {
                    Some(snapshot::Carried {
                        item: registries.items.name_of(stack.id)?.to_string(),
                        count: stack.count,
                    })
                }),
        );
    }

    Some(snapshot::NetSnapshot {
        chest: chest?,
        players,
        carried,
    })
}

/// Puts a [`capture`] back into a freshly built server, and answers how many
/// stacks were dropped because the current registries have no such item.
///
/// The server only. The clients' local mirrors are written by the caller from
/// the same snapshot, because they are entities in the world and this has only
/// the resource; a client left holding its own contents would show the old
/// chest until the next correction reached it.
pub fn restore(
    link: &mut NetLink,
    registries: &FrozenRegistries,
    wanted: &snapshot::NetSnapshot,
) -> usize {
    let bound: Vec<Vec<slotted_net::InventoryId>> = link
        .sessions
        .iter()
        .map(|(menu, _)| {
            link.server
                .bindings_of(*menu)
                .map(<[slotted_net::InventoryId]>::to_vec)
                .unwrap_or_default()
        })
        .collect();

    let mut dropped = 0;
    let mut chest_done = false;
    let store = link.server.store_mut();
    for (index, bindings) in bound.iter().enumerate() {
        let Some((shared, private)) = bindings.split_first() else {
            continue;
        };
        // Once: both sessions are bound to the same shared chest, and writing
        // it twice would be the same write twice.
        if !chest_done {
            if let Some(inventory) = store.inventory_mut(*shared) {
                dropped += snapshot::apply_inventory(&wanted.chest, registries, inventory);
            }
            chest_done = true;
        }
        let Some(pockets) = wanted.players.get(index) else {
            continue;
        };
        for (slot, id) in private.iter().enumerate() {
            let Some(source) = pockets.get(slot) else {
                continue;
            };
            if let Some(inventory) = store.inventory_mut(*id) {
                dropped += snapshot::apply_inventory(source, registries, inventory);
            }
        }
    }
    dropped
}

/// The inventories one session is bound to, in the order a client's
/// `OpenMenu` holds them: the shared chest, then that peer's own.
///
/// The caller writes the client's entities from this, so a restored server and
/// the two screens over it agree before the first correction.
#[must_use]
pub fn inventories_of(
    wanted: &snapshot::NetSnapshot,
    session: usize,
) -> Vec<&snapshot::InventorySnapshot> {
    let mut out = vec![&wanted.chest];
    if let Some(pockets) = wanted.players.get(session) {
        out.extend(pockets.iter());
    }
    out
}

/// `PostUpdate`: one tick of the link, then everything the server has to say.
///
/// The order matters. Advancing first is what makes a message sent last frame
/// arrive this one; pumping after it means a click submitted in `Update` is
/// answered no earlier than the latency allows, which is the whole point of
/// having a link at all.
pub fn pump_link(world: &mut World) {
    let Some(mut link) = world.remove_resource::<NetLink>() else {
        return;
    };
    let bus = world.resource::<Bus>().clone();
    link.link.advance(1);
    let outcomes = {
        let registries = world.resource::<Registries>().clone();
        let lookup = registries.lookup();
        link.server.pump(&link.end, &lookup)
    };
    for outcome in &outcomes {
        link.answered += 1;
        log_outcome(&bus, link.answered, *outcome);
    }
    link.server.flush(&link.end);
    world.insert_resource(link);
}

/// One server answer as a log line the page's message pane renders.
///
/// The `who` is `net` rather than a mod id, which is how the page tells these
/// apart from console output and puts them in their own pane.
fn log_outcome(bus: &Bus, seq: u64, outcome: Outcome) {
    let (level, text) = match outcome {
        Outcome::Acked => ("info", "Ack: the client predicted it right".to_owned()),
        Outcome::Corrected => (
            "warn",
            "SetContent: the prediction was wrong, whole container sent".to_owned(),
        ),
        Outcome::Refused(error) => ("error", format!("SetContent: click refused, {error}")),
        Outcome::Duplicate => (
            "info",
            "Ack: a retransmission, the recorded answer repeated".to_owned(),
        ),
        Outcome::Resynced => ("info", "SetContent: answering a resync request".to_owned()),
        Outcome::Closed => ("info", "Closed: the client left the session".to_owned()),
        Outcome::Denied(reason) => ("error", format!("Refused: {reason}")),
    };
    bus.log(level, "net", format!("#{seq} {text}"));
}

// ---------------------------------------------------------------------------
// One authority over two clients
// ---------------------------------------------------------------------------

/// Dispatches submissions to whichever client owns the menu.
///
/// `slotted-ecs` has one `Authority` resource because a game has one player.
/// This scene has two on one canvas, so something has to sit in that resource
/// and know which is which; a menu id is the only thing a submission carries
/// that says so, and the scene knows which id it opened for whom.
///
/// `poll` drains both, because the prediction loop asks the authority once a
/// frame and each client's answers are about its own menu anyway.
pub struct RoutingAuthority {
    clients: Vec<Arc<RemoteAuthority<ClientEnd>>>,
    by_menu: HashMap<u32, usize>,
    bus: Bus,
}

impl RoutingAuthority {
    fn client_of(&self, menu: MenuId) -> Option<&RemoteAuthority<ClientEnd>> {
        self.by_menu
            .get(&menu.0)
            .and_then(|index| self.clients.get(*index))
            .map(AsRef::as_ref)
    }
}

impl AuthorityPort for RoutingAuthority {
    fn submit(
        &self,
        menu: MenuId,
        action: ClickAction,
        predicted: &Delta,
    ) -> Result<(), AuthorityError> {
        let Some(client) = self.client_of(menu) else {
            // A menu this scene did not open: the only way here is a screen
            // that outlived its scene, and answering `Disconnected` makes the
            // prediction loop stop rather than silently diverge.
            return Err(AuthorityError::Disconnected);
        };
        let peer = self.by_menu.get(&menu.0).map_or(0, |i| i + 1);
        self.bus.log(
            "info",
            "net",
            format!("ClickContainer from client {peer}: {}", name_of(&action)),
        );
        client.submit(menu, action, predicted)
    }

    fn poll(&self) -> Vec<AuthorityEvent> {
        self.clients.iter().flat_map(|c| c.poll()).collect()
    }

    fn request_resync(&self, menu: MenuId) -> Result<ResyncRequest, AuthorityError> {
        match self.client_of(menu) {
            Some(client) => client.request_resync(menu),
            None => Ok(ResyncRequest::Unsupported),
        }
    }

    fn validation(&self) -> ValidationLevel {
        // The server validates every click at `Always`, so the clients had
        // better be checking the same thing before they predict it.
        ValidationLevel::Always
    }
}

/// A click action as the one word the message log shows.
fn name_of(action: &ClickAction) -> &'static str {
    match action {
        ClickAction::Pickup { .. } => "Pickup",
        ClickAction::QuickMove { .. } => "QuickMove",
        ClickAction::Swap { .. } => "Swap",
        ClickAction::Clone { .. } => "Clone",
        ClickAction::Throw { .. } => "Throw",
        ClickAction::Drag { .. } => "Drag",
        ClickAction::PickupAll { .. } => "PickupAll",
        _ => "Click",
    }
}

// ---------------------------------------------------------------------------
// Setting it up
// ---------------------------------------------------------------------------

/// Builds the server, the link, both clients and both screens.
fn build(world: &mut World, registries: &FrozenRegistries) -> Result<NetLink, String> {
    let def = showcase::chest::menu_def();
    let seeded = showcase::chest::inventories(registries);
    let (container, player_main, hotbar) = match seeded.as_slice() {
        [container, main, hotbar] => (container.clone(), main.clone(), hotbar.clone()),
        other => {
            return Err(format!(
                "the chest menu has {} inventories, not 3",
                other.len()
            ));
        }
    };

    // One store, one shared chest, and a private pocket set per player. The
    // sharing is the demonstration: `add_shared` is the whole of "both of you
    // are looking at the same box", and `add_private` is the whole of "and
    // neither of you can reach into the other's pockets".
    let mut server = MenuServer::new();
    let chest = server.add_shared(container.clone());
    let mut bindings = HashMap::new();
    for peer in [ALICE, BOB] {
        bindings.insert(
            peer,
            vec![
                chest,
                server.add_private(peer, player_main.clone()),
                server.add_private(peer, hotbar.clone()),
            ],
        );
    }

    let link = Loopback::new(0x5107_7ED0);
    let end = link.server();
    let screen = scenes::register_screen(world, showcase::chest::screen());

    let mut clients = Vec::new();
    let mut by_menu = HashMap::new();
    let mut sessions = Vec::new();
    let mut roots = Vec::new();

    for (index, peer) in [ALICE, BOB].into_iter().enumerate() {
        // Each client keeps its own copy of the world's state, which is what
        // makes a correction visible: the entities here are the client's, the
        // store's inventories are the server's, and they can disagree.
        let entities: Vec<Entity> = [container.clone(), player_main.clone(), hotbar.clone()]
            .into_iter()
            .map(|inventory| world.spawn(slotted::ecs::menu::Inventory(inventory)).id())
            .collect();

        let mut ids = world
            .remove_resource::<slotted::ecs::MenuIdAllocator>()
            .unwrap_or_default();
        let menu = {
            let mut commands = world.commands();
            open_menu(
                &mut commands,
                &mut ids,
                def.clone(),
                entities,
                Actor::SURVIVAL,
            )
        };
        world.insert_resource(ids);
        world.flush();

        let id = world
            .get::<OpenMenu>(menu)
            .map(|open| open.id)
            .ok_or_else(|| "the menu did not open".to_owned())?;
        let bound = bindings.remove(&peer).unwrap_or_default();
        server
            .open(id, peer, (*def).clone(), bound, Actor::SURVIVAL)
            .map_err(|e| format!("opening the server session for client {}: {e}", index + 1))?;

        clients.push(Arc::new(RemoteAuthority::new(link.client(peer))));
        by_menu.insert(id.0, index);
        sessions.push((id, peer));

        let root = {
            let mut commands = world.commands();
            spawn_screen(&mut commands, screen.clone(), Some(menu))
        };
        world.flush();
        roots.push(root);
    }

    // Side by side. A screen root is a full-window centring node, so two of
    // them would sit on top of each other; halving the width and pushing the
    // second one over is the whole of "two players, one canvas".
    //
    // Halving the column is not on its own enough. The chest panel inside it
    // is as wide as nine slots and an action rail make it, which is wider than
    // half a canvas at any window a person is likely to have; it overflowed
    // its column and the left client's Sort, Quick stack, Deposit all and Loot
    // all buttons ended up behind the right client's panel. So the root also
    // carries a scale, which [`fit_client_screens`] sets from what the panel
    // actually measures each frame.
    for (index, root) in roots.into_iter().enumerate() {
        if let Some(mut node) = world.get_mut::<Node>(root) {
            node.width = Val::Percent(50.0);
            node.left = Val::Percent(if index == 0 { 0.0 } else { 50.0 });
        }
        world
            .entity_mut(root)
            .insert((ClientScreen, UiTransform::IDENTITY));
    }

    let bus = world.resource::<Bus>().clone();
    world.insert_resource(slotted::ecs::Authority::new(RoutingAuthority {
        clients,
        by_menu,
        bus,
    }));

    let net = NetLink {
        server,
        link,
        end,
        sessions,
        answered: 0,
    };
    net.set_conditions(0, 0);
    Ok(net)
}
