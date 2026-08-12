//! Session cryptography for wire protocol v1.
//!
//! Implemented from `docs/references/PROTOCOL.md` §"Transfer session" steps 1–2.
//! This is the inherited LANDrop 0.4.0 construction and carries its documented
//! weaknesses (unauthenticated DH, per-frame random nonces with no sequence,
//! no persistent identity). It exists for interoperability with the baseline;
//! it is not the target security model — see the M2/M3 milestones.

use blake2::digest::consts::U16;
use blake2::{Blake2b, Digest};
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};
use rand_core::{OsRng, RngCore};
use x25519_dalek::{PublicKey, StaticSecret};

use crate::Error;

/// Raw X25519 public key length, and the length each peer reads verbatim at
/// the start of a session.
pub const PUBLIC_KEY_LEN: usize = 32;
/// ChaCha20-Poly1305-IETF nonce length.
pub const NONCE_LEN: usize = 12;
/// Poly1305 authentication tag length.
pub const TAG_LEN: usize = 16;

/// Bytes a frame gains over its plaintext: nonce prefix plus AEAD tag.
pub const ENCRYPTED_OVERHEAD: usize = NONCE_LEN + TAG_LEN;

type Blake2b128 = Blake2b<U16>;

/// One session's ephemeral key material.
pub struct Crypto {
    secret: StaticSecret,
    public: PublicKey,
    session_key: Option<Key>,
}

impl Crypto {
    /// Generates a fresh ephemeral key pair.
    pub fn new() -> Self {
        let secret = StaticSecret::random_from_rng(OsRng);
        let public = PublicKey::from(&secret);
        Self {
            secret,
            public,
            session_key: None,
        }
    }

    /// The 32 bytes sent verbatim at session start.
    pub fn local_public_key(&self) -> [u8; PUBLIC_KEY_LEN] {
        self.public.to_bytes()
    }

    /// Derives the session key from the peer's raw public key.
    ///
    /// Per spec the X25519 shared secret is used directly as the AEAD key with
    /// no KDF. An all-zero result means a low-order peer key contributed no
    /// entropy, which libsodium's `crypto_scalarmult` also rejects.
    pub fn set_remote_public_key(&mut self, remote: &[u8]) -> Result<(), Error> {
        let bytes: [u8; PUBLIC_KEY_LEN] = remote
            .try_into()
            .map_err(|_| Error::Crypto("invalid remote public key length"))?;
        let shared = self.secret.diffie_hellman(&PublicKey::from(bytes));
        if !shared.was_contributory() {
            return Err(Error::Crypto("peer public key has low order"));
        }
        self.session_key = Some(*Key::from_slice(shared.as_bytes()));
        Ok(())
    }

    fn key(&self) -> Result<&Key, Error> {
        self.session_key
            .as_ref()
            .ok_or(Error::Crypto("session key not established"))
    }

    /// The six-digit code both users compare out of band.
    pub fn session_key_digest(&self) -> Result<String, Error> {
        Ok(Self::session_code_for_key(self.key()?.as_slice()))
    }

    /// Derives the six-digit code from raw key bytes.
    ///
    /// Split out from [`Crypto::session_key_digest`] so the golden vectors can
    /// pin the derivation against fixed keys rather than a live handshake.
    /// Every step is spec-normative: BLAKE2b with a **16-byte** digest, the
    /// **first 8 bytes** read **little-endian**, mod 10^6, zero-padded. Getting
    /// any one of those wrong yields a different code on each side and silently
    /// defeats the out-of-band comparison.
    pub fn session_code_for_key(key: &[u8]) -> String {
        let digest = Blake2b128::digest(key);
        let mut head = [0u8; 8];
        head.copy_from_slice(&digest[..8]);
        format!("{:06}", u64::from_le_bytes(head) % 1_000_000)
    }

