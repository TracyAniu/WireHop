//! TLS 1.3 transport for wire protocol v2.
//!
//! From `docs/decisions/2026-08-12-v2-transport.md`. Replaces v1's bespoke
//! AEAD framing with the TLS record layer, which supplies sequencing, replay
//! detection, and rekeying that v1's random per-frame nonces cannot.
//!
//! **Certificates are not validated against a CA, by design.** Peers are
//! self-signed and identified by fingerprint, so this module accepts any
//! certificate and reports the fingerprint it saw. That is not a security
//! hole on its own — it is trust-on-first-use with the decision deferred to
//! the caller — but it means a caller that ignores the fingerprint has no
//! authentication at all. The trust store that turns a fingerprint into a
//! decision is M3; until then the session code is the check.

use std::sync::Arc;

use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, ServerName, UnixTime};
use rustls::server::danger::{ClientCertVerified, ClientCertVerifier};
use rustls::{
    ClientConfig, DigitallySignedStruct, DistinguishedName, ServerConfig, SignatureScheme,
};

use crate::identity::{Fingerprint, Identity};
use crate::Error;

/// Exporter label for the session code. Fixed forever: changing it changes
/// every device's displayed code.
const SAS_LABEL: &[u8] = b"wirehop v2 session code";

/// Bytes drawn from the exporter before reduction to six digits.
const SAS_BYTES: usize = 8;

/// Accepts any certificate and records nothing.
///
/// Verification is deliberately absent: identity is the fingerprint the
/// caller reads after the handshake. Signature checking still happens — this
/// only removes chain and name validation, which are meaningless for
/// self-signed peers on a LAN.
#[derive(Debug)]
struct AcceptAnyPeer;

impl AcceptAnyPeer {
    fn schemes() -> Vec<SignatureScheme> {
        vec![
            SignatureScheme::ECDSA_NISTP256_SHA256,
            SignatureScheme::ECDSA_NISTP384_SHA384,
            SignatureScheme::ED25519,
            SignatureScheme::RSA_PSS_SHA256,
            SignatureScheme::RSA_PSS_SHA384,
            SignatureScheme::RSA_PSS_SHA512,
        ]
    }

    fn verify_tls13(
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &rustls::crypto::ring::default_provider().signature_verification_algorithms,
        )
    }
}

impl ServerCertVerifier for AcceptAnyPeer {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        // TLS 1.2 is not enabled; reaching here would be a configuration bug.
        Err(rustls::Error::PeerIncompatible(
            rustls::PeerIncompatible::Tls12NotOffered,
        ))
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Self::verify_tls13(message, cert, dss)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        Self::schemes()
    }
}

impl ClientCertVerifier for AcceptAnyPeer {
    fn root_hint_subjects(&self) -> &[DistinguishedName] {
        &[]
    }

    fn verify_client_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _now: UnixTime,
    ) -> Result<ClientCertVerified, rustls::Error> {
        Ok(ClientCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Err(rustls::Error::PeerIncompatible(
            rustls::PeerIncompatible::Tls12NotOffered,
        ))
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Self::verify_tls13(message, cert, dss)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        Self::schemes()
    }
}

fn private_key(identity: &Identity) -> Result<PrivateKeyDer<'static>, Error> {
    PrivateKeyDer::try_from(identity.private_key_der().to_vec())
        .map_err(|_| Error::Crypto("identity private key is not valid DER"))
}

/// Client configuration presenting this device's identity.
///
/// **Mutual authentication is required**: both peers present certificates, so
/// each learns the other's fingerprint. A one-sided handshake would leave the
/// receiver unable to recognize the sender, which is exactly what trusted
/// devices needs.
pub fn client_config(identity: &Identity) -> Result<ClientConfig, Error> {
    let certificate = CertificateDer::from(identity.certificate_der().to_vec());
    ClientConfig::builder_with_protocol_versions(&[&rustls::version::TLS13])
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(AcceptAnyPeer))
        .with_client_auth_cert(vec![certificate], private_key(identity)?)
        .map_err(|_| Error::Crypto("cannot build the TLS client configuration"))
}

/// Server configuration presenting this device's identity and requiring the
/// client's.
pub fn server_config(identity: &Identity) -> Result<ServerConfig, Error> {
    let certificate = CertificateDer::from(identity.certificate_der().to_vec());
    ServerConfig::builder_with_protocol_versions(&[&rustls::version::TLS13])
        .with_client_cert_verifier(Arc::new(AcceptAnyPeer))
        .with_single_cert(vec![certificate], private_key(identity)?)
        .map_err(|_| Error::Crypto("cannot build the TLS server configuration"))
}

