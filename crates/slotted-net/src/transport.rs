//! The [`Transport`] port and the in-process [`Loopback`] adapter.
//!
//! Two methods, `send` and `poll`, and no opinion about sockets, threads or
//! replication crates. Everything above this line is testable by driving
//! `Loopback` by hand, which is why the interesting tests in this crate need
//! no network at all.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, MutexGuard};

use crate::message::{ClientMessage, PeerId, ServerMessage};

/// Why a message could not be handed to the transport.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TransportError {
    /// The peer is not connected (any more).
    #[error("peer {0:?} is not connected")]
    NoSuchPeer(PeerId),
    /// The transport is shut down.
    #[error("transport is closed")]
    Closed,
    /// Anything the adapter wants to report in its own words.
    #[error("{0}")]
    Other(String),
}

/// One end of a connection.
///
/// A client end sends [`ClientMessage`] and receives [`ServerMessage`]; a
/// server end is the same trait with the two swapped. Both directions are one
/// trait so an adapter writes the plumbing once.
///
/// Neither method blocks. `poll` returns what has arrived and nothing else,
/// so a caller drives it once per frame.
pub trait Transport: Send + Sync {
    /// What this end sends.
    type Out;
    /// What this end receives.
    type In;

    /// Queues `message` for `to`. A client passes [`PeerId::SERVER`].
    fn send(&self, to: PeerId, message: Self::Out) -> Result<(), TransportError>;

    /// Takes everything that has arrived since the last call, in the order it
    /// arrived, with the peer that sent it.
    fn poll(&self) -> Vec<(PeerId, Self::In)>;

    /// Everyone this end can currently send to. A client end reports its
    /// server; a server end reports its clients.
    fn peers(&self) -> Vec<PeerId>;

    /// Sends `message` to every peer. Reports the first failure and keeps
    /// going, so one dead peer does not silence the rest.
    fn broadcast(&self, message: Self::Out) -> Result<(), TransportError>
    where
        Self::Out: Clone,
    {
        let mut first = None;
        for peer in self.peers() {
            if let Err(error) = self.send(peer, message.clone())
                && first.is_none()
            {
                first = Some(error);
            }
        }
        first.map_or(Ok(()), Err)
    }
}

/// A client end behind a pointer, for a caller that must choose its transport
/// at runtime rather than at compile time.
///
/// The facade's `net` feature uses this: a game inserts one of these as a
/// resource before adding the plugins, and the wiring swaps
/// `LocalAuthority` for a [`RemoteAuthority`](crate::RemoteAuthority) over it
/// without the plugin group having to be generic.
pub type BoxedClientTransport = Box<dyn Transport<Out = ClientMessage, In = ServerMessage>>;

/// A server end behind a pointer. The counterpart of
/// [`BoxedClientTransport`].
pub type BoxedServerTransport = Box<dyn Transport<Out = ServerMessage, In = ClientMessage>>;

impl<O, I> Transport for Box<dyn Transport<Out = O, In = I>> {
    type Out = O;
    type In = I;

    fn send(&self, to: PeerId, message: O) -> Result<(), TransportError> {
        (**self).send(to, message)
    }

    fn poll(&self) -> Vec<(PeerId, I)> {
        (**self).poll()
    }

    fn peers(&self) -> Vec<PeerId> {
        (**self).peers()
    }
}

/// A client end: sends [`ClientMessage`], receives [`ServerMessage`].
pub trait ClientTransport: Transport<Out = ClientMessage, In = ServerMessage> {}
impl<T: Transport<Out = ClientMessage, In = ServerMessage>> ClientTransport for T {}

/// A server end: sends [`ServerMessage`], receives [`ClientMessage`].
pub trait ServerTransport: Transport<Out = ServerMessage, In = ClientMessage> {}
impl<T: Transport<Out = ServerMessage, In = ClientMessage>> ServerTransport for T {}

// ---------------------------------------------------------------------------
// Loopback
// ---------------------------------------------------------------------------

