//! The JSON messages exchanged inside encrypted frames.
//!
//! From `docs/references/PROTOCOL.md` §"Transfer session" steps 3, 4 and 6.
//! Serialization uses the canonical form defined there — compact separators
//! and lexicographically ordered keys — so golden vectors are byte-comparable
//! across implementations. Parsing accepts any key order.

use serde_json::{Map, Value};

use crate::policy;
use crate::protocol::{self, PeerNegotiation};
use crate::Error;

/// One entry of the metadata `files` array.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileMetadata {
    pub filename: String,
    pub size: u64,
}

/// The sender's opening frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Metadata {
    pub device_name: String,
    pub device_type: String,
    pub files: Vec<FileMetadata>,
}

impl Metadata {
    /// Serializes to canonical JSON with negotiation fields attached.
    ///
    /// Keys are inserted in lexicographic order (`caps`, `device_name`,
    /// `device_type`, `files`, `protocol_version`) because `serde_json::Map`
    /// preserves insertion order by default.
    pub fn to_canonical_json(&self) -> Vec<u8> {
        let files: Vec<Value> = self
            .files
            .iter()
            .map(|f| {
                let mut obj = Map::new();
                obj.insert("filename".into(), Value::String(f.filename.clone()));
                obj.insert("size".into(), Value::from(f.size));
                Value::Object(obj)
            })
            .collect();

        let mut obj = Map::new();
        let mut negotiation = Map::new();
        protocol::insert_negotiation_fields(&mut negotiation);

        obj.insert("caps".into(), negotiation["caps"].clone());
        obj.insert(
            "device_name".into(),
            Value::String(self.device_name.clone()),
        );
        obj.insert(
            "device_type".into(),
            Value::String(self.device_type.clone()),
        );
        obj.insert("files".into(), Value::Array(files));
        obj.insert(
            "protocol_version".into(),
            negotiation["protocol_version"].clone(),
        );

        serde_json::to_vec(&Value::Object(obj)).expect("Map<String, Value> always serializes")
    }

    /// Parses and fully validates a peer's metadata frame.
    ///
    /// Returns the negotiated peer profile alongside the metadata; negotiation
    /// never fails, but validation does.
    pub fn parse(data: &[u8]) -> Result<(Self, PeerNegotiation), Error> {
        let value: Value =
            serde_json::from_slice(data).map_err(|_| Error::Protocol("metadata is not JSON"))?;
        let obj = value
            .as_object()
            .ok_or(Error::Protocol("metadata is not a JSON object"))?;

        let device_name = obj
            .get("device_name")
            .and_then(Value::as_str)
            .ok_or(Error::Protocol("metadata has no device_name"))?;
        if !policy::is_safe_device_name(device_name) {
            return Err(Error::Protocol("metadata device_name is invalid"));
        }

        let entries = obj
            .get("files")
            .and_then(Value::as_array)
            .ok_or(Error::Protocol("metadata has no files array"))?;
        if entries.is_empty() || entries.len() > policy::MAX_FILES_PER_TRANSFER {
            return Err(Error::Protocol("metadata file count is out of bounds"));
        }

        let mut files = Vec::with_capacity(entries.len());
        let mut total: u64 = 0;
        for entry in entries {
            let entry = entry
                .as_object()
                .ok_or(Error::Protocol("file entry is not an object"))?;
            let filename = entry
                .get("filename")
                .and_then(Value::as_str)
                .ok_or(Error::Protocol("file entry has no filename"))?;
            if !policy::is_safe_filename(filename) {
                return Err(Error::Protocol("file entry filename is unsafe"));
            }
            let raw_size = entry
                .get("size")
                .and_then(Value::as_f64)
                .ok_or(Error::Protocol("file entry has no size"))?;
            let size =
                policy::parse_file_size(raw_size).ok_or(Error::Protocol("file size is invalid"))?;
            if !policy::can_append_file(total, size) {
                return Err(Error::Protocol("transfer exceeds the total size limit"));
            }
            total += size;
            files.push(FileMetadata {
                filename: filename.to_string(),
                size,
            });
        }

        let metadata = Self {
            device_name: device_name.to_string(),
            device_type: obj
                .get("device_type")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            files,
        };
        Ok((metadata, PeerNegotiation::adopt(obj)))
    }

    /// Total declared bytes across all files.
    pub fn total_size(&self) -> u64 {
        self.files.iter().map(|f| f.size).sum()
    }
}

/// Builds the receiver's canonical response frame.
pub fn response_to_canonical_json(accepted: bool) -> Vec<u8> {
    let mut negotiation = Map::new();
    protocol::insert_negotiation_fields(&mut negotiation);

    let mut obj = Map::new();
    obj.insert("caps".into(), negotiation["caps"].clone());
    obj.insert(
        "protocol_version".into(),
        negotiation["protocol_version"].clone(),
    );
    obj.insert("response".into(), Value::from(i64::from(accepted)));

    serde_json::to_vec(&Value::Object(obj)).expect("Map<String, Value> always serializes")
}

/// Parses a response frame. Any value other than `1` — including a missing or
/// non-numeric field — is a rejection.
pub fn parse_response(data: &[u8]) -> Result<(bool, PeerNegotiation), Error> {
    let value: Value =
        serde_json::from_slice(data).map_err(|_| Error::Protocol("response is not JSON"))?;
    let obj = value
        .as_object()
        .ok_or(Error::Protocol("response is not a JSON object"))?;

    let accepted = obj.get("response").and_then(Value::as_f64) == Some(1.0);
    Ok((accepted, PeerNegotiation::adopt(obj)))
}

