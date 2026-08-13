//! LAN peer discovery: datagram codec and peer table.
//!
//! From `docs/references/PROTOCOL.md` §"Discovery". Unauthenticated UDP by
//! design — nothing here may gate a security decision. The advertised
//! `protocol_version`/`caps` are recorded as hints and deliberately unused.
//!
//! Subnet broadcast rather than multicast is the primary channel, per the
//! research report §3.1: link-layer flooding reaches peers that IGMP state
//! across mesh APs and wired/wireless bridges routinely loses.

use std::collections::HashMap;
use std::net::IpAddr;
use std::time::{Duration, Instant};

use serde_json::{Map, Value};

use crate::policy;
use crate::protocol;

/// The fixed UDP port both implementations bind.
pub const DISCOVERY_PORT: u16 = 52637;

/// Datagrams larger than this are dropped before parsing.
pub const MAX_DATAGRAM_BYTES: usize = 4096;

/// How long a peer stays listed after its last advertisement.
///
/// The Qt baseline has no expiry at all, so a device that loses power or
/// leaves the network is listed forever. Refreshes are about one second
/// apart, so this tolerates a long run of losses before a live peer is
/// dropped, while still retiring one that is genuinely gone.
pub const PEER_TTL: Duration = Duration::from_secs(15);

/// A decoded datagram.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Datagram {
    /// "Report in" — answer with a unicast advertisement to the source.
    Request,
    /// A peer describing itself.
    Advertisement(Advertisement),
}

/// A peer's self-description. `port == 0` means "I am not available".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Advertisement {
    pub device_name: String,
    pub device_type: String,
    pub port: u16,
    /// Untrusted hint. Never gate behavior on it.
    pub protocol_version: i64,
    /// Untrusted hint. Never gate behavior on it.
    pub caps: Vec<String>,
}

/// Classifies and validates one datagram payload.
///
/// Returns `None` for anything malformed or out of bounds; discovery drops
/// rather than repairs, because a datagram is cheap and the next refresh will
/// ask again.
pub fn parse_datagram(data: &[u8]) -> Option<Datagram> {
    if data.is_empty() || data.len() > MAX_DATAGRAM_BYTES {
        return None;
    }
    let value: Value = serde_json::from_slice(data).ok()?;
    let obj = value.as_object()?;

    // `request` is the discriminator and must be a real boolean: a missing or
    // mistyped value is neither a request nor an advertisement.
    match obj.get("request") {
        Some(Value::Bool(true)) => return Some(Datagram::Request),
        Some(Value::Bool(false)) => {}
        _ => return None,
    }

    let device_name = obj.get("device_name")?.as_str()?;
    if !policy::is_safe_device_name(device_name) {
        return None;
    }

    let raw_port = obj.get("port")?.as_f64()?;
    if !raw_port.is_finite() || raw_port < 0.0 || raw_port.fract() != 0.0 || raw_port > 65535.0 {
        return None;
    }

    Some(Datagram::Advertisement(Advertisement {
        device_name: device_name.to_string(),
        device_type: obj
            .get("device_type")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        port: raw_port as u16,
        protocol_version: protocol::parse_version(obj.get("protocol_version")),
        caps: protocol::parse_caps(obj.get("caps")).into_iter().collect(),
    }))
}

/// Builds the request datagram.
pub fn build_request() -> Vec<u8> {
    let mut obj = Map::new();
    obj.insert("request".into(), Value::Bool(true));
    serde_json::to_vec(&Value::Object(obj)).expect("Map<String, Value> always serializes")
}

/// Builds this device's advertisement. Pass `port == 0` to announce that this
/// device is not available.
pub fn build_advertisement(device_name: &str, device_type: &str, port: u16) -> Vec<u8> {
    let mut negotiation = Map::new();
    protocol::insert_negotiation_fields(&mut negotiation);

    // Lexicographic key order, matching the canonical form in PROTOCOL.md.
    let mut obj = Map::new();
    obj.insert("caps".into(), negotiation["caps"].clone());
    obj.insert("device_name".into(), Value::String(device_name.into()));
    obj.insert("device_type".into(), Value::String(device_type.into()));
    obj.insert("port".into(), Value::from(port));
    obj.insert(
        "protocol_version".into(),
        negotiation["protocol_version"].clone(),
    );
    obj.insert("request".into(), Value::Bool(false));
    serde_json::to_vec(&Value::Object(obj)).expect("Map<String, Value> always serializes")
}

