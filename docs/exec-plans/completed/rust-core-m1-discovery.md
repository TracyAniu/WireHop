# Execution Plan: Rust Core M1 — Discovery Conformance and Peer Table

## Goal

The discovery datagram format becomes a specified, cross-verified part of the protocol rather than logic that exists only inside `DiscoveryService::socketReadyRead()`, and the Rust core gains a discovery service whose peer list is maintained by last-seen expiry and can be warmed by unicast probes to remembered addresses. Observable outcomes: discovery vectors in `docs/references/protocol-vectors.json` verified by both implementations; a Rust discovery service that answers requests, announces itself, and expires peers, tested over loopback; the Qt datagram rules covered by tests for the first time.

## Context

- Milestone map: `docs/exec-plans/completed/rust-core-m0-foundation.md`; architecture: `docs/decisions/2026-08-12-rust-core-architecture.md`.
- Research report §3.1: subnet broadcast beats multicast for delivery (link-layer flooding vs IGMP state), request/response self-heals dropped announcements, and the reliability gaps are expiry, cached-address warm start, and rebuilding sockets on network change.
- Current Qt behavior (`WireHop/discoveryservice.cpp`, `WireHop/sendtodialog.cpp`): UDP 52637, broadcast to `255.255.255.255` plus every interface broadcast address, `{"request":true}` triggers a unicast advertisement back to the source. **Peer identity is the IP address**; the device name is display-only. `port: 0` means "remove this peer from the list", handled in `SendToDialog::newHost`. There is **no expiry** — a peer that disappears silently stays listed until it announces `port: 0`.
- `discoveryservice.cpp` cannot link into the GUI-free test suite (it pulls in `QMessageBox`/`QApplication`), which is why discovery has never had a single automated test.

## Scope

- `PROTOCOL.md`: a normative discovery section — request vs advertisement shapes, `port: 0` semantics, validation bounds, self-filtering, "ignore unknown keys", and the explicit statement that identity is the source address.
- Qt: extract datagram construction and parsing out of `DiscoveryService` into the GUI-free `Protocol` module so it can be linked and tested. `DiscoveryService` keeps socket concerns (size bound, self-address filtering, send targets). Behavior preserved exactly.
- Rust `wirehop-core::discovery`: the same codec clean-room from the spec, a `PeerTable` with last-seen expiry and IP identity, and a `DiscoveryService` over UDP that binds, announces, answers requests, and probes remembered addresses by unicast.
- Conformance vectors extended with discovery datagrams; both suites verify them.
- Tests: Qt unit tests for the extracted codec; Rust unit tests for codec and peer table; a Rust loopback test for the service. **Tests must not broadcast to the LAN** — they use explicit unicast targets on 127.0.0.1.

## Non-goals

- mDNS and multicast channels, and network-change socket rebuilding (M1b). The report's own analysis puts subnet broadcast ahead of multicast for delivery, and mDNS is driven by the iOS multicast entitlement, which matters only once a mobile shell exists.
- Fingerprint-keyed address cache: identity is an M3 deliverable. M1 caches by address, which is what the current protocol can support.
- No change to the Qt peer-list UI, and no expiry added to the Qt side yet — the Rust table is where the new behavior lands first.
- No IPv6 scope-id handling yet.

## Compatibility and Risk

- **Wire:** none. The datagram format is unchanged; it is only being written down and cross-verified.
- **Behavior:** the Qt extraction must be a pure refactor. Risk is a subtle change in validation order or in which malformed inputs are dropped; mitigated by porting the checks verbatim and covering them with the new tests before and after.
- **Test hygiene:** a discovery test that broadcasts would spray the developer's real network and could pick up genuine peers, making results nondeterministic. All tests bind loopback and target explicit addresses.
- **Security:** discovery stays unauthenticated by design. The advertised `protocol_version`/`caps` remain untrusted hints and must not gate behavior; the peer table must not become an input to any trust decision.

## Implementation Steps

- [x] Specify the discovery datagram in `PROTOCOL.md`.
- [x] Extract Qt construction/parsing into `Protocol`; rewire `DiscoveryService`; behavior preserved (typecheck + smoke + suite green).
- [x] Qt tests for the extracted codec via the shared fixture (19 parsing cases).
- [x] Rust `discovery` codec, clean-room from the spec.
- [x] Rust `PeerTable`: IP identity, last-seen expiry, `port: 0` removal, name updates.
- [x] Rust `DiscoveryService` over UDP: bind, announce, answer requests unicast, probe remembered addresses, `poll_once` with timeout.
- [x] Extend the conformance fixture with discovery vectors; verified from both suites.
- [x] Docs: ARCHITECTURE (discovery in both implementations), TESTING (new coverage).

## Validation

- [x] `./scripts/lint.sh`
- [x] `./scripts/typecheck.sh`
- [x] `./scripts/test.sh` with `WIREHOP_REQUIRE_RUST=1` — 63 Qt + 64 Rust cases, 0 failures.
- [x] `./scripts/smoke.sh` — green after the refactor.
- [ ] Manual `docs/TESTING.md` workflow 2 (two peers on a LAN) — **not run.** Needs a second machine; tests deliberately never broadcast.

## Progress Log

- 2026-08-12: Plan created after M0 completed. Confirmed by reading the sources that `port: 0` removal is handled correctly in the dialog (not a defect), and that the genuine gaps are the absence of expiry and the untestability of `discoveryservice.cpp`.

## Open Questions

- Whether the Qt side should adopt the peer table and expiry in this program or stay as-is until a shell replaces it. Leaning toward leaving Qt alone: it is the baseline, and divergence in local list-keeping is not a wire concern.

## Completion Notes

M1a complete. The discovery datagram is now specified, extracted from `DiscoveryService` into the GUI-free `Protocol` module, implemented clean-room in Rust, and pinned across both implementations by 3 construction vectors and a 19-case parsing table. `core` additionally has a `PeerTable` with last-seen expiry (the Qt baseline has none) and a blocking `DiscoveryService` that answers requests unicast and can warm-start by probing remembered addresses.

The gate earned its keep again: it caught that the 4096-byte datagram bound lived in Qt's *socket* layer but in Rust's *parser*. Neither violated the spec at the system level, but the parser is the untrusted-input boundary, so the bound now exists on both sides in Qt — defense in depth, with the socket check retained to avoid buffering an oversized datagram at all.

Validation: lint, typecheck, smoke green; 63 Qt + 64 Rust cases, zero failures, under `WIREHOP_REQUIRE_RUST=1`.

**Deferred to M1b:** mDNS and multicast channels, network-change socket rebuild, IPv6 scope handling, and real multi-NIC broadcast enumeration in Rust. **Not verified:** two-peer LAN discovery (needs a second machine; tests deliberately never broadcast). The Rust service is blocking and single-threaded, and its self-filtering compares only the bound socket address — a production caller on a real network must exclude every local interface address.