/// What a [`Loopback`] link does to the messages crossing it.
///
/// All of it is deterministic: the same conditions and the same seed give the
/// same delivery order every run, so a test that catches a reordering bug
/// catches it again.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Conditions {
    /// Ticks a message waits before it can be polled. `0` delivers on the
    /// tick it was sent.
    pub latency: u32,
    /// Extra ticks, uniform in `0..=jitter`, added per message. Non-zero
    /// jitter is what reorders a stream: two messages sent in one tick can
    /// come out in either order.
    pub jitter: u32,
    /// Percentage of messages thrown away, `0..=100`.
    pub drop_percent: u8,
}

impl Conditions {
    /// A link that delivers everything, immediately, in order.
    pub const PERFECT: Self = Self {
        latency: 0,
        jitter: 0,
        drop_percent: 0,
    };

    /// `latency` ticks each way and nothing else.
    pub const fn delayed(latency: u32) -> Self {
        Self {
            latency,
            jitter: 0,
            drop_percent: 0,
        }
    }

    /// A link that loses `percent` of what crosses it.
    pub const fn lossy(percent: u8) -> Self {
        Self {
            latency: 0,
            jitter: 0,
            drop_percent: percent,
        }
    }

    /// A link that delivers everything but not in the order it was given.
    pub const fn reordering(jitter: u32) -> Self {
        Self {
            latency: 0,
            jitter,
            drop_percent: 0,
        }
    }
}

impl Default for Conditions {
    fn default() -> Self {
        Self::PERFECT
    }
}

#[derive(Debug)]
struct Queued<M> {
    due: u64,
    /// Send order, to break ties without depending on `Vec` internals.
    order: u64,
    from: PeerId,
    message: M,
}

#[derive(Debug, Default)]
struct LinkState {
    now: u64,
    order: u64,
    rng: u64,
    conditions: Conditions,
    next_peer: u64,
    to_server: Vec<Queued<ClientMessage>>,
    to_client: BTreeMap<PeerId, Vec<Queued<ServerMessage>>>,
    /// Messages the conditions threw away, for a test to assert on.
    dropped: u64,
}

impl LinkState {
    /// xorshift64*: small, deterministic and good enough to decide whether a
    /// packet lives.
    fn next_random(&mut self) -> u64 {
        let mut x = self.rng;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.rng = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// `(due, order)` for a message sent now, or `None` when the link eats it.
    fn schedule(&mut self) -> Option<(u64, u64)> {
        let conditions = self.conditions;
        if conditions.drop_percent > 0 {
            let roll = self.next_random() % 100;
            if roll < u64::from(conditions.drop_percent) {
                self.dropped += 1;
                return None;
            }
        }
        let jitter = if conditions.jitter == 0 {
            0
        } else {
            self.next_random() % u64::from(conditions.jitter + 1)
        };
        self.order += 1;
        Some((
            self.now + u64::from(conditions.latency) + jitter,
            self.order,
        ))
    }
}

fn take_due<M>(queue: &mut Vec<Queued<M>>, now: u64) -> Vec<(PeerId, M)> {
    let mut due: Vec<Queued<M>> = Vec::new();
    let mut later: Vec<Queued<M>> = Vec::new();
    for item in queue.drain(..) {
        if item.due <= now {
            due.push(item);
        } else {
            later.push(item);
        }
    }
    *queue = later;
    due.sort_by_key(|q| (q.due, q.order));
    due.into_iter().map(|q| (q.from, q.message)).collect()
}

/// An in-process link between one server and any number of clients.
///
/// Nothing here runs on its own. [`tick`](Self::tick) advances the link's
/// clock, which is what makes latency and reordering happen at a moment a
/// test chooses rather than at a moment the operating system chooses.
///
/// ```
/// use slotted_net::{ClientMessage, Loopback, MenuId, PeerId, Transport};
///
/// let link = Loopback::new(1);
/// let client = link.client(PeerId(1));
/// let server = link.server();
///
/// client
///     .send(PeerId::SERVER, ClientMessage::RequestResync { menu: MenuId(1) })
///     .unwrap();
/// assert_eq!(server.poll().len(), 1);
/// ```
#[derive(Debug, Clone)]
pub struct Loopback {
    state: Arc<Mutex<LinkState>>,
}

impl Loopback {
    /// A perfect link, seeded for its random decisions.
    pub fn new(seed: u64) -> Self {
        Self {
            state: Arc::new(Mutex::new(LinkState {
                // xorshift is stuck at zero, so a zero seed would be a link
                // that never drops anything however lossy it is set to be.
                rng: seed | 1,
                ..LinkState::default()
            })),
        }
    }