/// A peer currently believed reachable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Peer {
    pub address: IpAddr,
    pub device_name: String,
    pub port: u16,
}

/// The live peer list.
///
/// Identity is the **source IP address**, per the specification: `device_name`
/// is display text that may change between advertisements and may collide
/// between devices. Entries age out on a last-seen basis, which is what makes
/// a device that vanished without announcing stop being offered to the user.
#[derive(Debug, Default)]
pub struct PeerTable {
    entries: HashMap<IpAddr, (Peer, Instant)>,
    ttl: Option<Duration>,
}

impl PeerTable {
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
            ttl: Some(PEER_TTL),
        }
    }

    /// A table that never expires entries, for callers driving their own
    /// lifetime policy.
    pub fn without_expiry() -> Self {
        Self {
            entries: HashMap::new(),
            ttl: None,
        }
    }

    pub fn with_ttl(ttl: Duration) -> Self {
        Self {
            entries: HashMap::new(),
            ttl: Some(ttl),
        }
    }

    /// Records an advertisement, seen at `now`.
    ///
    /// Returns true when the visible peer set changed — a peer appeared,
    /// disappeared, or renamed — so a caller can avoid redrawing on every
    /// refresh tick.
    pub fn observe(&mut self, address: IpAddr, ad: &Advertisement, now: Instant) -> bool {
        // port 0 is a removal, not a peer listening on port 0.
        if ad.port == 0 {
            return self.entries.remove(&address).is_some();
        }

        let peer = Peer {
            address,
            device_name: ad.device_name.clone(),
            port: ad.port,
        };
        match self.entries.insert(address, (peer.clone(), now)) {
            Some((previous, _)) => previous != peer,
            None => true,
        }
    }

    /// Drops peers not seen within the TTL. Returns how many were removed.
    pub fn expire(&mut self, now: Instant) -> usize {
        let Some(ttl) = self.ttl else {
            return 0;
        };
        let before = self.entries.len();
        self.entries
            .retain(|_, (_, seen)| now.duration_since(*seen) < ttl);
        before - self.entries.len()
    }

    /// Current peers, ordered by address so the list is stable for display.
    pub fn peers(&self) -> Vec<Peer> {
        let mut peers: Vec<Peer> = self.entries.values().map(|(p, _)| p.clone()).collect();
        peers.sort_by_key(|p| p.address);
        peers
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Addresses worth probing directly on a later start.
    ///
    /// Warm-starting from these with unicast requests is what makes a known
    /// device appear immediately on networks where broadcast is filtered —
    /// the "open it and the device is already there" behavior.
    pub fn known_addresses(&self) -> Vec<IpAddr> {
        let mut addresses: Vec<IpAddr> = self.entries.keys().copied().collect();
        addresses.sort();
        addresses
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn ad(name: &str, port: u16) -> Advertisement {
        Advertisement {
            device_name: name.into(),
            device_type: "test".into(),
            port,
            protocol_version: protocol::VERSION,
            caps: vec![protocol::CAP_ACK.to_string()],
        }
    }

    fn ip(last: u8) -> IpAddr {
        IpAddr::from([192, 168, 1, last])
    }

    #[test]
    fn request_round_trips() {
        assert_eq!(parse_datagram(&build_request()), Some(Datagram::Request));
    }

    #[test]
    fn advertisement_round_trips() {
        let wire = build_advertisement("MacBook", "macos", 52638);
        let Some(Datagram::Advertisement(parsed)) = parse_datagram(&wire) else {
            panic!("expected an advertisement");
        };
        assert_eq!(parsed.device_name, "MacBook");
        assert_eq!(parsed.device_type, "macos");
        assert_eq!(parsed.port, 52638);
        assert_eq!(parsed.protocol_version, protocol::VERSION);
        assert_eq!(parsed.caps, vec![protocol::CAP_ACK.to_string()]);
    }

    #[test]
    fn unavailable_advertisement_round_trips() {
        let wire = build_advertisement("Hidden", "linux", 0);
        let Some(Datagram::Advertisement(parsed)) = parse_datagram(&wire) else {
            panic!("expected an advertisement");
        };
        assert_eq!(parsed.port, 0);
    }

    #[test]
    fn drops_malformed_datagrams() {
        let cases: Vec<Vec<u8>> = vec![
            b"".to_vec(),
            b"not json".to_vec(),
            b"[1,2,3]".to_vec(),
            // `request` missing or mistyped.
            json!({"device_name": "a", "port": 1})
                .to_string()
                .into_bytes(),
            json!({"request": "true"}).to_string().into_bytes(),
            json!({"request": 1}).to_string().into_bytes(),
            // Advertisement field problems.
            json!({"request": false, "port": 1})
                .to_string()
                .into_bytes(),
            json!({"request": false, "device_name": "a"})
                .to_string()
                .into_bytes(),
            json!({"request": false, "device_name": 42, "port": 1})
                .to_string()
                .into_bytes(),
            json!({"request": false, "device_name": "a", "port": "1"})
                .to_string()
                .into_bytes(),
            json!({"request": false, "device_name": "a", "port": -1})
                .to_string()
                .into_bytes(),
            json!({"request": false, "device_name": "a", "port": 70000})
                .to_string()
                .into_bytes(),
            json!({"request": false, "device_name": "a", "port": 1.5})
                .to_string()
                .into_bytes(),
            json!({"request": false, "device_name": "", "port": 1})
                .to_string()
                .into_bytes(),
            // Bidirectional override in a device name.
            json!({"request": false, "device_name": "evil\u{202E}name", "port": 1})
                .to_string()
                .into_bytes(),
            // Over the size bound.
            json!({"request": false, "device_name": "a", "port": 1, "pad": "x".repeat(5000)})
                .to_string()
                .into_bytes(),
        ];
        for case in cases {
            assert_eq!(
                parse_datagram(&case),
                None,
                "should drop: {}",
                String::from_utf8_lossy(&case)
            );
        }
    }

    #[test]
    fn ignores_unknown_keys() {
        let wire = json!({"request": false, "device_name": "a", "port": 5,
                          "future_field": {"nested": true}})
        .to_string()
        .into_bytes();
        assert!(matches!(
            parse_datagram(&wire),
            Some(Datagram::Advertisement(_))
        ));
    }

    #[test]
    fn table_tracks_appearance_rename_and_removal() {
        let mut table = PeerTable::new();
        let now = Instant::now();

        assert!(table.observe(ip(10), &ad("First", 52638), now));
        assert_eq!(table.len(), 1);

        // An identical repeat is not a visible change.
        assert!(!table.observe(ip(10), &ad("First", 52638), now));

        // A rename is.
        assert!(table.observe(ip(10), &ad("Renamed", 52638), now));
        assert_eq!(table.peers()[0].device_name, "Renamed");

        // port 0 removes rather than listing a peer on port 0.
        assert!(table.observe(ip(10), &ad("Renamed", 0), now));
        assert!(table.is_empty());

        // Removing an unknown peer is not a change.
        assert!(!table.observe(ip(11), &ad("Ghost", 0), now));
    }

    #[test]
    fn table_expires_peers_not_seen_within_the_ttl() {
        let ttl = Duration::from_secs(15);
        let mut table = PeerTable::with_ttl(ttl);
        let start = Instant::now();

        table.observe(ip(10), &ad("Stale", 52638), start);
        table.observe(ip(11), &ad("Fresh", 52638), start);

        // Refresh only one of them, then step past the TTL.
        let later = start + ttl - Duration::from_secs(1);
        table.observe(ip(11), &ad("Fresh", 52638), later);

        let after = start + ttl + Duration::from_millis(1);
        assert_eq!(table.expire(after), 1);
        assert_eq!(table.len(), 1);
        assert_eq!(table.peers()[0].device_name, "Fresh");
    }

    #[test]
    fn table_without_expiry_keeps_everything() {
        let mut table = PeerTable::without_expiry();
        let start = Instant::now();
        table.observe(ip(10), &ad("Kept", 52638), start);
        assert_eq!(table.expire(start + Duration::from_secs(86_400)), 0);
        assert_eq!(table.len(), 1);
    }

    #[test]
    fn peers_and_addresses_are_ordered_for_stable_display() {
        let mut table = PeerTable::new();
        let now = Instant::now();
        for last in [30u8, 10, 20] {
            table.observe(ip(last), &ad("Peer", 52638), now);
        }
        assert_eq!(
            table.peers().iter().map(|p| p.address).collect::<Vec<_>>(),
            vec![ip(10), ip(20), ip(30)]
        );
        assert_eq!(table.known_addresses(), vec![ip(10), ip(20), ip(30)]);
    }
}

// --- UDP service -----------------------------------------------------------

use std::net::{SocketAddr, UdpSocket};

/// What one processed datagram did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// A peer asked us to report in; an advertisement was sent back to it.
    Answered(SocketAddr),
    /// A peer's advertisement was recorded. `changed` is false when it merely
    /// refreshed an entry that already looked the same.
    Observed { peer: Peer, changed: bool },
    /// A peer announced `port: 0`; it is no longer listed.
    Withdrawn(IpAddr),
    /// Received but dropped: malformed, oversized, or from ourselves.
    Ignored,
}