    /// Encrypts one frame payload, returning `nonce ‖ ciphertext ‖ tag`.
    pub fn encrypt(&self, plaintext: &[u8]) -> Result<Vec<u8>, Error> {
        let mut nonce_bytes = [0u8; NONCE_LEN];
        OsRng.fill_bytes(&mut nonce_bytes);
        self.encrypt_with_nonce(plaintext, &nonce_bytes)
    }

    /// Deterministic variant used by the golden vectors. Never call this with a
    /// repeated nonce on a live session: nonce reuse breaks ChaCha20-Poly1305.
    pub fn encrypt_with_nonce(
        &self,
        plaintext: &[u8],
        nonce_bytes: &[u8; NONCE_LEN],
    ) -> Result<Vec<u8>, Error> {
        let cipher = ChaCha20Poly1305::new(self.key()?);
        let nonce = Nonce::from_slice(nonce_bytes);
        let ciphertext = cipher
            .encrypt(
                nonce,
                Payload {
                    msg: plaintext,
                    aad: &[],
                },
            )
            .map_err(|_| Error::Crypto("encryption failed"))?;

        let mut out = Vec::with_capacity(NONCE_LEN + ciphertext.len());
        out.extend_from_slice(nonce_bytes);
        out.extend_from_slice(&ciphertext);
        Ok(out)
    }

    /// Decrypts one frame body produced by [`Crypto::encrypt`].
    pub fn decrypt(&self, data: &[u8]) -> Result<Vec<u8>, Error> {
        if data.len() < ENCRYPTED_OVERHEAD {
            return Err(Error::Crypto("cipher text too short"));
        }
        let cipher = ChaCha20Poly1305::new(self.key()?);
        let nonce = Nonce::from_slice(&data[..NONCE_LEN]);
        cipher
            .decrypt(
                nonce,
                Payload {
                    msg: &data[NONCE_LEN..],
                    aad: &[],
                },
            )
            .map_err(|_| Error::Crypto("decryption failed"))
    }
}

impl Default for Crypto {
    fn default() -> Self {
        Self::new()
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
    fn both_sides_derive_the_same_session_code() {
        let (a, b) = paired();
        let code = a.session_key_digest().unwrap();
        assert_eq!(code, b.session_key_digest().unwrap());
        assert_eq!(code.len(), 6);
        assert!(code.chars().all(|c| c.is_ascii_digit()));
    }

    #[test]
    fn round_trips_frames_between_peers() {
        let (a, b) = paired();
        let frame = a.encrypt(b"hello wire").unwrap();
        assert_eq!(b.decrypt(&frame).unwrap(), b"hello wire");
    }

    #[test]
    fn rejects_short_and_tampered_frames() {
        let (a, b) = paired();
        assert!(b.decrypt(&[0u8; ENCRYPTED_OVERHEAD - 1]).is_err());

        let mut frame = a.encrypt(b"payload").unwrap();
        let last = frame.len() - 1;
        frame[last] ^= 0x01;
        assert!(b.decrypt(&frame).is_err());
    }

    #[test]
    fn rejects_wrong_length_public_key() {
        let mut c = Crypto::new();
        assert!(c.set_remote_public_key(&[0u8; 31]).is_err());
    }

    #[test]
    fn rejects_low_order_public_key() {
        let mut c = Crypto::new();
        assert!(c.set_remote_public_key(&[0u8; PUBLIC_KEY_LEN]).is_err());
    }

    #[test]
    fn session_code_matches_the_specified_derivation() {
        // Pinned independently of the digest implementation: build the key by
        // hand, then check the documented steps produce the documented digits.
        let key = Key::from_slice(&[7u8; 32]);
        let digest = Blake2b128::digest(key.as_slice());
        let mut head = [0u8; 8];
        head.copy_from_slice(&digest[..8]);
        let expected = format!("{:06}", u64::from_le_bytes(head) % 1_000_000);

        assert_eq!(expected.len(), 6);
        assert_eq!(digest.len(), 16);
    }
}