    fn lock(&self) -> MutexGuard<'_, LinkState> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Replaces the link conditions. Messages already in flight keep the
    /// delivery time they were given.
    pub fn set_conditions(&self, conditions: Conditions) {
        self.lock().conditions = conditions;
    }

    /// Advances the link's clock by one tick.
    pub fn tick(&self) {
        self.lock().now += 1;
    }

    /// Advances the clock by `n` ticks.
    pub fn advance(&self, n: u64) {
        self.lock().now += n;
    }

    /// How many messages the conditions have thrown away so far.
    pub fn dropped(&self) -> u64 {
        self.lock().dropped
    }

    /// `true` when nothing is waiting to be delivered on either side.
    pub fn is_quiet(&self) -> bool {
        let state = self.lock();
        state.to_server.is_empty() && state.to_client.values().all(Vec::is_empty)
    }

    /// The client end for `peer`, registering it as connected.
    pub fn client(&self, peer: PeerId) -> ClientEnd {
        self.lock().to_client.entry(peer).or_default();
        ClientEnd {
            link: self.clone(),
            peer,
        }
    }

    /// A client end with an id nobody has used yet.
    pub fn add_client(&self) -> ClientEnd {
        let peer = {
            let mut state = self.lock();
            state.next_peer += 1;
            PeerId(state.next_peer)
        };
        self.client(peer)
    }

    /// The server end.
    pub fn server(&self) -> ServerEnd {
        ServerEnd { link: self.clone() }
    }
}

/// One client's end of a [`Loopback`].
#[derive(Debug, Clone)]
pub struct ClientEnd {
    link: Loopback,
    peer: PeerId,
}

impl ClientEnd {
    /// Which client this end is, as the server sees it.
    pub fn peer(&self) -> PeerId {
        self.peer
    }
}

impl Transport for ClientEnd {
    type Out = ClientMessage;
    type In = ServerMessage;

    fn send(&self, _to: PeerId, message: ClientMessage) -> Result<(), TransportError> {
        let mut state = self.link.lock();
        let Some((due, order)) = state.schedule() else {
            return Ok(());
        };
        state.to_server.push(Queued {
            due,
            order,
            from: self.peer,
            message,
        });
        Ok(())
    }

    fn poll(&self) -> Vec<(PeerId, ServerMessage)> {
        let mut state = self.link.lock();
        let now = state.now;
        let Some(queue) = state.to_client.get_mut(&self.peer) else {
            return Vec::new();
        };
        take_due(queue, now)
    }

    fn peers(&self) -> Vec<PeerId> {
        vec![PeerId::SERVER]
    }
}

/// The server's end of a [`Loopback`].
#[derive(Debug, Clone)]
pub struct ServerEnd {
    link: Loopback,
}

impl Transport for ServerEnd {
    type Out = ServerMessage;
    type In = ClientMessage;

    fn send(&self, to: PeerId, message: ServerMessage) -> Result<(), TransportError> {
        let mut state = self.link.lock();
        if !state.to_client.contains_key(&to) {
            return Err(TransportError::NoSuchPeer(to));
        }
        let Some((due, order)) = state.schedule() else {
            return Ok(());
        };
        state.to_client.entry(to).or_default().push(Queued {
            due,
            order,
            from: PeerId::SERVER,
            message,
        });
        Ok(())
    }

