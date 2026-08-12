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
- Sender metadata JSON additionally carries `protocol_version` (int) and `caps` (string array); the receiver's response JSON mirrors both; each side stores the peer's set on the session and queries the negotiated intersection through `hasNegotiatedCap()`.
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

- [x] `./scripts/lint.sh` (exit 0, re-run on the developer machine)
- [x] `./scripts/typecheck.sh` — macOS 15.7 / Qt 5.15.16 arm64, clean under `-Wall -Wextra`.
- [x] `./scripts/test.sh` — **48 passing** (25 policy + 14 session + 9 protocol), 0 failed.
- [x] `./scripts/smoke.sh` — app launched and stayed alive.
- [ ] Manual `docs/TESTING.md` workflow 3 (accepted transfer, two peers) — **not run.** Only single-machine loopback was exercised. Requires a second machine and a human accept click; the GUI dialog path cannot be driven here (accessibility permissions block scripted window control).

## Progress Log

- 2026-08-12: Plan created from research doc §3.4 and ARCHITECTURE risk list. Implementation pending repository access beyond `docs/` in the current session.
- 2026-08-12: Implemented on branch `protocol-versioning` (branched from `macos-share-services`): Protocol module, session-level adoption (`peerProtocolVersion`/`peerCaps`/`peerHasCap`), sender/receiver/discovery field emission, cap-gated ACK grace, four new loopback tests plus `tst_protocol.cpp`, ADR + PROTOCOL.md + ARCHITECTURE/TESTING/PRODUCT updates. `lint.sh` green in the sandbox. **Compile and test execution did not run**: the sandbox lacks Qt 5 and its proxy denies apt mirrors and micromamba, so `typecheck.sh`/`test.sh`/`smoke.sh` must be run on a developer machine before validation can be marked complete. Code was human-review-ready via full-diff inspection only.

- 2026-08-12 (validation round): Ran the full gate set on the developer machine — the gap the previous entry recorded is closed. `lint`/`typecheck`/`test`/`smoke` all green; suite grew 46 → 48. Then ran an independent `/code-review` over `559d41c` (self-review had been the only prior check) and acted on it:
  - **Verified and rejected** the review's headline claim that the 2 s grace is a net regression. Its premise is correct — `de522fe:filetransferreceiver.cpp:247` shows the pre-ACK receiver calls `disconnectFromHost()` on completion, so LANDrop peers always resolved instantly via the close path and never consumed the 10 s window. But the harm case (a deployed "acks, no caps" peer) does not exist: `57df1d2` is not on master and every tag is inherited from upstream LANDrop, so ACK and negotiation ship together. Timeout left unchanged; the reasoning is now recorded in the ADR and PROTOCOL.md instead of the phantom peer row that justified it.
  - **Fixed:** `peerHasCap` was a raw peer claim, not the intersection this plan promised — renamed to `hasNegotiatedCap()` and now requires the capability in `localCaps()` too. Capability array serialized in sorted order (QSet iteration order is per-process randomized, which would have blocked byte-reproducible frames). Capability length bounded in UTF-8 bytes rather than UTF-16 units, matching `MAX_FILENAME_BYTES` on the same trust boundary (`MAX_CAP_LENGTH` → `MAX_CAP_BYTES`).
  - **Fixed in tests:** `LegacyReceiver` had been silently invalidated by this very commit — it inherits `respond()`, so since negotiation was added it advertises `caps:["ack"]` and no longer models a legacy peer. Renamed `SilentAckReceiver` with a comment stating what it actually covers; the capless paths were already covered by the raw-socket tests. Removed the 3-way duplication of the framing helpers, and stopped `readNextFrame` from letting a decrypt exception unwind through QTest (that aborts the whole binary). Added `capsAcceptMaximumByteLengthEntry` and `negotiationFieldOrderIsDeterministic`.
  - **Deferred** (recorded, not actioned): metadata frames have no size accounting and `handshake1Finished` discards `encryptAndSend`'s result, so a policy-accepted 1024-file selection can exceed the 65,507-byte payload ceiling and strand the sender until the 300 s HANDSHAKE2 watchdog — pre-existing, not introduced here. Discovery advertises negotiation fields no code reads (a deliberate ADR choice, kept).

## Open Questions

- None.

## Completion Notes

Shipped on branch `protocol-versioning`. Version/capability negotiation rides additively inside the encrypted metadata and response frames; malformed or absent fields degrade to version 0 with no capabilities and never abort a session. `ack` is the first negotiated capability, gating only the sender's acknowledgment-wait duration.

Validation: lint, typecheck, 48-test suite, and smoke all green on macOS 15.7 / Qt 5.15.16 arm64, plus an independent code review whose findings are resolved above. **Not verified:** a real two-machine transfer (TESTING.md workflow 3) and any non-macOS runtime — single-machine loopback is the extent of the evidence. Interoperability with a genuine LANDrop 0.4.0 binary is covered only by raw-socket emulation of its wire shape, not by running the real application.