/// The peer's fingerprint, from the certificate it presented.
pub fn peer_fingerprint(certificates: Option<&[CertificateDer<'_>]>) -> Result<Fingerprint, Error> {
    let end_entity = certificates
        .and_then(|chain| chain.first())
        .ok_or(Error::Crypto("peer presented no certificate"))?;
    Ok(Fingerprint::of_certificate(end_entity))
}

/// Derives the six-digit session code from the TLS exporter.
///
/// This is the substantive difference from v1. There the code came from the
/// *unauthenticated* Diffie-Hellman output, so comparing it was advisory. Here
/// it is bound to the authenticated channel: a man in the middle terminates
/// two separate TLS sessions with different exporter output, so the two
/// devices necessarily display different codes.
/// Generic over the connection role: `ClientConnection` and `ServerConnection`
/// both deref to `ConnectionCommon`, so either can be passed directly.
pub fn session_code<Data>(connection: &rustls::ConnectionCommon<Data>) -> Result<String, Error> {
    // The buffer is taken by value and returned, so no key material is left
    // behind if the export fails.
    let output = connection
        .export_keying_material([0u8; SAS_BYTES], SAS_LABEL, None)
        .map_err(|_| Error::Crypto("cannot export keying material"))?;
    Ok(format!("{:06}", u64::from_le_bytes(output) % 1_000_000))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustls::{ClientConnection, ServerConnection};
    use std::io::{Read, Write};

    /// Drives a client and server through a handshake over in-memory buffers.
    fn handshake(
        client_identity: &Identity,
        server_identity: &Identity,
    ) -> (ClientConnection, ServerConnection) {
        let mut client = ClientConnection::new(
            Arc::new(client_config(client_identity).unwrap()),
            ServerName::try_from("wirehop").unwrap(),
        )
        .unwrap();
        let mut server =
            ServerConnection::new(Arc::new(server_config(server_identity).unwrap())).unwrap();

        // Pump both directions until neither wants to write.
        for _ in 0..16 {
            let mut to_server = Vec::new();
            client.write_tls(&mut to_server).ok();
            if !to_server.is_empty() {
                server.read_tls(&mut to_server.as_slice()).unwrap();
                server.process_new_packets().unwrap();
            }

            let mut to_client = Vec::new();
            server.write_tls(&mut to_client).ok();
            if !to_client.is_empty() {
                client.read_tls(&mut to_client.as_slice()).unwrap();
                client.process_new_packets().unwrap();
            }

            if !client.is_handshaking() && !server.is_handshaking() {
                break;
            }
        }
        assert!(
            !client.is_handshaking(),
            "client handshake did not complete"
        );
        assert!(
            !server.is_handshaking(),
            "server handshake did not complete"
        );
        (client, server)
    }

    #[test]
    fn both_sides_learn_the_peer_fingerprint() {
        let client_identity = Identity::generate().unwrap();
        let server_identity = Identity::generate().unwrap();
        let (client, server) = handshake(&client_identity, &server_identity);

        // Mutual authentication: each side sees the other's certificate.
        assert_eq!(
            peer_fingerprint(client.peer_certificates()).unwrap(),
            server_identity.fingerprint()
        );
        assert_eq!(
            peer_fingerprint(server.peer_certificates()).unwrap(),
            client_identity.fingerprint()
        );
    }

    #[test]
    fn both_sides_derive_the_same_session_code() {
        let (client, server) = handshake(
            &Identity::generate().unwrap(),
            &Identity::generate().unwrap(),
        );

        let code = session_code(&client).unwrap();
        assert_eq!(code, session_code(&server).unwrap());
        assert_eq!(code.len(), 6);
        assert!(code.chars().all(|c| c.is_ascii_digit()));
    }

    #[test]
    fn separate_sessions_derive_different_codes() {
        // The property a man in the middle cannot defeat: it must terminate
        // two distinct TLS sessions, whose exporters differ, so the two
        // devices show different codes.
        let (client_a, _) = handshake(
            &Identity::generate().unwrap(),
            &Identity::generate().unwrap(),
        );
        let (client_b, _) = handshake(
            &Identity::generate().unwrap(),
            &Identity::generate().unwrap(),
        );
        assert_ne!(
            session_code(&client_a).unwrap(),
            session_code(&client_b).unwrap()
        );
    }

    #[test]
    fn application_data_flows_after_the_handshake() {
        let (mut client, mut server) = handshake(
            &Identity::generate().unwrap(),
            &Identity::generate().unwrap(),
        );

        client.writer().write_all(b"v2 payload").unwrap();
        let mut wire = Vec::new();
        client.write_tls(&mut wire).unwrap();
        server.read_tls(&mut wire.as_slice()).unwrap();
        server.process_new_packets().unwrap();

        let mut received = Vec::new();
        server.reader().read_to_end(&mut received).ok();
        assert_eq!(received, b"v2 payload");
    }

    #[test]
    fn negotiates_tls13_only() {
        let (client, _) = handshake(
            &Identity::generate().unwrap(),
            &Identity::generate().unwrap(),
        );
        assert_eq!(
            client.protocol_version(),
            Some(rustls::ProtocolVersion::TLSv1_3)
        );
    }
}