/// Blocking UDP discovery.
///
/// Targets are passed in explicitly rather than discovered here, so a caller
/// chooses between broadcast addresses, a multicast group, or unicast probes
/// of remembered peers — and so tests can drive it over loopback without
/// spraying the real network.
pub struct DiscoveryService {
    socket: UdpSocket,
    device_name: String,
    device_type: String,
    /// The TCP port peers should connect to. 0 announces "not available".
    server_port: u16,
    table: PeerTable,
}

impl DiscoveryService {
    pub fn bind(
        bind_addr: SocketAddr,
        device_name: &str,
        device_type: &str,
        server_port: u16,
    ) -> Result<Self, crate::Error> {
        let socket = UdpSocket::bind(bind_addr).map_err(crate::Error::Io)?;
        socket.set_broadcast(true).map_err(crate::Error::Io)?;
        Ok(Self {
            socket,
            device_name: device_name.to_string(),
            device_type: device_type.to_string(),
            server_port,
            table: PeerTable::new(),
        })
    }

    pub fn local_addr(&self) -> Result<SocketAddr, crate::Error> {
        self.socket.local_addr().map_err(crate::Error::Io)
    }

    pub fn peers(&self) -> Vec<Peer> {
        self.table.peers()
    }

    /// Addresses to probe on a later start; persist these to get a warm start.
    pub fn known_addresses(&self) -> Vec<IpAddr> {
        self.table.known_addresses()
    }

