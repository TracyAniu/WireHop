# Execution Plan: Rust Core M0 — Foundation and Interop Proof

## Goal

A Rust core crate exists in this repository, builds and tests through the standard harness scripts, and **proves it speaks the same wire protocol as the C++/Qt baseline**: golden vectors reproduced byte-for-byte by both implementations, and a loopback interop test in which the Rust core completes a real encrypted transfer against the existing application's session code. Observable outcome: `./scripts/test.sh` runs both the Qt suite and the Rust suite, and CI fails if the two implementations diverge on the wire.

## Context

- Architecture decision: `docs/decisions/2026-08-12-rust-core-architecture.md` (Rust core + native shells; core implemented from spec, not ported; Qt app stays as baseline and interop peer).
- Wire specification: `docs/references/PROTOCOL.md` — the load-bearing asset for this program. v1 is the current format.
- Research report `docs/跨平台局域网文件传输工具技术调研.md` §3.1 (discovery), §3.2 (transport and performance discipline), §3.3 (security model), §3.4 (dual-protocol negotiation), §5 (stack selection).
- Deterministic `caps` serialization landed 2026-08-12 (`WireHop/protocol.cpp`), which is what makes byte-reproducible golden vectors possible at all.
- Harness entry points to extend: `scripts/{lint,typecheck,test}.sh` and `scripts/_common.sh`; CI at `.github/workflows/package.yml`.

## Milestone map (this program, for future sessions)

| Milestone | Content | Plan |
| --- | --- | --- |
| **M0** | Core skeleton, harness/CI integration, golden vectors, v1 interop with the Qt baseline | **this plan** |
| M1 | Discovery reliability: mDNS + subnet broadcast + multicast in parallel, last-seen expiry, cached-fingerprint→IP unicast probe at startup, network-change socket rebuild, multi-NIC/IPv6 scope handling | follow-up |
| M2 | v2 transport substrate (TCP + TLS 1.3 / `rustls`, persistent self-signed identity) **and** performance: large blocks (1–4 MiB), read→encrypt→send pipeline, parallel streams, small-file packing stream, BLAKE3 | follow-up |
| M3 | Trust devices: TOFU fingerprint store, confirmation code as a short authentication string from the TLS exporter, trusted-peer auto-accept | follow-up |
| M4 | LocalSend v2 compatibility layer (HTTPS REST), selected by capability negotiation | follow-up |
| M5+ | QUIC fast path (`quinn`); native shells (Swift/Kotlin); Wi-Fi Aware | follow-up |

Sequencing note: the owner ranked performance ahead of security, but the TLS 1.3 substrate and the identity layer are one design decision (report §3.2 + §3.3), and large-frame framing built on the current custom AEAD handshake would be rewritten when TLS lands. M2 therefore carries the transport substrate together with the performance work; M3 keeps the trust/UX half of "security". Adopting TLS also retires the report's three §2.1 criticisms (unauthenticated DH, sequence-less random nonces, no persistent identity) in one step.

## Scope

- A Cargo workspace at `core/` with one library crate (`wirehop-core`) plus a thin CLI binary for exercising it without a GUI.
- v1 protocol implementation in Rust, written **from `PROTOCOL.md`**: X25519 key exchange, ChaCha20-Poly1305-IETF framing with the 2-byte big-endian length prefix, metadata/response/file-data/ack message sequence, `protocol_version`/`caps` negotiation with the documented bounds (≤32 caps, 1–32 UTF-8 bytes, fail-to-legacy), and the six-digit BLAKE2b session code.
- A golden-vector fixture set checked into the repo, covering: negotiation field serialization, a full metadata frame, a response frame, the ack frame, and session-code derivation from a fixed key pair. Both implementations verify against the same fixtures.
- Harness integration: `lint.sh` gains `cargo fmt --check` + `cargo clippy -D warnings`; `typecheck.sh` gains `cargo check`; `test.sh` gains `cargo test`. Each degrades with a clear message when the Rust toolchain is absent, so the Qt-only path keeps working.
- A cross-implementation interop test: the Rust CLI acts as sender against the Qt receiver session (and the reverse), over loopback, verifying byte-identical file delivery and a negotiated `ack`.
- CI: run the Rust jobs alongside the existing test job; packaging stays gated on both.

## Non-goals

- No discovery work (M1), no TLS/v2 transport (M2), no trust store (M3), no LocalSend layer (M4), no QUIC, no native shells.
- No changes to the Qt application's behavior. It is the reference; if a divergence is found, the fix goes to whichever side contradicts `PROTOCOL.md`, and the spec is corrected when it is the ambiguous party.
- No UniFFI bindings yet — nothing consumes the core until a shell exists.
- No port of C++ sources into Rust (see the ADR: clean-room from spec).

## Compatibility and Risk

- **Wire:** none. M0 adds an implementation, not a format. The Rust core targets v1 exactly as specified.
- **Divergence risk** — the central one: two implementations of one protocol drift silently. Mitigated by golden vectors plus the interop test as CI gates. Any spec ambiguity discovered while writing the Rust side is a documentation defect to fix in `PROTOCOL.md`, and that is a deliberate benefit of clean-room implementation.
- **Toolchain risk:** the repo becomes dual-toolchain. Contributors without Rust must still be able to build and test the Qt app; the scripts must skip rather than fail, and the skip must be loud enough that CI never silently stops checking the core.
- **Licensing:** the LANDrop BSD-3-Clause obligation stays confined to `WireHop/`. Do not copy code across; keep the notices intact.
- **Security:** M0 reimplements the inherited v1 crypto, which the research report criticizes on three counts. Those are addressed in M2/M3, not here — the Rust v1 path must not be presented as an improvement over the baseline's security.

