# Architecture

## Overview

WireHop is a single-process Qt Widgets application. `main.cpp` creates the application and a `TrayIcon`; the tray object owns the long-lived TCP server, UDP discovery service, settings/about dialogs, and menu. Short-lived dialogs and transfer-session objects handle outbound and inbound transfers through Qt signals and slots.

### Two implementations, one specification

The repository now holds **two independent implementations of the same wire protocol**, and understanding their relationship is a prerequisite for changing either:

| | `WireHop/` | `core/` |
| --- | --- | --- |
| Language | C++11 / Qt 5 | Rust |
| Role | the shipping desktop application, and the interoperability reference | future implementation for all platforms, consumed by native shells |
| Crypto | libsodium | pure-Rust `blake2`, `chacha20poly1305`, `x25519-dalek` |
| Status | complete for v1 | v1 protocol, blocking sessions, discovery codec/peer table/service; no UI |

They share **no code**, by decision (`docs/decisions/2026-08-12-rust-core-architecture.md`). UniFFI binds Swift and Kotlin but not C++, so the Qt application is never a consumer of the core — the two are peers that must interoperate on the wire. The core is therefore written clean-room from `docs/references/PROTOCOL.md`, which makes the specification (not shared code) the load-bearing asset and turns any ambiguity in it into a defect that surfaces during implementation rather than in the field.

What keeps them from drifting apart is mechanical, not editorial:

- `docs/references/protocol-vectors.json` — canonical message bytes, the session-code derivation, and a negotiation-parsing table, verified by `tests/tst_protocolvectors.cpp` and `core/wirehop-cli/tests/vectors.rs`. The Rust side also re-emits it, so the committed file cannot go stale.
- `tests/tst_interop.cpp` — live transfers in both directions between the Qt session objects and the Rust `wirehop-cli` process, asserting byte-identical delivery, negotiated capabilities, and the acknowledgment.

Both run in `./scripts/test.sh` and are gated in CI by `WIREHOP_REQUIRE_RUST=1`. If they cannot be kept green, that is the signal to revisit the architecture decision before more platform shells are written.

## Tech Stack

- Language: C++11.
- UI/runtime: Qt 5 Core, Gui, Widgets, and Network modules.
- Build system: qmake project at `WireHop/WireHop.pro`, then platform `make` or Visual Studio tooling.
- Cryptography: libsodium scalar multiplication and ChaCha20-Poly1305 IETF authenticated encryption.
- Persistence: platform-native `QSettings`; received files are written to the configured download directory.
- Localization: Qt `.ts`/`.qm` resources; Simplified Chinese is currently present.
- CI: GitHub Actions packaging jobs for Linux, Windows, and macOS.

## Directory Map

| Path | Responsibility |
| --- | --- |
| `core/` | Rust workspace: `wirehop-core` library and `wirehop-cli` driver. Second implementation of the wire protocol. |
| `WireHop/` | Application C++ sources, headers, qmake project, UI forms, resources, and platform metadata. |
| `WireHop/icons/` | Tray, dialog, and packaging artwork. |
| `WireHop/locales/` | Qt translation source and compiled catalog. |
| `misc/` | Linux desktop entry used during installation/packaging. |
| `.github/workflows/` | Cross-platform packaging and manual artifact cleanup. |
| `docs/` | Product, architecture, standards, security, testing, decisions, and task state. |
| `scripts/` | Stable local build and validation entry points. |

## Module Boundaries

- `main.cpp` owns application metadata, localization setup, top-level error handling, and event-loop startup.
- `TrayIcon` composes application-level services and opens the user workflows.
- `Settings` is the only abstraction over persistent `QSettings` keys.
- `DiscoveryService` owns the UDP socket, the datagram size bound, self-address filtering, and the choice of send targets; it must not initiate transfers or own UI state. Datagram construction and parsing live in `Protocol` so they are testable without a GUI — `discoveryservice.cpp` pulls in `QMessageBox` and cannot link into the test suite.
- Peer-list lifetime differs between the implementations by design: the Qt side keeps a peer until it announces `port: 0` (`SendToDialog::newHost`), while `core`'s `PeerTable` also ages entries out on a last-seen basis. Local list-keeping is not a wire concern.
- `FileTransferServer` accepts TCP connections and creates receiver sessions/dialogs.
- `FileTransferSession` owns shared framing, key negotiation, encryption/decryption, state, and transfer signals.
- `FileTransferSender` and `FileTransferReceiver` implement their respective metadata and byte-stream state machines. They receive the device name and download path through their constructors and stay QtCore/QtNetwork-only so the loopback test suite can link them; opening the download folder is a signal handled by the dialog layer.
- `FileTransferPolicy` is the reusable boundary for portable leaf filenames, transfer-size limits, collision naming, and non-overwriting temporary-file commits.
- Dialog classes translate user actions and session signals into UI. Generated `ui_*.h` files come from the checked-in `.ui` forms and must not be edited directly.
- `Crypto` is the libsodium boundary. Protocol or cryptographic changes require explicit compatibility and security review.
- `core/wirehop-core` mirrors this layering in Rust: `crypto` (key exchange, AEAD, session code), `frame` (length-prefixed framing), `message` (metadata/response/ack plus canonical JSON), `policy` (untrusted-input bounds), `store` (non-overwriting commit), `session` (blocking sender/receiver state machines), `discovery` (datagram codec, peer table with last-seen expiry, UDP service), `dnssd` (DNS-SD service type and TXT contract), `identity` (persistent certificate and fingerprint), `tls` (TLS 1.3 configurations and exporter-derived session code).
- Discovery has **two transports and one contract**. Subnet broadcast is primary and lives in `discovery`; DNS-SD is complementary and its schema lives in `dnssd`, deliberately without a transport. Apple platforms must reach mDNS through the system Bonjour API — an app that multicasts itself, including via a bundled mDNS library, needs an entitlement Apple grants only by application — so on those platforms the shell supplies results and the core must not assume it owns a socket. `core/wirehop-cli` is a GUI-free driver used by the conformance and interop gates.
- `Protocol` (`protocol.h/.cpp`) owns the wire version constant, capability identifiers, and bounded parsing of peer negotiation fields (see `docs/references/PROTOCOL.md`). Sessions adopt peer version/capabilities only from decrypted frames; the copies in discovery datagrams are untrusted hints.

