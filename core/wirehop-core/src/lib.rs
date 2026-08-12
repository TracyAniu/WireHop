//! WireHop core: a clean-room implementation of the WireHop wire protocol.
//!
//! This crate is written from [`docs/references/PROTOCOL.md`], not ported from
//! the C++/Qt application. That is deliberate (see
//! `docs/decisions/2026-08-12-rust-core-architecture.md`): implementing from
//! the specification validates the specification, keeps the inherited LANDrop
//! BSD-3-Clause obligation confined to the Qt tree, and turns any divergence
//! into a documentation bug rather than a silent behavior fork.
//!
//! The two implementations are held together by golden vectors and a loopback
//! interop test, both CI gates.
//!
//! Scope today is protocol v1 — the inherited LANDrop 0.4.0 format plus the
//! additive version/capability negotiation. The v2 transport (TLS 1.3,
//! persistent identity, large frames) is a later milestone.

pub mod crypto;
pub mod discovery;
pub mod dnssd;
pub mod frame;
pub mod identity;
pub mod message;
pub mod policy;
pub mod protocol;
pub mod session;
pub mod store;
pub mod tls;

/// Errors surfaced by the core.
///
/// Deliberately coarse: a peer must not learn which validation rule it
/// tripped, and the session's response to every variant is the same — abort.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("cryptographic failure: {0}")]
    Crypto(&'static str),
    #[error("protocol violation: {0}")]
    Protocol(&'static str),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}
