//! DNS-SD contract for the complementary discovery channel.
//!
//! From `docs/references/PROTOCOL.md` §"Discovery over DNS-SD". This module is
//! deliberately transport-free: it defines the service type, the instance
//! naming rule, and the TXT schema, and converts them to and from the
//! [`Advertisement`](crate::discovery::Advertisement) the rest of the core
//! already understands.
//!
//! **Why transport-free.** iOS 14+ requires the
//! `com.apple.developer.networking.multicast` entitlement — granted only by
//! application to Apple — for an app to send or receive multicast itself,
//! which includes a bundled mDNS responder binding `224.0.0.251:5353`. Apple's
//! Bonjour API needs no entitlement because the system daemon does the
//! multicast on the app's behalf. So on Apple platforms the *shell* supplies
//! discovery results and the core must not assume it owns a socket. Keeping
//! the schema here and the transport elsewhere is what lets a Bonjour-backed
//! shell and a multicast-backed desktop path feed one peer list.

use crate::discovery::Advertisement;
use crate::policy;
use crate::protocol;

/// The DNS-SD service type, without the trailing domain.
pub const SERVICE_TYPE: &str = "_wirehop._tcp";

/// The domain DNS-SD registrations live in.
pub const SERVICE_DOMAIN: &str = "local.";

/// TXT key for the protocol version.
pub const TXT_VERSION: &str = "v";
/// TXT key for the capability list.
pub const TXT_CAPS: &str = "caps";
/// TXT key for the advisory device type.
pub const TXT_TYPE: &str = "type";

/// Builds the TXT key/value pairs advertising this device.
///
/// Ordered by key so the record set is reproducible, for the same reason the
/// JSON capability array is sorted.
pub fn build_txt(device_type: &str) -> Vec<(String, String)> {
    let caps: Vec<String> = protocol::local_caps().into_iter().collect();
    vec![
        (TXT_CAPS.to_string(), caps.join(",")),
        (TXT_TYPE.to_string(), device_type.to_string()),
        (TXT_VERSION.to_string(), protocol::VERSION.to_string()),
    ]
}

/// Reads a TXT key, treating keys case-insensitively as DNS-SD requires.
fn txt_get<'a>(txt: &'a [(String, String)], key: &str) -> Option<&'a str> {
    txt.iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(key))
        .map(|(_, v)| v.as_str())
}

/// Parses the capability list from its comma-separated TXT form.
///
/// Bounds match the JSON form exactly — at most 32 entries, each 1–32 UTF-8
/// bytes — and any violation discards the whole list rather than keeping the
/// entries that happened to be well formed.
pub fn parse_caps(value: Option<&str>) -> Vec<String> {
    let Some(value) = value else {
        return Vec::new();
    };
    if value.is_empty() {
        return Vec::new();
    }

    let mut caps = Vec::new();
    for entry in value.split(',') {
        if entry.is_empty()
            || entry.len() > protocol::MAX_CAP_BYTES
            || caps.len() == protocol::MAX_CAPS
        {
            return Vec::new();
        }
        caps.push(entry.to_string());
    }
    caps.sort();
    caps.dedup();
    caps
}

/// Parses the protocol version from its decimal TXT form.
///
/// Anything absent or unparsable yields 0, the legacy result, exactly as for
/// the JSON field.
pub fn parse_version(value: Option<&str>) -> i64 {
    value
        .and_then(|v| v.parse::<i64>().ok())
        .filter(|v| *v >= 1 && *v <= 1_000_000)
        .unwrap_or(0)
}

/// Strips the ` (2)`, ` (3)`, … suffix mDNS appends when instance names
/// collide, recovering the device name the peer intended.
///
/// A browsing implementation must not assume instance names are unique or
/// stable; two devices sharing a name is ordinary, not an error.
pub fn device_name_from_instance(instance: &str) -> &str {
    let trimmed = instance.trim_end();
    let Some(open) = trimmed.rfind(" (") else {
        return instance;
    };
    if !trimmed.ends_with(')') {
        return instance;
    }
    let inner = &trimmed[open + 2..trimmed.len() - 1];
    if inner.is_empty() || !inner.bytes().all(|b| b.is_ascii_digit()) {
        return instance;
    }
    &trimmed[..open]
}