    /// Ages out peers not seen within the TTL.
    pub fn expire(&mut self, now: Instant) -> usize {
        self.table.expire(now)
    }

    /// Asks the given targets to report in.
    ///
    /// Pointing this at remembered unicast addresses is the warm start: on a
    /// network where broadcast is filtered, a known device answers directly
    /// instead of never appearing.
    pub fn request(&self, targets: &[SocketAddr]) -> Result<(), crate::Error> {
        let datagram = build_request();
        for target in targets {
            self.socket
                .send_to(&datagram, target)
                .map_err(crate::Error::Io)?;
        }
        Ok(())
    }

    /// Announces this device to the given targets.
    pub fn announce(&self, targets: &[SocketAddr]) -> Result<(), crate::Error> {
        let datagram = build_advertisement(&self.device_name, &self.device_type, self.server_port);
        for target in targets {
            self.socket
                .send_to(&datagram, target)
                .map_err(crate::Error::Io)?;
        }
        Ok(())
    }

    /// Receives and handles at most one datagram.
    ///
    /// Returns `Ok(None)` when the timeout elapses with nothing to read, so a
    /// caller can interleave this with its own refresh and expiry schedule.
    pub fn poll_once(
        &mut self,
        timeout: Duration,
        now: Instant,
    ) -> Result<Option<Event>, crate::Error> {
        self.socket
            .set_read_timeout(Some(timeout))
            .map_err(crate::Error::Io)?;

        let mut buf = [0u8; MAX_DATAGRAM_BYTES];
        let (read, from) = match self.socket.recv_from(&mut buf) {
            Ok(v) => v,
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                return Ok(None)
            }
            Err(e) => return Err(crate::Error::Io(e)),
        };

