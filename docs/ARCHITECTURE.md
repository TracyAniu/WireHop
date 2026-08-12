# Architecture

## Overview

WireHop is a single-process Qt Widgets application. `main.cpp` creates the application and a `TrayIcon`; the tray object owns the long-lived TCP server, UDP discovery service, settings/about dialogs, and menu. Short-lived dialogs and transfer-session objects handle outbound and inbound transfers through Qt signals and slots.

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
- `DiscoveryService` owns UDP discovery and emits peer endpoints; it must not initiate transfers or own UI state.
- `FileTransferServer` accepts TCP connections and creates receiver sessions/dialogs.
- `FileTransferSession` owns shared framing, key negotiation, encryption/decryption, state, and transfer signals.
- `FileTransferSender` and `FileTransferReceiver` implement their respective metadata and byte-stream state machines. They receive the device name and download path through their constructors and stay QtCore/QtNetwork-only so the loopback test suite can link them; opening the download folder is a signal handled by the dialog layer.
- `FileTransferPolicy` is the reusable boundary for portable leaf filenames, transfer-size limits, collision naming, and non-overwriting temporary-file commits.
- Dialog classes translate user actions and session signals into UI. Generated `ui_*.h` files come from the checked-in `.ui` forms and must not be edited directly.
- `Crypto` is the libsodium boundary. Protocol or cryptographic changes require explicit compatibility and security review.

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
4. The sender transmits encrypted JSON metadata with device information, filenames, and sizes.
5. The receiver displays the request and sends an encrypted accept/reject response.
6. When accepted, the sender streams encrypted chunks and the receiver writes bytes in metadata order to hidden temporary files in the configured directory.
7. A completed temporary file is atomically renamed when the platform permits. Existing destination names are preserved and the received file receives a numbered suffix.
8. After committing the last file, the receiver sends one encrypted `{"ack":1}` frame (best effort) and disconnects. The sender waits up to 10 seconds in `WAITING_FOR_ACK`: an acknowledgment yields "Done!", while a close or timeout without one yields a qualified "sent, not confirmed" message, never an error.
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

- The Qt Test suite covers transfer-policy and crypto error paths, but there is no automated loopback peer-transfer or UI suite yet.
- The transfer protocol has no explicit version field, so wire changes can silently break compatibility.
- Discovery trusts unauthenticated LAN broadcasts. The session code and receiver confirmation are the user-visible peer check.
- Incoming filenames and declared sizes cross a trust boundary. Current limits and non-overwriting commit behavior are centralized in `FileTransferPolicy`; see `docs/SECURITY.md` before changing them.
- The checked-in packaging workflow and action versions reflect an older source snapshot and may require maintenance before release use.
