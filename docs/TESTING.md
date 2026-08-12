# Testing

## Prerequisites

- Qt 5 with `qmake` available on `PATH`, or set `QMAKE_BIN`.
- A C++ toolchain and `make`.
- libsodium development headers and library. When `pkg-config` is available, the harness passes its include and library directories to qmake.
- A working native system tray for the startup smoke check.

## Commands

| Command | Coverage |
| --- | --- |
| `./scripts/dev.sh` | Configures, compiles, and runs the native application. |
| `./scripts/typecheck.sh` | Performs the closest available static check by compiling all application sources. |
| `./scripts/lint.sh` | Checks harness shell syntax, `features.json`, trailing whitespace, and `git diff --check`. |
| `./scripts/test.sh` | Builds and runs the Qt Test suite (filename/size/port policy, collision-safe commits, cryptographic input handling, a loopback sender/receiver integration suite, and the conformance vectors), then the Rust core suite. |
| `./scripts/smoke.sh` | Compiles, starts WireHop, and verifies the process remains alive for a short interval. |

Application build artifacts default to the ignored `build-agent/` directory and test artifacts to `build-agent-tests/`. `WIREHOP_BUILD_DIR`, `WIREHOP_TEST_BUILD_DIR`, `QMAKE_BIN`, `WIREHOP_JOBS`, and `WIREHOP_SMOKE_SECONDS` are supported overrides. The corresponding legacy `LANDROP_*` names remain accepted as fallbacks.

Never run `macdeployqt` inside the build directory: it rewrites the bundle in place, invalidates the code signature (launches then die with SIGKILL on Apple Silicon), and breaks later incremental builds (SIGABRT during QApplication startup). Packaging must deploy into a staging copy; `configure_wirehop` refuses to reuse a build directory that contains a deployed `qt.conf`.

## Current Strategy

The Qt Test target covers portable filename validation, declared size arithmetic, port parsing, collision naming, non-overwriting temporary-file commits, shared-key encryption round trips, malformed key lengths, short ciphertext, and authentication failure. A GUI-free loopback suite (`tests/tst_filetransfersession.cpp`) runs real sender/receiver sessions over 127.0.0.1 into isolated temporary directories: accepted multi-file transfer with byte comparison, rejection, mid-transfer disconnect without leftover partial files, repeated-respond handling, watchdog timeout of an idle peer, and malformed encrypted metadata. Protocol negotiation is covered end-to-end: version/capability adoption on both sides, LANDrop-0.4.0-shaped metadata and response peers emulated on raw sockets, an out-of-bounds capability list degrading to legacy while the transfer completes, and the short acknowledgment grace window for capless peers; `tests/tst_protocol.cpp` unit-tests the bounded negotiation parsers, the UTF-8 byte bound, and deterministic capability ordering. Discovery datagram construction and parsing are covered for the first time via the extracted `Protocol` codec (the service itself still cannot link into the suite: it pulls in `QMessageBox`). The suite is 63 Qt cases as of 2026-08-12, plus 64 Rust cases in `core/`.

The session/sender/receiver sources link without QtGui (`QT -= gui` in `tests/tests.pro`); keep transfer primitives free of widget and QDesktopServices dependencies so this stays true. Compilation catches type/link/resource integration errors, and the smoke wrapper covers initial native process startup.

**Cross-implementation conformance.** `docs/references/protocol-vectors.json` is the contract between the C++/Qt application and the Rust core (`core/`). `tests/tst_protocolvectors.cpp` and `core/wirehop-cli/tests/vectors.rs` both verify against it, and the Rust side additionally re-emits the fixture to prove the committed file is not stale. It pins the session-code derivation, the negotiation-parsing table including malformed input, the canonical JSON bytes of every message, and the discovery datagrams — construction byte-for-byte plus a 19-case parsing table.

Discovery tests must never broadcast: a test that sprayed the real network would pick up genuine peers and stop being deterministic. Both suites bind loopback and use explicit unicast targets.

A failure means, in order of likelihood: the Rust core diverged, the Qt application diverged, or `PROTOCOL.md` was ambiguous enough that both readings were defensible. Investigate in that order. **Never regenerate the fixture to make the build green** — that discards the only signal that the two implementations still agree. Regeneration (`cargo run -p wirehop-cli -- emit-vectors > docs/references/protocol-vectors.json`) is a wire-protocol change and is reviewed as one.

CI (`.github/workflows/package.yml`) runs `lint.sh` and `test.sh` on every push and pull request; the packaging jobs run only after that job passes.

Dialog behavior, persistence, real two-machine transfer, and non-macOS runtime behavior still require manual validation.

## Critical Manual Workflows

1. Launch: verify the tray icon appears, the application reports its listening port, and every menu item opens the expected dialog or action.
2. Discovery: run two compatible peers on the same LAN, refresh the send dialog, and verify discoverable peers appear and disappear correctly.
3. Accepted transfer: send empty and non-empty regular files, compare the six-digit code on both peers, accept, verify progress/completion, and byte-compare every received file.
4. Rejected/error transfer: reject a request and test disconnect, invalid endpoint, unwritable destination, and interrupted transfer behavior without leftover misleading success state.
5. Settings: change device name, download path, discoverability, and port; restart where required and verify persistence and advertised behavior.
6. Localization: run with Simplified Chinese locale after user-visible text changes and inspect affected dialogs for missing or clipped translations.
7. macOS Services intake: register the bundle (`lsregister -f <path>/WireHop.app`, then `/System/Library/CoreServices/pbs -update`), right-click a file in Finder → Services → "Send with WireHop" (用 WireHop 发送 on a Chinese account), and verify the send dialog opens preloaded. `./scripts/dev.sh <file>` covers the CLI intake path. Note: editing `Info.plist` requires the `_common.sh` staleness guard (qmake never regenerates the bundle plist on its own).
8. macOS Share sheet: after registering the bundle, `pluginkit -m -i io.github.tracyaniu.wirehop.share` must list the extension (`+` prefix when enabled; enable with `pluginkit -e use -i …`). Right-click a file → Share → WireHop must open the preloaded send dialog. `open -a <path>/WireHop.app <file>` is the scriptable proxy for the forwarding chain. Signing rule: the appex must keep its sandbox entitlements — never re-sign the app with `codesign --deep` after embedding (it strips them); reseal the outer bundle without `--deep` instead.

Use isolated test files and a dedicated temporary download directory. Never overwrite valuable user data during validation.

## When to Add Tests

- Every bug fix should gain a regression test when the affected logic can be isolated.
- When extending a production method, re-check every test double that **inherits** it. A double that overrides only one method silently stops modelling what its name claims once the inherited method changes — `SilentAckReceiver` (formerly `LegacyReceiver`) was invalidated exactly this way when `respond()` gained negotiation fields. Model foreign/legacy peers on raw sockets, not by subclassing our own implementation.
- Test helpers must not let exceptions unwind into QTest: an escaping `std::exception` terminates the whole binary and discards every remaining result.
- Protocol parsing, framing, size arithmetic, filename handling, and cryptographic error paths require tests when changed.
- State-machine or QObject lifetime changes require success, rejection, disconnect, and repeated-callback coverage.
- Packaging-only changes require at least the affected platform build/package job or an equivalent local check.

## Validation Reporting

State exactly which wrappers and manual paths ran. Do not describe peer-to-peer, UI, or untested platform behavior as verified merely because the unit suite passes.
