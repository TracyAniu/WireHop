//! GUI-free driver for `wirehop-core`.
//!
//! `emit-vectors` prints the golden-vector fixture that both implementations
//! verify against. Regenerating is deliberate and rare: the fixture is the
//! contract between the Rust core and the C++/Qt baseline, so a change to it
//! is a wire-protocol change and must be reviewed as one.

use std::process::ExitCode;

use wirehop_core::discovery;
use wirehop_core::dnssd;
use wirehop_core::message::{self, FileMetadata, Metadata};
use wirehop_core::protocol;

mod peer;

fn usage() -> ExitCode {
    eprintln!("usage: wirehop-cli <command>");
    eprintln!("  emit-vectors");
    eprintln!("  send --port P [--host H] [--name N] FILE...");
    eprintln!("  receive --dir D [--port P] [--reject]");
    ExitCode::from(2)
}

fn main() -> ExitCode {
    let argv: Vec<String> = std::env::args().collect();
    let rest: Vec<String> = argv.iter().skip(2).cloned().collect();
    match argv.get(1).map(String::as_str) {
        Some("emit-vectors") => {
            println!("{}", vectors::render());
            ExitCode::SUCCESS
        }
        Some("send") => peer::send(&rest),
        Some("receive") => peer::receive(&rest),
        _ => usage(),
    }
}

mod vectors {
    use super::*;

    /// One resolved-service fixture case: name, instance, port, TXT pairs.
    type DnssdCase<'a> = (&'a str, &'a str, u16, Vec<(&'a str, &'a str)>);
    use serde_json::{json, Map, Value};
    use wirehop_core::crypto::Crypto;

    /// Fixed session keys. Any 32 bytes exercise the digest chain; these are
    /// chosen to be obviously synthetic so nobody mistakes them for secrets.
    const SESSION_KEYS: [[u8; 32]; 3] = [[0x00; 32], [0x07; 32], [0xFF; 32]];

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    fn session_code_vectors() -> Vec<Value> {
        SESSION_KEYS
            .iter()
            .map(|key| {
                json!({
                    "session_key_hex": hex(key),
                    "code": Crypto::session_code_for_key(key),
                })
            })
            .collect()
    }

    fn metadata_cases() -> Vec<Metadata> {
        vec![
            Metadata {
                device_name: "test-device".into(),
                device_type: "macos".into(),
                files: vec![FileMetadata {
                    filename: "a.txt".into(),
                    size: 14,
                }],
            },
            // Multi-file including a zero-byte entry, which carries no data
            // frames at all, and a non-ASCII name to pin UTF-8 handling.
            Metadata {
                device_name: "测试设备".into(),
                device_type: "linux".into(),
                files: vec![
                    FileMetadata {
                        filename: "报告.pdf".into(),
                        size: 200_000,
                    },
                    FileMetadata {
                        filename: "empty.dat".into(),
                        size: 0,
                    },
                ],
            },
        ]
    }

    fn negotiation_cases() -> Vec<Value> {
        let cases: Vec<(&str, Value)> = vec![
            ("current", json!({"protocol_version": 1, "caps": ["ack"]})),
            ("absent", json!({})),
            (
                "unknown-cap",
                json!({"protocol_version": 1, "caps": ["resume"]}),
            ),
            (
                "version-zero",
                json!({"protocol_version": 0, "caps": ["ack"]}),
            ),
            (
                "version-string",
                json!({"protocol_version": "1", "caps": ["ack"]}),
            ),
            (
                "version-fractional",
                json!({"protocol_version": 1.5, "caps": ["ack"]}),
            ),
            (
                "caps-not-array",
                json!({"protocol_version": 1, "caps": "ack"}),
            ),
            (
                "caps-non-string",
                json!({"protocol_version": 1, "caps": ["ack", 42]}),
            ),
            (
                "caps-empty-entry",
                json!({"protocol_version": 1, "caps": [""]}),
            ),
            (
                "caps-oversized-entry",
                json!({"protocol_version": 1, "caps": ["a".repeat(protocol::MAX_CAP_BYTES + 1)]}),
            ),
            (
                "caps-too-many",
                json!({"protocol_version": 1,
                       "caps": (0..=protocol::MAX_CAPS).map(|i| format!("cap{i}")).collect::<Vec<_>>()}),
            ),
        ];

        cases
            .into_iter()
            .map(|(name, input)| {
                let obj = input.as_object().cloned().unwrap_or_default();
                let peer = protocol::PeerNegotiation::adopt(&obj);
                json!({
                    "name": name,
                    "input": input,
                    "expected_version": peer.version,
                    "expected_caps": peer.caps.iter().cloned().collect::<Vec<_>>(),
                    "expected_ack_negotiated": peer.has_negotiated_cap(protocol::CAP_ACK),
                })
            })
            .collect()
    }