        // A device must not discover itself. This compares the full socket
        // address, which is exact for loopback; a production caller on a real
        // network must also exclude every local interface address.
        if self.socket.local_addr().is_ok_and(|local| local == from) {
            return Ok(Some(Event::Ignored));
        }

        Ok(Some(match parse_datagram(&buf[..read]) {
            None => Event::Ignored,
            Some(Datagram::Request) => {
                let datagram =
                    build_advertisement(&self.device_name, &self.device_type, self.server_port);
                // Unicast back to the asker, never a broadcast.
                self.socket
                    .send_to(&datagram, from)
                    .map_err(crate::Error::Io)?;
                Event::Answered(from)
            }
            Some(Datagram::Advertisement(ad)) => {
                let changed = self.table.observe(from.ip(), &ad, now);
                if ad.port == 0 {
                    Event::Withdrawn(from.ip())
                } else {
                    Event::Observed {
                        peer: Peer {
                            address: from.ip(),
                            device_name: ad.device_name,
                            port: ad.port,
                        },
                        changed,
                    }
                }
            }
        }))
    }
}

#[cfg(test)]
mod service_tests {
    use super::*;

    fn service(name: &str, port: u16) -> DiscoveryService {
        DiscoveryService::bind("127.0.0.1:0".parse().unwrap(), name, "test", port).unwrap()
    }

    #[test]
    fn a_request_is_answered_and_the_answer_is_recorded() {
        let mut asker = service("Asker", 52638);
        let mut peer = service("Peer", 40000);
        let peer_addr = peer.local_addr().unwrap();

        asker.request(&[peer_addr]).unwrap();

        let now = Instant::now();
        // The peer answers the request...
        assert!(matches!(
            peer.poll_once(Duration::from_secs(5), now).unwrap(),
            Some(Event::Answered(_))
        ));

        // ...and the asker records the advertisement that came back.
        let event = asker.poll_once(Duration::from_secs(5), now).unwrap();
        match event {
            Some(Event::Observed { peer, changed }) => {
                assert_eq!(peer.device_name, "Peer");
                assert_eq!(peer.port, 40000);
                assert!(changed);
            }
            other => panic!("expected an observation, got {other:?}"),
        }
        assert_eq!(asker.peers().len(), 1);
        assert_eq!(asker.known_addresses().len(), 1);
    }

    #[test]
    fn an_unavailable_peer_is_withdrawn() {
        let mut asker = service("Asker", 52638);
        let peer = service("Gone", 0); // announces port 0
        let peer_addr = peer.local_addr().unwrap();

        let now = Instant::now();
        peer.announce(&[asker.local_addr().unwrap()]).unwrap();
        assert_eq!(
            asker.poll_once(Duration::from_secs(5), now).unwrap(),
            Some(Event::Withdrawn(peer_addr.ip()))
        );
        assert!(asker.peers().is_empty());
    }

    #[test]
    fn polling_times_out_without_traffic() {
        let mut idle = service("Idle", 52638);
        let started = Instant::now();
        assert_eq!(
            idle.poll_once(Duration::from_millis(50), started).unwrap(),
            None
        );
        assert!(started.elapsed() >= Duration::from_millis(40));
    }

    #[test]
    fn malformed_traffic_is_ignored_without_disturbing_the_table() {
        let mut listener = service("Listener", 52638);
        let noise = UdpSocket::bind("127.0.0.1:0").unwrap();
        noise
            .send_to(b"not a datagram", listener.local_addr().unwrap())
            .unwrap();

        assert_eq!(
            listener
                .poll_once(Duration::from_secs(5), Instant::now())
                .unwrap(),
            Some(Event::Ignored)
        );
        assert!(listener.peers().is_empty());
    }
}
