//! GUI-free driver for `wirehop-core`.
//!
//! `emit-vectors` prints the golden-vector fixture that both implementations
//! verify against. Regenerating is deliberate and rare: the fixture is the
//! contract between the Rust core and the C++/Qt baseline, so a change to it
//! is a wire-protocol change and must be reviewed as one.

use std::process::ExitCode;

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

        serde_json::to_string_pretty(&Value::Object(root)).expect("fixture serializes")
    }
}