    fn advertisement_cases() -> Vec<Value> {
        [
            ("MacBook", "macos", 52638u16),
            // port 0 announces "not available", which peers treat as removal.
            ("Hidden", "windows", 0),
            ("测试设备", "linux", 1),
        ]
        .into_iter()
        .map(|(name, kind, port)| {
            json!({
                "device_name": name,
                "device_type": kind,
                "port": port,
                "canonical_json":
                    String::from_utf8(discovery::build_advertisement(name, kind, port)).unwrap(),
            })
        })
        .collect()
    }

    /// Inputs are raw JSON text so both implementations parse identical bytes.
    fn discovery_parsing_cases() -> Vec<Value> {
        let long_pad = "x".repeat(5000);
        let cases: Vec<(&str, String)> = vec![
            ("request", json!({"request": true}).to_string()),
            (
                "advertisement",
                json!({"request": false, "device_name": "Peer", "port": 52638}).to_string(),
            ),
            (
                "advertisement-unavailable",
                json!({"request": false, "device_name": "Peer", "port": 0}).to_string(),
            ),
            (
                "advertisement-unknown-key",
                json!({"request": false, "device_name": "Peer", "port": 5, "future": [1, 2]})
                    .to_string(),
            ),
            ("not-json", "not json at all".to_string()),
            ("not-object", json!([1, 2, 3]).to_string()),
            (
                "request-missing",
                json!({"device_name": "a", "port": 1}).to_string(),
            ),
            ("request-string", json!({"request": "true"}).to_string()),
            ("request-number", json!({"request": 1}).to_string()),
            (
                "name-missing",
                json!({"request": false, "port": 1}).to_string(),
            ),
            (
                "name-not-string",
                json!({"request": false, "device_name": 42, "port": 1}).to_string(),
            ),
            (
                "name-empty",
                json!({"request": false, "device_name": "", "port": 1}).to_string(),
            ),
            (
                "name-bidi-override",
                json!({"request": false, "device_name": "evil\u{202E}name", "port": 1}).to_string(),
            ),
            (
                "port-missing",
                json!({"request": false, "device_name": "a"}).to_string(),
            ),
            (
                "port-string",
                json!({"request": false, "device_name": "a", "port": "1"}).to_string(),
            ),
            (
                "port-negative",
                json!({"request": false, "device_name": "a", "port": -1}).to_string(),
            ),
            (
                "port-too-large",
                json!({"request": false, "device_name": "a", "port": 70000}).to_string(),
            ),
            (
                "port-fractional",
                json!({"request": false, "device_name": "a", "port": 1.5}).to_string(),
            ),
            (
                "oversized",
                json!({"request": false, "device_name": "a", "port": 1, "pad": long_pad})
                    .to_string(),
            ),
        ];

        cases
            .into_iter()
            .map(|(name, input)| {
                let expected = match discovery::parse_datagram(input.as_bytes()) {
                    None => json!({"kind": "invalid"}),
                    Some(discovery::Datagram::Request) => json!({"kind": "request"}),
                    Some(discovery::Datagram::Advertisement(ad)) => json!({
                        "kind": "advertisement",
                        "device_name": ad.device_name,
                        "device_type": ad.device_type,
                        "port": ad.port,
                    }),
                };
                json!({"name": name, "input": input, "expected": expected})
            })
            .collect()
    }

