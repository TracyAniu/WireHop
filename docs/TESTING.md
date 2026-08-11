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
| `./scripts/test.sh` | Builds and runs the Qt Test suite for filename/size policy, collision-safe commits, and cryptographic input handling. |
| `./scripts/smoke.sh` | Compiles, starts WireHop, and verifies the process remains alive for a short interval. |

Application build artifacts default to the ignored `build-agent/` directory and test artifacts to `build-agent-tests/`. `WIREHOP_BUILD_DIR`, `WIREHOP_TEST_BUILD_DIR`, `QMAKE_BIN`, `WIREHOP_JOBS`, and `WIREHOP_SMOKE_SECONDS` are supported overrides. The corresponding legacy `LANDROP_*` names remain accepted as fallbacks.

## Current Strategy

The Qt Test target covers portable filename validation, declared size arithmetic, collision naming, non-overwriting temporary-file commits, shared-key encryption round trips, malformed key lengths, short ciphertext, and authentication failure. Compilation catches type/link/resource integration errors, and the smoke wrapper covers initial native process startup.

Network framing, dialog behavior, persistence, actual peer-to-peer transfer, interruption cleanup timing, and non-macOS behavior still require manual validation. The next automated layer should be a loopback sender/receiver integration test with isolated temporary directories.

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

State exactly which wrappers and manual paths ran. Do not describe peer-to-peer, UI, or untested platform behavior as verified merely because the unit suite passes.
