# 2026-08-12: Wire protocol v2 — TLS 1.3 transport and persistent identity

## Context

Wire protocol v1 is the LANDrop 0.4.0 construction: ephemeral X25519 exchanged as bare public keys, its raw output used directly as a ChaCha20-Poly1305 key, per-frame random nonces with no sequence, and a 2-byte length prefix capping frames at 65,535 bytes. `docs/跨平台局域网文件传输工具技术调研.md` §2.1 lists the consequences precisely: the Diffie-Hellman exchange is unauthenticated, so a man in the middle succeeds unless users compare the six-digit code; sequence-less nonces cannot detect a dropped or replayed frame; there is no persistent identity, so "trusted devices" is impossible; and the frame ceiling caps throughput.

The six-digit code is derived from the *unauthenticated* shared secret. Comparing it detects a MITM only because both endpoints would compute different codes — it is a detection mechanism that depends entirely on the user actually comparing, with no protocol-level consequence if they do not.

## Decision

Introduce **protocol v2: TCP + TLS 1.3**, via `rustls`, with a persistent self-signed certificate as device identity. v1 is retained indefinitely for LANDrop 0.4.0 and Qt-baseline compatibility; v2 is not a replacement but a second, negotiated transport.

Four decisions, each replacing a specific v1 weakness:

1. **TLS 1.3 (`rustls`, TLS 1.3 only, `ring` provider).** Replaces the bespoke AEAD framing. The record layer supplies sequencing, replay detection, and rekeying — retiring the sequence-less-nonce weakness rather than hand-rolling a counter. `ring` over the default `aws-lc-rs` deliberately: it avoids a cmake/NASM build dependency, which matters for the iOS and Android cross-compilation this program is heading toward.
2. **Persistent self-signed certificate as identity.** Generated once and stored; **fingerprint = SHA-256 of the DER certificate**, matching LocalSend's model. This is the prerequisite for trusted devices (M3) and the thing v1 structurally cannot have.
3. **Session code from the TLS exporter (RFC 5705), not from raw key material.** The six digits become a short authentication string bound to the *authenticated* channel. Under a MITM the two sides derive different exporter output, so the codes differ — the same user gesture now has cryptographic force behind it rather than being advisory.
4. **v2 listens on its own TCP port.** v1 receivers read exactly 32 raw bytes at connect, so no preamble, ALPN token, or version byte can be added to the v1 port without corrupting them — the same constraint that ruled out a version byte for v1. A separate port is the only addition that cannot break an existing peer.

## Alternatives considered

*Upgrading in place on the v1 port* was rejected: any byte sent before the 32-byte key breaks LANDrop 0.4.0, and probing with a TLS ClientHello would be interpreted as key material and corrupt the session.

*Negotiating v2 inside a v1 session and switching on the same connection* was rejected as needless complexity: by the time negotiation completes the transfer is already underway, so the upgrade could only apply to a subsequent connection anyway — which the advertised port achieves without the state machine.

*Keeping the custom AEAD and adding a sequence number* would fix replay detection but not the unauthenticated exchange or the absent identity, and would leave us maintaining a bespoke record layer. TLS 1.3 solves all three with a reviewed implementation.

*`aws-lc-rs`* is rustls's default and is FIPS-oriented, but its build requirements are hostile to mobile cross-compilation.

## Compatibility and failure behavior

Nothing about v1 changes: same port, same bytes, same behavior. A v2-capable device runs both listeners. Selection is:

- The v2 port is advertised in discovery (`v2_port`) and, authoritatively, in the v1 in-session metadata/response fields.
- A sender that knows a peer's v2 port connects there and speaks TLS; otherwise — manual IP entry, no discovery, a v1-only peer — it connects to the v1 port and speaks v1.
- A failed v2 connection falls back to v1 rather than failing the transfer.

**Known downgrade gap, stated plainly:** discovery is unauthenticated, so an attacker who suppresses or rewrites the `v2_port` hint can force a v1 session. This does not make anything worse than today — v1 with code comparison is exactly the current security level — but it does mean v2's guarantees are not yet enforceable against an active attacker. Closing it requires remembering that a peer supports v2 and refusing to downgrade, which depends on the trust store in M3. Until then, v2 protects against passive interception and accidental misconnection, not against an active downgrade.

`protocol_version` is `2` inside a v2 session and stays `1` inside a v1 session, which is what the field was reserved for: a message-format break, not a feature flag.

## Validation

The cross-implementation conformance gate does not extend to v2: the Qt baseline stays on v1, so v2 has one implementation and no second opinion. That is a real loss of assurance and is accepted knowingly — v2's compensating checks are that the record layer is `rustls` rather than our own, and that identity, exporter derivation, and framing are covered by Rust tests. The v1 conformance and interop gates continue to run unchanged, so the transport that both implementations speak stays cross-verified.