## Implementation Steps

- [x] Read `AGENTS.md` and `PROTOCOL.md`; confirm a clean tree; install and pin the Rust toolchain (`rust-toolchain.toml`, rustup stable).
- [x] Create the `core/` Cargo workspace: `wirehop-core` library + `wirehop-cli` binary; `.gitignore` for `core/target/`.
- [x] Implement the crypto layer from spec: X25519 scalarmult, ChaCha20-Poly1305-IETF with 12-byte random nonce prefix, BLAKE2b session-code derivation.
- [x] Implement framing: 2-byte big-endian length prefix, incremental `FrameReader`, 64,000-byte quantum, compile-time bound check.
- [x] Implement negotiation: version/caps emission with sorted caps, bounded parsing with fail-to-legacy, negotiated-intersection rule.
- [x] Implement messages and receiver-side validation (metadata/response/ack, filename and size policy).
- [x] Add the golden-vector fixture and the Rust test that verifies it, including a staleness check that re-emits and compares.
- [x] Add the mirroring Qt-side test (`tests/tst_protocolvectors.cpp`) that verifies the **same** fixture.
- [x] Extend `scripts/{lint,typecheck,test}.sh` with the Rust steps, with graceful skip plus `WIREHOP_REQUIRE_RUST=1` strict mode.
- [x] Extend CI to install Rust, cache the build, and run both suites with strict mode; packaging stays gated on them.
- [x] Docs: `AGENTS.md` (repo map, stable commands, fixture rule), `docs/TESTING.md` (conformance gate).
- [ ] Build the loopback interop test (Rust CLI ↔ Qt session, both directions) and add it to `test.sh`. **Not started.**
- [ ] `docs/ARCHITECTURE.md`: two implementations, one spec. **Not started.**

## Validation

- [x] `./scripts/lint.sh` (shell + `cargo fmt --check` + `cargo clippy -D warnings`, all clean)
- [x] `./scripts/typecheck.sh` (Qt build + `cargo check --all-targets`)
- [x] `./scripts/test.sh` — **55 Qt cases + 41 Rust cases, 0 failures**, including both halves of the conformance gate.
- [x] `./scripts/smoke.sh` — re-run after the `Crypto` refactor; app launched and stayed alive.
- [ ] Interop verified in both directions with byte-comparison of delivered files, and the negotiated `ack` observed. **Not done — the loopback interop test is unwritten.**
- [x] Toolchain-absent path exercised: `run_cargo` skips with a clear message and returns 0; `WIREHOP_REQUIRE_RUST=1` returns 2.

## Progress Log

- 2026-08-12 (implementation, part 1): Installed rustup stable; built the `core/` workspace with `wirehop-core` (crypto, frame, protocol, policy, message) and `wirehop-cli`. **The conformance gate is live and green**: the Qt application and the clean-room Rust core agree byte-for-byte on session codes, all 11 negotiation-parsing cases, and canonical JSON. 55 Qt + 41 Rust cases pass; lint and typecheck clean.

  Writing the second implementation surfaced **three spec defects in `PROTOCOL.md`**, exactly the benefit the ADR predicted — all now fixed and pinned by vectors: (1) the session code said only "a BLAKE2b digest … mod 10^6", omitting the 16-byte digest length, the first-8-bytes selection, and the little-endian read; any of the three read differently produces a different code on each side and silently defeats the out-of-band check. (2) File-data framing did not state that boundaries are implied solely by declared sizes, that a sender never straddles a boundary while a receiver must tolerate one, or that a zero-byte file carries no data frames at all. (3) "Absent or malformed fields ⇒ version 0 with no capabilities" read as though a bad version voids caps; both implementations in fact degrade the two fields independently, so a peer may be version 0 *with* `ack`. Also documented receiver validation bounds and the canonical-JSON rule the vectors depend on.

  One behavior-preserving production change: `Crypto::sessionKeyDigest()` now delegates to a new static `Crypto::sessionCodeForKey()` so the derivation can be pinned against fixed keys instead of a live handshake.

  **Remaining for M0:** the loopback interop test (Rust CLI ↔ Qt session, both directions) and the `ARCHITECTURE.md` update. Everything else in this plan is done and verified.
- 2026-08-12: Plan created after the owner chose the research report's main line (Rust core + native shells) over the report's own §5 correction note, with milestone order discovery → performance → security → LocalSend. Recorded the architecture decision in `docs/decisions/2026-08-12-rust-core-architecture.md`, including the consequence that UniFFI does not bind C++ and therefore the Qt app is an interop peer rather than a consumer of the core. Implementation not started.

## Open Questions

- Minimum supported Rust version: currently unpinned beyond `stable`. Pin once a shell needs a specific version.
- Whether to vendor dependencies for offline/reproducible CI builds.

Resolved during implementation: repository layout is a single repo (`core/` beside `WireHop/`), and the fixture lives spec-adjacent at `docs/references/protocol-vectors.json`.

## Completion Notes

(to be filled at completion)