/// Converts a resolved DNS-SD service into the core's advertisement type.
///
/// Returns `None` when the instance name fails the same validation applied to
/// `device_name` everywhere else, or when the port is unusable. DNS-SD has no
/// "not available" encoding — a device that is not discoverable does not
/// register — so a zero port is a malformed record, not a withdrawal.
pub fn advertisement_from_service(
    instance: &str,
    port: u16,
    txt: &[(String, String)],
) -> Option<Advertisement> {
    let device_name = device_name_from_instance(instance);
    if !policy::is_safe_device_name(device_name) || port == 0 {
        return None;
    }

    Some(Advertisement {
        device_name: device_name.to_string(),
        device_type: txt_get(txt, TXT_TYPE).unwrap_or_default().to_string(),
        port,
        protocol_version: parse_version(txt_get(txt, TXT_VERSION)),
        caps: parse_caps(txt_get(txt, TXT_CAPS)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn txt(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn built_txt_round_trips() {
        let records = build_txt("macos");
        assert_eq!(
            parse_version(txt_get(&records, TXT_VERSION)),
            protocol::VERSION
        );
        assert_eq!(
            parse_caps(txt_get(&records, TXT_CAPS)),
            vec![protocol::CAP_ACK.to_string()]
        );
        assert_eq!(txt_get(&records, TXT_TYPE), Some("macos"));
    }

    #[test]
    fn txt_keys_are_ordered_and_case_insensitive() {
        let records = build_txt("linux");
        let keys: Vec<&str> = records.iter().map(|(k, _)| k.as_str()).collect();
        let mut sorted = keys.clone();
        sorted.sort_unstable();
        assert_eq!(keys, sorted);

        // DNS-SD keys are case-insensitive.
        let upper = txt(&[("CAPS", "ack"), ("V", "1")]);
        assert_eq!(parse_version(txt_get(&upper, TXT_VERSION)), 1);
        assert_eq!(
            parse_caps(txt_get(&upper, TXT_CAPS)),
            vec!["ack".to_string()]
        );
    }

    #[test]
    fn version_degrades_to_legacy_on_bad_input() {
        assert_eq!(parse_version(Some("1")), 1);
        assert_eq!(parse_version(Some("7")), 7);
        assert_eq!(parse_version(None), 0);
        assert_eq!(parse_version(Some("")), 0);
        assert_eq!(parse_version(Some("one")), 0);
        assert_eq!(parse_version(Some("0")), 0);
        assert_eq!(parse_version(Some("-1")), 0);
        assert_eq!(parse_version(Some("1.5")), 0);
        assert_eq!(parse_version(Some("999999999")), 0);
    }

    #[test]
    fn caps_are_bounded_exactly_like_the_json_form() {
        assert_eq!(
            parse_caps(Some("ack,resume")),
            vec!["ack".to_string(), "resume".to_string()]
        );
        assert!(parse_caps(None).is_empty());
        assert!(parse_caps(Some("")).is_empty());

        // An empty entry, an oversized entry, or too many entries discards the
        // whole list rather than keeping the well-formed remainder.
        assert!(parse_caps(Some("ack,,resume")).is_empty());
        assert!(parse_caps(Some(&format!(
            "ack,{}",
            "a".repeat(protocol::MAX_CAP_BYTES + 1)
        )))
        .is_empty());

        let too_many: Vec<String> = (0..=protocol::MAX_CAPS).map(|i| format!("c{i}")).collect();
        assert!(parse_caps(Some(&too_many.join(","))).is_empty());

        // Exactly at the bound is fine.
        let at_bound: Vec<String> = (0..protocol::MAX_CAPS).map(|i| format!("c{i}")).collect();
        assert_eq!(
            parse_caps(Some(&at_bound.join(","))).len(),
            protocol::MAX_CAPS
        );
    }

    #[test]
    fn instance_names_survive_the_mdns_conflict_suffix() {
        assert_eq!(device_name_from_instance("MacBook"), "MacBook");
        assert_eq!(device_name_from_instance("MacBook (2)"), "MacBook");
        assert_eq!(device_name_from_instance("MacBook (17)"), "MacBook");
        assert_eq!(device_name_from_instance("测试设备 (3)"), "测试设备");

        // Parenthesised text that is not a conflict suffix is part of the name.
        assert_eq!(device_name_from_instance("Mac (work)"), "Mac (work)");
        assert_eq!(device_name_from_instance("Mac ()"), "Mac ()");
        assert_eq!(device_name_from_instance("(2)"), "(2)");
    }

    #[test]
    fn resolved_service_becomes_an_advertisement() {
        let records = txt(&[("v", "1"), ("caps", "ack"), ("type", "ios")]);
        let ad = advertisement_from_service("iPhone (2)", 52638, &records).unwrap();

        assert_eq!(ad.device_name, "iPhone");
        assert_eq!(ad.device_type, "ios");
        assert_eq!(ad.port, 52638);
        assert_eq!(ad.protocol_version, 1);
        assert_eq!(ad.caps, vec!["ack".to_string()]);
    }

    #[test]
    fn resolved_service_rejects_unusable_records() {
        let records = build_txt("macos");
        // DNS-SD has no "not available" form: a zero port is malformed.
        assert!(advertisement_from_service("Peer", 0, &records).is_none());
        assert!(advertisement_from_service("", 52638, &records).is_none());
        assert!(advertisement_from_service("evil\u{202E}name", 52638, &records).is_none());
        assert!(advertisement_from_service(&"a".repeat(256), 52638, &records).is_none());
    }

    #[test]
    fn missing_txt_degrades_to_a_legacy_peer() {
        let ad = advertisement_from_service("Bare", 52638, &[]).unwrap();
        assert_eq!(ad.protocol_version, 0);
        assert!(ad.caps.is_empty());
        assert_eq!(ad.device_type, "");
    }
}