    fn poll(&self) -> Vec<(PeerId, ClientMessage)> {
        let mut state = self.link.lock();
        let now = state.now;
        let mut queue = std::mem::take(&mut state.to_server);
        let out = take_due(&mut queue, now);
        state.to_server = queue;
        out
    }

    fn peers(&self) -> Vec<PeerId> {
        self.link.lock().to_client.keys().copied().collect()
    }
}

#[allow(clippy::unwrap_used)]
#[cfg(test)]
mod tests {
    use super::*;
    use slotted_model::MenuId;

    fn resync(n: u32) -> ClientMessage {
        ClientMessage::RequestResync { menu: MenuId(n) }
    }

    #[test]
    fn a_perfect_link_delivers_in_order_on_the_same_tick() {
        let link = Loopback::new(7);
        let client = link.client(PeerId(1));
        let server = link.server();
        for n in 0..3 {
            client.send(PeerId::SERVER, resync(n)).unwrap();
        }
        let got: Vec<_> = server.poll().into_iter().map(|(_, m)| m.menu()).collect();
        assert_eq!(got, vec![MenuId(0), MenuId(1), MenuId(2)]);
        assert!(link.is_quiet());
    }

    #[test]
    fn latency_holds_a_message_until_its_tick() {
        let link = Loopback::new(7);
        link.set_conditions(Conditions::delayed(2));
        let client = link.client(PeerId(1));
        let server = link.server();
        client.send(PeerId::SERVER, resync(0)).unwrap();
        assert!(server.poll().is_empty());
        link.tick();
        assert!(server.poll().is_empty());
        link.tick();
        assert_eq!(server.poll().len(), 1);
    }

    #[test]
    fn jitter_reorders_a_stream() {
        let link = Loopback::new(12345);
        link.set_conditions(Conditions::reordering(4));
        let client = link.client(PeerId(1));
        let server = link.server();
        for n in 0..8 {
            client.send(PeerId::SERVER, resync(n)).unwrap();
        }
        link.advance(8);
        let got: Vec<u32> = server.poll().into_iter().map(|(_, m)| m.menu().0).collect();
        assert_eq!(got.len(), 8, "nothing is lost, only shuffled");
        let mut sorted = got.clone();
        sorted.sort_unstable();
        assert_eq!(sorted, (0..8).collect::<Vec<_>>());
        assert_ne!(got, sorted, "and the order really did change");
    }

    #[test]
    fn a_lossy_link_loses_roughly_its_share() {
        let link = Loopback::new(99);
        link.set_conditions(Conditions::lossy(50));
        let client = link.client(PeerId(1));
        let server = link.server();
        for n in 0..1000 {
            client.send(PeerId::SERVER, resync(n)).unwrap();
        }
        let arrived = server.poll().len();
        assert_eq!(arrived as u64 + link.dropped(), 1000);
        assert!(
            (400..600).contains(&arrived),
            "half of a thousand, give or take: {arrived}"
        );
    }

    #[test]
    fn the_server_reaches_every_client_and_only_connected_ones() {
        let link = Loopback::new(3);
        let a = link.add_client();
        let b = link.add_client();
        let server = link.server();
        assert_eq!(server.peers(), vec![a.peer(), b.peer()]);
        server
            .broadcast(ServerMessage::Ack {
                menu: MenuId(1),
                state_id: 1,
                seq: 1,
            })
            .unwrap();
        assert_eq!(a.poll().len(), 1);
        assert_eq!(b.poll().len(), 1);
        assert_eq!(
            server.send(
                PeerId(99),
                ServerMessage::SetProperty {
                    menu: MenuId(1),
                    id: slotted_model::PropertyId(0),
                    value: 1,
                }
            ),
            Err(TransportError::NoSuchPeer(PeerId(99)))
        );
    }
}
