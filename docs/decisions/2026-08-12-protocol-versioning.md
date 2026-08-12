# 2026-08-12: Wire-protocol version and capability negotiation

## Context

WireHop inherited the LANDrop 0.4.0 wire format, which has no version field; `docs/ARCHITECTURE.md` listed "wire changes can silently break compatibility" as a standing risk, and the roadmap (research report §3.4) requires negotiated features (resume, larger frames, trusted devices). The completion-ACK extension already proved that additive JSON keys inside encrypted frames are ignored by legacy parsers.

## Decision

In-session, additive negotiation. The sender's metadata frame and the receiver's response frame each carry `protocol_version` (this build: 1) and `caps` (string array). Rules: absent or malformed fields mean a version-0 peer with no capabilities and never abort a session; the untrusted list is bounded (≤ 32 entries, 1–32 chars, strings only) and discarded wholesale on any violation; negotiated features take effect only after the response frame; version bumps are reserved for message-format breaks while orthogonal features are capabilities; unknown capabilities are ignored. Discovery advertisements carry the same fields strictly as hints — discovery is unauthenticated UDP and must not gate behavior.

`ack` is the first formalized capability: the sender keeps the 10-second acknowledgment window only for peers that advertised it, and applies a 2-second grace window otherwise, so LANDrop 0.4.0 receivers that neither acknowledge nor close no longer stall the sender for 10 seconds.

## Alternatives considered

A version byte before the key exchange was rejected: LANDrop 0.4.0 receivers read exactly 32 raw key bytes, so any prefix breaks them, and manual IP connections lack the discovery pre-knowledge that could gate it. Discovery-only negotiation was rejected as authority because it is spoofable and absent for manual connections (kept as a hint). Keeping the unconditional 10-second ACK wait was rejected because it penalizes every legacy transfer; the accepted trade is that a pre-capability WireHop 0.1.0 receiver whose final commit takes longer than the grace shows qualified success on the sender even though delivery succeeded.

## Compatibility and failure behavior

Wire compatibility is purely additive; the full pairing matrix is recorded in `docs/references/PROTOCOL.md`. Failure behavior: malformed negotiation input downgrades to version-0 semantics, existing malformed-JSON error paths are unchanged, and the response to a rejecting receiver still carries the fields harmlessly.

## Validation

`tst_protocol.cpp` covers the bounded parsers. The loopback suite covers v1↔v1 adoption on both ends, LANDrop-0.4.0-shaped metadata and response peers emulated on raw sockets, an out-of-bounds capability list degrading to legacy while the transfer completes, and the grace-window bound (< 8 s where the legacy window was 10 s). Compile/test execution status is tracked in `docs/exec-plans/active/protocol-versioning.md`.