## Data Flow

### Startup and discovery

1. `main.cpp` creates `TrayIcon`.
2. The tray starts `FileTransferServer`, using the configured port or an ephemeral port when the setting is zero.
3. `DiscoveryService` binds UDP port 52637 and broadcasts compact JSON containing the device name, platform type, and TCP port.
4. Send dialogs refresh discovery every second and maintain a list of peer IP/port endpoints. Manual address and port entry is also supported.

### Transfer

1. The sender opens a TCP connection; the server creates a receiver session for an accepted socket.
2. Both peers exchange ephemeral public keys and derive a shared session key. The UI displays a six-digit digest for user comparison.
3. Subsequent messages use a two-byte big-endian length followed by a nonce and ChaCha20-Poly1305 ciphertext.
4. The sender transmits encrypted JSON metadata with device information, filenames, and sizes, plus its `protocol_version` and `caps` negotiation fields.
5. The receiver displays the request and sends an encrypted accept/reject response carrying the same negotiation fields; both sides adopt the peer's version/capabilities at this point.
6. When accepted, the sender streams encrypted chunks and the receiver writes bytes in metadata order to hidden temporary files in the configured directory.
7. A completed temporary file is atomically renamed when the platform permits. Existing destination names are preserved and the received file receives a numbered suffix.
8. After committing the last file, the receiver sends one encrypted `{"ack":1}` frame (best effort) and disconnects. The sender waits in `WAITING_FOR_ACK` — the full 10-second window when the peer negotiated the `ack` capability, otherwise a 2-second grace window: an acknowledgment yields "Done!", while a close or timeout without one yields a qualified "sent, not confirmed" message, never an error.
9. Progress signals update the dialog; the receiver's dialog opens the download directory in response to the session's signal.

The acknowledgment is additive and keeps LANDrop 0.4.0 wire compatibility:

| | Legacy receiver (no ACK) | WireHop receiver (sends ACK) |
| --- | --- | --- |
| Legacy sender | Unchanged behavior. | The ACK frame arrives after the legacy sender finished; its `processReceivedData` ignores frames outside `HANDSHAKE2`, so both sides show completion as before. |
| WireHop sender | The receiver closes right after the last byte; the sender reports "Sent, but the receiver did not confirm delivery." as qualified success. | The sender shows "Done!" only after the receiver confirms every file was committed. |

## Dependency Rules

- Keep UI decisions out of discovery, cryptography, and transfer primitives; communicate through signals and slots.
- Keep network ownership explicit through QObject parenting and preserve asynchronous event-loop behavior.
- Do not add a second settings mechanism or bypass `Settings` for persisted application preferences.
- Preserve protocol framing and peer compatibility unless the task explicitly includes a versioning/migration design.
- Add external libraries through `WireHop/WireHop.pro` and document platform packaging implications.

## External Interfaces

| Interface | Purpose | Integration point |
| --- | --- | --- |
| Local UDP port 52637 | Peer discovery request/advertisement JSON. | `DiscoveryService` |
| macOS Services ("Send with WireHop") | Finder context-menu file intake via NSServices/pasteboard. | `macservices.mm` → `TrayIcon::sendFiles` |
| macOS Share sheet ("WireHop") | Share-extension appex (`WireHop/shareext/`, built by `scripts/build-share-extension.sh`) forwards file URLs to the app through LaunchServices. | appex → document open → `FileOpenCollector` |
| File-open events and CLI arguments | External file intake (open-with, dock drop, `wirehop <files>`); `CFBundleDocumentTypes` (viewer, rank None) makes the app a valid open-with target. | `FileOpenCollector` in `main.cpp` → `TrayIcon::sendFiles` |
| Local TCP listener | Key exchange, approval metadata, and encrypted file stream. | `FileTransferServer` / transfer sessions |
| GitHub Releases API for `TracyAniu/WireHop` | Manual update version check using the latest release tag. | `SettingsDialog` |
| Latest WireHop GitHub release | Browser destination when an update is accepted. | `SettingsDialog` |
| Platform settings store | Device name, download path, discoverability, server port. | `Settings` |

On first launch, `Settings` copies compatible preferences from the legacy `LANDrop` / `LANDrop` settings namespace when the corresponding WireHop key is absent. The migration is marked complete and is not repeated.

## Known Tradeoffs and Risks

- The Qt Test suite covers transfer-policy, crypto error paths, and GUI-free loopback peer transfers including protocol negotiation, but there is no automated UI/dialog suite and no automated two-machine coverage yet.
- Wire changes must follow the version/capability negotiation rules in `docs/references/PROTOCOL.md`: new features are capabilities gated on values adopted inside the encrypted session, and `protocol_version` bumps are reserved for message-format breaks.
- Discovery trusts unauthenticated LAN broadcasts. The session code and receiver confirmation are the user-visible peer check.
- Incoming filenames and declared sizes cross a trust boundary. Current limits and non-overwriting commit behavior are centralized in `FileTransferPolicy`; see `docs/SECURITY.md` before changing them.
- The checked-in packaging workflow and action versions reflect an older source snapshot and may require maintenance before release use.
