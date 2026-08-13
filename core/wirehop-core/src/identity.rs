//! Persistent device identity for wire protocol v2.
//!
//! From `docs/decisions/2026-08-12-v2-transport.md`. A device generates one
//! self-signed certificate and keeps it; its **fingerprint is the SHA-256 of
//! the DER certificate**, which is what makes a device recognizable across
//! sessions.
//!
//! This is the capability v1 structurally lacks. v1 generates a fresh
//! ephemeral key per session, so every peer is a stranger every time and
//! "trusted devices" cannot be built on it at all.

use std::path::Path;

use rcgen::{CertificateParams, DistinguishedName, DnType, KeyPair};
use sha2::{Digest, Sha256};

use crate::Error;

/// Filenames inside the identity directory.
const CERT_FILE: &str = "identity.crt.der";
const KEY_FILE: &str = "identity.key.der";

/// A device's long-term identity.
#[derive(Clone)]
pub struct Identity {
    certificate_der: Vec<u8>,
    private_key_der: Vec<u8>,
}

impl Identity {
    /// Generates a fresh identity.
    ///
    /// The certificate carries no meaningful subject: peers are recognized by
    /// fingerprint, never by name or by a certificate authority. Putting a
    /// device name in it would invite treating that name as verified, which
    /// it is not.
    pub fn generate() -> Result<Self, Error> {
        let key_pair =
            KeyPair::generate().map_err(|_| Error::Crypto("cannot generate an identity key"))?;

        let mut params = CertificateParams::new(vec!["wirehop".to_string()])
            .map_err(|_| Error::Crypto("cannot build certificate parameters"))?;
        let mut name = DistinguishedName::new();
        name.push(DnType::CommonName, "WireHop device");
        params.distinguished_name = name;

        let certificate = params
            .self_signed(&key_pair)
            .map_err(|_| Error::Crypto("cannot self-sign the identity certificate"))?;

        Ok(Self {
            certificate_der: certificate.der().to_vec(),
            private_key_der: key_pair.serialize_der(),
        })
    }

    /// Loads the identity from `dir`, generating and persisting one if absent.
    ///
    /// Regenerating would silently change this device's fingerprint and make
    /// every peer that had trusted it treat it as a stranger, so a load
    /// failure on an existing file is an error rather than a quiet reset.
    pub fn load_or_create(dir: &Path) -> Result<Self, Error> {
        let cert_path = dir.join(CERT_FILE);
        let key_path = dir.join(KEY_FILE);

        if cert_path.exists() && key_path.exists() {
            return Ok(Self {
                certificate_der: std::fs::read(&cert_path).map_err(Error::Io)?,
                private_key_der: std::fs::read(&key_path).map_err(Error::Io)?,
            });
        }

        std::fs::create_dir_all(dir).map_err(Error::Io)?;
        let identity = Self::generate()?;
        std::fs::write(&cert_path, &identity.certificate_der).map_err(Error::Io)?;
        Self::write_private_key(&key_path, &identity.private_key_der)?;
        Ok(identity)
    }

    /// Writes the private key with owner-only permissions where the platform
    /// supports it. A world-readable identity key would let any local process
    /// impersonate this device to its trusted peers.
    #[cfg(unix)]
    fn write_private_key(path: &Path, bytes: &[u8]) -> Result<(), Error> {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;

        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)
            .map_err(Error::Io)?;
        file.write_all(bytes).map_err(Error::Io)
    }

    #[cfg(not(unix))]
    fn write_private_key(path: &Path, bytes: &[u8]) -> Result<(), Error> {
        std::fs::write(path, bytes).map_err(Error::Io)
    }

    pub fn certificate_der(&self) -> &[u8] {
        &self.certificate_der
    }

    pub fn private_key_der(&self) -> &[u8] {
        &self.private_key_der
    }

    /// SHA-256 of the DER certificate: this device's stable identity.
    pub fn fingerprint(&self) -> Fingerprint {
        Fingerprint::of_certificate(&self.certificate_der)
    }
}

/// A peer's certificate fingerprint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Fingerprint([u8; 32]);

impl Fingerprint {
    pub fn of_certificate(der: &[u8]) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(der);
        let digest = hasher.finalize();
        let mut bytes = [0u8; 32];
        bytes.copy_from_slice(&digest);
        Self(bytes)
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Lowercase hex, for storage and logs.
    pub fn to_hex(&self) -> String {
        self.0.iter().map(|b| format!("{b:02x}")).collect()
    }

    pub fn from_hex(text: &str) -> Option<Self> {
        if text.len() != 64 {
            return None;
        }
        let mut bytes = [0u8; 32];
        for (i, byte) in bytes.iter_mut().enumerate() {
            *byte = u8::from_str_radix(text.get(i * 2..i * 2 + 2)?, 16).ok()?;
        }
        Some(Self(bytes))
    }
}

impl std::fmt::Display for Fingerprint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_hex())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn tempdir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "wirehop-identity-{}-{}-{:?}",
            tag,
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn generated_identities_are_distinct() {
        let a = Identity::generate().unwrap();
        let b = Identity::generate().unwrap();
        assert_ne!(a.fingerprint(), b.fingerprint());
        assert!(!a.certificate_der().is_empty());
        assert!(!a.private_key_der().is_empty());
    }

    #[test]
    fn identity_persists_across_loads() {
        let dir = tempdir("persist");
        let first = Identity::load_or_create(&dir).unwrap();
        let second = Identity::load_or_create(&dir).unwrap();

        // The whole point: the fingerprint must survive a restart, or every
        // peer that trusted this device would see a stranger.
        assert_eq!(first.fingerprint(), second.fingerprint());
        assert_eq!(first.certificate_der(), second.certificate_der());
    }

    #[test]
    fn fingerprint_is_the_sha256_of_the_certificate() {
        let identity = Identity::generate().unwrap();
        let mut hasher = Sha256::new();
        hasher.update(identity.certificate_der());
        assert_eq!(identity.fingerprint().as_bytes()[..], hasher.finalize()[..]);
    }

    #[test]
    fn fingerprints_round_trip_through_hex() {
        let identity = Identity::generate().unwrap();
        let hex = identity.fingerprint().to_hex();
        assert_eq!(hex.len(), 64);
        assert_eq!(Fingerprint::from_hex(&hex), Some(identity.fingerprint()));

        assert_eq!(Fingerprint::from_hex(""), None);
        assert_eq!(Fingerprint::from_hex(&"a".repeat(63)), None);
        assert_eq!(Fingerprint::from_hex(&"z".repeat(64)), None);
    }

    #[cfg(unix)]
    #[test]
    fn private_key_is_not_world_readable() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempdir("perms");
        Identity::load_or_create(&dir).unwrap();

        let mode = std::fs::metadata(dir.join(KEY_FILE))
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600, "identity key must be owner-only, got {mode:o}");
    }
}
