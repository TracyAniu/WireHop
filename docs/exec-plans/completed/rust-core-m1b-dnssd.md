# Execution Plan: Rust Core M1b — DNS-SD Contract for Bonjour and mDNS

## Goal

WireHop is discoverable over DNS-SD, specified once and implemented by two different transports: a raw-multicast responder on platforms where that is allowed, and Apple's Bonjour API on iOS/macOS where it is not. Observable outcome: a service type, instance-naming rule, and TXT schema written into `PROTOCOL.md` and implemented as a bounded codec in the core, so the forthcoming iOS shell has something concrete to implement against and both transports produce interchangeable results.

## Context

- Chosen ordering: mDNS before the iOS shell, so iOS gets discovery rather than manual IP entry only. M2b (v2 sessions and throughput) is deferred behind both.
- **The constraint that shapes this milestone.** iOS 14+ requires `com.apple.developer.networking.multicast` — an entitlement granted only by application to Apple — for an app to send or receive multicast itself. That includes a self-implemented mDNS responder such as `mdns-sd`, which binds `224.0.0.251:5353` and joins the group directly. Calling Apple's Bonjour API (`NWBrowser`/`NWListener`) needs no entitlement because the system daemon performs the multicast on the app's behalf. Adding an mDNS crate to the core therefore does **not** make iOS work; only a shell-provided Bonjour backend does.
- Research report §3.1: mDNS survives on APs that filter custom multicast groups, because 5353 is commonly given special treatment for AirPlay/AirPrint. It is a complement to subnet broadcast (M1a), not a replacement — broadcast still has the higher delivery rate on ordinary LANs.
- Existing discovery: `core/wirehop-core/src/discovery.rs` (datagram codec, `PeerTable`, UDP service), specified in `PROTOCOL.md` §Discovery.

## Scope

- `PROTOCOL.md`: DNS-SD service type, instance-naming rule and its collision behavior, the TXT record schema with bounds, and an explicit statement that Apple platforms must use the system Bonjour API and why.
- `core::discovery::dnssd`: service-type constant, TXT encode/decode with the same bounded-validation discipline as the datagram codec, and conversion to and from the existing `Advertisement` so both transports feed one `PeerTable`.
- A backend seam: discovery results enter the core as data, so a Bonjour-backed shell and a multicast-backed core path are interchangeable.
- Conformance vectors extended with TXT cases, giving the iOS shell an executable target.
- Tests: deterministic codec and naming tests only.

## Non-goals

- **No raw-multicast responder in this milestone.** A real mDNS responder multicasts on the developer's actual network, which violates the test discipline established in M1a (discovery tests never broadcast) and cannot be verified deterministically. The `mdns-sd` backend is a follow-up, gated and manually verified.
- No change to the UDP broadcast channel, which remains the primary transport on desktop.
- No fingerprint in TXT yet: identity exists (M2a) but publishing it in an unauthenticated record needs the trust-model decision from M3.
- No Qt-side mDNS. The baseline stays on broadcast.

## Compatibility and Risk

- **Wire:** additive. A peer that does not speak DNS-SD is unaffected; the broadcast channel is unchanged.
- **Cross-verification gap:** the Qt baseline has no mDNS, so TXT vectors are pinned by one implementation only. They are still worth committing — their audience is the iOS shell, which will be the second implementation.
- **Duplicate peers:** a device found over both broadcast and DNS-SD must not appear twice. `PeerTable` is keyed on IP address, which merges them, but only if the DNS-SD path resolves to the same address family and value.
- **Security:** DNS-SD is as unauthenticated as broadcast. TXT contents are untrusted input and are bounded exactly like datagram fields; nothing there may gate a security decision.

## Implementation Steps

- [x] Specify the DNS-SD service type, instance naming, and TXT schema in `PROTOCOL.md`, including the Apple entitlement rationale.
- [x] `core::dnssd`: service type, bounded TXT codec, conversion to `Advertisement`.
- [x] Instance-name handling: conflict-suffix stripping, without misreading ordinary parentheses as a suffix.
- [x] Extend the conformance fixture with 11 resolved-service cases and 7 instance-name cases; verified from the Rust suite.
- [x] Docs: ARCHITECTURE and TESTING updated.

## Validation

- [x] `./scripts/lint.sh`
- [x] `./scripts/typecheck.sh`
- [x] `./scripts/test.sh` with `WIREHOP_REQUIRE_RUST=1` — 63 Qt + 83 Rust cases, 0 failures.
- [x] `./scripts/smoke.sh`
- [ ] Real Bonjour interoperability — **deferred to the iOS shell milestone**, where `dns-sd -B _wirehop._tcp` on macOS is the check.

## Progress Log

- 2026-08-12: Plan created. Corrected the premise that adding an mDNS crate to the core would unblock iOS: it would not, because a self-implemented responder needs the same entitlement. The deliverable is therefore the cross-backend contract, not a library integration.

## Open Questions

- Whether the device name belongs in the DNS-SD instance name, in TXT, or both. Instance name is idiomatic and gets automatic conflict resolution; TXT is easier to change without re-registering.

## Completion Notes

M1b delivers the **contract**, not a library integration — which is the correction this milestone turned on. Adding an mDNS crate to the core would not have unblocked iOS: a self-implemented responder binds `224.0.0.251:5353` and joins the group, which needs the same entitlement as any other multicast. Only Apple's Bonjour API avoids it, and that lives in the shell. So `core::dnssd` is transport-free: service type, instance-naming rule, bounded TXT codec, and conversion into the existing `Advertisement` so a Bonjour-backed shell and a multicast-backed desktop path feed one `PeerTable`.

The fixture now carries 11 resolved-service cases and 7 instance-name cases. That is the iOS shell's executable target: given an instance name, port, and TXT set, this is the peer it must produce.

Validation: lint, typecheck, smoke green; 63 Qt + 83 Rust cases, zero failures.

**Not done, deliberately:** no raw-multicast responder. A real one multicasts on the developer's network, which breaks the M1a discipline that discovery tests never broadcast and cannot be verified deterministically. **Not verified:** real Bonjour interoperability — `dns-sd -B _wirehop._tcp` on macOS is the check, and it belongs to the iOS shell milestone where there is something to browse for. Nothing registers the service yet on any platform.