    /// Resolved-service cases, the executable target for a Bonjour-backed
    /// shell: given an instance name, port, and TXT set, this is the peer the
    /// core must produce.
    fn dnssd_txt_cases() -> Vec<Value> {
        let long_cap = "a".repeat(protocol::MAX_CAP_BYTES + 1);
        let cases: Vec<DnssdCase> = vec![
            (
                "current",
                "MacBook",
                52638,
                vec![("v", "1"), ("caps", "ack"), ("type", "macos")],
            ),
            // DNS-SD keys are case-insensitive.
            (
                "upper-keys",
                "MacBook",
                52638,
                vec![("V", "1"), ("CAPS", "ack")],
            ),
            // No TXT at all is a legacy peer, not a failure.
            ("bare", "Bare", 52638, vec![]),
            ("conflict-suffix", "iPhone (2)", 52638, vec![("v", "1")]),
            ("unknown-key", "Peer", 5, vec![("v", "1"), ("future", "x")]),
            ("bad-version", "Peer", 5, vec![("v", "one")]),
            ("caps-empty-entry", "Peer", 5, vec![("caps", "ack,,resume")]),
            ("caps-oversized", "Peer", 5, vec![("caps", &long_cap)]),
            // Rejections.
            ("zero-port", "Peer", 0, vec![]),
            ("empty-instance", "", 5, vec![]),
            ("bidi-instance", "evil\u{202E}name", 5, vec![]),
        ];

        cases
            .into_iter()
            .map(|(name, instance, port, txt)| {
                let records: Vec<(String, String)> = txt
                    .iter()
                    .map(|(k, v)| (k.to_string(), v.to_string()))
                    .collect();
                let expected = match dnssd::advertisement_from_service(instance, port, &records) {
                    None => json!({"kind": "rejected"}),
                    Some(ad) => json!({
                        "kind": "advertisement",
                        "device_name": ad.device_name,
                        "device_type": ad.device_type,
                        "port": ad.port,
                        "protocol_version": ad.protocol_version,
                        "caps": ad.caps,
                    }),
                };
                json!({
                    "name": name,
                    "instance": instance,
                    "port": port,
                    "txt": txt.iter().map(|(k, v)| json!([k, v])).collect::<Vec<_>>(),
                    "expected": expected,
                })
            })
            .collect()
    }

    /// The mDNS conflict-suffix rule, isolated.
    fn dnssd_instance_cases() -> Vec<Value> {
        [
            "MacBook",
            "MacBook (2)",
            "MacBook (17)",
            "Mac (work)",
            "Mac ()",
            "(2)",
            "测试设备 (3)",
        ]
        .into_iter()
        .map(|instance| {
            json!({
                "instance": instance,
                "device_name": dnssd::device_name_from_instance(instance),
            })
        })
        .collect()
    }

    pub fn render() -> String {
        let metadata: Vec<Value> = metadata_cases()
            .into_iter()
            .map(|m| {
                let files: Vec<Value> = m
                    .files
                    .iter()
                    .map(|f| json!({"filename": f.filename, "size": f.size}))
                    .collect();
                json!({
                    "device_name": m.device_name,
                    "device_type": m.device_type,
                    "files": files,
                    "canonical_json": String::from_utf8(m.to_canonical_json()).unwrap(),
                })
            })
            .collect();

        let responses: Vec<Value> = [true, false]
            .into_iter()
            .map(|accepted| {
                json!({
                    "accepted": accepted,
                    "canonical_json":
                        String::from_utf8(message::response_to_canonical_json(accepted)).unwrap(),
                })
            })
            .collect();

        let mut root = Map::new();
        root.insert("_generated_by".into(), json!("wirehop-cli emit-vectors"));
        root.insert(
            "_purpose".into(),
            json!(
                "Cross-implementation conformance fixture for docs/references/PROTOCOL.md. \
                   Both the Rust core and the C++/Qt application verify against this file; a \
                   diff here is a wire-protocol change."
            ),
        );
        root.insert("protocol_version".into(), json!(protocol::VERSION));
        root.insert("session_codes".into(), json!(session_code_vectors()));
        root.insert("canonical_metadata".into(), json!(metadata));
        root.insert("canonical_responses".into(), json!(responses));
        root.insert(
            "canonical_ack".into(),
            json!(String::from_utf8(message::ack_to_canonical_json()).unwrap()),
        );
        root.insert("negotiation_parsing".into(), json!(negotiation_cases()));
        root.insert(
            "discovery_request".into(),
            json!(String::from_utf8(discovery::build_request()).unwrap()),
        );
        root.insert(
            "discovery_advertisements".into(),
            json!(advertisement_cases()),
        );
        root.insert("discovery_parsing".into(), json!(discovery_parsing_cases()));
        root.insert("dnssd_service_type".into(), json!(dnssd::SERVICE_TYPE));
        root.insert("dnssd_txt".into(), json!(dnssd_txt_cases()));
        root.insert("dnssd_instance_names".into(), json!(dnssd_instance_cases()));

        serde_json::to_string_pretty(&Value::Object(root)).expect("fixture serializes")
    }
}