/// The receiver's completion acknowledgment, sent after the last file commits.
pub fn ack_to_canonical_json() -> Vec<u8> {
    let mut obj = Map::new();
    obj.insert("ack".into(), Value::from(1));
    serde_json::to_vec(&Value::Object(obj)).expect("Map<String, Value> always serializes")
}

/// Whether a frame received while awaiting acknowledgment is one.
pub fn is_ack(data: &[u8]) -> bool {
    serde_json::from_slice::<Value>(data)
        .ok()
        .and_then(|v| v.get("ack").and_then(Value::as_f64))
        == Some(1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Metadata {
        Metadata {
            device_name: "test-device".into(),
            device_type: "macos".into(),
            files: vec![
                FileMetadata {
                    filename: "a.txt".into(),
                    size: 14,
                },
                FileMetadata {
                    filename: "empty.dat".into(),
                    size: 0,
                },
            ],
        }
    }

    #[test]
    fn metadata_round_trips() {
        let json = sample().to_canonical_json();
        let (parsed, peer) = Metadata::parse(&json).unwrap();
        assert_eq!(parsed, sample());
        assert_eq!(peer.version, protocol::VERSION);
        assert!(peer.has_negotiated_cap(protocol::CAP_ACK));
    }

    #[test]
    fn metadata_keys_are_canonical() {
        let json = String::from_utf8(sample().to_canonical_json()).unwrap();
        assert!(json.starts_with(r#"{"caps":["ack"],"device_name":"test-device""#));
        assert!(!json.contains(' '), "canonical form is compact");

        let keys: Vec<&str> = [
            "caps",
            "device_name",
            "device_type",
            "files",
            "protocol_version",
        ]
        .into_iter()
        .collect();
        let positions: Vec<usize> = keys
            .iter()
            .map(|k| json.find(&format!("\"{k}\":")).unwrap())
            .collect();
        let mut sorted = positions.clone();
        sorted.sort_unstable();
        assert_eq!(positions, sorted, "keys must be lexicographically ordered");
    }

    #[test]
    fn metadata_parsing_accepts_any_key_order() {
        let reordered = br#"{"protocol_version":1,"files":[{"size":3,"filename":"a.txt"}],"device_type":"linux","device_name":"peer","caps":["ack"]}"#;
        let (parsed, peer) = Metadata::parse(reordered).unwrap();
        assert_eq!(parsed.device_name, "peer");
        assert_eq!(parsed.files[0].size, 3);
        assert!(peer.has_negotiated_cap(protocol::CAP_ACK));
    }

    #[test]
    fn legacy_metadata_without_negotiation_fields_still_parses() {
        let legacy = br#"{"device_name":"landrop","device_type":"windows","files":[{"filename":"a.txt","size":1}]}"#;
        let (parsed, peer) = Metadata::parse(legacy).unwrap();
        assert_eq!(parsed.device_name, "landrop");
        assert_eq!(peer.version, 0);
        assert!(!peer.has_negotiated_cap(protocol::CAP_ACK));
    }

    #[test]
    fn metadata_rejects_unsafe_input() {
        let cases: Vec<&[u8]> = vec![
            b"not json",
            br#"["not","an","object"]"#,
            br#"{"device_type":"x","files":[]}"#,
            br#"{"device_name":"a","files":[]}"#,
            br#"{"device_name":"a","files":[{"filename":"../x","size":1}]}"#,
            br#"{"device_name":"a","files":[{"filename":"a.txt","size":-1}]}"#,
            br#"{"device_name":"a","files":[{"filename":"a.txt","size":1.5}]}"#,
            br#"{"device_name":"a","files":[{"filename":"a.txt"}]}"#,
        ];
        for case in cases {
            assert!(
                Metadata::parse(case).is_err(),
                "should reject: {}",
                String::from_utf8_lossy(case)
            );
        }
    }

    #[test]
    fn response_round_trips_both_ways() {
        let (accepted, peer) = parse_response(&response_to_canonical_json(true)).unwrap();
        assert!(accepted);
        assert!(peer.has_negotiated_cap(protocol::CAP_ACK));

        let (rejected, _) = parse_response(&response_to_canonical_json(false)).unwrap();
        assert!(!rejected);
    }

    #[test]
    fn bare_legacy_response_is_accepted_as_capless() {
        let (accepted, peer) = parse_response(br#"{"response":1}"#).unwrap();
        assert!(accepted);
        assert_eq!(peer.version, 0);
        assert!(!peer.has_negotiated_cap(protocol::CAP_ACK));
    }

    #[test]
    fn anything_but_one_is_a_rejection() {
        for body in [
            &br#"{"response":0}"#[..],
            &br#"{"response":"1"}"#[..],
            &br#"{}"#[..],
        ] {
            let (accepted, _) = parse_response(body).unwrap();
            assert!(
                !accepted,
                "should reject: {}",
                String::from_utf8_lossy(body)
            );
        }
    }

    #[test]
    fn ack_round_trips() {
        assert_eq!(ack_to_canonical_json(), br#"{"ack":1}"#);
        assert!(is_ack(&ack_to_canonical_json()));
        assert!(!is_ack(br#"{"ack":0}"#));
        assert!(!is_ack(br#"{}"#));
        assert!(!is_ack(b"garbage"));
    }
}
