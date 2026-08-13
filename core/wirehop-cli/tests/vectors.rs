//! Rust half of the cross-implementation conformance gate.
//!
//! The C++/Qt counterpart is `tests/tst_protocolvectors.cpp`. Both read the
//! same committed fixture. This side additionally checks that the fixture on
//! disk is exactly what the current core would emit, so the file can never
//! drift away from the code that generated it without the build going red.

use std::path::PathBuf;
use std::process::Command;

use serde_json::Value;
use wirehop_core::crypto::Crypto;
use wirehop_core::discovery;
use wirehop_core::message::{self, FileMetadata, Metadata};
use wirehop_core::protocol::{self, PeerNegotiation};

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/references/protocol-vectors.json")
}

fn fixture() -> Value {
    let raw = std::fs::read(fixture_path()).expect("conformance fixture is committed");
    serde_json::from_slice(&raw).expect("fixture is valid JSON")
}

#[test]
fn fixture_targets_this_protocol_version() {
    assert_eq!(
        fixture()["protocol_version"].as_i64(),
        Some(protocol::VERSION)
    );
}

#[test]
fn session_codes_match_the_fixture() {
    let v = fixture();
    let cases = v["session_codes"].as_array().expect("session_codes array");
    assert!(!cases.is_empty());

    for case in cases {
        let hex = case["session_key_hex"].as_str().unwrap();
        let key: Vec<u8> = (0..hex.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
            .collect();
        assert_eq!(key.len(), 32);
        assert_eq!(
            Crypto::session_code_for_key(&key),
            case["code"].as_str().unwrap(),
            "session code mismatch for key {hex}"
        );
    }
}

#[test]
fn negotiation_parsing_matches_the_fixture() {
    let v = fixture();
    let cases = v["negotiation_parsing"]
        .as_array()
        .expect("negotiation_parsing array");
    assert!(!cases.is_empty());

    for case in cases {
        let name = case["name"].as_str().unwrap();
        let input = case["input"].as_object().cloned().unwrap_or_default();
        let peer = PeerNegotiation::adopt(&input);

        assert_eq!(
            peer.version,
            case["expected_version"].as_i64().unwrap(),
            "version mismatch for case {name}"
        );

        let expected: Vec<&str> = case["expected_caps"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c.as_str().unwrap())
            .collect();
        let actual: Vec<&str> = peer.caps.iter().map(String::as_str).collect();
        assert_eq!(actual, expected, "caps mismatch for case {name}");

        assert_eq!(
            peer.has_negotiated_cap(protocol::CAP_ACK),
            case["expected_ack_negotiated"].as_bool().unwrap(),
            "ack negotiation mismatch for case {name}"
        );
    }
}

#[test]
fn canonical_metadata_matches_the_fixture() {
    let v = fixture();
    let cases = v["canonical_metadata"]
        .as_array()
        .expect("canonical_metadata array");
    assert!(!cases.is_empty());

    for case in cases {
        let metadata = Metadata {
            device_name: case["device_name"].as_str().unwrap().to_string(),
            device_type: case["device_type"].as_str().unwrap().to_string(),
            files: case["files"]
                .as_array()
                .unwrap()
                .iter()
                .map(|f| FileMetadata {
                    filename: f["filename"].as_str().unwrap().to_string(),
                    size: f["size"].as_u64().unwrap(),
                })
                .collect(),
        };

        let expected = case["canonical_json"].as_str().unwrap();
        assert_eq!(
            String::from_utf8(metadata.to_canonical_json()).unwrap(),
            expected
        );

        // And the same bytes must parse back to the same metadata.
        let (parsed, peer) = Metadata::parse(expected.as_bytes()).unwrap();
        assert_eq!(parsed, metadata);
        assert_eq!(peer.version, protocol::VERSION);
    }
}

#[test]
fn canonical_responses_and_ack_match_the_fixture() {
    let v = fixture();
    let cases = v["canonical_responses"].as_array().unwrap();
    assert_eq!(cases.len(), 2);

    for case in cases {
        let accepted = case["accepted"].as_bool().unwrap();
        let expected = case["canonical_json"].as_str().unwrap();
        assert_eq!(
            String::from_utf8(message::response_to_canonical_json(accepted)).unwrap(),
            expected
        );

        let (parsed, _) = message::parse_response(expected.as_bytes()).unwrap();
        assert_eq!(parsed, accepted);
    }

    let ack = v["canonical_ack"].as_str().unwrap();
    assert_eq!(
        String::from_utf8(message::ack_to_canonical_json()).unwrap(),
        ack
    );
    assert!(message::is_ack(ack.as_bytes()));
}

/// The committed fixture must be byte-identical to a fresh emission.
///
/// Without this, someone could hand-edit the fixture to paper over a real
/// divergence and both conformance suites would still pass.
#[test]
fn committed_fixture_is_reproducible() {
    let output = Command::new(env!("CARGO_BIN_EXE_wirehop-cli"))
        .arg("emit-vectors")
        .output()
        .expect("emit-vectors runs");
    assert!(output.status.success());

    let regenerated = String::from_utf8(output.stdout).unwrap();
    let committed = std::fs::read_to_string(fixture_path()).unwrap();
    assert_eq!(
        regenerated.trim_end(),
        committed.trim_end(),
        "committed fixture is stale; regenerate with \
         `cargo run -p wirehop-cli -- emit-vectors > docs/references/protocol-vectors.json` \
         and review the diff as a wire-protocol change"
    );
}

#[test]
fn discovery_datagrams_match_the_fixture() {
    let v = fixture();
    assert_eq!(
        String::from_utf8(discovery::build_request()).unwrap(),
        v["discovery_request"].as_str().unwrap()
    );

    let ads = v["discovery_advertisements"].as_array().unwrap();
    assert!(!ads.is_empty());
    for case in ads {
        let built = discovery::build_advertisement(
            case["device_name"].as_str().unwrap(),
            case["device_type"].as_str().unwrap(),
            case["port"].as_u64().unwrap() as u16,
        );
        assert_eq!(
            String::from_utf8(built).unwrap(),
            case["canonical_json"].as_str().unwrap()
        );
    }
}

#[test]
fn discovery_parsing_matches_the_fixture() {
    let v = fixture();
    let cases = v["discovery_parsing"].as_array().unwrap();
    assert!(!cases.is_empty());

    for case in cases {
        let name = case["name"].as_str().unwrap();
        let input = case["input"].as_str().unwrap();
        let expected = &case["expected"];
        let actual = discovery::parse_datagram(input.as_bytes());

        match expected["kind"].as_str().unwrap() {
            "request" => assert_eq!(actual, Some(discovery::Datagram::Request), "case {name}"),
            "advertisement" => match actual {
                Some(discovery::Datagram::Advertisement(ad)) => {
                    assert_eq!(
                        ad.device_name,
                        expected["device_name"].as_str().unwrap(),
                        "case {name}"
                    );
                    assert_eq!(
                        ad.device_type,
                        expected["device_type"].as_str().unwrap(),
                        "case {name}"
                    );
                    assert_eq!(
                        u64::from(ad.port),
                        expected["port"].as_u64().unwrap(),
                        "case {name}"
                    );
                }
                other => panic!("case {name}: expected an advertisement, got {other:?}"),
            },
            _ => assert_eq!(actual, None, "case {name}"),
        }
    }
}

#[test]
fn dnssd_vectors_match_the_fixture() {
    let v = fixture();
    assert_eq!(
        v["dnssd_service_type"].as_str().unwrap(),
        wirehop_core::dnssd::SERVICE_TYPE
    );

    for case in v["dnssd_instance_names"].as_array().unwrap() {
        assert_eq!(
            wirehop_core::dnssd::device_name_from_instance(case["instance"].as_str().unwrap()),
            case["device_name"].as_str().unwrap(),
            "instance {}",
            case["instance"]
        );
    }

    for case in v["dnssd_txt"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let txt: Vec<(String, String)> = case["txt"]
            .as_array()
            .unwrap()
            .iter()
            .map(|pair| {
                (
                    pair[0].as_str().unwrap().to_string(),
                    pair[1].as_str().unwrap().to_string(),
                )
            })
            .collect();

        let actual = wirehop_core::dnssd::advertisement_from_service(
            case["instance"].as_str().unwrap(),
            case["port"].as_u64().unwrap() as u16,
            &txt,
        );
        let expected = &case["expected"];

        match expected["kind"].as_str().unwrap() {
            "rejected" => assert!(actual.is_none(), "case {name} should be rejected"),
            _ => {
                let ad = actual.unwrap_or_else(|| panic!("case {name} should resolve"));
                assert_eq!(
                    ad.device_name,
                    expected["device_name"].as_str().unwrap(),
                    "case {name}"
                );
                assert_eq!(
                    u64::from(ad.port),
                    expected["port"].as_u64().unwrap(),
                    "case {name}"
                );
                assert_eq!(
                    ad.protocol_version,
                    expected["protocol_version"].as_i64().unwrap(),
                    "case {name}"
                );
                let expected_caps: Vec<&str> = expected["caps"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|c| c.as_str().unwrap())
                    .collect();
                assert_eq!(ad.caps, expected_caps, "case {name}");
            }
        }
    }
}
