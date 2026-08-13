//! Wire-protocol version and capability negotiation.
//!
//! From `docs/references/PROTOCOL.md` §"Version and capability negotiation".
//! The governing rule is that negotiation may downgrade a session but must
//! never abort one: absent or malformed fields mean a version-0 peer with no
//! capabilities, and the transfer proceeds with legacy semantics.

use std::collections::BTreeSet;

use serde_json::{Map, Value};

/// Highest message-format version this build speaks. Version 0 is the implicit
/// LANDrop 0.4.0 format, which carries no negotiation fields at all.
pub const VERSION: i64 = 1;

/// Upper bound on how many capabilities a peer may advertise.
pub const MAX_CAPS: usize = 32;
/// Upper bound on one capability identifier, in UTF-8 bytes.
///
/// Bytes, not `char`s: counting UTF-16 code units (as `QString::size()` does)
/// would admit up to 4x the intended length.
pub const MAX_CAP_BYTES: usize = 32;

/// Sanity ceiling on a peer-supplied version number.
const MAX_VERSION: i64 = 1_000_000;

/// The completion-acknowledgment capability.
///
/// Endpoint-level, not role-specific: it means "this build implements the ack
/// extension in whichever role it plays".
pub const CAP_ACK: &str = "ack";

/// The capability set this build advertises.
pub fn local_caps() -> BTreeSet<String> {
    let mut caps = BTreeSet::new();
    caps.insert(CAP_ACK.to_string());
    caps
}

/// Adds `protocol_version` and `caps` to a metadata, response, or discovery
/// object. Purely additive — LANDrop 0.4.0 reads fixed keys and ignores these.
///
/// `BTreeSet` iteration is ordered, which is what keeps the serialized array
/// byte-reproducible across processes and runs.
pub fn insert_negotiation_fields(obj: &mut Map<String, Value>) {
    let caps: Vec<Value> = local_caps().into_iter().map(Value::String).collect();
    obj.insert("protocol_version".into(), Value::from(VERSION));
    obj.insert("caps".into(), Value::Array(caps));
}

/// Parses a peer's `protocol_version`. Anything that is not a sane positive
/// integer yields 0, the legacy result.
pub fn parse_version(value: Option<&Value>) -> i64 {
    let Some(number) = value.and_then(Value::as_f64) else {
        return 0;
    };
    if !number.is_finite() || number < 1.0 || number > MAX_VERSION as f64 || number.fract() != 0.0 {
        return 0;
    }
    number as i64
}

/// Parses a peer's `caps` array with bounds enforced. Any violation discards
/// the whole list and yields the legacy result: an empty set.
pub fn parse_caps(value: Option<&Value>) -> BTreeSet<String> {
    let empty = BTreeSet::new();
    let Some(Value::Array(items)) = value else {
        return empty;
    };
    if items.len() > MAX_CAPS {
        return empty;
    }

    let mut caps = BTreeSet::new();
    for item in items {
        let Some(cap) = item.as_str() else {
            return BTreeSet::new();
        };
        if cap.is_empty() || cap.len() > MAX_CAP_BYTES {
            return BTreeSet::new();
        }
        caps.insert(cap.to_string());
    }
    caps
}

/// What a session records about the peer after reading a metadata or response
/// frame.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PeerNegotiation {
    pub version: i64,
    pub caps: BTreeSet<String>,
}

impl PeerNegotiation {
    /// Reads both fields out of a decoded frame. Never fails: malformed input
    /// degrades to the legacy peer profile.
    pub fn adopt(obj: &Map<String, Value>) -> Self {
        Self {
            version: parse_version(obj.get("protocol_version")),
            caps: parse_caps(obj.get("caps")),
        }
    }

    /// The negotiated intersection, not a raw peer claim: a capability counts
    /// only when this build also implements it, so a peer cannot induce
    /// behavior this build does not support by advertising a capability alone.
    pub fn has_negotiated_cap(&self, cap: &str) -> bool {
        self.caps.contains(cap) && local_caps().contains(cap)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn version_parses_sane_positive_integers() {
        assert_eq!(parse_version(Some(&json!(1))), 1);
        assert_eq!(parse_version(Some(&json!(7))), 7);
    }

    #[test]
    fn version_rejects_malformed_values() {
        assert_eq!(parse_version(None), 0);
        assert_eq!(parse_version(Some(&json!("1"))), 0);
        assert_eq!(parse_version(Some(&json!(true))), 0);
        assert_eq!(parse_version(Some(&json!(0))), 0);
        assert_eq!(parse_version(Some(&json!(-3))), 0);
        assert_eq!(parse_version(Some(&json!(1.5))), 0);
        assert_eq!(parse_version(Some(&json!(1e9))), 0);
    }

    #[test]
    fn caps_parse_bounded_string_arrays() {
        let caps = parse_caps(Some(&json!(["ack", "resume"])));
        assert_eq!(caps.len(), 2);
        assert!(caps.contains("ack"));
        assert!(caps.contains("resume"));
    }

    #[test]
    fn caps_reject_malformed_arrays() {
        assert!(parse_caps(None).is_empty());
        assert!(parse_caps(Some(&json!("ack"))).is_empty());

        let too_many: Vec<String> = (0..=MAX_CAPS).map(|i| format!("cap{i}")).collect();
        assert!(parse_caps(Some(&json!(too_many))).is_empty());

        assert!(parse_caps(Some(&json!(["ack", 42]))).is_empty());
        assert!(parse_caps(Some(&json!([""]))).is_empty());
        assert!(parse_caps(Some(&json!(["a".repeat(MAX_CAP_BYTES + 1)]))).is_empty());
    }

    #[test]
    fn cap_bound_counts_utf8_bytes_not_characters() {
        // 16 CJK characters: 16 chars, but 48 UTF-8 bytes.
        let multibyte = "中".repeat(16);
        assert_eq!(multibyte.chars().count(), 16);
        assert!(multibyte.len() > MAX_CAP_BYTES);
        assert!(parse_caps(Some(&json!([multibyte]))).is_empty());

        assert_eq!(
            parse_caps(Some(&json!(["a".repeat(MAX_CAP_BYTES)]))).len(),
            1
        );
    }

    #[test]
    fn negotiation_fields_round_trip() {
        let mut obj = Map::new();
        insert_negotiation_fields(&mut obj);

        assert_eq!(parse_version(obj.get("protocol_version")), VERSION);
        assert_eq!(parse_caps(obj.get("caps")), local_caps());
    }

    #[test]
    fn serialized_caps_are_sorted() {
        let mut obj = Map::new();
        insert_negotiation_fields(&mut obj);

        let caps: Vec<&str> = obj["caps"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        let mut sorted = caps.clone();
        sorted.sort_unstable();
        assert_eq!(caps, sorted);
    }

    #[test]
    fn absent_fields_yield_the_legacy_peer_profile() {
        let peer = PeerNegotiation::adopt(&Map::new());
        assert_eq!(peer.version, 0);
        assert!(peer.caps.is_empty());
        assert!(!peer.has_negotiated_cap(CAP_ACK));
    }

    #[test]
    fn negotiated_cap_requires_both_sides() {
        let mut obj = Map::new();
        insert_negotiation_fields(&mut obj);
        let peer = PeerNegotiation::adopt(&obj);
        assert!(peer.has_negotiated_cap(CAP_ACK));

        // A capability only the peer implements is not negotiated.
        let unknown = PeerNegotiation::adopt(
            json!({"protocol_version": 1, "caps": ["resume"]})
                .as_object()
                .unwrap(),
        );
        assert!(!unknown.has_negotiated_cap("resume"));
    }
}
