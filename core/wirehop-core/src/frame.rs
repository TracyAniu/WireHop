//! Length-prefixed framing for wire protocol v1.
//!
//! From `docs/references/PROTOCOL.md` §"Transfer session" step 2: every message
//! after the key exchange is a 2-byte big-endian ciphertext length followed by
//! that many bytes. The 2-byte prefix is what caps a frame at 65,535 bytes and
//! is the reason larger blocks require a `protocol_version` bump (M2).

use crate::crypto::{Crypto, ENCRYPTED_OVERHEAD};
use crate::Error;

/// Largest value expressible in the 2-byte length prefix.
pub const MAX_FRAME_LEN: usize = u16::MAX as usize;
/// Largest plaintext that still fits once nonce and tag are added.
pub const MAX_PAYLOAD_LEN: usize = MAX_FRAME_LEN - ENCRYPTED_OVERHEAD;
/// Plaintext bytes the sender puts in each file-data frame.
pub const TRANSFER_QUANTUM: usize = 64_000;

// A quantum must always survive framing; enforced at compile time so the two
// constants can never drift apart.
const _: () = assert!(TRANSFER_QUANTUM <= MAX_PAYLOAD_LEN);

/// Encrypts `payload` and prefixes the 2-byte big-endian length.
pub fn encode(crypto: &Crypto, payload: &[u8]) -> Result<Vec<u8>, Error> {
    if payload.len() + ENCRYPTED_OVERHEAD > MAX_FRAME_LEN {
        return Err(Error::Protocol("message exceeds the protocol size limit"));
    }
    let body = crypto.encrypt(payload)?;
    let mut out = Vec::with_capacity(2 + body.len());
    out.extend_from_slice(&(body.len() as u16).to_be_bytes());
    out.extend_from_slice(&body);
    Ok(out)
}

/// Incremental reader that pulls whole frames out of a byte stream.
///
/// A caller feeds arbitrary chunks and drains complete frames; partial frames
/// stay buffered. This mirrors how a TCP peer actually delivers data.
#[derive(Default)]
pub struct FrameReader {
    buffer: Vec<u8>,
}

impl FrameReader {
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends freshly read bytes.
    pub fn push(&mut self, data: &[u8]) {
        self.buffer.extend_from_slice(data);
    }

    /// Bytes held back because they do not yet form a whole frame.
    pub fn buffered(&self) -> usize {
        self.buffer.len()
    }

    /// Removes and decrypts the next complete frame, if one is buffered.
    pub fn next_frame(&mut self, crypto: &Crypto) -> Result<Option<Vec<u8>>, Error> {
        if self.buffer.len() < 2 {
            return Ok(None);
        }
        let len = u16::from_be_bytes([self.buffer[0], self.buffer[1]]) as usize;
        if self.buffer.len() < len + 2 {
            return Ok(None);
        }
        let body = self.buffer[2..len + 2].to_vec();
        self.buffer.drain(..len + 2);
        crypto.decrypt(&body).map(Some)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paired() -> (Crypto, Crypto) {
        let mut a = Crypto::new();
        let mut b = Crypto::new();
        let a_pub = a.local_public_key();
        let b_pub = b.local_public_key();
        a.set_remote_public_key(&b_pub).unwrap();
        b.set_remote_public_key(&a_pub).unwrap();
        (a, b)
    }

    #[test]
    fn round_trips_a_single_frame() {
        let (a, b) = paired();
        let wire = encode(&a, b"one message").unwrap();

        let mut reader = FrameReader::new();
        reader.push(&wire);
        assert_eq!(reader.next_frame(&b).unwrap().unwrap(), b"one message");
        assert!(reader.next_frame(&b).unwrap().is_none());
    }

    #[test]
    fn reassembles_frames_split_across_reads() {
        let (a, b) = paired();
        let wire = encode(&a, b"split me").unwrap();

        let mut reader = FrameReader::new();
        // One byte at a time: nothing emerges until the final byte lands.
        for (i, byte) in wire.iter().enumerate() {
            reader.push(&[*byte]);
            let got = reader.next_frame(&b).unwrap();
            if i + 1 == wire.len() {
                assert_eq!(got.unwrap(), b"split me");
            } else {
                assert!(got.is_none(), "frame emerged early at byte {i}");
            }
        }
    }

    #[test]
    fn drains_multiple_frames_from_one_read() {
        let (a, b) = paired();
        let mut wire = encode(&a, b"first").unwrap();
        wire.extend_from_slice(&encode(&a, b"second").unwrap());

        let mut reader = FrameReader::new();
        reader.push(&wire);
        assert_eq!(reader.next_frame(&b).unwrap().unwrap(), b"first");
        assert_eq!(reader.next_frame(&b).unwrap().unwrap(), b"second");
        assert!(reader.next_frame(&b).unwrap().is_none());
        assert_eq!(reader.buffered(), 0);
    }

    #[test]
    fn rejects_a_payload_that_cannot_be_framed() {
        let (a, _) = paired();
        assert!(encode(&a, &vec![0u8; MAX_PAYLOAD_LEN + 1]).is_err());
        assert!(encode(&a, &vec![0u8; MAX_PAYLOAD_LEN]).is_ok());
    }

    #[test]
    fn a_transfer_quantum_frame_round_trips_at_full_size() {
        let (a, b) = paired();
        let payload = vec![0xABu8; TRANSFER_QUANTUM];
        let wire = encode(&a, &payload).unwrap();

        let mut reader = FrameReader::new();
        reader.push(&wire);
        assert_eq!(reader.next_frame(&b).unwrap().unwrap(), payload);
    }
}
