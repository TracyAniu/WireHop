# Execution Plan: Rust Core M2 — v2 Transport and Performance

## Goal

Wire protocol v2: TLS 1.3 over TCP with a persistent device identity, and the throughput work that becomes possible once the framing is no longer capped at 65,535 bytes. Observable outcomes: a device has a stable fingerprint across restarts; a v2 session's six-digit code is derived from the TLS exporter so it is bound to the authenticated channel; transfers over v2 move large blocks through a read→encrypt→send pipeline.

## Context

- Decision: `docs/decisions/2026-08-12-v2-transport.md` — TLS 1.3 via `rustls` (`ring` provider), self-signed identity with SHA-256 fingerprint, exporter-derived session code, and a **separate TCP port** for v2 because v1 receivers read exactly 32 raw bytes and no preamble can precede them.
- Research report §2.1 (the three v1 weaknesses this retires), §3.2 (transport choice and the performance discipline), §3.3 (three-layer security model).
- v1 is retained indefinitely; the conformance and interop gates against the Qt baseline continue to cover it unchanged.

## Scope

**M2a — substrate (this session):**
- `identity`: generate, persist, and load a self-signed certificate; fingerprint = SHA-256 of the DER; owner-only key file permissions.
- `tls`: TLS 1.3-only client and server configurations with **mutual** authentication, fingerprint extraction from the peer's certificate, and the exporter-derived session code.

**M2b — sessions and throughput (next):**
- v2 framing (4-byte length, 1–4 MiB blocks) and the v2 session state machine over the TLS stream.
- v2 listener on its own port; `v2_port` advertised in discovery and in v1 in-session fields; fall back to v1 when absent or on connection failure.
- read→encrypt→send pipeline with bounded queues and buffer reuse; per-stage throughput counters.
- Small-file packing stream; BLAKE3 per block for integrity and as the resume baseline.
- Performance discipline from §3.2: an iperf3 ceiling per link, a netem loss × latency matrix in CI, and a >5% regression block.

## Non-goals

- Trust store, TOFU prompts, and trusted-device auto-accept (M3). v2 reports fingerprints; deciding what they mean is M3's job.
- QUIC (M5). LocalSend compatibility (M4).
- No change to the Qt baseline, which stays on v1.

## Compatibility and Risk

- **Wire:** v1 is untouched — same port, same bytes. v2 is additive on a separate port.
- **Loss of the second opinion:** the conformance gate does not extend to v2, because only one implementation speaks it. Accepted knowingly (see the ADR); the compensating factors are that the record layer is `rustls` rather than hand-rolled, and that v1 — the transport both implementations speak — stays cross-verified.
- **Downgrade:** discovery is unauthenticated, so suppressing the `v2_port` hint forces v1. No worse than today's security level, but v2's guarantees are not enforceable against an active attacker until the M3 trust store can refuse a downgrade for a known-v2 peer.
- **Certificate verification is deliberately absent** (self-signed peers, no CA). A caller that ignores the reported fingerprint gets no authentication at all; this is safe only because the session code is still the user-facing check until M3.

## Implementation Steps

- [x] ADR for the v2 transport, including transport selection and the downgrade gap.
- [x] `identity`: generation, persistence, fingerprint, owner-only key permissions.
- [x] `tls`: TLS 1.3-only mutual-auth configurations, peer fingerprint extraction, exporter session code.
- [ ] v2 framing and session state machine over TLS. **Not started.**
- [ ] v2 listener, port advertisement, and v1 fallback. **Not started.**
- [ ] Pipeline, large blocks, small-file packing, BLAKE3. **Not started.**
- [ ] Performance harness (iperf3 ceiling, netem matrix, regression block). **Not started.**

## Validation

- [x] `./scripts/lint.sh` (including `cargo clippy -D warnings`)
- [x] `./scripts/typecheck.sh`
- [x] `./scripts/test.sh` with `WIREHOP_REQUIRE_RUST=1` — 63 Qt + 74 Rust cases, 0 failures.
- [x] `./scripts/smoke.sh`
- [ ] Throughput measured against an iperf3 ceiling. **Not started** — no v2 transfer exists yet to measure.

## Progress Log

- 2026-08-12: M2a landed. ADR recorded; `identity` and `tls` implemented and tested. TLS tests drive a real handshake over in-memory buffers and assert mutual fingerprint learning, matching exporter-derived codes on both sides, distinct codes across distinct sessions (the property a MITM cannot defeat), application data flow, and TLS 1.3-only negotiation. No v2 session or listener yet, so nothing on the wire has changed and v1 remains the only transport in use.

## Open Questions

- Where the identity directory lives per platform once a shell exists; today `load_or_create` takes an explicit path.
- Whether v2 should require the session-code comparison on every session or only on first contact, once the trust store lands.

## Completion Notes

(to be filled at completion)
