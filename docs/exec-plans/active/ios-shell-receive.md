# Execution Plan: iOS Shell — v1 Receiver

## Goal

An iPhone receives a file from the macOS WireHop application over the real network: the transfer appears on the phone, both devices show the same six-digit code, the user accepts, and the bytes land intact. This closes the gap that every previous milestone has recorded as unverified — real two-machine transfer — because the iPhone is the second machine.

## Context

- Ordering chosen by the owner: the shell jumps ahead of M2b, and speaks **v1**, because v1 is complete, cross-verified, and already spoken by the macOS baseline. The shell will need a second pass for v2 after M2b; that is accepted to get real-network verification sooner.
- Architecture: `docs/decisions/2026-08-12-rust-core-architecture.md` — native shells over the Rust core.
- Discovery contract: `docs/exec-plans/completed/rust-core-m1b-dnssd.md` and `PROTOCOL.md` §"Discovery over DNS-SD". iOS must reach mDNS through Bonjour (`NWListener`/`NWBrowser`); a bundled responder would need the multicast entitlement.
- Toolchain confirmed present: Xcode 16.2, iOS 18.2 SDK, simulators, and two Apple Development signing identities, so physical-device deployment is possible.

## The API gap this exposes

`session::receive_files(stream, dir, accept)` takes the accept decision as a parameter. That suits a CLI and suits tests, but no graphical shell can use it: a receiver must complete the handshake, show the session code and file list, **wait for the user**, and only then transfer. Splitting the receive path into two phases is a prerequisite for any shell, not an iOS detail.

## Scope

**Phase A — core receive API (this session):**
- `IncomingTransfer`: handshake and metadata parse, stopping before the response. Exposes session code, device name, file list, total size, and the negotiated peer profile.
- `accept(dir)` and `reject()` consume it and drive the rest of the existing state machine.
- `receive_files` retained as a thin wrapper so the CLI, tests, and interop gate are unchanged.
- iOS cross-compilation verified: `aarch64-apple-ios` and `aarch64-apple-ios-sim` targets build the core.

**Phase B — FFI:**
- UniFFI bindings per the architecture decision, exposing the listener, the pending-transfer fields, and accept/reject.
- A script producing an XCFramework for device and simulator.

**Phase C — Swift app:**
- SwiftUI receive-only app: listener, incoming-transfer sheet showing the code and files, accept/reject, files written into Documents (`UIFileSharingEnabled`, `LSSupportsOpeningDocumentsInPlace`).
- Bonjour registration of `_wirehop._tcp` with the TXT records from `core::dnssd`.
- `Info.plist`: `NSLocalNetworkUsageDescription` and `NSBonjourServices`; iOS 14+ prompts for local-network permission on first use.

## Non-goals

- **Sending from iOS.** That needs a document/photo picker, security-scoped resources, and large-file memory handling — a separate milestone. Receiving alone exercises the real network, real TCP, real crypto, and Qt ↔ Rust-core interoperability, which is what the two-machine gap is about.
- v2/TLS on iOS (after M2b), background receiving (iOS suspends sockets; the app must be foreground, as LocalSend and LANDrop also require), and any App Store distribution.
- No change to the macOS application.

## Compatibility and Risk

- **Wire:** none. The shell speaks v1 exactly as specified.
- **API:** `receive_files` keeps its signature and behavior, so the CLI, the Rust session tests, and the Qt interop gate are untouched. The risk is a behavior change hidden in the refactor; the existing tests are the guard, and they must pass unmodified.
- **Local-network permission:** iOS 14+ shows a system prompt on first local-network access. A denied prompt looks exactly like a broken network, so the app must state what it needs before triggering it.
- **Discovery may not work on the first try** even with Bonjour: the macOS side does not register a DNS-SD service yet — it broadcasts. Until the macOS side also registers, the phone will not be discovered automatically and the Mac must be pointed at it by IP. The first real transfer test should therefore use manual IP entry and not depend on discovery.
- **Signing:** Apple Development identities give 7-day provisioning without a paid account; a device build may need re-signing during testing.

## Implementation Steps

- [x] Phase A: `IncomingTransfer` two-phase receive; `receive_files` preserved as a wrapper; existing tests pass unchanged.
- [x] Phase A: iOS targets installed; core cross-compiles for `aarch64-apple-ios` and `aarch64-apple-ios-sim`.
- [ ] Phase B: UniFFI bindings and XCFramework build script.
- [ ] Phase C: SwiftUI receiver, Bonjour registration, Info.plist keys.
- [ ] Two-machine test: macOS app → iPhone, manual IP, byte-compare the received file.
- [ ] Docs: ARCHITECTURE (shell layer), TESTING (the two-machine workflow, now runnable).

## Validation

- [x] `./scripts/lint.sh`, `./scripts/typecheck.sh`, `./scripts/test.sh`, `./scripts/smoke.sh` — green after Phase A (63 Qt + 85 Rust cases).
- [x] Core builds for `aarch64-apple-ios` and `aarch64-apple-ios-sim`, release profile.
- [ ] **Real two-machine transfer**, byte-compared. This is the milestone's reason to exist; anything short of it leaves the gap open.

## Progress Log

- 2026-08-12: Plan created. Recorded the receive-API gap found while planning: the current entry point takes the accept decision as a parameter, which no graphical shell can use.
- 2026-08-12: **Phase A done.** `IncomingTransfer` splits receive into request → decide → transfer, exposing the session code, device name, file list, total size, and negotiated peer before anything is answered or written. `receive_files` is now a thin wrapper over it, so the CLI, the Rust session tests, and the Qt interop gate are untouched — and all of them passing unmodified is the evidence the refactor preserved behavior. Two new tests pin the property a shell depends on: everything needed to prompt is available before responding, nothing is on disk at that point, and a deliberately delayed decision still completes. iOS targets installed and the core cross-compiles for device and simulator, which clears the main unknown — `ring` and `rustls` build for iOS without extra toolchain work.

  Still ahead: UniFFI bindings and the XCFramework (Phase B), then the SwiftUI receiver and Bonjour registration (Phase C). **No iOS code exists yet**, and the two-machine transfer this plan exists to perform has not happened.

## Open Questions

- Whether the macOS side should also register the DNS-SD service so the phone is discoverable without manual IP. Likely yes, but it is Qt-side work on the baseline and was deliberately out of scope in M1b.
- Where iOS stores received files so they are visible in the Files app, and whether that needs a document-type declaration.

## Completion Notes

(to be filled at completion)
