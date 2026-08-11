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
| `./scripts/test.sh` | Intentionally exits 2 because no automated Qt test target exists. |
| `./scripts/smoke.sh` | Compiles, starts LANDrop, and verifies the process remains alive for a short interval. |

Build artifacts default to the ignored `build-agent/` directory. `LANDROP_BUILD_DIR`, `QMAKE_BIN`, `LANDROP_JOBS`, and `LANDROP_SMOKE_SECONDS` are supported overrides.

## Current Strategy

The repository currently has no Qt Test target, fixtures, or automated peer-transfer test. Compilation catches type/link/resource integration errors, and the smoke wrapper covers only initial native process startup. Network, dialog, persistence, and actual file-transfer behavior still require manual validation.

When adding automated coverage, prioritize pure protocol/metadata/crypto tests first, then loopback sender/receiver integration with isolated temporary directories. Make `scripts/test.sh` invoke the suite once a real target exists.

## Critical Manual Workflows

1. Launch: verify the tray icon appears, the application reports its listening port, and every menu item opens the expected dialog or action.
2. Discovery: run two compatible peers on the same LAN, refresh the send dialog, and verify discoverable peers appear and disappear correctly.
3. Accepted transfer: send empty and non-empty regular files, compare the six-digit code on both peers, accept, verify progress/completion, and byte-compare every received file.
4. Rejected/error transfer: reject a request and test disconnect, invalid endpoint, unwritable destination, and interrupted transfer behavior without leftover misleading success state.
5. Settings: change device name, download path, discoverability, and port; restart where required and verify persistence and advertised behavior.
6. Localization: run with Simplified Chinese locale after user-visible text changes and inspect affected dialogs for missing or clipped translations.

Use isolated test files and a dedicated temporary download directory. Never overwrite valuable user data during validation.

## When to Add Tests

- Every bug fix should gain a regression test when the affected logic can be isolated.
- Protocol parsing, framing, size arithmetic, filename handling, and cryptographic error paths require tests when changed.
- State-machine or QObject lifetime changes require success, rejection, disconnect, and repeated-callback coverage.
- Packaging-only changes require at least the affected platform build/package job or an equivalent local check.

## Validation Reporting

State exactly which wrappers and manual paths ran. An intentional status-2 result from `scripts/test.sh` means automated behavior remains unverified; do not describe it as a passing test suite.
