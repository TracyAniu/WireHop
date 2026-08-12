# Execution Plan: Protocol Version and Capability Negotiation

## Goal

WireHop peers exchange a protocol version and capability list inside the existing encrypted handshake, store the negotiated result on the session, and use it to gate the completion-ACK wait — while remaining wire-compatible with LANDrop 0.4.0 peers in both directions. Observable outcomes: the loopback suite shows capabilities negotiated end-to-end; a sender talking to a peer that advertises no `ack` capability no longer sits in the 10-second `WAITING_FOR_ACK` window (short grace only); `docs/references/PROTOCOL.md` exists and describes wire format v0 (legacy) and v1.

## Context

- `docs/ARCHITECTURE.md` → Known Tradeoffs and Risks: "The transfer protocol has no explicit version field, so wire changes can silently break compatibility." This plan removes that risk.
- `docs/跨平台局域网文件传输工具技术调研.md` §3.4: capability-bit negotiation is the prerequisite for the fast-path/resume/trusted-devices roadmap.
- `docs/ENGINEERING_STANDARDS.md` → Protocol and State Machines: protocol changes require an architecture decision covering compatibility, rollout/versioning, and failure behavior.
- Precedent: the additive ACK extension (`docs/ARCHITECTURE.md` Data Flow step 8) proved that unknown JSON keys inside encrypted frames are ignored by LANDrop 0.4.0 parsers.
- Affected sources: `WireHop/filetransfersession.{h,cpp}`, `WireHop/filetransfersender.{h,cpp}`, `WireHop/filetransferreceiver.{h,cpp}`, `WireHop/discoveryservice.cpp`, `tests/tst_filetransfersession.cpp`.

## Scope

- One shared protocol-constants header: `PROTOCOL_VERSION = 1`, known capability identifiers (`ack` first), and validation bounds.
- Sender metadata JSON additionally carries `protocol_version` (int) and `caps` (string array); the receiver's response JSON mirrors both; each side stores the peer's set and the negotiated intersection on the session.
- ACK becomes the first formalized capability. Sender completion policy: peer advertised `ack` → keep the full 10 s wait; peer sent a response without caps (legacy LANDrop or WireHop ≤ 0.1.0) → short grace wait (~2 s) so a 0.1.0 receiver's fast ACK is still caught, then the existing qualified "sent, not confirmed" message; an ACK arriving during grace still yields "Done!".
- Discovery advertisement (`DiscoveryService::sendInfo`) additionally carries the same two fields as an untrusted hint; no parsing/UI use yet.
- Bounded validation of the new untrusted fields (caps: strings only, ≤ 32 entries, ≤ 32 chars each; non-conforming input ⇒ treat peer as legacy).
- Loopback regression tests for all peer-version pairings and malformed input.
- New ADR under `docs/decisions/`, new `docs/references/PROTOCOL.md`, and updates to ARCHITECTURE/TESTING/PRODUCT where facts change.

## Non-goals

- No framing changes (two-byte length prefix stays), no large frames, no resume, no trusted devices — each is a follow-up capability that this machinery enables.
- No discovery-side parsing or UI surfacing of peer capabilities (discovery is unauthenticated; the in-session values are authoritative).
- No settings schema or dialog changes; no new user-visible strings expected (existing completion messages are reused).

## Compatibility and Risk

- Wire compatibility: purely additive JSON keys inside existing encrypted frames. Pairing matrix to be recorded in the ADR: legacy↔legacy unchanged; v1 sender → legacy receiver (fields ignored, grace-wait path); legacy sender → v1 receiver (absent fields ⇒ pv 0, response still carries v1 fields which legacy ignores); v1↔v1 (full negotiation). WireHop 0.1.0 peers behave as legacy-plus-ACK; the grace wait preserves their confirmation UX.
- Failure behavior: absent, malformed, or out-of-bounds version/caps ⇒ peer treated as pv 0 with no caps; the session proceeds. Malformed JSON overall keeps the existing error path.
- Security: negotiation happens inside the AEAD channel after key exchange; the discovery copy is a spoofable hint and must never gate security decisions. No new logging of sensitive data.
- Behavior-change risk: sender completion timing changes for capless peers; covered by dedicated regression tests for both cap states.

## Implementation Steps

- [x] Read `AGENTS.md`, run `git status --short`, protect any existing changes; trace metadata/response/ACK code paths in the current sources.
- [x] Add the protocol-constants module (`WireHop/protocol.h/.cpp`) and include it from session/sender/receiver/discovery; register in both `.pro` files.
- [x] Receiver: validate and store peer version/caps from metadata (`adoptPeerNegotiation`); emit version/caps in the response JSON.
- [x] Sender: emit version/caps in metadata; adopt response fields; cap-gated ACK wait (`ACK_GRACE_TIMEOUT_MSECS = 2000` for capless peers via `watchdogIntervalMsecs`).
- [x] `DiscoveryService::sendInfo`: advertisement fields added (untrusted hint).
- [x] Tests: `capabilityNegotiationIsAdopted`, `legacyResponseUsesShortAckGrace` (8 s bound proves the window shrank), `legacyMetadataStillTransfers` (raw-socket LANDrop emulation, verifies response fields + ack), `oversizedCapsListIsTreatedAsLegacy`; new `tests/tst_protocol.cpp` for the bounded parsers.
- [x] Docs: ADR `docs/decisions/2026-08-12-protocol-versioning.md`; `docs/references/PROTOCOL.md`; ARCHITECTURE (data flow, module boundaries, risks); TESTING (loopback coverage); PRODUCT (Capability term).

## Validation

- [x] `./scripts/lint.sh` (agent sandbox, exit 0)
- [ ] `./scripts/typecheck.sh` — **blocked in the agent sandbox**: no Qt 5 toolchain and package mirrors are proxy-denied. Must run on a developer machine.
- [ ] `./scripts/test.sh` (extended loopback suite) — same local-run gap.
- [ ] `./scripts/smoke.sh` — same local-run gap.
- [ ] Manual `docs/TESTING.md` workflow 3 (accepted transfer, two peers) — record explicitly if only single-machine loopback was exercised.

## Progress Log

- 2026-08-12: Plan created from research doc §3.4 and ARCHITECTURE risk list. Implementation pending repository access beyond `docs/` in the current session.
- 2026-08-12: Implemented on branch `protocol-versioning` (branched from `macos-share-services`): Protocol module, session-level adoption (`peerProtocolVersion`/`peerCaps`/`peerHasCap`), sender/receiver/discovery field emission, cap-gated ACK grace, four new loopback tests plus `tst_protocol.cpp`, ADR + PROTOCOL.md + ARCHITECTURE/TESTING/PRODUCT updates. `lint.sh` green in the sandbox. **Compile and test execution did not run**: the sandbox lacks Qt 5 and its proxy denies apt mirrors and micromamba, so `typecheck.sh`/`test.sh`/`smoke.sh` must be run on a developer machine before validation can be marked complete. Code was human-review-ready via full-diff inspection only.

## Open Questions

- None.

## Completion Notes

(to be filled at completion)
