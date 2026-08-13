# 2026-08-12: Rust core with native shells

## Context

WireHop today is a C++/Qt desktop application derived from LANDrop 0.4.0, with macOS Services and Share-sheet integration and an in-session protocol version/capability negotiation layer (`docs/references/PROTOCOL.md`). `docs/跨平台局域网文件传输工具技术调研.md` evaluates the target product — AirDrop/LANDrop-class transfer across Windows/macOS/iOS/Android with deep shell integration — and recommends a single Rust core bound to thin native shells (§5, "质量线"), because both headline requirements (LANDrop-class throughput, deep system integration) are that route's strengths, and because every right-click/share integration component is native code regardless of UI framework.

That report also carries a correction note (§5 附注) arguing the C++/Qt desktop should not be rewritten. The owner has decided otherwise: the current application is a **baseline**, and development proceeds along the report's main line.

## Decision

Adopt a Rust core crate as the future implementation of discovery, transport, cryptography, trust storage, and the transfer engine, bound to native shells (SwiftUI on macOS/iOS, Compose on Android, Tauri or WinUI on Windows). The existing C++/Qt application remains a working, shipping baseline and an interoperability reference until a native shell reaches parity; it is not deleted and not retrofitted to consume the Rust core.

Consequences of that last clause, stated explicitly because it is easy to assume otherwise: **UniFFI generates Swift/Kotlin/Python bindings, not C++ ones.** The Qt application is therefore not a consumer of the core. The two implementations coexist as independent peers that must interoperate on the wire, which makes the wire specification — not shared code — the load-bearing asset.

Three rules follow from that:

1. The Rust core is implemented **from `docs/references/PROTOCOL.md`, not by porting the C++ sources.** A clean-room implementation from the spec validates the spec, keeps the LANDrop BSD-3-Clause obligation confined to the existing tree, and turns any divergence into a spec bug rather than a silent behavior fork.
2. **Golden test vectors** are promoted to a first-class deliverable. Both implementations must reproduce them byte-for-byte. Deterministic serialization of negotiation fields (landed 2026-08-12) is a prerequisite.
3. The core speaks **both** protocol generations: v1 (the current custom X25519 + ChaCha20-Poly1305 framing) for interop with the baseline and with LANDrop 0.4.0, and the v2 fast path, selected by the existing capability negotiation. This is the report's own §3.4 "双协议并行" conclusion applied to our own two implementations.

## Alternatives considered

Keeping C++/Qt and adding features incrementally was rejected by the owner: it caps the product at Qt's poor mobile story (plus LGPL static-linking obligations) and has no QUIC path. Deferring the core-language choice until after a LocalSend compatibility layer was rejected as sequencing that delays the irreversible decision without reducing its cost. Retrofitting the Qt app onto the Rust core via a C ABI (cbindgen) was rejected as work on a component scheduled for replacement.

## Compatibility and failure behavior

The wire protocol is the compatibility boundary, not the codebase. The baseline keeps working unchanged throughout. Interop between the Rust core and the Qt baseline is a CI gate, not a manual hope; if it cannot be kept green, that is the signal to re-examine this decision before more shells are written. Adding a second toolchain raises contributor setup and CI cost — accepted deliberately.

The v2 transport (TCP + TLS 1.3 via `rustls`, self-signed certificates with fingerprint identity) is a breaking wire change and requires a `protocol_version` bump; it ships as a negotiated capability so v1 peers are unaffected. Adopting TLS also retires three weaknesses the report lists for the inherited design (§2.1): unauthenticated DH, random per-frame nonces with no sequence, and the absence of persistent identity — the confirmation code becomes a short authentication string derived from the TLS key exporter, which is what makes it genuinely MITM-resistant rather than advisory.

## Validation

Each milestone carries its own execution plan. Program-level gates: golden vectors reproduced by both implementations; a loopback interop test (Rust core ↔ Qt baseline) in CI; and, once the v2 transport lands, the report's §3.2 performance discipline — an iperf3 ceiling per link, a netem loss × latency matrix in CI, and a >5% regression block.
